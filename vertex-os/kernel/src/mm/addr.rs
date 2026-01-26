//! Physical and Virtual address types

use core::fmt;

/// A physical memory address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// Create a new physical address
    #[inline]
    pub const fn new(addr: u64) -> Self {
        // x86_64 physical addresses are 52 bits
        Self(addr & 0x000F_FFFF_FFFF_FFFF)
    }
    
    /// Create a physical address without masking (unsafe)
    #[inline]
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }
    
    /// Get the raw address value
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
    
    /// Check if the address is null
    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
    
    /// Align the address down to the given alignment
    #[inline]
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }
    
    /// Align the address up to the given alignment
    #[inline]
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }
    
    /// Check if the address is aligned to the given alignment
    #[inline]
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }
    
    /// Add an offset to the address
    #[inline]
    pub const fn add(self, offset: u64) -> Self {
        Self::new(self.0 + offset)
    }
    
    /// Subtract an offset from the address
    #[inline]
    pub const fn sub(self, offset: u64) -> Self {
        Self::new(self.0 - offset)
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

/// A virtual memory address
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Create a new virtual address (with canonical form enforcement)
    #[inline]
    pub const fn new(addr: u64) -> Self {
        // x86_64 requires canonical addresses (sign-extended from bit 47)
        let addr = ((addr << 16) as i64 >> 16) as u64;
        Self(addr)
    }
    
    /// Create a virtual address without canonicalization (unsafe)
    #[inline]
    pub const unsafe fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }
    
    /// Get the raw address value
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
    
    /// Convert to a pointer
    #[inline]
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }
    
    /// Convert to a mutable pointer
    #[inline]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }
    
    /// Check if the address is null
    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
    
    /// Align the address down to the given alignment
    #[inline]
    pub const fn align_down(self, align: u64) -> Self {
        Self(self.0 & !(align - 1))
    }
    
    /// Align the address up to the given alignment
    #[inline]
    pub const fn align_up(self, align: u64) -> Self {
        Self((self.0 + align - 1) & !(align - 1))
    }
    
    /// Check if the address is aligned to the given alignment
    #[inline]
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }
    
    /// Add an offset to the address
    #[inline]
    pub const fn add(self, offset: u64) -> Self {
        Self::new(self.0.wrapping_add(offset))
    }
    
    /// Subtract an offset from the address
    #[inline]
    pub const fn sub(self, offset: u64) -> Self {
        Self::new(self.0.wrapping_sub(offset))
    }
    
    /// Get the page table indices for this address
    #[inline]
    pub const fn page_table_indices(self) -> [usize; 4] {
        [
            ((self.0 >> 39) & 0x1FF) as usize, // PML4
            ((self.0 >> 30) & 0x1FF) as usize, // PDPT
            ((self.0 >> 21) & 0x1FF) as usize, // PD
            ((self.0 >> 12) & 0x1FF) as usize, // PT
        ]
    }
    
    /// Get the page offset (lower 12 bits)
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

/// Page size constants
pub const PAGE_SIZE: u64 = 4096;
pub const LARGE_PAGE_SIZE: u64 = 2 * 1024 * 1024; // 2 MiB
pub const HUGE_PAGE_SIZE: u64 = 1024 * 1024 * 1024; // 1 GiB
