//! Physical and virtual address newtypes.

use core::fmt;

/// 4 KiB page size.
pub const PAGE_SIZE: u64 = 4096;
/// 2 MiB large page size.
pub const LARGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
/// 1 GiB huge page size.
pub const HUGE_PAGE_SIZE: u64 = 1024 * 1024 * 1024;

/// A physical memory address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(addr & 0x000F_FFFF_FFFF_FFFF)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }

    #[inline]
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }

    #[inline]
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }

    #[inline]
    pub const fn add(self, offset: u64) -> Self {
        Self::new(self.0 + offset)
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

/// A virtual memory address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Truncate to the canonical 48-bit form (sign extended from bit 47).
    #[inline]
    pub const fn new(addr: u64) -> Self {
        Self(((addr << 16) as i64 >> 16) as u64)
    }

    /// Construct without canonicalisation.
    ///
    /// # Safety
    /// The caller must pass a canonical address.
    #[inline]
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    #[inline]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    #[inline]
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }

    #[inline]
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }

    #[inline]
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }

    /// PML4, PDPT, PD and PT indices for this address.
    #[inline]
    pub const fn page_table_indices(self) -> [usize; 4] {
        [
            ((self.0 >> 39) & 0x1FF) as usize,
            ((self.0 >> 30) & 0x1FF) as usize,
            ((self.0 >> 21) & 0x1FF) as usize,
            ((self.0 >> 12) & 0x1FF) as usize,
        ]
    }

    #[inline]
    pub const fn page_offset(self) -> u64 {
        self.0 & 0xFFF
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
