#![no_std]
extern crate alloc;

use alloc::string::String;
use mork_common::mork_kernel_log;
use mork_common::types::ResultWithErr;
use crate::page_table::PageTable;

pub mod page_table;
mod heap;

pub fn init(kernel_page_table: &mut PageTable) -> ResultWithErr<String> {
    mork_kernel_log!(info, "start mm init");
    let (_, kernel_end, memory_end) = mork_hal::get_memory_info().map_err(|_| "fail to get memory info")?;
    heap::init(kernel_end, memory_end);
    page_table::map_kernel_window(kernel_page_table)?;
    kernel_page_table.page_table_impl.active();
    mork_kernel_log!(info, "kernel page table map success");
    Ok(())
}