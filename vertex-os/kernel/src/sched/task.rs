//! Kernel thread representation.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::cpu;

/// Size of a kernel stack in bytes.
///
/// Stacks come from the bump heap and grow downwards, so an overflow writes
/// straight into the node store rather than faulting. The graphical front end
/// composes a full frame within one task, which needs far more room than the
/// shell did; 64 KiB leaves margin and the guard word below catches the rest.
pub const KERNEL_STACK_SIZE: usize = 64 * 1024;

/// Guard word stored at the lowest address of every allocated stack.
pub const STACK_CANARY: u64 = 0x5645_5254_4558_4F53; // "VERTEXOS"

pub type TaskId = u64;
pub type TaskEntry = extern "C" fn() -> !;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Dead,
}

/// Saved register frame; must match `switch_context`'s pops exactly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64,
}

impl Context {
    pub const fn zeroed() -> Self {
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
}

/// A kernel thread.
pub struct Task {
    pub id: TaskId,
    pub name: [u8; 32],
    pub state: TaskState,
    /// Stack pointer of the saved frame (0 until the task first runs).
    pub stack_ptr: u64,
    /// Base of the allocated stack, for reporting.
    pub stack_base: u64,
    /// Scheduler ticks left before preemption.
    pub slices_left: u32,
    /// Ticks the task has run for.
    pub cpu_ticks: u64,
    /// Number of times the task was switched in.
    pub switches: u64,
}

impl Task {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            name: [0; 32],
            state: TaskState::Dead,
            stack_ptr: 0,
            stack_base: 0,
            slices_left: 0,
            cpu_ticks: 0,
            switches: 0,
        }
    }

    /// The idle task: it represents the boot context and has no allocated
    /// stack of its own.
    pub fn idle(name: &str) -> Self {
        let mut task = Self::empty();
        task.id = 0;
        task.state = TaskState::Running;
        task.slices_left = u32::MAX;
        let bytes = name.as_bytes();
        let len = bytes.len().min(task.name.len() - 1);
        task.name[..len].copy_from_slice(&bytes[..len]);
        task
    }

    /// Build a runnable task, allocating its stack from the kernel heap.
    pub fn new(name: &str, entry: TaskEntry, stack: *mut u8, slices: u32) -> Option<Self> {
        if stack.is_null() {
            return None;
        }

        let mut task = Self {
            id: NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed),
            name: [0; 32],
            state: TaskState::Ready,
            stack_ptr: 0,
            stack_base: stack as u64,
            slices_left: slices,
            cpu_ticks: 0,
            switches: 0,
        };

        let bytes = name.as_bytes();
        let len = bytes.len().min(task.name.len() - 1);
        task.name[..len].copy_from_slice(&bytes[..len]);

        task.setup_stack(entry)?;
        task.plant_canary();
        Some(task)
    }

    /// Lay out the initial frame so the first switch "returns" into
    /// [`task_entry_trampoline`] with the entry point in `rbx`.
    fn setup_stack(&mut self, entry: TaskEntry) -> Option<()> {
        let top = (self.stack_base as usize + KERNEL_STACK_SIZE) & !0xF;
        let frame = top.checked_sub(core::mem::size_of::<Context>())?;

        unsafe {
            let context = frame as *mut Context;
            context.write(Context {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                rbx: entry as u64,
                rbp: 0,
                rip: task_entry_trampoline as *const () as u64,
            });
        }

        self.stack_ptr = frame as u64;
        Some(())
    }

    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&c| c == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }

    /// Write the guard word at the bottom of this task's stack.
    fn plant_canary(&self) {
        if self.stack_base == 0 {
            return;
        }
        unsafe {
            (self.stack_base as *mut u64).write_volatile(STACK_CANARY);
        }
    }

    /// True when the stack has grown past its allocation.
    ///
    /// The canary sits at the lowest address, which a downward-growing stack
    /// only reaches after overflowing everything above it.
    pub fn stack_overflowed(&self) -> bool {
        if self.stack_base == 0 {
            return false;
        }
        unsafe { (self.stack_base as *const u64).read_volatile() != STACK_CANARY }
    }
}

/// First code a new task executes.
///
/// `switch_context` has already restored the callee-saved registers, so `rbx`
/// holds the real entry point. Interrupts are switched back on here because the
/// switch that brought us here may have happened inside an interrupt gate.
#[unsafe(naked)]
pub extern "C" fn task_entry_trampoline() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "sub rsp, 8", // imitate a call frame so the ABI stack alignment holds
        "sti",
        "jmp rbx"
            )
}

/// The idle task: waits for the next interrupt.
pub extern "C" fn idle_task() -> ! {
    loop {
        cpu::hlt();
    }
}
