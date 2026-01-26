//! CPU utilities for x86_64

/// Halt the CPU until the next interrupt
#[inline]
pub fn hlt() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

/// Disable interrupts
#[inline]
pub fn cli() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
}

/// Enable interrupts
#[inline]
pub fn sti() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}

/// Halt loop - disables interrupts and halts forever
#[inline]
pub fn halt_loop() -> ! {
    loop {
        cli();
        hlt();
    }
}

/// Read the CR2 register (page fault linear address)
#[inline]
pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Read the CR3 register (page table base address)
#[inline]
pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Write to the CR3 register (flush TLB)
#[inline]
pub unsafe fn write_cr3(value: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) value, options(nomem, nostack, preserves_flags));
}

/// Invalidate a TLB entry
#[inline]
pub fn invlpg(addr: u64) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags));
    }
}

/// Read from an I/O port
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

/// Write to an I/O port
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}
