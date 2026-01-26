//! Programmable Interval Timer (PIT) driver
//!
//! The 8253/8254 PIT provides a hardware timer that generates IRQ0.
//! We use it for timekeeping and preemptive scheduling.

use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use super::cpu::outb;
use super::pic;

/// PIT I/O ports
const PIT_CHANNEL0_DATA: u16 = 0x40;  // Channel 0 data port
const PIT_CHANNEL1_DATA: u16 = 0x41;  // Channel 1 data port (unused)
const PIT_CHANNEL2_DATA: u16 = 0x42;  // Channel 2 data port (PC speaker)
const PIT_COMMAND: u16 = 0x43;         // Mode/Command register

/// PIT base frequency in Hz (approximately 1.193182 MHz)
const PIT_BASE_FREQUENCY: u32 = 1193182;

/// Desired timer frequency in Hz (100 Hz = 10ms ticks)
pub const TIMER_FREQUENCY: u32 = 100;

/// Divisor to achieve desired frequency
const DIVISOR: u16 = (PIT_BASE_FREQUENCY / TIMER_FREQUENCY) as u16;

/// Number of ticks since boot
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Whether the scheduler is enabled
static SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

/// Number of milliseconds per tick
pub const MS_PER_TICK: u64 = 1000 / TIMER_FREQUENCY as u64;

/// PIT command byte bits
mod command {
    /// Channel 0 select
    pub const CHANNEL0: u8 = 0b00_00_0000;
    /// Access mode: lobyte/hibyte
    pub const ACCESS_LOHI: u8 = 0b00_11_0000;
    /// Operating mode: rate generator (mode 2)
    pub const MODE_RATE_GENERATOR: u8 = 0b0000_010_0;
    /// Operating mode: square wave (mode 3)
    pub const MODE_SQUARE_WAVE: u8 = 0b0000_011_0;
    /// Binary counting mode (not BCD)
    pub const BINARY_MODE: u8 = 0b0000_000_0;
}

/// Initialize the PIT
///
/// Configures channel 0 to generate interrupts at TIMER_FREQUENCY Hz
pub fn init() {
    unsafe {
        // Configure the PIT:
        // - Channel 0 (connected to IRQ0)
        // - Access mode: lobyte/hibyte (send divisor as two bytes)
        // - Mode 2: Rate generator (counts down and reloads)
        // - Binary counting mode
        let cmd = command::CHANNEL0 
                | command::ACCESS_LOHI 
                | command::MODE_RATE_GENERATOR 
                | command::BINARY_MODE;
        
        outb(PIT_COMMAND, cmd);
        
        // Send divisor (low byte first, then high byte)
        outb(PIT_CHANNEL0_DATA, (DIVISOR & 0xFF) as u8);
        outb(PIT_CHANNEL0_DATA, ((DIVISOR >> 8) & 0xFF) as u8);
    }
    
    // Enable IRQ0 (timer) in the PIC
    pic::enable_irq(pic::IRQ_TIMER);
}

/// Get the current tick count since boot
#[inline]
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Get uptime in milliseconds
#[inline]
pub fn uptime_ms() -> u64 {
    get_ticks() * MS_PER_TICK
}

/// Get uptime in seconds
#[inline]
pub fn uptime_secs() -> u64 {
    uptime_ms() / 1000
}

/// Simple busy-wait sleep for the specified number of milliseconds
/// Note: This is a blocking sleep and should be avoided in interrupt handlers
pub fn sleep_ms(ms: u64) {
    let target = get_ticks() + (ms / MS_PER_TICK) + 1;
    while get_ticks() < target {
        super::cpu::hlt();
    }
}

/// Enable scheduler integration with timer
pub fn enable_scheduler() {
    SCHEDULER_ENABLED.store(true, Ordering::Release);
}

/// Disable scheduler integration with timer
pub fn disable_scheduler() {
    SCHEDULER_ENABLED.store(false, Ordering::Release);
}

/// Timer interrupt handler - called from IRQ0 handler
pub fn handle_interrupt() {
    // Increment tick counter
    TICKS.fetch_add(1, Ordering::Relaxed);
    
    // Send End of Interrupt to PIC (must be done before scheduling)
    pic::send_eoi(pic::IRQ_TIMER);
    
    // Notify the scheduler (for preemption)
    if SCHEDULER_ENABLED.load(Ordering::Acquire) {
        crate::sched::timer_tick();
    }
}

/// Format uptime as a human-readable string (returns hours, minutes, seconds)
pub fn format_uptime() -> (u64, u64, u64) {
    let total_secs = uptime_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    (hours, minutes, seconds)
}
