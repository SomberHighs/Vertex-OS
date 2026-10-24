//! Round-robin preemptive scheduler.
//!
//! Invariants:
//!   * slot 0 is always the idle task;
//!   * every mutation happens with interrupts disabled, so an interrupt can
//!     never observe a half-updated run queue;
//!   * the run queue lives in static storage and is only touched through raw
//!     pointers, because a context switch suspends the current frame while
//!     another task runs the same code path.

use crate::arch::x86_64::cpu;
use crate::arch::x86_64::serial;
use crate::kprintln;
use crate::mm::heap;
use crate::sync::SyncCell;

use super::context;
use super::task::{Task, TaskEntry, TaskId, TaskState, KERNEL_STACK_SIZE};

pub const MAX_TASKS: usize = 32;
const DEFAULT_SLICES: u32 = 5; // 50 ms at 100 Hz

struct Scheduler {
    tasks: [Option<Task>; MAX_TASKS],
    current: usize,
    live: usize,
    initialized: bool,
}

impl Scheduler {
    const fn new() -> Self {
        const NONE: Option<Task> = None;
        Self {
            tasks: [NONE; MAX_TASKS],
            current: 0,
            live: 0,
            initialized: false,
        }
    }

    fn free_slot(&self) -> Option<usize> {
        // Slot 0 is reserved for the idle task.
        (1..MAX_TASKS).find(|&i| self.tasks[i].is_none())
    }

    /// Round-robin pick: the next Ready task after `from`, else idle.
    fn pick_next(&self, from: usize) -> Option<usize> {
        for offset in 1..MAX_TASKS {
            let index = (from + offset) % MAX_TASKS;
            if index == 0 {
                continue;
            }
            if let Some(task) = self.tasks[index].as_ref() {
                if task.state == TaskState::Ready {
                    return Some(index);
                }
            }
        }
        if self.tasks[0].is_some() {
            Some(0)
        } else {
            None
        }
    }
}

static SCHEDULER: SyncCell<Scheduler> = SyncCell::new(Scheduler::new());

#[inline]
fn raw() -> *mut Scheduler {
    SCHEDULER.as_ptr()
}

/// Create the idle task. Called once during boot.
pub fn init() {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        if (*sched).initialized {
            return;
        }
        // The boot context is the idle task: its stack pointer is captured the
        // first time we switch away from it, so no stack is allocated here.
        (*sched).tasks[0] = Some(Task::idle("idle"));
        (*sched).current = 0;
        (*sched).live = 1;
        (*sched).initialized = true;
    });
}

/// Spawn a kernel thread. Returns its id, or `None` when the table is full.
pub fn spawn(name: &str, entry: TaskEntry) -> Option<TaskId> {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        if !(*sched).initialized {
            return None;
        }
        let slot = (*sched).free_slot()?;
        let stack = heap::alloc(KERNEL_STACK_SIZE, 16);
        let task = Task::new(name, entry, stack, DEFAULT_SLICES)?;
        let id = task.id;
        (*sched).tasks[slot] = Some(task);
        (*sched).live += 1;
        Some(id)
    })
}

/// Preemption point, called from the timer interrupt.
pub fn timer_tick() {
    unsafe {
        let sched = raw();
        if !(*sched).initialized {
            return;
        }

        let current = (*sched).current;
        let mut expired = false;
        if let Some(task) = (*sched).tasks[current].as_ref() {
            stop_on_stack_overflow(task);
        }
        if let Some(task) = (*sched).tasks[current].as_mut() {
            task.cpu_ticks += 1;
            if task.slices_left > 0 {
                task.slices_left -= 1;
            }
            expired = task.slices_left == 0;
        }
        if !expired {
            return;
        }

        let Some(next) = (*sched).pick_next(current) else {
            return;
        };
        if next == current {
            if let Some(task) = (*sched).tasks[current].as_mut() {
                task.slices_left = DEFAULT_SLICES;
            }
            return;
        }

        switch_to(sched, current, next);
        // We resume here when this task is scheduled again.
        if let Some(task) = (*sched).tasks[(*sched).current].as_mut() {
            task.slices_left = DEFAULT_SLICES;
        }
    }
}

/// The guard word has been overwritten: the stack already wrote into the heap.
///
/// Reporting goes straight to the serial port because a fault like this can
/// happen inside an interrupt, where taking the console lock could deadlock.
fn stop_on_stack_overflow(task: &Task) {
    if !task.stack_overflowed() {
        return;
    }
    serial::write_str("\n[FAIL] kernel stack overflow in task '");
    serial::write_str(task.name());
    serial::write_str("' - the guard word was overwritten, halting\n");
    cpu::halt_loop();
}

/// Give up the CPU voluntarily.
pub fn yield_cpu() {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        if !(*sched).initialized {
            return;
        }
        let current = (*sched).current;
        if let Some(task) = (*sched).tasks[current].as_ref() {
            stop_on_stack_overflow(task);
        }
        if let Some(task) = (*sched).tasks[current].as_mut() {
            if task.state == TaskState::Running {
                task.state = TaskState::Ready;
            }
        }
        if let Some(next) = (*sched).pick_next(current) {
            if next != current {
                switch_to(sched, current, next);
            }
        }
        // Resumed (or nothing else was runnable): keep running.
        let now = (*sched).current;
        if let Some(task) = (*sched).tasks[now].as_mut() {
            task.state = TaskState::Running;
            task.slices_left = DEFAULT_SLICES;
        }
    });
}

/// Terminate the calling task.
pub fn exit() -> ! {
    unsafe {
        cpu::without_interrupts(|| {
            let sched = raw();
            let current = (*sched).current;
            if let Some(task) = (*sched).tasks[current].as_mut() {
                task.state = TaskState::Dead;
                (*sched).live = (*sched).live.saturating_sub(1);
            }
            if let Some(next) = (*sched).pick_next(current) {
                if next != current {
                    switch_to(sched, current, next);
                }
            }
        });
    }
    // Nothing else is runnable: park forever.
    loop {
        cpu::hlt();
    }
}

/// Block the calling task until [`unblock`] is called for it.
pub fn block_current() {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        let current = (*sched).current;
        if let Some(task) = (*sched).tasks[current].as_mut() {
            task.state = TaskState::Blocked;
        }
        if let Some(next) = (*sched).pick_next(current) {
            if next != current {
                switch_to(sched, current, next);
            }
        }
    });
}

/// Mark a blocked task runnable again.
pub fn unblock(id: TaskId) {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        for slot in (*sched).tasks.iter_mut() {
            if let Some(task) = slot.as_mut() {
                if task.id == id && task.state == TaskState::Blocked {
                    task.state = TaskState::Ready;
                    return;
                }
            }
        }
    });
}

/// ID of the running task.
pub fn current_task_id() -> TaskId {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        (*sched).tasks[(*sched).current]
            .as_ref()
            .map(|task| task.id)
            .unwrap_or(0)
    })
}

/// Number of live tasks.
pub fn task_count() -> usize {
    cpu::without_interrupts(|| unsafe { (*raw()).live })
}

/// Print a table of tasks.
pub fn print_stats() {
    cpu::without_interrupts(|| unsafe {
        let sched = raw();
        kprintln!("   id  name             state     ticks  switches");
        for (index, slot) in (*sched).tasks.iter().enumerate() {
            if let Some(task) = slot.as_ref() {
                let marker = if index == (*sched).current { "*" } else { " " };
                kprintln!(
                    " {} {:>3} {:<16} {:<9} {:>6} {:>9}",
                    marker,
                    task.id,
                    task.name(),
                    match task.state {
                        TaskState::Ready => "ready",
                        TaskState::Running => "running",
                        TaskState::Blocked => "blocked",
                        TaskState::Dead => "dead",
                    },
                    task.cpu_ticks,
                    task.switches
                );
            }
        }
    });
}

/// Perform the actual switch. Caller holds the CPU with interrupts disabled.
unsafe fn switch_to(sched: *mut Scheduler, current: usize, next: usize) {
    let old_slot = match (*sched).tasks[current].as_mut() {
        Some(task) => &mut task.stack_ptr as *mut u64,
        None => return,
    };
    let new_stack = match (*sched).tasks[next].as_mut() {
        Some(task) => {
            let stack = task.stack_ptr;
            if stack == 0 {
                // Nothing to resume (the idle task before its first switch).
                return;
            }
            task.state = TaskState::Running;
            task.slices_left = DEFAULT_SLICES;
            task.switches += 1;
            stack
        }
        None => return,
    };

    if let Some(task) = (*sched).tasks[current].as_mut() {
        if task.state == TaskState::Running {
            task.state = TaskState::Ready;
        }
    }
    (*sched).current = next;

    // Everything is committed before the switch: the incoming task may run
    // arbitrary code before this frame is ever resumed.
    context::switch(old_slot, new_stack);
}
