//! Panic handler for the kernel

use core::panic::PanicInfo;
use crate::arch::x86_64::cpu;
use crate::kprintln;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Disable interrupts
    cpu::cli();
    
    kprintln!();
    kprintln!("!!! KERNEL PANIC !!!");
    kprintln!();
    
    if let Some(location) = info.location() {
        kprintln!("Location: {}:{}:{}", 
            location.file(), 
            location.line(), 
            location.column()
        );
    }
    
    if let Some(message) = info.message() {
        kprintln!("Message: {}", message);
    }
    
    kprintln!();
    kprintln!("System halted.");
    
    cpu::halt_loop();
}
