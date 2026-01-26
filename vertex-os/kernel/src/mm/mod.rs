//! Memory management subsystem

pub mod pmm;
pub mod vmm;
pub mod addr;

pub use addr::{PhysAddr, VirtAddr};

use limine::memory_map::Entry;

/// Initialize the memory management subsystem
pub fn init(memory_map: &[&Entry], hhdm_offset: u64) {
    pmm::init(memory_map, hhdm_offset);
    vmm::init(hhdm_offset);
}
