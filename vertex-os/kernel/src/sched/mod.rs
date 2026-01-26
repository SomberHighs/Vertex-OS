//! Task scheduling subsystem
//!
//! Provides kernel task management with a simple round-robin scheduler.
//!
//! # Overview
//!
//! The scheduler supports:
//! - Kernel tasks (threads) with separate stacks
//! - Preemptive scheduling via timer interrupts
//! - Voluntary yielding
//! - Task blocking and unblocking
//!
//! # Usage
//!
//! ```
//! // Initialize the scheduler (creates idle task)
//! sched::init();
//!
//! // Spawn a new task
//! sched::spawn("my_task", my_task_function);
//!
//! // Yield to other tasks
//! sched::yield_cpu();
//!
//! // Exit current task
//! sched::exit();
//! ```

pub mod task;
pub mod context;
pub mod scheduler;

pub use task::{Task, TaskId, TaskState, KERNEL_STACK_SIZE};
pub use scheduler::{
    init,
    spawn,
    schedule,
    yield_cpu,
    exit,
    block_current,
    unblock,
    current_task_id,
    task_count,
    timer_tick,
    print_stats,
};
