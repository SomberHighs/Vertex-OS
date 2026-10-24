//! CPU primitives for x86_64.

use core::arch::asm;

/// Halt the CPU until the next interrupt arrives.
#[inline]
pub fn hlt() {
    unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

/// Disable maskable interrupts.
#[inline]
pub fn cli() {
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
}

/// Enable maskable interrupts.
#[inline]
pub fn sti() {
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

/// Are maskable interrupts currently enabled?
#[inline]
pub fn interrupts_enabled() -> bool {
    let flags: u64;
    unsafe { asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags)) };
    flags & (1 << 9) != 0
}

/// Disable interrupts and report whether they were enabled before.
///
/// Pair with [`restore_interrupts`]; used to build interrupt-safe critical
/// sections without changing the state a caller relies on.
#[inline]
pub fn disable_interrupts() -> bool {
    let were_enabled = interrupts_enabled();
    cli();
    were_enabled
}

/// Restore the interrupt flag saved by [`disable_interrupts`].
#[inline]
pub fn restore_interrupts(was_enabled: bool) {
    if was_enabled {
        sti();
    }
}

/// Run a closure with interrupts disabled, restoring the previous state.
///
/// This is the primitive every shared kernel structure is guarded by: on a
/// single CPU it is both cheaper and safer than a spinlock, because an
/// interrupt handler can never observe a half-updated structure.
#[inline]
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let was_enabled = disable_interrupts();
    let result = f();
    restore_interrupts(was_enabled);
    result
}

/// Read CR2, the faulting linear address of the last page fault.
#[inline]
pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe { asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags)) };
    value
}

/// Read CR3, the physical address of the active PML4.
#[inline]
pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe { asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags)) };
    value
}

/// Write CR3, which also flushes the TLB.
#[inline]
pub unsafe fn write_cr3(value: u64) {
    asm!("mov cr3, {}", in(reg) value, options(nomem, nostack, preserves_flags));
}

/// Read CR0.
#[inline]
pub fn read_cr0() -> u64 {
    let value: u64;
    unsafe { asm!("mov {}, cr0", out(reg) value, options(nomem, nostack, preserves_flags)) };
    value
}

/// Write CR0.
#[inline]
pub unsafe fn write_cr0(value: u64) {
    asm!("mov cr0, {}", in(reg) value, options(nomem, nostack, preserves_flags));
}

/// Invalidate a single TLB entry.
#[inline]
pub fn invlpg(addr: u64) {
    unsafe { asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags)) };
}

/// Read a byte from an I/O port.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

/// Write a byte to an I/O port.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

/// Small delay by writing to the unused POST diagnostic port.
#[inline]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}

/// Halt forever with interrupts disabled.
#[inline]
pub fn halt_loop() -> ! {
    loop {
        cli();
        hlt();
    }
}

/// Reset the machine through the 8042 keyboard controller.
pub fn reboot() -> ! {
    unsafe {
        // Drain the controller, then pulse the reset line.
        while inb(0x64) & 0x02 != 0 {
            core::hint::spin_loop();
        }
        outb(0x64, 0xFE);
    }
    // If the controller ignored us, fall back to a triple fault.
    loop {
        cli();
        hlt();
    }
}

/// Shut the machine down (QEMU/Bochs ACPI-less trick, then halt).
pub fn shutdown() -> ! {
    unsafe {
        outb(0x604, 0x00); // QEMU q35 ACPI shutdown
        outb(0x604, 0x20);
        outb(0xB004, 0x00); // older QEMU / Bochs
        outb(0xB004, 0x20);
    }
    loop {
        cli();
        hlt();
    }
}
