//! Interrupt and exception entry stubs.
//!
//! Every stub is a `naked` function: it saves the full register set, aligns the
//! stack for the SysV ABI, calls a Rust handler with a pointer to the saved
//! frame, and returns with `iretq`. Aligning the stack matters because rustc
//! may emit SSE moves that fault on unaligned addresses.

use crate::kprintln;
use super::cpu;
use super::keyboard;
use super::timer;

/// The frame the CPU pushes on a same-privilege interrupt.
#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// General purpose registers, in the order they are pushed.
#[repr(C)]
pub struct SavedRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

#[repr(C)]
pub struct ExceptionContext {
    pub regs: SavedRegisters,
    pub frame: InterruptStackFrame,
}

#[repr(C)]
pub struct ExceptionContextWithError {
    pub regs: SavedRegisters,
    pub error_code: u64,
    pub frame: InterruptStackFrame,
}

impl InterruptStackFrame {
    fn dump(&self) {
        kprintln!("  RIP:   {:#018x}", self.instruction_pointer);
        kprintln!("  CS:    {:#06x}", self.code_segment);
        kprintln!("  FLAGS: {:#018x}", self.cpu_flags);
        kprintln!("  RSP:   {:#018x}", self.stack_pointer);
        kprintln!("  SS:    {:#06x}", self.stack_segment);
    }
}

impl SavedRegisters {
    fn dump(&self) {
        kprintln!("  RAX: {:#018x}  RBX: {:#018x}", self.rax, self.rbx);
        kprintln!("  RCX: {:#018x}  RDX: {:#018x}", self.rcx, self.rdx);
        kprintln!("  RSI: {:#018x}  RDI: {:#018x}", self.rsi, self.rdi);
        kprintln!("  RBP: {:#018x}  R8:  {:#018x}", self.rbp, self.r8);
        kprintln!("  R9:  {:#018x}  R10: {:#018x}", self.r9, self.r10);
        kprintln!("  R11: {:#018x}  R12: {:#018x}", self.r11, self.r12);
        kprintln!("  R13: {:#018x}  R14: {:#018x}", self.r13, self.r14);
        kprintln!("  R15: {:#018x}", self.r15);
    }
}

pub mod page_fault_error {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const RESERVED: u64 = 1 << 3;
    pub const INSTRUCTION: u64 = 1 << 4;
}

fn decode_page_fault(error: u64) -> &'static str {
    match (
        error & page_fault_error::PRESENT != 0,
        error & page_fault_error::WRITE != 0,
        error & page_fault_error::USER != 0,
        error & page_fault_error::INSTRUCTION != 0,
    ) {
        (false, false, false, false) => "kernel read of a non-present page",
        (false, false, false, true) => "kernel instruction fetch from a non-present page",
        (false, true, false, false) => "kernel write to a non-present page",
        (false, false, true, false) => "user read of a non-present page",
        (false, false, true, true) => "user instruction fetch from a non-present page",
        (false, true, true, false) => "user write to a non-present page",
        (true, false, false, false) => "kernel read protection violation",
        (true, false, false, true) => "kernel instruction fetch protection violation",
        (true, true, false, false) => "kernel write protection violation",
        (true, false, true, false) => "user read protection violation",
        (true, false, true, true) => "user instruction fetch protection violation",
        (true, true, true, false) => "user write protection violation",
        _ => "unclassified page fault",
    }
}

/// Common report path for fatal (and recoverable) exceptions.
fn report(
    title: &str,
    error: Option<u64>,
    regs: &SavedRegisters,
    frame: &InterruptStackFrame,
    fatal: bool,
) {
    // The fault may have happened while the console lock was held.
    crate::console::force_unlock();

    kprintln!();
    kprintln!("=================== EXCEPTION ===================");
    kprintln!("  {}", title);
    if let Some(code) = error {
        kprintln!("  error code: {:#018x}", code);
    }
    kprintln!("=================================================");
    kprintln!("  frame:");
    frame.dump();
    kprintln!("  registers:");
    regs.dump();
    kprintln!();

    if fatal {
        kprintln!("FATAL: this exception cannot be recovered. Halting.");
        cpu::halt_loop();
    }
    kprintln!("(recoverable exception ignored)");
}

/// Exception without an error code.
macro_rules! exception_no_error {
    ($stub:ident, $inner:ident, $title:literal, fatal: $fatal:expr) => {
        #[unsafe(naked)]
        pub extern "C" fn $stub() -> ! {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                // Align the stack to 16 bytes for the Rust handler.
                "mov rbp, rsp",
                "and rsp, -16",
                "mov rdi, rbp",
                "call {handler}",
                "mov rsp, rbp",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "iretq",
                handler = sym $inner
            )
        }

        extern "C" fn $inner(ctx: &ExceptionContext) {
            report($title, None, &ctx.regs, &ctx.frame, $fatal);
        }
    };
}

/// Exception whose frame carries an error code.
macro_rules! exception_with_error {
    ($stub:ident, $inner:ident, $title:literal, fatal: $fatal:expr) => {
        #[unsafe(naked)]
        pub extern "C" fn $stub() -> ! {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "mov rbp, rsp",
                "and rsp, -16",
                "mov rdi, rbp",
                "call {handler}",
                "mov rsp, rbp",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "add rsp, 8", // discard the CPU error code
                "iretq",
                handler = sym $inner
            )
        }

        extern "C" fn $inner(ctx: &ExceptionContextWithError) {
            report(
                $title,
                Some(ctx.error_code),
                &ctx.regs,
                &ctx.frame,
                $fatal,
            );
        }
    };
}

/// IRQ entry stub (no error code, EOI is sent by the handler).
///
/// `$body` must already exist as an `extern "C" fn()` in this module.
macro_rules! irq_stub {
    ($stub:ident, $body:ident) => {
        #[unsafe(naked)]
        pub extern "C" fn $stub() -> ! {
            core::arch::naked_asm!(
                "push rax",
                "push rbx",
                "push rcx",
                "push rdx",
                "push rsi",
                "push rdi",
                "push rbp",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                "mov rbp, rsp",
                "and rsp, -16",
                "call {handler}",
                "mov rsp, rbp",
                "pop r15",
                "pop r14",
                "pop r13",
                "pop r12",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rbp",
                "pop rdi",
                "pop rsi",
                "pop rdx",
                "pop rcx",
                "pop rbx",
                "pop rax",
                "iretq",
                handler = sym $body
            )
        }
    };
}

exception_no_error!(divide_error, divide_error_inner, "Divide error (#DE)", fatal: true);
exception_no_error!(debug, debug_inner, "Debug (#DB)", fatal: false);
exception_no_error!(nmi, nmi_inner, "Non-maskable interrupt", fatal: false);
exception_no_error!(breakpoint, breakpoint_inner, "Breakpoint (#BP)", fatal: false);
exception_no_error!(overflow, overflow_inner, "Overflow (#OF)", fatal: false);
exception_no_error!(bound_range, bound_range_inner, "Bound range exceeded (#BR)", fatal: true);
exception_no_error!(invalid_opcode, invalid_opcode_inner, "Invalid opcode (#UD)", fatal: true);
exception_no_error!(device_not_available, device_not_available_inner, "Device not available (#NM)", fatal: true);
exception_no_error!(x87_floating_point, x87_floating_point_inner, "x87 floating point (#MF)", fatal: true);
exception_no_error!(simd_floating_point, simd_floating_point_inner, "SIMD floating point (#XM)", fatal: true);
exception_no_error!(virtualization, virtualization_inner, "Virtualization exception (#VE)", fatal: true);
exception_no_error!(machine_check, machine_check_inner, "Machine check (#MC) - hardware error", fatal: true);

exception_with_error!(double_fault, double_fault_inner, "Double fault (#DF)", fatal: true);
exception_with_error!(invalid_tss, invalid_tss_inner, "Invalid TSS (#TS)", fatal: true);
exception_with_error!(segment_not_present, segment_not_present_inner, "Segment not present (#NP)", fatal: true);
exception_with_error!(stack_segment, stack_segment_inner, "Stack segment fault (#SS)", fatal: true);
exception_with_error!(alignment_check, alignment_check_inner, "Alignment check (#AC)", fatal: true);
exception_with_error!(general_protection, general_protection_inner, "General protection fault (#GP)", fatal: true);

/// Page fault entry, with CR2 decoding.
#[unsafe(naked)]
pub extern "C" fn page_fault() -> ! {
    core::arch::naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rbp, rsp",
        "and rsp, -16",
        "mov rdi, rbp",
        "call {handler}",
        "mov rsp, rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "add rsp, 8",
        "iretq",
        handler = sym page_fault_inner
            )
}

extern "C" fn page_fault_inner(ctx: &ExceptionContextWithError) {
    use page_fault_error::*;

    crate::console::force_unlock();

    let address = cpu::read_cr2();
    let error = ctx.error_code;

    kprintln!();
    kprintln!("================== PAGE FAULT ==================");
    kprintln!("  faulting address: {:#018x}", address);
    kprintln!("  cause:            {}", decode_page_fault(error));
    kprintln!("  error code:       {:#018x}", error);
    kprintln!(
        "  [present={} write={} user={} reserved={} fetch={}]",
        error & PRESENT != 0,
        error & WRITE != 0,
        error & USER != 0,
        error & RESERVED != 0,
        error & INSTRUCTION != 0
    );
    kprintln!("===============================================");
    kprintln!("  frame:");
    ctx.frame.dump();
    kprintln!("  registers:");
    ctx.regs.dump();
    kprintln!();
    kprintln!("FATAL: unhandled page fault. Halting.");
    cpu::halt_loop();
}

/// Installed on every vector we do not handle explicitly.
#[unsafe(naked)]
pub extern "C" fn spurious() -> ! {
    core::arch::naked_asm!("iretq")
}

irq_stub!(timer_interrupt, timer_interrupt_body);
irq_stub!(keyboard_interrupt, keyboard_interrupt_body);

extern "C" fn timer_interrupt_body() {
    timer::handle_interrupt();
}

extern "C" fn keyboard_interrupt_body() {
    keyboard::handle_interrupt();
}
