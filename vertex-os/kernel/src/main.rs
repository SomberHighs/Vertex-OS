//! Vertex OS Kernel Entry Point

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(naked_functions)]

mod panic;
mod arch;
mod mm;
mod console;
mod sync;
mod sched;

use limine::request::{FramebufferRequest, MemoryMapRequest, HhdmRequest};

/// Request a framebuffer from Limine
#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

/// Request the memory map from Limine
#[used]
#[link_section = ".requests"]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

/// Request the Higher Half Direct Map offset
#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

/// Kernel entry point called by Limine bootloader
#[no_mangle]
unsafe extern "C" fn _start() -> ! {
    // Initialize the Global Descriptor Table
    arch::x86_64::gdt::init();
    
    // Initialize the Interrupt Descriptor Table
    arch::x86_64::idt::init();
    
    // Initialize the Programmable Interrupt Controller
    arch::x86_64::pic::init();
    
    // Initialize the timer (100 Hz)
    arch::x86_64::timer::init();
    
    // Initialize the framebuffer console if available
    if let Some(response) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(framebuffer) = response.framebuffers().next() {
            console::init(framebuffer);
            kprintln!("Vertex OS v0.1.0");
            kprintln!("================");
            kprintln!();
        }
    }
    
    // Initialize memory management
    if let (Some(mmap_response), Some(hhdm_response)) = (
        MEMORY_MAP_REQUEST.get_response(),
        HHDM_REQUEST.get_response()
    ) {
        let hhdm_offset = hhdm_response.offset();
        mm::init(mmap_response.entries(), hhdm_offset);
        kprintln!("[OK] Memory manager initialized");
    } else {
        kprintln!("[FAIL] Could not get memory map from bootloader");
    }
    
    kprintln!("[OK] Timer initialized ({} Hz)", arch::x86_64::timer::TIMER_FREQUENCY);
    
    // Initialize the scheduler
    sched::init();
    
    // Spawn demo tasks
    sched::spawn("task_a", demo_task_a);
    sched::spawn("task_b", demo_task_b);
    
    kprintln!();
    kprintln!("Kernel initialization complete.");
    kprintln!("Keyboard input enabled. Type something!");
    kprintln!();
    
    // Enable preemptive scheduling
    arch::x86_64::timer::enable_scheduler();
    
    // Enable interrupts and enter the main loop
    arch::x86_64::cpu::sti();
    
    // The idle task runs here - it will be preempted by other tasks
    loop {
        arch::x86_64::cpu::hlt();
    }
}

/// Demo task A - prints periodically
fn demo_task_a() -> ! {
    let mut counter = 0u64;
    loop {
        counter += 1;
        if counter % 100 == 0 {
            kprintln!("[Task A] Counter: {}", counter);
        }
        // Yield to other tasks
        sched::yield_cpu();
    }
}

/// Demo task B - prints periodically
fn demo_task_b() -> ! {
    let mut counter = 0u64;
    loop {
        counter += 1;
        if counter % 100 == 0 {
            kprintln!("[Task B] Counter: {}", counter);
        }
        // Yield to other tasks
        sched::yield_cpu();
    }
}
