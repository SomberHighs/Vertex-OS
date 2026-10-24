//! Virtual memory manager: 4-level paging on the active address space.
//!
//! Limine leaves us with an identity-ish kernel mapping, the higher-half direct
//! map (HHDM) and the kernel image mapped at `0xffffffff80000000`. We keep that
//! PML4 and extend it: new page tables are allocated from the PMM and reached
//! through the HHDM.

use super::addr::{PhysAddr, VirtAddr, PAGE_SIZE};
use super::pmm;
use crate::arch::x86_64::cpu;

pub mod flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const NO_CACHE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

/// Flags for ordinary kernel data pages.
pub const KERNEL_DATA: u64 =
    flags::PRESENT | flags::WRITABLE | flags::GLOBAL | flags::NO_EXECUTE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    AlreadyMapped,
    OutOfMemory,
    HugePageConflict,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub fn is_present(self) -> bool {
        self.0 & flags::PRESENT != 0
    }

    #[inline]
    pub fn is_huge(self) -> bool {
        self.0 & flags::HUGE != 0
    }

    #[inline]
    pub fn addr(self) -> PhysAddr {
        PhysAddr::new(self.0 & 0x000F_FFFF_FFFF_F000)
    }

    #[inline]
    pub fn set(&mut self, addr: PhysAddr, flags: u64) {
        self.0 = (addr.as_u64() & 0x000F_FFFF_FFFF_F000) | flags;
    }

    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// One page table: 512 entries covering 2 MiB of address space.
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    /// The PML4 currently loaded in CR3.
    ///
    /// # Safety
    /// The caller must not replace CR3's tables underneath the returned
    /// reference, and must keep interrupts disabled while mutating it.
    pub unsafe fn active() -> &'static mut PageTable {
        let cr3 = cpu::read_cr3() & 0x000F_FFFF_FFFF_F000;
        let virt = pmm::phys_to_virt(PhysAddr::new(cr3));
        &mut *virt.as_mut_ptr::<PageTable>()
    }

    #[inline]
    pub fn entry(&self, index: usize) -> PageTableEntry {
        self.entries[index]
    }

    #[inline]
    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }
}

/// Walk `table` and return the table at `index`, allocating it if `create`.
unsafe fn next_table(
    table: &mut PageTable,
    index: usize,
    create: bool,
) -> Result<&'static mut PageTable, MapError> {
    let entry = table.entry_mut(index);
    if !entry.is_present() {
        if !create {
            return Err(MapError::OutOfMemory);
        }
        let page = pmm::alloc_page().ok_or(MapError::OutOfMemory)?;
        entry.set(page, flags::PRESENT | flags::WRITABLE | flags::NO_EXECUTE);
        return Ok(&mut *pmm::phys_to_virt(page).as_mut_ptr::<PageTable>());
    }
    if entry.is_huge() {
        return Err(MapError::HugePageConflict);
    }
    Ok(&mut *pmm::phys_to_virt(entry.addr()).as_mut_ptr::<PageTable>())
}

/// Map one 4 KiB page.
///
/// # Safety
/// Caller must hold the address-space lock (interrupts off) and must not map
/// an address that is already in use.
pub unsafe fn map_page(
    pml4: &mut PageTable,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: u64,
) -> Result<(), MapError> {
    let indices = virt.page_table_indices();

    let pdpt = next_table(pml4, indices[0], true)?;
    let pd = next_table(pdpt, indices[1], true)?;
    let pt = next_table(pd, indices[2], true)?;

    let entry = pt.entry_mut(indices[3]);
    if entry.is_present() {
        return Err(MapError::AlreadyMapped);
    }
    entry.set(phys, flags | flags::PRESENT);
    Ok(())
}

/// Unmap one page and return its physical address.
///
/// # Safety
/// See [`map_page`].
pub unsafe fn unmap_page(
    pml4: &mut PageTable,
    virt: VirtAddr,
) -> Result<PhysAddr, MapError> {
    let indices = virt.page_table_indices();

    let pdpt = next_table(pml4, indices[0], false)?;
    let pd = next_table(pdpt, indices[1], false)?;
    let pt = next_table(pd, indices[2], false)?;

    let entry = pt.entry_mut(indices[3]);
    if !entry.is_present() {
        return Err(MapError::AlreadyMapped);
    }
    let phys = entry.addr();
    entry.clear();
    cpu::invlpg(virt.as_u64());
    Ok(phys)
}

/// Translate a virtual address using the given page table root.
pub unsafe fn translate(pml4: &PageTable, virt: VirtAddr) -> Option<PhysAddr> {
    let indices = virt.page_table_indices();

    let pml4e = pml4.entry(indices[0]);
    if !pml4e.is_present() {
        return None;
    }
    let pdpt = &*pmm::phys_to_virt(pml4e.addr()).as_ptr::<PageTable>();
    let pdpte = pdpt.entry(indices[1]);
    if !pdpte.is_present() {
        return None;
    }
    if pdpte.is_huge() {
        // 1 GiB page.
        return Some(PhysAddr::new(
            pdpte.addr().as_u64() + virt.as_u64() % super::addr::HUGE_PAGE_SIZE,
        ));
    }
    let pd = &*pmm::phys_to_virt(pdpte.addr()).as_ptr::<PageTable>();
    let pde = pd.entry(indices[2]);
    if !pde.is_present() {
        return None;
    }
    if pde.is_huge() {
        // 2 MiB page.
        return Some(PhysAddr::new(
            pde.addr().as_u64() + virt.as_u64() % super::addr::LARGE_PAGE_SIZE,
        ));
    }
    let pt = &*pmm::phys_to_virt(pde.addr()).as_ptr::<PageTable>();
    let pte = pt.entry(indices[3]);
    if !pte.is_present() {
        return None;
    }
    Some(PhysAddr::new(
        pte.addr().as_u64() + virt.page_offset() % PAGE_SIZE,
    ))
}
