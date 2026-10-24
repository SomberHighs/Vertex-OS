//! Global Descriptor Table.
//!
//! Long mode barely uses segmentation, but we still need a valid GDT with a
//! 64-bit code and data descriptor to reload the segment registers after
//! taking over from the bootloader.

use core::mem::size_of;

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

    /// Ring 0, 64-bit code: present, executable, readable.
    const fn kernel_code() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x9A,
            granularity: 0xAF, // 4 KiB granularity, long mode
            base_high: 0,
        }
    }

    /// Ring 0, writable data.
    const fn kernel_data() -> Self {
        Self {
            limit_low: 0xFFFF,
            base_low: 0,
            base_middle: 0,
            access: 0x92,
            granularity: 0xCF,
            base_high: 0,
        }
    }
}

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

#[repr(C, align(16))]
struct Gdt {
    null: GdtEntry,
    kernel_code: GdtEntry,
    kernel_data: GdtEntry,
}

#[used]
#[unsafe(link_section = ".data")]
static GDT: Gdt = Gdt {
    null: GdtEntry::null(),
    kernel_code: GdtEntry::kernel_code(),
    kernel_data: GdtEntry::kernel_data(),
};

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;

extern "C" {
    /// Far-return trampoline that reloads CS (defined below).
    fn vertex_reload_cs(selector: u64);
    /// Reload DS/ES/FS/GS/SS from one selector.
    fn vertex_load_data_segments(selector: u64);
}

core::arch::global_asm!(
    r#"
    .section .text
    .global vertex_reload_cs
vertex_reload_cs:
    # rdi = new CS selector. A far return is the only way to reload CS in
    # long mode: push [selector, target] and retfq into it.
    push rdi
    lea rax, [rip + 1f]
    push rax
    retfq
1:
    ret

    .global vertex_load_data_segments
vertex_load_data_segments:
    # rdi = selector
    mov eax, edi
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    ret
    "#
);

/// Load the Vertex GDT and reload every segment register.
pub fn init() {
    let pointer = GdtPointer {
        limit: (size_of::<Gdt>() - 1) as u16,
        base: &GDT as *const Gdt as u64,
    };

    unsafe {
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );

        vertex_load_data_segments(KERNEL_DATA_SELECTOR as u64);
        vertex_reload_cs(KERNEL_CODE_SELECTOR as u64);
    }
}
