//! # Pair 09 (R2 Safe): Atomic telemetry counter (`r2_01_cell_sync_safe.rs`)
//!
//! Uses `AtomicUsize` for concurrent counter updates across threads.
//! Passes cleanly under Miri (Exit Code 0).

use core::sync::atomic::{AtomicUsize, Ordering};

pub struct TelemetryCounter {
    count: AtomicUsize,
}

// SOUND: `AtomicUsize` provides synchronized atomic operations across threads.
unsafe impl Sync for TelemetryCounter {}

impl TelemetryCounter {
    pub fn new(init: usize) -> Self {
        Self {
            count: AtomicUsize::new(init),
        }
    }

    pub fn increment(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_01_cell_sync() {
        let counter = TelemetryCounter::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..10 {
                    counter.increment();
                }
            });
            s.spawn(|| {
                for _ in 0..10 {
                    counter.increment();
                }
            });
        });
        assert_eq!(counter.get(), 20);
    }
}

