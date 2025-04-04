

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use buddy_system_allocator::Heap;
use spin::mutex::Mutex;
use mork_common::mork_kernel_log;

const ORDER: usize = 32;

static HEAP: Mutex<Heap<ORDER>> = Mutex::new(Heap::empty());

pub fn init(free_mem_start: usize, free_mem_end: usize) {
    mork_kernel_log!(debug, "start: {:#x}, end: {:#x}", free_mem_start, free_mem_end);
    unsafe {
        HEAP.lock().init(free_mem_start, free_mem_end - free_mem_start);
    }
}

struct Global;

#[global_allocator]
static GLOBAL: Global = Global;

unsafe impl GlobalAlloc for Global {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.lock().alloc(layout).ok()
            .map_or(0 as *mut u8, |allocation| allocation.as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP.lock().dealloc(unsafe { NonNull::new_unchecked(ptr) }, layout);
        return;
    }
}