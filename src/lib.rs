#![no_std]
extern crate alloc;

use alloc::string::String;
use mork_common::mork_kernel_log;
use mork_common::types::ResultWithErr;
use mork_hal::mm::PageTableImpl;

pub mod page_table;
mod heap;

pub fn init(kernel_page_table: &mut PageTableImpl) -> ResultWithErr<String> {
    mork_kernel_log!(info, "start mm init");
    heap::init();
    page_table::map_kernel_window(kernel_page_table)?;
    kernel_page_table.active();
    mork_kernel_log!(info, "kernel page table map success");
    Ok(())
}