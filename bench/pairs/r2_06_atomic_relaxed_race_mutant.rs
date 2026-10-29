//! Pair r2_06_atomic_relaxed_race (Mutant): Unsynchronized non-atomic write before Relaxed store.
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub struct StatusChannel {
    payload: UnsafeCell<u64>,
    ready: AtomicBool,
}

// MUTATION: Unsound `unsafe impl Sync` where concurrent `publish` calls race on `payload`.
unsafe impl Sync for StatusChannel {}

impl StatusChannel {
    pub const fn new(initial: u64) -> Self {
        Self {
            payload: UnsafeCell::new(initial),
            ready: AtomicBool::new(false),
        }
    }

    pub fn publish(&self, val: u64) {
        // BUG: Unsynchronized write to `UnsafeCell<u64>` across concurrent threads.
        unsafe {
            *self.payload.get() = val;
        }
        self.ready.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_06_atomic_relaxed_race() {
        let chan = StatusChannel::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                chan.publish(111);
            });
            s.spawn(|| {
                chan.publish(222);
            });
        });
    }
}

