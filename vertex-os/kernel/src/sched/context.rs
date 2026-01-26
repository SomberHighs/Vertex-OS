//! Context switching implementation
//!
//! Low-level assembly routines for switching between task contexts.

use super::task::Context;

extern "C" {
    /// Switch from one task context to another
    /// 
    /// # Arguments
    /// * `old_ctx` - Pointer to save the current context (stack pointer)
    /// * `new_ctx` - Pointer to the new context to load (stack pointer)
    /// 
    /// # Safety
    /// Both pointers must be valid and properly aligned.
    /// The new context must have been set up by setup_stack or a previous switch_context.
    pub fn switch_context(old_ctx: *mut u64, new_ctx: u64);
}

core::arch::global_asm!(r#"
.global switch_context

// switch_context(old_ctx: *mut u64, new_ctx: u64)
// rdi = pointer to old task's stack pointer (to save current rsp)
// rsi = new task's stack pointer (to load)
switch_context:
    // Save callee-saved registers on the current stack
    // These are the registers that must be preserved across function calls
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    
    // Save the current stack pointer to *old_ctx
    mov [rdi], rsp
    
    // Load the new stack pointer
    mov rsp, rsi
    
    // Restore callee-saved registers from the new stack
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    
    // Return to the new task
    // (the return address is now on top of the stack)
    ret
"#);

/// Perform a context switch from the current task to a new task
/// 
/// # Safety
/// - Must be called with interrupts disabled
/// - Both stack pointers must be valid
#[inline]
pub unsafe fn do_switch(old_stack_ptr: *mut u64, new_stack_ptr: u64) {
    switch_context(old_stack_ptr, new_stack_ptr);
}
