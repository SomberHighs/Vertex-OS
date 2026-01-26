//! Virtual Memory Manager
//!
//! Handles page table management and virtual address space operations.

use super::addr::{PhysAddr, VirtAddr, PAGE_SIZE};
use super::pmm;
use bitflags::bitflags;

bitflags! {
    /// Page table entry flags
    #[derive(Clone, Copy, Debug)]
    pub struct PageFlags: u64 {
        /// Page is present in memory
        const PRESENT = 1 << 0;
        /// Page is writable
        const WRITABLE = 1 << 1;
        /// Page is accessible from user mode
        const USER = 1 << 2;
        /// Write-through caching
        const WRITE_THROUGH = 1 << 3;
        /// Disable caching
        const NO_CACHE = 1 << 4;
        /// Page was accessed
        const ACCESSED = 1 << 5;
        /// Page was written to
        const DIRTY = 1 << 6;
        /// Huge page (2MiB or 1GiB)
        const HUGE = 1 << 7;
        /// Global page (not flushed on CR3 switch)
        const GLOBAL = 1 << 8;
        /// Disable execution
        const NO_EXECUTE = 1 << 63;
    }
}

/// A page table entry
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create a new empty entry
    pub const fn new() -> Self {
        Self(0)
    }
    
    /// Create an entry with address and flags
    pub const fn with_addr_flags(addr: PhysAddr, flags: PageFlags) -> Self {
        Self((addr.as_u64() & 0x000F_FFFF_FFFF_F000) | flags.bits())
    }
    
    /// Get the flags
    pub fn flags(&self) -> PageFlags {
        PageFlags::from_bits_truncate(self.0)
    }
    
    /// Get the physical address
    pub fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 & 0x000F_FFFF_FFFF_F000)
    }
    
    /// Check if present
    pub fn is_present(&self) -> bool {
        self.0 & PageFlags::PRESENT.bits() != 0
    }
    
    /// Check if huge page
    pub fn is_huge(&self) -> bool {
        self.0 & PageFlags::HUGE.bits() != 0
    }
    
    /// Set the entry
    pub fn set(&mut self, addr: PhysAddr, flags: PageFlags) {
        self.0 = (addr.as_u64() & 0x000F_FFFF_FFFF_F000) | flags.bits();
    }
    
    /// Clear the entry
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// A page table (512 entries)
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// Create a new empty page table
    pub const fn new() -> Self {
        Self {
            entries: [PageTableEntry::new(); 512],
        }
    }
    
    /// Get an entry
    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }
    
    /// Get a mutable entry
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }
}

/// Higher Half Direct Map offset (set during init)
static mut HHDM_OFFSET: u64 = 0;

/// Initialize the virtual memory manager
pub fn init(hhdm_offset: u64) {
    unsafe {
        HHDM_OFFSET = hhdm_offset;
    }
}

/// Convert a physical address to a virtual address using HHDM
pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    unsafe { VirtAddr::new(phys.as_u64() + HHDM_OFFSET) }
}

/// Convert a virtual address to a physical address (only works for HHDM addresses)
pub fn virt_to_phys(virt: VirtAddr) -> PhysAddr {
    unsafe { PhysAddr::new(virt.as_u64() - HHDM_OFFSET) }
}

/// Map a virtual address to a physical address
/// 
/// # Safety
/// Caller must ensure the addresses are valid and the mapping won't cause issues.
pub unsafe fn map_page(
    pml4: &mut PageTable,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageFlags,
) -> Result<(), &'static str> {
    let indices = virt.page_table_indices();
    
    // Get or create PDPT
    let pdpt = get_or_create_table(pml4, indices[0], flags)?;
    
    // Get or create PD
    let pd = get_or_create_table(pdpt, indices[1], flags)?;
    
    // Get or create PT
    let pt = get_or_create_table(pd, indices[2], flags)?;
    
    // Set the page table entry
    let entry = pt.entry_mut(indices[3]);
    if entry.is_present() {
        return Err("Page already mapped");
    }
    
    entry.set(phys, flags | PageFlags::PRESENT);
    
    Ok(())
}

/// Unmap a virtual address
/// 
/// # Safety
/// Caller must ensure the address is mapped and unmapping won't cause issues.
pub unsafe fn unmap_page(pml4: &mut PageTable, virt: VirtAddr) -> Result<PhysAddr, &'static str> {
    let indices = virt.page_table_indices();
    
    // Walk the page tables
    let pdpt = get_table(pml4, indices[0]).ok_or("PDPT not present")?;
    let pd = get_table(pdpt, indices[1]).ok_or("PD not present")?;
    let pt = get_table(pd, indices[2]).ok_or("PT not present")?;
    
    let entry = pt.entry_mut(indices[3]);
    if !entry.is_present() {
        return Err("Page not mapped");
    }
    
    let phys = entry.addr();
    entry.clear();
    
    // Invalidate TLB entry
    crate::arch::x86_64::cpu::invlpg(virt.as_u64());
    
    Ok(phys)
}

/// Get or create a page table at the given index
unsafe fn get_or_create_table(
    parent: &mut PageTable,
    index: usize,
    flags: PageFlags,
) -> Result<&mut PageTable, &'static str> {
    let entry = parent.entry_mut(index);
    
    if !entry.is_present() {
        // Allocate a new page table
        let page = pmm::alloc_page().ok_or("Out of memory")?;
        
        // Zero the new page table
        let virt = phys_to_virt(page);
        core::ptr::write_bytes(virt.as_mut_ptr::<u8>(), 0, PAGE_SIZE as usize);
        
        // Set the entry
        entry.set(page, flags | PageFlags::PRESENT | PageFlags::WRITABLE);
    }
    
    let addr = phys_to_virt(entry.addr());
    Ok(&mut *addr.as_mut_ptr::<PageTable>())
}

/// Get a page table at the given index
unsafe fn get_table(parent: &PageTable, index: usize) -> Option<&mut PageTable> {
    let entry = parent.entry(index);
    
    if !entry.is_present() {
        return None;
    }
    
    let addr = phys_to_virt(entry.addr());
    Some(&mut *addr.as_mut_ptr::<PageTable>())
}
