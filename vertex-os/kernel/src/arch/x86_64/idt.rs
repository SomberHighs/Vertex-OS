//! Interrupt Descriptor Table.

use core::mem::size_of;

use super::gdt::KERNEL_CODE_SELECTOR;
use super::interrupts;
use super::pic::PIC1_OFFSET;
use crate::sync::SyncCell;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = KERNEL_CODE_SELECTOR;
        self.ist = 0;
        self.type_attr = 0x8E; // present, ring 0, 64-bit interrupt gate
        self.reserved = 0;
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

pub const IDT_ENTRIES: usize = 256;

#[repr(C, align(16))]
struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

static IDT: SyncCell<Idt> = SyncCell::new(Idt {
    entries: [IdtEntry::missing(); IDT_ENTRIES],
});

/// Install the IDT and load it.
pub fn init() {
    unsafe {
        let idt = IDT.get();

        // Every gate starts as a harmless `iretq` stub. A non-present gate
        // would escalate a stray interrupt into a #GP and then a double fault.
        for entry in idt.entries.iter_mut() {
            entry.set_handler(interrupts::spurious as *const () as u64);
        }

        // CPU exceptions.
        idt.entries[0].set_handler(interrupts::divide_error as *const () as u64);
        idt.entries[1].set_handler(interrupts::debug as *const () as u64);
        idt.entries[2].set_handler(interrupts::nmi as *const () as u64);
        idt.entries[3].set_handler(interrupts::breakpoint as *const () as u64);
        idt.entries[4].set_handler(interrupts::overflow as *const () as u64);
        idt.entries[5].set_handler(interrupts::bound_range as *const () as u64);
        idt.entries[6].set_handler(interrupts::invalid_opcode as *const () as u64);
        idt.entries[7].set_handler(interrupts::device_not_available as *const () as u64);
        idt.entries[8].set_handler(interrupts::double_fault as *const () as u64);
        idt.entries[10].set_handler(interrupts::invalid_tss as *const () as u64);
        idt.entries[11].set_handler(interrupts::segment_not_present as *const () as u64);
        idt.entries[12].set_handler(interrupts::stack_segment as *const () as u64);
        idt.entries[13].set_handler(interrupts::general_protection as *const () as u64);
        idt.entries[14].set_handler(interrupts::page_fault as *const () as u64);
        idt.entries[16].set_handler(interrupts::x87_floating_point as *const () as u64);
        idt.entries[17].set_handler(interrupts::alignment_check as *const () as u64);
        idt.entries[18].set_handler(interrupts::machine_check as *const () as u64);
        idt.entries[19].set_handler(interrupts::simd_floating_point as *const () as u64);
        idt.entries[20].set_handler(interrupts::virtualization as *const () as u64);

        // Hardware IRQs after the PIC remap.
        idt.entries[PIC1_OFFSET as usize].set_handler(interrupts::timer_interrupt as *const () as u64);
        idt.entries[(PIC1_OFFSET + 1) as usize].set_handler(interrupts::keyboard_interrupt as *const () as u64);

        let pointer = IdtPointer {
            limit: (size_of::<Idt>() - 1) as u16,
            base: idt as *const Idt as u64,
        };

        core::arch::asm!(
            "lidt [{}]",
            in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );
    }
}
