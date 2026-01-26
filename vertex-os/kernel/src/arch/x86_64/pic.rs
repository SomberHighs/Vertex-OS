//! 8259 Programmable Interrupt Controller (PIC)
//!
//! The PC has two PICs (master and slave) which need to be remapped
//! because their default IRQ mappings conflict with CPU exceptions.

use super::cpu::{outb, inb};

/// Master PIC command port
const PIC1_COMMAND: u16 = 0x20;
/// Master PIC data port
const PIC1_DATA: u16 = 0x21;
/// Slave PIC command port
const PIC2_COMMAND: u16 = 0xA0;
/// Slave PIC data port
const PIC2_DATA: u16 = 0xA1;

/// End of Interrupt command
const PIC_EOI: u8 = 0x20;

/// ICW1: Initialization command (ICW4 needed)
const ICW1_ICW4: u8 = 0x01;
/// ICW1: Initialization bit
const ICW1_INIT: u8 = 0x10;
/// ICW4: 8086 mode
const ICW4_8086: u8 = 0x01;

/// Offset for master PIC (IRQ 0-7 -> vectors 32-39)
pub const PIC1_OFFSET: u8 = 32;
/// Offset for slave PIC (IRQ 8-15 -> vectors 40-47)
pub const PIC2_OFFSET: u8 = 40;

/// IRQ numbers
pub const IRQ_TIMER: u8 = 0;
pub const IRQ_KEYBOARD: u8 = 1;

/// Small IO wait by writing to unused port
#[inline]
unsafe fn io_wait() {
    outb(0x80, 0);
}

/// Initialize the 8259 PIC
///
/// Remaps IRQs to vectors 32-47 to avoid conflicts with CPU exceptions (0-31)
pub fn init() {
    unsafe {
        // Save the current masks
        let mask1 = inb(PIC1_DATA);
        let mask2 = inb(PIC2_DATA);

        // Start initialization sequence (cascade mode)
        outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();

        // ICW2: Set vector offsets
        outb(PIC1_DATA, PIC1_OFFSET);
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET);
        io_wait();

        // ICW3: Tell Master PIC that there is a slave PIC at IRQ2
        outb(PIC1_DATA, 4); // IRQ2 has slave (bit 2)
        io_wait();
        // ICW3: Tell Slave PIC its cascade identity
        outb(PIC2_DATA, 2); // Slave ID is 2
        io_wait();

        // ICW4: Set 8086 mode
        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // Disable all IRQs initially - they will be enabled individually
        // Bit = 0 means enabled, bit = 1 means disabled
        outb(PIC1_DATA, 0xFF); // Disable all IRQs on master
        outb(PIC2_DATA, 0xFF); // Disable all IRQs on slave

        let _ = (mask1, mask2); // Suppress unused warning
    }
    
    // Enable keyboard IRQ by default
    enable_irq(IRQ_KEYBOARD);
}

/// Send End of Interrupt signal to PIC
///
/// Must be called at the end of every IRQ handler
pub fn send_eoi(irq: u8) {
    unsafe {
        // If IRQ came from slave PIC (IRQ 8-15), send EOI to both
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        // Always send EOI to master
        outb(PIC1_COMMAND, PIC_EOI);
    }
}

/// Enable a specific IRQ
pub fn enable_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA);
            outb(PIC1_DATA, mask & !(1 << irq));
        } else {
            let mask = inb(PIC2_DATA);
            outb(PIC2_DATA, mask & !(1 << (irq - 8)));
        }
    }
}

/// Disable a specific IRQ
pub fn disable_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA);
            outb(PIC1_DATA, mask | (1 << irq));
        } else {
            let mask = inb(PIC2_DATA);
            outb(PIC2_DATA, mask | (1 << (irq - 8)));
        }
    }
}
