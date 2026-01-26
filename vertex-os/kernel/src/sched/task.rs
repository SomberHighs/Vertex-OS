//! Task (Thread) management
//!
//! Defines the Task structure and its associated context for context switching.

use core::sync::atomic::{AtomicU64, Ordering};
use crate::mm::addr::PAGE_SIZE;

/// Size of the kernel stack for each task (16 KiB)
pub const KERNEL_STACK_SIZE: usize = 4 * PAGE_SIZE as usize;

/// Task ID type
pub type TaskId = u64;

/// Global task ID counter
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a new unique task ID
fn new_task_id() -> TaskId {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskState {
    /// Task is ready to run
    Ready,
    /// Task is currently running
    Running,
    /// Task is blocked waiting for something
    Blocked,
    /// Task has exited
    Dead,
}

/// CPU context saved during context switch
/// This must match the order registers are pushed/popped in switch_context
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Context {
    // Callee-saved registers (saved by switch_context)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    // Return address (where to continue after switch)
    pub rip: u64,
}

impl Context {
    /// Create a new empty context
    pub const fn empty() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rip: 0,
        }
    }
    
    /// Create a new context that will start at the given entry point
    pub fn new(entry_point: u64) -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rip: entry_point,
        }
    }
}

/// A kernel task (thread)
#[repr(C)]
pub struct Task {
    /// Unique task identifier
    pub id: TaskId,
    /// Task name (for debugging)
    pub name: [u8; 32],
    /// Current task state
    pub state: TaskState,
    /// Saved CPU context (stack pointer points into kernel_stack)
    pub context: Context,
    /// Stack pointer (points to saved context on stack)
    pub stack_ptr: u64,
    /// Kernel stack (bottom of stack, stack grows down)
    pub kernel_stack: [u8; KERNEL_STACK_SIZE],
    /// Priority (lower = higher priority, 0 = highest)
    pub priority: u8,
    /// Time slice remaining (in ticks)
    pub time_slice: u32,
    /// Total CPU time used (in ticks)
    pub cpu_time: u64,
}

impl Task {
    /// Create a new task with the given entry point
    /// 
    /// # Safety
    /// The entry point must be a valid function pointer that never returns
    pub fn new(name: &str, entry_point: fn() -> !) -> Self {
        let id = new_task_id();
        
        // Copy name
        let mut name_buf = [0u8; 32];
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(31);
        name_buf[..len].copy_from_slice(&name_bytes[..len]);
        
        let mut task = Self {
            id,
            name: name_buf,
            state: TaskState::Ready,
            context: Context::empty(),
            stack_ptr: 0,
            kernel_stack: [0; KERNEL_STACK_SIZE],
            priority: 128, // Default middle priority
            time_slice: 10, // Default 10 ticks (100ms at 100Hz)
            cpu_time: 0,
        };
        
        // Set up the initial stack
        task.setup_stack(entry_point as u64);
        
        task
    }
    
    /// Set up the initial stack for a new task
    fn setup_stack(&mut self, entry_point: u64) {
        // Stack grows downward, so we start at the top
        let stack_top = self.kernel_stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;
        
        // Align stack to 16 bytes (required by System V ABI)
        let stack_top = stack_top & !0xF;
        
        // We need to set up the stack so that when switch_context "returns",
        // it will jump to our entry point. The context is at the top of the stack.
        let context_ptr = stack_top - core::mem::size_of::<Context>() as u64;
        
        // Initialize the context on the stack
        let context = unsafe { &mut *(context_ptr as *mut Context) };
        context.rip = task_entry_trampoline as u64;
        context.rbp = 0; // No previous frame
        context.rbx = entry_point; // Pass entry point in rbx
        context.r12 = 0;
        context.r13 = 0;
        context.r14 = 0;
        context.r15 = 0;
        
        // Save the stack pointer (pointing to the context)
        self.stack_ptr = context_ptr;
    }
    
    /// Get the task name as a string slice
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..len]).unwrap_or("???")
    }
    
    /// Check if this task is the idle task
    pub fn is_idle(&self) -> bool {
        self.id == 0
    }
}

/// Trampoline function that sets up the environment before jumping to the task entry point
/// This is called when a new task is first scheduled
#[naked]
unsafe extern "C" fn task_entry_trampoline() {
    core::arch::asm!(
        // Entry point was passed in rbx by setup_stack
        // Set up a clean stack frame
        "xor rbp, rbp",      // Clear frame pointer
        // Re-enable interrupts (they're disabled during context switch)
        "sti",
        // Jump to the actual entry point (in rbx)
        "jmp rbx",
        options(noreturn)
    )
}

/// The idle task - runs when no other task is ready
pub fn idle_task() -> ! {
    loop {
        // Enable interrupts and halt until next interrupt
        unsafe {
            core::arch::asm!(
                "sti",
                "hlt",
                options(nomem, nostack)
            );
        }
    }
}
