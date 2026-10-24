//! Kernel heap: a bump allocator over a lazily mapped virtual region.
//!
//! Every page is backed on demand by the PMM and mapped with `KERNEL_DATA`
//! flags, so the kernel pays only for what it touches. There is no `free`:
//! node storage is append-only by design (see `vfs::node`), and the region is
//! large enough for the lifetime of a boot.

use crate::arch::x86_64::cpu;
use crate::sync::SyncCell;

use super::addr::{PhysAddr, VirtAddr, PAGE_SIZE};
use super::{pmm, vmm};

/// Base of the heap's virtual window.
pub const HEAP_START: u64 = 0xffff_9000_0000_0000;
/// How much virtual space the heap may grow into (64 MiB).
pub const HEAP_SIZE: u64 = 64 * 1024 * 1024;

struct BumpHeap {
    next: u64,
    mapped_end: u64,
    allocations: usize,
    initialized: bool,
}

static HEAP: SyncCell<BumpHeap> = SyncCell::new(BumpHeap {
    next: HEAP_START,
    mapped_end: HEAP_START,
    allocations: 0,
    initialized: false,
});

/// Prepare the heap window.
pub fn init() {
    unsafe {
        let heap = HEAP.get();
        heap.next = HEAP_START;
        heap.mapped_end = HEAP_START;
        heap.allocations = 0;
        heap.initialized = true;
    }
}

/// Allocate `size` bytes aligned to `align`.
///
/// Returns a null pointer if the heap window is exhausted or physical memory
/// has run out.
///
/// # Safety
/// The returned memory is uninitialised and never reclaimed.
pub unsafe fn alloc(size: usize, align: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let align = align.max(1);

    cpu::without_interrupts(|| {
        let heap = HEAP.get();
        if !heap.initialized {
            return core::ptr::null_mut();
        }

        let start = (heap.next + (align as u64 - 1)) & !(align as u64 - 1);
        let end = start + size as u64;
        if start < HEAP_START || end > HEAP_START + HEAP_SIZE {
            return core::ptr::null_mut();
        }

        // Back the new pages on demand.
        let mut frontier = heap.mapped_end;
        while frontier < end {
            let phys = match pmm::alloc_page() {
                Some(phys) => phys,
                None => return core::ptr::null_mut(),
            };
            let pml4 = vmm::PageTable::active();
            if let Err(_err) = vmm::map_page(pml4, VirtAddr::new(frontier), phys, vmm::KERNEL_DATA) {
                pmm::free_page(phys);
                return core::ptr::null_mut();
            }
            frontier += PAGE_SIZE;
        }
        heap.mapped_end = frontier;
        heap.next = end;
        heap.allocations += 1;

        start as *mut u8
    })
}

/// Allocate `count` zeroed objects of type `T`.
///
/// # Safety
/// See [`alloc`].
pub unsafe fn alloc_array<T>(count: usize) -> *mut T {
    let size = core::mem::size_of::<T>().saturating_mul(count);
    let ptr = alloc(size, core::mem::align_of::<T>()) as *mut T;
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr as *mut u8, 0, size);
    }
    ptr
}

/// Copy a byte slice into a fresh heap allocation.
///
/// # Safety
/// See [`alloc`].
pub unsafe fn alloc_copy(bytes: &[u8]) -> *mut u8 {
    let ptr = alloc(bytes.len().max(1), 8);
    if !ptr.is_null() {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr
}

/// Heap usage in bytes: (used, reserved capacity).
pub fn usage() -> (u64, u64) {
    cpu::without_interrupts(|| unsafe {
        let heap = HEAP.get();
        (heap.next.saturating_sub(HEAP_START), HEAP_SIZE)
    })
}

/// Number of successful allocations.
pub fn allocation_count() -> usize {
    cpu::without_interrupts(|| unsafe { HEAP.get().allocations })
}

/// Translate a heap pointer back into a physical address.
pub fn heap_phys(ptr: *const u8) -> Option<PhysAddr> {
    if (ptr as u64) < HEAP_START {
        return None;
    }
    unsafe { vmm::translate(vmm::PageTable::active(), VirtAddr::new(ptr as u64)) }
}
