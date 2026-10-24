//! Kernel panic handler.

use core::panic::PanicInfo;

use crate::arch::x86_64::cpu;
use crate::console::FG_ERROR;
use crate::{kprintln, kprintln_color};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cpu::cli();
    // The fault may have happened while the console lock was held.
    crate::console::force_unlock();
    crate::console::force_unlock_capture();

    kprintln_color!(FG_ERROR, "\n!!! KERNEL PANIC !!!");
    if let Some(location) = info.location() {
        kprintln_color!(
            FG_ERROR,
            "  at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        );
    }
    kprintln_color!(FG_ERROR, "  {}", info.message());
    kprintln!();
    kprintln!("System halted. Attach a debugger or reboot.");

    cpu::halt_loop();
}
