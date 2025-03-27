#![no_std]
extern crate alloc;

use alloc::string::String;
use log::info;
use mork_common::types::ResultWithErr;
use mork_hal::mm::PageTableImpl;

pub mod page_table;
mod heap;

pub fn init(kernel_page_table: &mut PageTableImpl) -> ResultWithErr<String> {
    info!("start mm init");
    heap::init();
    page_table::map_kernel_window(kernel_page_table)?;
    kernel_page_table.active();
    Ok(())
}