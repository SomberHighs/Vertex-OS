//! Physical Memory Manager
//!
//! A simple bitmap-based physical memory allocator.

use super::addr::{PhysAddr, PAGE_SIZE};
use crate::sync::SpinLock;
use crate::kprintln;
use limine::memory_map::{Entry, EntryType};

/// Maximum supported physical memory (4 GiB for now)
const MAX_MEMORY: u64 = 4 * 1024 * 1024 * 1024;

/// Number of pages we can track
const MAX_PAGES: usize = (MAX_MEMORY / PAGE_SIZE) as usize;

/// Bitmap size in bytes
const BITMAP_SIZE: usize = MAX_PAGES / 8;

/// Physical memory allocator state
struct PhysicalMemoryAllocator {
    /// Bitmap where 1 = free, 0 = used
    bitmap: [u8; BITMAP_SIZE],
    /// Total number of pages
    total_pages: usize,
    /// Number of free pages
    free_pages: usize,
    /// Higher Half Direct Map offset
    hhdm_offset: u64,
}

impl PhysicalMemoryAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [0; BITMAP_SIZE],
            total_pages: 0,
            free_pages: 0,
            hhdm_offset: 0,
        }
    }
    
    /// Mark a page as free
    fn mark_free(&mut self, page: usize) {
        if page < MAX_PAGES {
            let byte = page / 8;
            let bit = page % 8;
            if self.bitmap[byte] & (1 << bit) == 0 {
                self.bitmap[byte] |= 1 << bit;
                self.free_pages += 1;
            }
        }
    }
    
    /// Mark a page as used
    fn mark_used(&mut self, page: usize) {
        if page < MAX_PAGES {
            let byte = page / 8;
            let bit = page % 8;
            if self.bitmap[byte] & (1 << bit) != 0 {
                self.bitmap[byte] &= !(1 << bit);
                self.free_pages -= 1;
            }
        }
    }
    
    /// Check if a page is free
    fn is_free(&self, page: usize) -> bool {
        if page < MAX_PAGES {
            let byte = page / 8;
            let bit = page % 8;
            self.bitmap[byte] & (1 << bit) != 0
        } else {
            false
        }
    }
    
    /// Allocate a single page
    fn alloc_page(&mut self) -> Option<PhysAddr> {
        for byte_idx in 0..BITMAP_SIZE {
            if self.bitmap[byte_idx] != 0 {
                for bit in 0..8 {
                    if self.bitmap[byte_idx] & (1 << bit) != 0 {
                        let page = byte_idx * 8 + bit;
                        self.mark_used(page);
                        return Some(PhysAddr::new(page as u64 * PAGE_SIZE));
                    }
                }
            }
        }
        None
    }
    
    /// Allocate contiguous pages
    fn alloc_pages(&mut self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.alloc_page();
        }
        
        let mut start = 0;
        let mut found = 0;
        
        for page in 0..MAX_PAGES {
            if self.is_free(page) {
                if found == 0 {
                    start = page;
                }
                found += 1;
                if found == count {
                    // Found enough contiguous pages
                    for p in start..start + count {
                        self.mark_used(p);
                    }
                    return Some(PhysAddr::new(start as u64 * PAGE_SIZE));
                }
            } else {
                found = 0;
            }
        }
        
        None
    }
    
    /// Free a single page
    fn free_page(&mut self, addr: PhysAddr) {
        let page = (addr.as_u64() / PAGE_SIZE) as usize;
        self.mark_free(page);
    }
    
    /// Free contiguous pages
    fn free_pages(&mut self, addr: PhysAddr, count: usize) {
        let start = (addr.as_u64() / PAGE_SIZE) as usize;
        for page in start..start + count {
            self.mark_free(page);
        }
    }
}

static PMM: SpinLock<PhysicalMemoryAllocator> = SpinLock::new(PhysicalMemoryAllocator::new());

/// Initialize the physical memory manager
pub fn init(memory_map: &[&Entry], hhdm_offset: u64) {
    let mut pmm = PMM.lock();
    pmm.hhdm_offset = hhdm_offset;
    
    let mut total_memory = 0u64;
    let mut usable_memory = 0u64;
    
    // First pass: count memory
    for entry in memory_map {
        total_memory += entry.length;
        if entry.entry_type == EntryType::USABLE {
            usable_memory += entry.length;
        }
    }
    
    // Second pass: mark usable regions as free
    for entry in memory_map {
        if entry.entry_type == EntryType::USABLE {
            let start_page = (entry.base / PAGE_SIZE) as usize;
            let page_count = (entry.length / PAGE_SIZE) as usize;
            
            for page in start_page..start_page + page_count {
                if page < MAX_PAGES {
                    pmm.mark_free(page);
                    pmm.total_pages = pmm.total_pages.max(page + 1);
                }
            }
        }
    }
    
    kprintln!("  Total memory: {} MiB", total_memory / 1024 / 1024);
    kprintln!("  Usable memory: {} MiB", usable_memory / 1024 / 1024);
    kprintln!("  Free pages: {}", pmm.free_pages);
}

/// Allocate a single physical page
pub fn alloc_page() -> Option<PhysAddr> {
    PMM.lock().alloc_page()
}

/// Allocate multiple contiguous physical pages
pub fn alloc_pages(count: usize) -> Option<PhysAddr> {
    PMM.lock().alloc_pages(count)
}

/// Free a single physical page
pub fn free_page(addr: PhysAddr) {
    PMM.lock().free_page(addr)
}

/// Free multiple contiguous physical pages
pub fn free_pages(addr: PhysAddr, count: usize) {
    PMM.lock().free_pages(addr, count)
}

/// Get the number of free pages
pub fn free_page_count() -> usize {
    PMM.lock().free_pages
}

/// Get the HHDM offset
pub fn hhdm_offset() -> u64 {
    PMM.lock().hhdm_offset
}
