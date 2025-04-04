use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use mork_capability::cap::PageTableCap;
use mork_common::types::ResultWithErr;
use mork_common::utils::alignas::is_aligned;
use mork_common::mork_kernel_log;
use mork_common::syscall::message_info::ResponseLabel;
use mork_hal::config::HAL_PAGE_LEVEL;
use mork_hal::KERNEL_OFFSET;
use mork_hal::mm::{PageTableEntryImpl, PageTableImpl};
use crate::page_table::SearchResult::{Found, Missing};

#[repr(C, align(4096))]
#[derive(Clone, Copy)]
pub struct PageTable {
    pub page_table_impl: PageTableImpl,
}

impl PageTable {
    pub fn new() -> Self {
        Self { page_table_impl: PageTableImpl::new() }
    }

    pub fn get_ptr(&self) -> usize {
        self as *const _ as usize
    }
    pub fn from_cap(cap: &PageTableCap) -> &mut Self {
        unsafe {
            &mut *((cap.base_ptr() << 12) as usize as *mut Self)
        }
    }
}

pub struct MutPageTableWrapper<'a> {
    page_table: &'a mut PageTable,
    level: usize,
}

pub enum SearchResult<'a> {
    Found(usize, &'a mut PageTable),
    Missing(usize, &'a mut PageTable),
}

impl<'a> MutPageTableWrapper<'a> {
    pub fn new(root: &'a mut PageTable) -> Self {
        Self {
            page_table: root,
            level: 0,
        }
    }

    pub fn map_kernel(&mut self, vaddr: usize, paddr: usize) -> Result<usize, String> {
        let aligned_size = PageTableImpl::get_size(0).unwrap();
        if !is_aligned(vaddr, aligned_size) || !is_aligned(paddr, aligned_size) {
            return Err(format!("Kernel map vaddr must aligned for the first level, vaddr: {:#x}, {:#x}", vaddr, paddr));
        }
        let mask = (1usize << 39) - 1;
        self.page_table.page_table_impl.map_frame_for_kernel(vaddr & mask, paddr - KERNEL_OFFSET, 0);
        Ok(aligned_size)
    }

    pub fn map_page_table(&mut self, vaddr: usize, paddr: usize) -> Result<usize, ResponseLabel> {
        if !is_aligned(vaddr, 4096) || !is_aligned(paddr, 4096) {
            mork_kernel_log!(warn, "vaddr/paddr must be aligned, {:#x}, {:#x}", vaddr, paddr);
            return Err(ResponseLabel::InvalidParam);
        }
        match self.search_for_modify(vaddr, HAL_PAGE_LEVEL) {
            Missing(level, page_table) => {
                if level == HAL_PAGE_LEVEL - 1 {
                    mork_kernel_log!(warn, "page table has been mapped, {:#x}, {:#x}", vaddr, paddr);
                    Err(ResponseLabel::MappedAlready)
                } else {
                    page_table.page_table_impl.map_page_table(vaddr, paddr - KERNEL_OFFSET, level);
                    Ok(level + 1)
                }
            }
            _ => {
                mork_kernel_log!(warn, "frame has been mapped, {:#x}, {:#x}", vaddr, paddr);
                Err(ResponseLabel::MappedAlready)
            }
        }
    }

    pub fn map_frame(&mut self, vaddr: usize, paddr: usize, is_x: bool, is_w: bool, is_r: bool)
        -> ResultWithErr<ResponseLabel> {
        if !is_aligned(vaddr, 4096) || !is_aligned(paddr, 4096) {
            mork_kernel_log!(warn, "vaddr/paddr must be aligned, {:#x}, {:#x}", vaddr, paddr);
            return Err(ResponseLabel::InvalidParam);
        }
        match self.search_for_modify(vaddr, HAL_PAGE_LEVEL) {
            Missing(level, page_table) => {
                if level == HAL_PAGE_LEVEL - 1 {
                    page_table
                        .page_table_impl
                        .map_frame_for_user(
                            vaddr,
                            paddr - KERNEL_OFFSET,
                            level,
                            is_x, is_w, is_r
                        );
                    Ok(())
                } else {
                    mork_kernel_log!(warn, "page table need to been mapped first, {:#x}, {:#x}", vaddr, paddr);
                    Err(ResponseLabel::PageTableMiss)
                }
            }
            _ => {
                mork_kernel_log!(warn, "frame has been mapped, {:#x}, {:#x}", vaddr, paddr);
                Err(ResponseLabel::MappedAlready)
            }
        }
    }

    pub fn unmap_frame(&mut self, vaddr: usize) -> ResultWithErr<ResponseLabel> {
        if !is_aligned(vaddr, 4096) {
            mork_kernel_log!(warn, "vaddr must be aligned, {:#x}", vaddr);
            return Err(ResponseLabel::InvalidParam);
        }
        match self.search_for_modify(vaddr, HAL_PAGE_LEVEL) {
            Found(level, page_table) => {
                mork_kernel_log!(debug, "found frame in level {} page table, vaddr: {:#x}",
                    level, vaddr);
                page_table.page_table_impl.unmap_frame(vaddr, level);
                Ok(())
            }
            Missing(level, _) => {
                mork_kernel_log!(warn, "fail to lookup vaddr {:#x}, level: {}", vaddr, level);
                Err(ResponseLabel::InvalidParam)
            }
        }
    }

    pub fn unmap_page_table(&mut self, vaddr: usize, paddr: usize, level: usize) -> ResultWithErr<ResponseLabel> {
        if !is_aligned(vaddr, 4096) {
            mork_kernel_log!(warn, "vaddr must be aligned, {:#x}", vaddr);
            return Err(ResponseLabel::InvalidParam);
        }
        match self.search_for_modify(vaddr, level - 1)  {
            Found(_, _) => {
                mork_kernel_log!(warn, "mapped frame founded, unmap frame first, vaddr: {:#x}", vaddr);
                Err(ResponseLabel::MappedAlready)
            }
            Missing(level_inner, page_table) => {
                let index = PageTableImpl::get_index(vaddr, level_inner).unwrap();
                let pte = page_table.page_table_impl[index];
                unsafe {
                    if pte.get_page_table().get_ptr() != paddr {
                        mork_kernel_log!(warn, "page table not matched, target paddr: {:#x}, get paddr: {:#x}",
                            paddr, pte.get_page_table().get_ptr());
                        return Err(ResponseLabel::InvalidParam);
                    }
                    page_table.page_table_impl[index] = PageTableEntryImpl::default();
                    Ok(())
                }
            }
        }
    }
    pub fn map_root_task_frame(&mut self, vaddr: usize, paddr: usize, is_x: bool, is_w: bool, is_r: bool)
        -> ResultWithErr<String> {
        if !is_aligned(vaddr, 4096) || !is_aligned(paddr, 4096) {
            return Err(format!("vaddr/paddr must be aligned, {:#x}, {:#x}", vaddr, paddr).into());
        }

        match self.search_for_modify(vaddr, HAL_PAGE_LEVEL) {
            Missing(level, page_table) => {
                if level == HAL_PAGE_LEVEL - 1 {
                    // mork_kernel_log!(debug, "map_root_task_frame, paddr: {:#x}, vaddr: {:#x}, \
                    //     is_x: {}, is_w: {}, is_r: {}", paddr, vaddr, is_x, is_w, is_r);
                    page_table
                        .page_table_impl
                        .map_frame_for_user(
                            vaddr,
                            paddr - KERNEL_OFFSET,
                            level,
                            is_x, is_w, is_r
                        );
                } else {
                    let inner_page_table = Box::leak(Box::new(PageTable::new()));
                    // mork_kernel_log!(debug, "inner_page_table_ptr: {:#x}", inner_page_table.get_ptr());
                    page_table
                        .page_table_impl
                        .map_page_table(
                            vaddr,
                            inner_page_table.get_ptr() - KERNEL_OFFSET,
                            level,
                        );
                    let mut wrapper = Self {
                        page_table: inner_page_table,
                        level: level + 1,
                    };
                    return wrapper.map_root_task_frame(vaddr, paddr, is_x, is_w, is_r);
                }
            }
            _ => {
                mork_kernel_log!(warn, "vaddr {:#x} has been mapped", vaddr);
            }
        }
        Ok(())
    }

    fn search_for_modify(&mut self, vaddr: usize, max_level: usize) -> SearchResult {
        let mut current_level = self.level;
        let mut current_pt: &mut PageTable = &mut *self.page_table;

        loop {
            if current_level >= max_level {
                // return Err(format!("Exceed max level {}", HAL_PAGE_LEVEL));
                mork_kernel_log!(warn, "Exceed max level: {}", max_level);
                return Missing(current_level, current_pt);
            }

            let index = PageTableImpl::get_index(vaddr, current_level)
                .expect("Invalid page table index");

            let pte = &mut current_pt.page_table_impl[index]; // 可变借用

            if !pte.valid() {
                return Missing(current_level, current_pt);
            }

            if pte.is_leaf() {
                return Found(current_level, current_pt);
            }

            // 进入下一级时需要转移所有权
            let next_pt = unsafe {
                &mut *(pte.get_page_table().get_ptr() as *mut PageTable)
            };
            current_pt = next_pt;
            current_level += 1;
        }
    }
}

pub fn map_kernel_window(kernel_page_table: &mut PageTable) -> ResultWithErr<String> {
    let mut local_kernel_page_table = PageTable::new();
    let mut wrapper = MutPageTableWrapper::new(&mut local_kernel_page_table);
    let (_, _, end) = mork_hal::get_memory_info().map_err(|()| "failed to get memory info")?;
    // ROOT_PAGE_TABLE.map()
    let mut start = KERNEL_OFFSET;
    while start < end {
        start += wrapper.map_kernel(start, start)?;
    }
    *kernel_page_table = local_kernel_page_table;
    Ok(())
}