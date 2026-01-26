//! Global Descriptor Table (GDT) for x86_64
//!
//! In 64-bit long mode, segmentation is mostly disabled, but we still need
//! a minimal GDT with null, kernel code, and kernel data segments.

use core::mem::size_of;

/// GDT Entry (Segment Descriptor)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_middle: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_middle: 0,
            access: 0,
            granularity: 0,
            base_high: 0,
        }
    }
    
    /// Create a kernel code segment (64-bit)
    const fn kernel_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x9A,       // Present, Ring 0, Code, Executable, Readable
            granularity: 0xAF, // 4K granularity, 64-bit
            base_high: 0,
        }
    }
    
    /// Create a kernel data segment
    const fn kernel_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x92,       // Present, Ring 0, Data, Writable
            granularity: 0xCF, // 4K granularity, 32-bit (ignored in 64-bit mode)
            base_high: 0,
        }
    }
}

/// GDT Pointer structure for lgdt instruction
#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

/// The Global Descriptor Table
#[repr(C, align(16))]
struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
}

static mut GDT: Gdt = Gdt {
    null: GdtEntry::null(),
    kernel_code: GdtEntry::kernel_code(),
    kernel_data: GdtEntry::kernel_data(),
};

/// Kernel code segment selector (index 1, Ring 0)
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;

/// Kernel data segment selector (index 2, Ring 0)
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;

/// Initialize the GDT
pub fn init() {
    unsafe {
        let gdt_ptr = GdtPointer {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: &GDT as *const _ as u64,
        };
        
        // Load the GDT
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &gdt_ptr,
            options(readonly, nostack, preserves_flags)
        );
        
        // Reload segment registers
        // We need to do a far jump to reload CS
        core::arch::asm!(
            "push {sel}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) KERNEL_CODE_SELECTOR as u64,
            tmp = lateout(reg) _,
            options(preserves_flags)
        );
        
        // Reload data segment registers
        core::arch::asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            "mov ss, {0:x}",
            in(reg) KERNEL_DATA_SELECTOR,
            options(nostack, preserves_flags)
        );
    }
}
