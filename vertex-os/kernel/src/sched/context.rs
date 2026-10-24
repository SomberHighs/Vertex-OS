//! Low-level context switching.
//!
//! A saved context is nothing but a stack pointer. On the stack, from the
//! lowest address upwards, it holds:
//!
//! ```text
//!   r15, r14, r13, r12, rbx, rbp, return address
//! ```
//!
//! `switch_context` pushes the callee-saved registers, stores `rsp` into the
//! outgoing task, loads the incoming `rsp`, pops the same registers and
//! returns into the incoming task. Because every switch site is either this
//! call or an interrupt handler that resumes through this call, a task always
//! resumes exactly where it was switched out.

use crate::sched::task::Context;

extern "C" {
    /// Switch stacks; see the module documentation for the frame layout.
    ///
    /// # Safety
    /// `old_slot` must point at writable storage and `new_stack_ptr` must be a
    /// stack set up by `Task::setup_stack` or a previous switch.
    pub fn switch_context(old_slot: *mut u64, new_stack_ptr: u64);
}

core::arch::global_asm!(
    r#"
    .text
    .global switch_context

# switch_context(old_slot: *mut u64, new_stack_ptr: u64)
#   rdi = where to store the outgoing stack pointer
#   rsi = incoming stack pointer
switch_context:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    mov [rdi], rsp
    mov rsp, rsi

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    ret
    "#
);

/// Switch from the current task's stack to `new_stack_ptr`.
///
/// # Safety
/// Interrupts must be disabled and both stacks must be valid.
#[inline]
pub unsafe fn switch(old_slot: *mut u64, new_stack_ptr: u64) {
    switch_context(old_slot, new_stack_ptr);
}

/// Size of the saved frame described above.
pub const CONTEXT_FRAME_SIZE: usize = core::mem::size_of::<Context>();
