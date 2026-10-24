//! Memory management.

pub mod addr;
pub mod heap;
pub mod pmm;
pub mod vmm;

pub use addr::VirtAddr;

use limine::memmap::Entry;

extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// Address range occupied by the kernel image.
pub fn kernel_image_range() -> (u64, u64) {
    let start = unsafe { &__kernel_start as *const u8 as u64 };
    let end = unsafe { &__kernel_end as *const u8 as u64 };
    (start, end)
}

/// Initialise the physical allocator from the bootloader memory map, then the
/// kernel heap.
pub fn init(memory_map: &[&Entry], hhdm_offset: u64) {
    let (start, end) = kernel_image_range();
    pmm::init(memory_map, hhdm_offset, start, end);
    heap::init();
}
