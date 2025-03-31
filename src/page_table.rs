use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use mork_common::types::ResultWithErr;
use mork_common::utils::alignas::is_aligned;
use mork_common::mork_kernel_log;
use mork_hal::config::HAL_PAGE_LEVEL;
use mork_hal::KERNEL_OFFSET;
use mork_hal::mm::PageTableImpl;

pub struct MutPageTableWrapper<'a> {
    page_table: &'a mut PageTableImpl,
    level: usize,
}

#[derive(Debug, PartialEq)]
pub enum SearchResult {
    Found(usize),
    Missing(usize),
}

impl<'a> MutPageTableWrapper<'a> {
    pub fn new(root: &'a mut PageTableImpl) -> Self {
        Self {
            page_table: root,
            level: 0,
        }
    }

    pub fn map_kernel(&mut self, vaddr: usize, paddr: usize) -> Result<usize, String> {
        let aligned_size = PageTableImpl::get_size(0).unwrap();
        if !is_aligned(vaddr, aligned_size) || !is_aligned(paddr, aligned_size) {
            return Err(format!("Kernel map vaddr must aligned for the first level, vaddr: {}, {}", vaddr, paddr));
        }
        let mask = (1usize << 39) - 1;
        self.page_table.map_frame_for_kernel(vaddr & mask, paddr, 0);
        Ok(aligned_size)
    }

    pub fn map_normal_frame(&mut self, vaddr: usize, paddr: usize, is_x: bool, is_w: bool, is_r: bool)
        -> ResultWithErr<String> {
        if !is_aligned(vaddr, 4096) || !is_aligned(paddr, 4096) {
            return Err(format!("vaddr/addr vaddr must be aligned, {}, {}", vaddr, paddr).into());
        }

        match self.search_for_insert(vaddr)? {
            (SearchResult::Missing(level), page_table) => {
                if level == HAL_PAGE_LEVEL - 1 {
                    mork_kernel_log!(debug, "map_normal_frame, paddr: {:#x}, vaddr: {:#x}, \
                        is_x: {}, is_w: {}, is_r: {}", paddr, vaddr, is_x, is_w, is_r);
                    page_table
                        .map_frame_for_user(
                            vaddr,
                            paddr,
                            level,
                            is_x, is_w, is_r
                        );
                } else {
                    let inner_page_table = Box::leak(Box::new(PageTableImpl::new()));
                    mork_kernel_log!(debug, "inner_page_table_ptr: {:#x}", inner_page_table.get_ptr());
                    page_table
                        .map_page_table(
                            vaddr,
                            inner_page_table.get_ptr() - KERNEL_OFFSET,
                            level,
                        );
                    let mut wrapper = Self {
                        page_table: inner_page_table,
                        level: level + 1,
                    };
                    return wrapper.map_normal_frame(vaddr, paddr, is_x, is_w, is_r);
                }
            }
            _ => {
                mork_kernel_log!(warn, "vaddr {:#x} has been mapped", vaddr);
            }
        }
        Ok(())
    }

    pub fn search_for_insert(&mut self, vaddr: usize) -> Result<(SearchResult, &mut PageTableImpl), String> {
        let mut current_level = self.level;
        let mut current_pt: &mut PageTableImpl = &mut *self.page_table;

        loop {
            if current_level >= HAL_PAGE_LEVEL {
                return Err(format!("Exceed max level {}", HAL_PAGE_LEVEL));
            }

            let index = PageTableImpl::get_index(vaddr, current_level)
                .ok_or("Invalid page table index")?;

            let pte = &mut current_pt[index]; // 可变借用

            if !pte.valid() {
                return Ok((SearchResult::Missing(current_level), current_pt));
            }

            if pte.is_leaf() {
                return Ok((SearchResult::Found(current_level), current_pt));
            }

            // 进入下一级时需要转移所有权
            let next_pt = unsafe { pte.get_page_table() };
            current_pt = next_pt;
            current_level += 1;
        }
    }
}

pub fn map_kernel_window(kernel_page_table: &mut PageTableImpl) -> ResultWithErr<String> {
    let mut local_kernel_page_table = PageTableImpl::new();
    let mut wrapper = MutPageTableWrapper::new(&mut local_kernel_page_table);
    let (_, _, end) = mork_hal::get_memory_info().map_err(|()| "failed to get memory info")?;
    // ROOT_PAGE_TABLE.map()
    let mut start = 0;
    while start < end {
        start += wrapper.map_kernel(start + KERNEL_OFFSET, start)?;
    }
    *kernel_page_table = local_kernel_page_table;
    Ok(())
}