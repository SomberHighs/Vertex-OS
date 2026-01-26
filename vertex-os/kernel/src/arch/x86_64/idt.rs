//! Interrupt Descriptor Table (IDT) for x86_64

use core::mem::size_of;
use super::gdt::KERNEL_CODE_SELECTOR;
use super::interrupts;
use super::pic::PIC1_OFFSET;

/// IDT Entry (Gate Descriptor)
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
        self.type_attr = 0x8E; // Present, Ring 0, Interrupt Gate
        self.reserved = 0;
    }
}

/// IDT Pointer structure for lidt instruction
#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

/// Number of IDT entries (256 for x86_64)
const IDT_ENTRIES: usize = 256;

/// The Interrupt Descriptor Table
#[repr(C, align(16))]
struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

static mut IDT: Idt = Idt {
    entries: [IdtEntry::missing(); IDT_ENTRIES],
};

/// Initialize the IDT
pub fn init() {
    unsafe {
        // Set up exception handlers (vectors 0-31)
        IDT.entries[0].set_handler(interrupts::divide_error as u64);
        IDT.entries[1].set_handler(interrupts::debug as u64);
        IDT.entries[2].set_handler(interrupts::nmi as u64);
        IDT.entries[3].set_handler(interrupts::breakpoint as u64);
        IDT.entries[4].set_handler(interrupts::overflow as u64);
        IDT.entries[5].set_handler(interrupts::bound_range as u64);
        IDT.entries[6].set_handler(interrupts::invalid_opcode as u64);
        IDT.entries[7].set_handler(interrupts::device_not_available as u64);
        IDT.entries[8].set_handler(interrupts::double_fault as u64);
        IDT.entries[10].set_handler(interrupts::invalid_tss as u64);
        IDT.entries[11].set_handler(interrupts::segment_not_present as u64);
        IDT.entries[12].set_handler(interrupts::stack_segment as u64);
        IDT.entries[13].set_handler(interrupts::general_protection as u64);
        IDT.entries[14].set_handler(interrupts::page_fault as u64);
        IDT.entries[16].set_handler(interrupts::x87_floating_point as u64);
        IDT.entries[17].set_handler(interrupts::alignment_check as u64);
        IDT.entries[18].set_handler(interrupts::machine_check as u64);
        IDT.entries[19].set_handler(interrupts::simd_floating_point as u64);
        IDT.entries[20].set_handler(interrupts::virtualization as u64);
        
        // Hardware IRQ handlers (remapped to vectors 32-47)
        // IRQ0 = Timer (vector 32)
        IDT.entries[PIC1_OFFSET as usize].set_handler(interrupts::timer_interrupt as u64);
        // IRQ1 = Keyboard (vector 33)
        IDT.entries[(PIC1_OFFSET + 1) as usize].set_handler(interrupts::keyboard_interrupt as u64);
        
        let idt_ptr = IdtPointer {
            limit: (size_of::<Idt>() - 1) as u16,
            base: &IDT as *const _ as u64,
        };
        
        // Load the IDT
        core::arch::asm!(
            "lidt [{}]",
            in(reg) &idt_ptr,
            options(readonly, nostack, preserves_flags)
        );
    }
}
