//! Physical memory manager: a bitmap over the first 4 GiB.
//!
//! The bootloader memory map is the source of truth. Every region that is not
//! `USABLE` stays allocated, and the kernel image plus the first megabyte are
//! reserved explicitly so the allocator can never hand out live memory.

use core::sync::atomic::{AtomicU64, Ordering};

use limine::memmap::{Entry, MEMMAP_USABLE};

use super::addr::{PhysAddr, PAGE_SIZE};
use crate::sync::SyncCell;

/// Highest physical address we track.
const MAX_MEMORY: u64 = 4 * 1024 * 1024 * 1024;
const MAX_PAGES: usize = (MAX_MEMORY / PAGE_SIZE) as usize;
const BITMAP_BYTES: usize = MAX_PAGES / 8;

struct PhysicalMemoryAllocator {
    /// 1 = free, 0 = used.
    bitmap: [u8; BITMAP_BYTES],
    total_pages: usize,
    free_pages: usize,
}

impl PhysicalMemoryAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [0; BITMAP_BYTES],
            total_pages: 0,
            free_pages: 0,
        }
    }

    #[inline]
    fn is_free(&self, page: usize) -> bool {
        page < MAX_PAGES && self.bitmap[page / 8] & (1 << (page % 8)) != 0
    }

    fn mark_free(&mut self, page: usize) {
        if page < MAX_PAGES && !self.is_free(page) {
            self.bitmap[page / 8] |= 1 << (page % 8);
            self.free_pages += 1;
        }
    }

    fn mark_used(&mut self, page: usize) {
        if page < MAX_PAGES && self.is_free(page) {
            self.bitmap[page / 8] &= !(1 << (page % 8));
            self.free_pages -= 1;
        }
    }

    fn free_range(&mut self, base: u64, length: u64) {
        let first = (base + PAGE_SIZE - 1) / PAGE_SIZE;
        let last = (base + length) / PAGE_SIZE;
        for page in first..last {
            self.mark_free(page as usize);
        }
    }

    fn reserve_range(&mut self, base: u64, length: u64) {
        if length == 0 {
            return;
        }
        let first = base / PAGE_SIZE;
        let last = (base + length + PAGE_SIZE - 1) / PAGE_SIZE;
        for page in first..last {
            self.mark_used(page as usize);
        }
    }

    fn alloc_page(&mut self) -> Option<PhysAddr> {
        for byte_index in 0..BITMAP_BYTES {
            let byte = self.bitmap[byte_index];
            if byte == 0 {
                continue;
            }
            for bit in 0..8 {
                if byte & (1 << bit) != 0 {
                    let page = byte_index * 8 + bit;
                    self.mark_used(page);
                    return Some(PhysAddr::new(page as u64 * PAGE_SIZE));
                }
            }
        }
        None
    }

    fn alloc_pages(&mut self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.alloc_page();
        }

        let mut run_start = 0usize;
        let mut run_len = 0usize;
        for page in 0..MAX_PAGES {
            if self.is_free(page) {
                if run_len == 0 {
                    run_start = page;
                }
                run_len += 1;
                if run_len == count {
                    for p in run_start..run_start + count {
                        self.mark_used(p);
                    }
                    return Some(PhysAddr::new(run_start as u64 * PAGE_SIZE));
                }
            } else {
                run_len = 0;
            }
        }
        None
    }
}

static PMM: SyncCell<PhysicalMemoryAllocator> = SyncCell::new(PhysicalMemoryAllocator::new());
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Initialise the allocator from the bootloader memory map.
pub fn init(entries: &[&Entry], hhdm_offset: u64, kernel_start: u64, kernel_end: u64) {
    HHDM_OFFSET.store(hhdm_offset, Ordering::Release);

    unsafe {
        let pmm = PMM.get();

        // Everything starts allocated; only USABLE regions are freed below.
        for page in 0..MAX_PAGES {
            pmm.mark_used(page);
        }
        pmm.free_pages = 0;

        // "Total" is the highest usable address: MMIO holes and reserved
        // regions above RAM must not inflate the reported figure.
        let mut highest_usable = 0u64;
        for entry in entries {
            if entry.type_ == MEMMAP_USABLE {
                let end = (entry.base + entry.length).min(MAX_MEMORY);
                if end > highest_usable {
                    highest_usable = end;
                }
                pmm.free_range(entry.base, entry.length);
            }
        }
        pmm.total_pages = (highest_usable / PAGE_SIZE) as usize;

        // Never hand out the real-mode IVT/BIOS area.
        pmm.reserve_range(0, 0x10_0000);
        // ...nor the kernel image itself.
        pmm.reserve_range(kernel_start, kernel_end.saturating_sub(kernel_start));
    }
}

/// Allocate one zeroed physical page.
pub fn alloc_page() -> Option<PhysAddr> {
    let page = unsafe { PMM.get().alloc_page() };
    if let Some(addr) = page {
        zero_page(addr);
    }
    page
}

/// Allocate `count` contiguous zeroed physical pages.
pub fn alloc_pages(count: usize) -> Option<PhysAddr> {
    let page = unsafe { PMM.get().alloc_pages(count) };
    if let Some(addr) = page {
        for i in 0..count {
            zero_page(addr.add(i as u64 * PAGE_SIZE));
        }
    }
    page
}

fn zero_page(addr: PhysAddr) {
    let virt = phys_to_virt(addr);
    unsafe {
        core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, PAGE_SIZE as usize);
    }
}

/// Free one page.
pub fn free_page(addr: PhysAddr) {
    unsafe {
        PMM.get().mark_free((addr.as_u64() / PAGE_SIZE) as usize);
    }
}

/// Free `count` contiguous pages.
pub fn free_pages(addr: PhysAddr, count: usize) {
    unsafe {
        let pmm = PMM.get();
        for i in 0..count {
            pmm.mark_free(((addr.as_u64() / PAGE_SIZE) as usize) + i);
        }
    }
}

/// Higher-half direct map offset.
#[inline]
pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Acquire)
}

/// Translate a physical address into its direct-map virtual address.
#[inline]
pub fn phys_to_virt(addr: PhysAddr) -> crate::mm::VirtAddr {
    crate::mm::VirtAddr::new(addr.as_u64() + hhdm_offset())
}

/// Number of free pages.
pub fn free_page_count() -> usize {
    unsafe { PMM.get().free_pages }
}

/// Total tracked pages.
pub fn total_page_count() -> usize {
    unsafe { PMM.get().total_pages }
}

/// Free memory in MiB.
pub fn free_mib() -> u64 {
    free_page_count() as u64 * PAGE_SIZE / 1024 / 1024
}

/// Total memory in MiB.
pub fn total_mib() -> u64 {
    total_page_count() as u64 * PAGE_SIZE / 1024 / 1024
}
