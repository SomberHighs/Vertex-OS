//! Interrupt handlers for x86_64

use crate::kprintln;
use super::cpu;
use super::keyboard;
use super::timer;

/// Interrupt stack frame pushed by the CPU (without error code)
/// This matches what the CPU pushes on the stack when an interrupt occurs
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

/// Saved register state during interrupt
#[repr(C)]
#[derive(Debug, Clone, Copy)]
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

/// Combined exception context for handlers without error code
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExceptionContext {
    pub regs: SavedRegisters,
    pub frame: InterruptStackFrame,
}

/// Combined exception context for handlers with error code
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExceptionContextWithError {
    pub regs: SavedRegisters,
    pub error_code: u64,
    pub frame: InterruptStackFrame,
}

impl InterruptStackFrame {
    /// Print the interrupt stack frame
    pub fn dump(&self) {
        kprintln!("  RIP: {:#018x}", self.instruction_pointer);
        kprintln!("  CS:  {:#06x}", self.code_segment);
        kprintln!("  FLAGS: {:#018x}", self.cpu_flags);
        kprintln!("  RSP: {:#018x}", self.stack_pointer);
        kprintln!("  SS:  {:#06x}", self.stack_segment);
    }
}

impl SavedRegisters {
    /// Print all saved registers
    pub fn dump(&self) {
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

/// Page fault error code flags
pub mod page_fault_error {
    pub const PRESENT: u64 = 1 << 0;      // Page was present
    pub const WRITE: u64 = 1 << 1;        // Write access caused fault
    pub const USER: u64 = 1 << 2;         // Fault occurred in user mode
    pub const RESERVED: u64 = 1 << 3;     // Reserved bit set in page table
    pub const INSTRUCTION: u64 = 1 << 4;  // Instruction fetch caused fault
}

/// Decode page fault error code into human readable string
fn decode_page_fault_error(error: u64) -> &'static str {
    use page_fault_error::*;
    
    let present = error & PRESENT != 0;
    let write = error & WRITE != 0;
    let user = error & USER != 0;
    let instruction = error & INSTRUCTION != 0;
    
    match (present, write, user, instruction) {
        (false, false, false, false) => "kernel read non-present page",
        (false, false, false, true) => "kernel fetch non-present page",
        (false, true, false, false) => "kernel write non-present page",
        (false, false, true, false) => "user read non-present page",
        (false, false, true, true) => "user fetch non-present page",
        (false, true, true, false) => "user write non-present page",
        (true, false, false, false) => "kernel read protection violation",
        (true, false, false, true) => "kernel fetch protection violation",
        (true, true, false, false) => "kernel write protection violation",
        (true, false, true, false) => "user read protection violation",
        (true, false, true, true) => "user fetch protection violation",
        (true, true, true, false) => "user write protection violation",
        _ => "unknown page fault",
    }
}

// =============================================================================
// Exception handlers without error code
// =============================================================================

macro_rules! exception_handler {
    ($name:ident, $msg:expr, recoverable: $recoverable:expr) => {
        #[naked]
        pub extern "C" fn $name() {
            unsafe {
                core::arch::asm!(
                    // Save all general purpose registers
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
                    // Pass pointer to ExceptionContext as first argument
                    "mov rdi, rsp",
                    "call {}",
                    // Restore registers
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
                    sym $name##_inner,
                    options(noreturn)
                )
            }
        }
        
        extern "C" fn $name##_inner(ctx: *const ExceptionContext) {
            let ctx = unsafe { &*ctx };
            
            kprintln!();
            kprintln!("==================== EXCEPTION ====================");
            kprintln!("  {}", $msg);
            kprintln!("===================================================");
            kprintln!();
            kprintln!("Stack Frame:");
            ctx.frame.dump();
            kprintln!();
            kprintln!("Registers:");
            ctx.regs.dump();
            kprintln!();
            
            if !$recoverable {
                kprintln!("FATAL: Cannot recover from this exception. Halting.");
                cpu::halt_loop();
            } else {
                kprintln!("Attempting to continue...");
            }
        }
    };
}

// =============================================================================
// Exception handlers with error code
// =============================================================================

macro_rules! exception_handler_with_error {
    ($name:ident, $msg:expr, recoverable: $recoverable:expr) => {
        #[naked]
        pub extern "C" fn $name() {
            unsafe {
                core::arch::asm!(
                    // Save all general purpose registers
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
                    // Pass pointer to ExceptionContextWithError as first argument
                    "mov rdi, rsp",
                    "call {}",
                    // Restore registers
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
                    "add rsp, 8", // Pop error code
                    "iretq",
                    sym $name##_inner,
                    options(noreturn)
                )
            }
        }
        
        extern "C" fn $name##_inner(ctx: *const ExceptionContextWithError) {
            let ctx = unsafe { &*ctx };
            
            kprintln!();
            kprintln!("==================== EXCEPTION ====================");
            kprintln!("  {}", $msg);
            kprintln!("  Error Code: {:#018x}", ctx.error_code);
            kprintln!("===================================================");
            kprintln!();
            kprintln!("Stack Frame:");
            ctx.frame.dump();
            kprintln!();
            kprintln!("Registers:");
            ctx.regs.dump();
            kprintln!();
            
            if !$recoverable {
                kprintln!("FATAL: Cannot recover from this exception. Halting.");
                cpu::halt_loop();
            } else {
                kprintln!("Attempting to continue...");
            }
        }
    };
}

// =============================================================================
// CPU Exception Handlers
// =============================================================================

// Recoverable exceptions (can potentially continue execution)
exception_handler!(divide_error, "Division by Zero (#DE)", recoverable: false);
exception_handler!(debug, "Debug (#DB)", recoverable: true);
exception_handler!(breakpoint, "Breakpoint (#BP)", recoverable: true);
exception_handler!(overflow, "Overflow (#OF)", recoverable: true);
exception_handler!(bound_range, "Bound Range Exceeded (#BR)", recoverable: false);
exception_handler!(invalid_opcode, "Invalid Opcode (#UD)", recoverable: false);
exception_handler!(device_not_available, "Device Not Available (#NM)", recoverable: false);
exception_handler!(x87_floating_point, "x87 Floating-Point Exception (#MF)", recoverable: false);
exception_handler!(simd_floating_point, "SIMD Floating-Point Exception (#XM)", recoverable: false);
exception_handler!(virtualization, "Virtualization Exception (#VE)", recoverable: false);

// Non-Maskable Interrupt - special handling
exception_handler!(nmi, "Non-Maskable Interrupt (NMI)", recoverable: true);

// Fatal exceptions with error codes
exception_handler_with_error!(double_fault, "Double Fault (#DF)", recoverable: false);
exception_handler_with_error!(invalid_tss, "Invalid TSS (#TS)", recoverable: false);
exception_handler_with_error!(segment_not_present, "Segment Not Present (#NP)", recoverable: false);
exception_handler_with_error!(stack_segment, "Stack-Segment Fault (#SS)", recoverable: false);
exception_handler_with_error!(alignment_check, "Alignment Check (#AC)", recoverable: false);

// Machine check - always fatal
exception_handler!(machine_check, "Machine Check (#MC) - HARDWARE ERROR", recoverable: false);

// =============================================================================
// Special Exception Handlers (with custom logic)
// =============================================================================

/// General Protection Fault handler - provides detailed error information
#[naked]
pub extern "C" fn general_protection() {
    unsafe {
        core::arch::asm!(
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
            "mov rdi, rsp",
            "call {}",
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
            sym general_protection_inner,
            options(noreturn)
        )
    }
}

extern "C" fn general_protection_inner(ctx: *const ExceptionContextWithError) {
    let ctx = unsafe { &*ctx };
    
    kprintln!();
    kprintln!("==================== EXCEPTION ====================");
    kprintln!("  General Protection Fault (#GP)");
    kprintln!("===================================================");
    kprintln!();
    
    // Decode error code for #GP
    let error = ctx.error_code;
    if error == 0 {
        kprintln!("Cause: Unspecified (error code = 0)");
        kprintln!("  Possible causes:");
        kprintln!("  - Accessing invalid memory");
        kprintln!("  - Privilege level violation");
        kprintln!("  - Writing to read-only segment");
    } else {
        // Error code contains segment selector index
        let external = error & 1 != 0;
        let table = (error >> 1) & 0x3;
        let index = (error >> 3) & 0x1FFF;
        
        let table_name = match table {
            0 => "GDT",
            1 => "IDT",
            2 => "LDT",
            3 => "IDT",
            _ => "Unknown",
        };
        
        kprintln!("Error Code: {:#06x}", error);
        kprintln!("  External: {}", external);
        kprintln!("  Table: {} ({})", table, table_name);
        kprintln!("  Selector Index: {}", index);
    }
    
    kprintln!();
    kprintln!("Stack Frame:");
    ctx.frame.dump();
    kprintln!();
    kprintln!("Registers:");
    ctx.regs.dump();
    kprintln!();
    kprintln!("FATAL: Cannot recover from General Protection Fault. Halting.");
    cpu::halt_loop();
}

/// Page Fault handler - reads CR2 and provides detailed information
#[naked]
pub extern "C" fn page_fault() {
    unsafe {
        core::arch::asm!(
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
            "mov rdi, rsp",
            "call {}",
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
            sym page_fault_inner,
            options(noreturn)
        )
    }
}

extern "C" fn page_fault_inner(ctx: *const ExceptionContextWithError) {
    let ctx = unsafe { &*ctx };
    
    // Read CR2 to get the faulting address
    let faulting_address = cpu::read_cr2();
    let error = ctx.error_code;
    
    kprintln!();
    kprintln!("==================== PAGE FAULT ====================");
    kprintln!("  Faulting Address: {:#018x}", faulting_address);
    kprintln!("  Error Code:       {:#018x}", error);
    kprintln!("====================================================");
    kprintln!();
    
    // Decode error code
    kprintln!("Fault Details:");
    kprintln!("  Cause: {}", decode_page_fault_error(error));
    kprintln!("  Page Present:      {}", if error & page_fault_error::PRESENT != 0 { "Yes" } else { "No" });
    kprintln!("  Access Type:       {}", if error & page_fault_error::WRITE != 0 { "Write" } else { "Read" });
    kprintln!("  Privilege Level:   {}", if error & page_fault_error::USER != 0 { "User" } else { "Kernel" });
    kprintln!("  Reserved Bit Set:  {}", if error & page_fault_error::RESERVED != 0 { "Yes" } else { "No" });
    kprintln!("  Instruction Fetch: {}", if error & page_fault_error::INSTRUCTION != 0 { "Yes" } else { "No" });
    
    kprintln!();
    kprintln!("Stack Frame:");
    ctx.frame.dump();
    kprintln!();
    kprintln!("Registers:");
    ctx.regs.dump();
    kprintln!();
    
    // In the future, we could attempt recovery here:
    // - If it's a demand paging fault, allocate the page
    // - If it's a copy-on-write fault, copy the page
    // - If it's a stack growth fault, grow the stack
    
    kprintln!("FATAL: Page fault cannot be handled yet. Halting.");
    cpu::halt_loop();
}

// =============================================================================
// Hardware IRQ Handlers
// =============================================================================

/// Timer interrupt handler (IRQ0, vector 32)
#[naked]
pub extern "C" fn timer_interrupt() {
    unsafe {
        core::arch::asm!(
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
            "call {}",
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
            sym timer_interrupt_inner,
            options(noreturn)
        )
    }
}

extern "C" fn timer_interrupt_inner() {
    timer::handle_interrupt();
}

/// Keyboard interrupt handler (IRQ1, vector 33)
#[naked]
pub extern "C" fn keyboard_interrupt() {
    unsafe {
        core::arch::asm!(
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
            "call {}",
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
            sym keyboard_interrupt_inner,
            options(noreturn)
        )
    }
}

extern "C" fn keyboard_interrupt_inner() {
    keyboard::handle_interrupt();
}
