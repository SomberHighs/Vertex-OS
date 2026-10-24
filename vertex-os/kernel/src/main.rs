//! Vertex OS kernel entry point.
//!
//! Boot sequence:
//!   1. serial + framebuffer console,
//!   2. memory map, HHDM and the physical allocator,
//!   3. GDT/IDT/PIC, the PIT and the keyboard,
//!   4. the node graph filesystem,
//!   5. the scheduler and the desktop task, which starts the Loom front end.

#![no_std]
#![no_main]
// Hardware and scheduler helpers form a complete API even where the shell does
// not call every entry point yet.
#![allow(dead_code)]

mod arch;
mod console;
mod crt;
mod gui;
mod mm;
mod panic;
mod sched;
mod shell;
mod sync;
mod vfs;

use limine::request::{
    BootloaderInfoRequest, FramebufferRequest, HhdmRequest, MemmapRequest, StackSizeRequest,
};
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};

use crate::arch::x86_64::cpu;
use crate::console::{FG_DIM, FG_ERROR, FG_OK};

// Limine request structures. The `requests` section is scanned by the
// bootloader, so these must stay `used` and contiguous.
#[used]
#[unsafe(link_section = ".requests_start")]
static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

// Limine 8.x/9.x implement base revisions 0-3; the `limine` crate defaults to
// the newer draft revision 6. Every request used here is a revision 0 request,
// so targeting revision 3 is both sufficient and portable across bootloaders.
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(3);

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new(64 * 1024);

#[used]
#[unsafe(link_section = ".requests")]
static BOOTLOADER_REQUEST: BootloaderInfoRequest = BootloaderInfoRequest::new();

#[used]
#[unsafe(link_section = ".requests_end")]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

/// Kernel entry point, called by Limine in long mode with interrupts off.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    arch::x86_64::serial::init();
    kprintln_color!(FG_DIM, "Vertex OS 0.2.0 booting");
    kprintln!("=======================");

    if !BASE_REVISION.is_supported() {
        kprintln_color!(
            FG_ERROR,
            "[FAIL] Limine base revision {:?} is not supported",
            BASE_REVISION.actual_revision()
        );
        cpu::halt_loop();
    }
    if let Some(response) = BOOTLOADER_REQUEST.response() {
        kprintln!("[ok] bootloader: {} {}", response.name(), response.version());
    }

    // --- console -----------------------------------------------------------
    match FRAMEBUFFER_REQUEST.response() {
        Some(response) => match response.framebuffers().first() {
            Some(framebuffer) => {
                if console::init(framebuffer) {
                    let (cols, rows) = console::size();
                    kprintln!(
                        "[ok] framebuffer: {}x{} @ {}bpp ({}x{} chars)",
                        framebuffer.width,
                        framebuffer.height,
                        framebuffer.bpp,
                        cols,
                        rows
                    );
                } else {
                    kprintln_color!(FG_ERROR, "[warn] unsupported framebuffer format");
                }
            }
            None => kprintln_color!(FG_ERROR, "[warn] no framebuffer available"),
        },
        None => kprintln_color!(FG_ERROR, "[warn] framebuffer request denied"),
    }

    // --- memory ------------------------------------------------------------
    let (Some(memory_map), Some(hhdm)) = (
        MEMORY_MAP_REQUEST.response(),
        HHDM_REQUEST.response(),
    ) else {
        kprintln_color!(FG_ERROR, "[FAIL] bootloader did not provide a memory map");
        cpu::halt_loop();
    };

    mm::init(memory_map.entries(), hhdm.offset);
    kprintln!(
        "[ok] memory: {} MiB total, {} MiB free, HHDM at {:#x}",
        mm::pmm::total_mib(),
        mm::pmm::free_mib(),
        hhdm.offset
    );
    kprintln!(
        "[ok] kernel image: {:#x}..{:#x}",
        mm::kernel_image_range().0,
        mm::kernel_image_range().1
    );

    // --- cpu ---------------------------------------------------------------
    arch::x86_64::gdt::init();
    arch::x86_64::idt::init();
    arch::x86_64::pic::init();
    arch::x86_64::keyboard::init();
    arch::x86_64::timer::init();
    kprintln!(
        "[ok] interrupts: GDT/IDT loaded, PIC remapped, PIT at {} Hz",
        arch::x86_64::timer::TIMER_FREQUENCY
    );

    // --- filesystem --------------------------------------------------------
    vfs::init();
    let mut root_hex = [0u8; 64];
    vfs::node::format_id(&vfs::node::root(), &mut root_hex);
    kprintln!(
        "[ok] node graph: {} nodes, root {}",
        vfs::node::node_count(),
        core::str::from_utf8(&root_hex).unwrap_or("?")
    );

    // --- scheduler ---------------------------------------------------------
    sched::init();
    match sched::spawn("shell", shell::run) {
        Some(id) => kprintln!("[ok] scheduler: shell task spawned (id {})", id),
        None => kprintln_color!(FG_ERROR, "[FAIL] could not spawn the shell task"),
    }

    kprintln_color!(FG_OK, "boot complete");
    kprintln!();

    arch::x86_64::timer::enable_scheduler();
    cpu::sti();
    sched::yield_cpu();

    // The idle task: wait for interrupts forever.
    loop {
        cpu::hlt();
    }
}
