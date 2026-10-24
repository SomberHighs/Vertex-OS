//! Synchronization primitives.

pub mod cell;
pub mod spinlock;

pub use cell::SyncCell;
pub use spinlock::SpinLock;
