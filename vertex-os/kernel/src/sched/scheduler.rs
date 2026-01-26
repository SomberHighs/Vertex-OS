//! Round-robin scheduler
//!
//! A simple preemptive round-robin scheduler for kernel tasks.

use super::task::{Task, TaskId, TaskState, idle_task, KERNEL_STACK_SIZE};
use super::context;
use crate::sync::SpinLock;
use crate::arch::x86_64::cpu;
use crate::kprintln;

/// Maximum number of tasks
const MAX_TASKS: usize = 64;

/// Default time slice in timer ticks
const DEFAULT_TIME_SLICE: u32 = 10;

/// Scheduler state
struct Scheduler {
    /// Array of tasks (Option allows empty slots)
    tasks: [Option<Task>; MAX_TASKS],
    /// Index of the currently running task
    current: usize,
    /// Number of active tasks
    task_count: usize,
    /// Whether the scheduler is initialized
    initialized: bool,
    /// Ticks until next preemption
    ticks_remaining: u32,
}

impl Scheduler {
    /// Create a new scheduler
    const fn new() -> Self {
        // Can't use array initialization with Option<Task> because Task doesn't impl Copy
        // So we use a const initializer trick
        const NONE: Option<Task> = None;
        Self {
            tasks: [NONE; MAX_TASKS],
            current: 0,
            task_count: 0,
            initialized: false,
            ticks_remaining: DEFAULT_TIME_SLICE,
        }
    }
    
    /// Find an empty slot for a new task
    fn find_empty_slot(&self) -> Option<usize> {
        for (i, slot) in self.tasks.iter().enumerate() {
            if slot.is_none() {
                return Some(i);
            }
        }
        None
    }
    
    /// Find the next ready task (round-robin)
    fn find_next_ready(&self) -> Option<usize> {
        let start = (self.current + 1) % MAX_TASKS;
        let mut i = start;
        
        loop {
            if let Some(ref task) = self.tasks[i] {
                if task.state == TaskState::Ready {
                    return Some(i);
                }
            }
            
            i = (i + 1) % MAX_TASKS;
            if i == start {
                break;
            }
        }
        
        // Check if current task is still ready (or running)
        if let Some(ref task) = self.tasks[self.current] {
            if task.state == TaskState::Ready || task.state == TaskState::Running {
                return Some(self.current);
            }
        }
        
        // Fall back to idle task (always at index 0)
        Some(0)
    }
}

/// Global scheduler instance
static SCHEDULER: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

/// Initialize the scheduler
/// 
/// Creates the idle task and marks the scheduler as ready.
pub fn init() {
    let mut sched = SCHEDULER.lock();
    
    if sched.initialized {
        return;
    }
    
    // Create the idle task at slot 0
    let mut idle = Task::new("idle", idle_task);
    idle.state = TaskState::Running; // Idle starts as "running"
    idle.priority = 255; // Lowest priority
    
    // The idle task's ID should be 0 for is_idle() check
    // But new_task_id() starts at 1, so we manually set it
    // Actually, let's keep it consistent - idle is just the first task
    
    sched.tasks[0] = Some(idle);
    sched.current = 0;
    sched.task_count = 1;
    sched.initialized = true;
    sched.ticks_remaining = DEFAULT_TIME_SLICE;
    
    kprintln!("[OK] Scheduler initialized");
}

/// Spawn a new task
/// 
/// Returns the task ID if successful, None if no slots available.
pub fn spawn(name: &str, entry: fn() -> !) -> Option<TaskId> {
    let mut sched = SCHEDULER.lock();
    
    let slot = sched.find_empty_slot()?;
    
    let task = Task::new(name, entry);
    let id = task.id;
    
    sched.tasks[slot] = Some(task);
    sched.task_count += 1;
    
    kprintln!("[SCHED] Spawned task '{}' (id={}) at slot {}", name, id, slot);
    
    Some(id)
}

/// Called by the timer interrupt to potentially trigger a context switch
/// 
/// This is the preemption point - if the current task's time slice is exhausted,
/// we switch to the next ready task.
pub fn timer_tick() {
    // Try to acquire the lock without blocking
    // If we can't get it, another CPU might be scheduling
    if let Some(mut sched) = SCHEDULER.try_lock() {
        if !sched.initialized {
            return;
        }
        
        // Update CPU time for current task
        if let Some(ref mut task) = sched.tasks[sched.current] {
            task.cpu_time += 1;
        }
        
        // Decrement time slice
        if sched.ticks_remaining > 0 {
            sched.ticks_remaining -= 1;
        }
        
        // If time slice exhausted, schedule a new task
        if sched.ticks_remaining == 0 {
            drop(sched); // Release lock before scheduling
            schedule();
        }
    }
}

/// Perform a context switch to the next ready task
/// 
/// This can be called voluntarily (yield) or by the timer (preemption).
pub fn schedule() {
    // Disable interrupts during scheduling
    cpu::cli();
    
    let mut sched = SCHEDULER.lock();
    
    if !sched.initialized {
        cpu::sti();
        return;
    }
    
    let current_idx = sched.current;
    
    // Mark current task as ready (unless it's dead or blocked)
    if let Some(ref mut task) = sched.tasks[current_idx] {
        if task.state == TaskState::Running {
            task.state = TaskState::Ready;
        }
    }
    
    // Find next task to run
    let next_idx = sched.find_next_ready().unwrap_or(0);
    
    // If same task, just reset time slice and return
    if next_idx == current_idx {
        sched.ticks_remaining = DEFAULT_TIME_SLICE;
        if let Some(ref mut task) = sched.tasks[current_idx] {
            task.state = TaskState::Running;
        }
        drop(sched);
        cpu::sti();
        return;
    }
    
    // Get stack pointers for context switch
    let (old_stack_ptr, new_stack_ptr) = {
        let current_task = sched.tasks[current_idx].as_mut().unwrap();
        let old_ptr = &mut current_task.stack_ptr as *mut u64;
        
        let next_task = sched.tasks[next_idx].as_mut().unwrap();
        next_task.state = TaskState::Running;
        let new_ptr = next_task.stack_ptr;
        
        (old_ptr, new_ptr)
    };
    
    // Update scheduler state
    sched.current = next_idx;
    sched.ticks_remaining = DEFAULT_TIME_SLICE;
    
    // Release the lock before context switch
    drop(sched);
    
    // Perform the actual context switch
    // Interrupts will be re-enabled by the new task
    unsafe {
        context::do_switch(old_stack_ptr, new_stack_ptr);
    }
    
    // We return here when this task is scheduled again
    cpu::sti();
}

/// Voluntarily yield the CPU to another task
pub fn yield_cpu() {
    schedule();
}

/// Exit the current task
pub fn exit() -> ! {
    {
        let mut sched = SCHEDULER.lock();
        let current = sched.current;
        
        if let Some(ref mut task) = sched.tasks[current] {
            kprintln!("[SCHED] Task '{}' (id={}) exiting", task.name(), task.id);
            task.state = TaskState::Dead;
        }
        
        sched.task_count -= 1;
    }
    
    // Schedule another task
    schedule();
    
    // Should never reach here
    loop {
        cpu::hlt();
    }
}

/// Block the current task
pub fn block_current() {
    let mut sched = SCHEDULER.lock();
    let current = sched.current;
    
    if let Some(ref mut task) = sched.tasks[current] {
        task.state = TaskState::Blocked;
    }
    
    drop(sched);
    schedule();
}

/// Unblock a task by ID
pub fn unblock(task_id: TaskId) {
    let mut sched = SCHEDULER.lock();
    
    for slot in sched.tasks.iter_mut() {
        if let Some(ref mut task) = slot {
            if task.id == task_id && task.state == TaskState::Blocked {
                task.state = TaskState::Ready;
                return;
            }
        }
    }
}

/// Get the current task ID
pub fn current_task_id() -> TaskId {
    let sched = SCHEDULER.lock();
    sched.tasks[sched.current]
        .as_ref()
        .map(|t| t.id)
        .unwrap_or(0)
}

/// Get the number of active tasks
pub fn task_count() -> usize {
    SCHEDULER.lock().task_count
}

/// Print scheduler statistics
pub fn print_stats() {
    let sched = SCHEDULER.lock();
    
    kprintln!("=== Scheduler Stats ===");
    kprintln!("Active tasks: {}", sched.task_count);
    kprintln!("Current task: {}", sched.current);
    kprintln!();
    
    for (i, slot) in sched.tasks.iter().enumerate() {
        if let Some(ref task) = slot {
            kprintln!(
                "  [{}] {} (id={}) - {:?} - CPU time: {} ticks",
                i,
                task.name(),
                task.id,
                task.state,
                task.cpu_time
            );
        }
    }
}
