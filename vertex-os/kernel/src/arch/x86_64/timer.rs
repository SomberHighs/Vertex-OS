//! Programmable Interval Timer (8253/8254) channel 0.
//!
//! Generates IRQ0 at [`TIMER_FREQUENCY`] Hz. The handler updates the tick
//! counter and drives scheduler preemption; it never prints, because the
//! console is not re-entrant from interrupt context.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::cpu::outb;
use super::pic;

const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

/// Timer frequency in Hz.
pub const TIMER_FREQUENCY: u32 = 100;

/// Milliseconds per tick.
pub const MS_PER_TICK: u64 = 1000 / TIMER_FREQUENCY as u64;

static TICKS: AtomicU64 = AtomicU64::new(0);
static SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

mod command {
    pub const CHANNEL0: u8 = 0b00_00_0000;
    pub const ACCESS_LOHI: u8 = 0b00_11_0000;
    pub const MODE_RATE_GENERATOR: u8 = 0b0000_010_0;
    pub const BINARY_MODE: u8 = 0b0000_000_0;
}

/// Program channel 0 and unmask IRQ0.
pub fn init() {
    let divisor = (PIT_BASE_FREQUENCY / TIMER_FREQUENCY) as u16;

    unsafe {
        outb(
            PIT_COMMAND,
            command::CHANNEL0
                | command::ACCESS_LOHI
                | command::MODE_RATE_GENERATOR
                | command::BINARY_MODE,
        );
        outb(PIT_CHANNEL0, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL0, ((divisor >> 8) & 0xFF) as u8);
    }

    pic::enable_irq(pic::IRQ_TIMER);
}

/// Ticks since boot.
#[inline]
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Uptime in milliseconds.
#[inline]
pub fn uptime_ms() -> u64 {
    ticks() * MS_PER_TICK
}

/// Uptime in seconds.
#[inline]
pub fn uptime_secs() -> u64 {
    uptime_ms() / 1000
}

/// Allow the timer to preempt the running task.
pub fn enable_scheduler() {
    SCHEDULER_ENABLED.store(true, Ordering::Release);
}

/// Stop preemption (the tick counter keeps running).
pub fn disable_scheduler() {
    SCHEDULER_ENABLED.store(false, Ordering::Release);
}

/// Busy-wait for `ms` milliseconds. Requires interrupts to be enabled.
pub fn sleep_ms(ms: u64) {
    let target = ticks() + ms / MS_PER_TICK + 1;
    while ticks() < target {
        super::cpu::hlt();
    }
}

/// IRQ0 handler.
pub fn handle_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    pic::send_eoi(pic::IRQ_TIMER);

    if SCHEDULER_ENABLED.load(Ordering::Acquire) {
        crate::sched::timer_tick();
    }
}
