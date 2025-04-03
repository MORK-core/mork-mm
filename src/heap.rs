

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use buddy_system_allocator::Heap;
use spin::mutex::Mutex;


const HEAP_SIZE: usize = 1 << 24;
const ORDER: usize = 32;

static HEAP_MEM: [u64; HEAP_SIZE / 8] = [0u64; HEAP_SIZE / 8];

static HEAP: Mutex<Heap<ORDER>> = Mutex::new(Heap::empty());

pub fn init() {
    unsafe {
        HEAP.lock().init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE);
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