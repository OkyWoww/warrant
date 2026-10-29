//! # Pair 09 (R2 Mutant): Unsound `Sync` on `Cell<usize>` (`r2_01_cell_sync_mutant.rs`)
//!
//! Mutation: Replaced `AtomicUsize` with `core::cell::Cell<usize>` while keeping `unsafe impl Sync`.
//! Fails under Miri with `Data race detected between`.

use core::cell::Cell;

pub struct TelemetryCounter {
    count: Cell<usize>,
}

// UNSOUND MUTANT: `Cell<usize>` performs unsynchronized non-atomic reads and writes.
unsafe impl Sync for TelemetryCounter {}

impl TelemetryCounter {
    pub fn new(init: usize) -> Self {
        Self {
            count: Cell::new(init),
        }
    }

    pub fn increment(&self) {
        self.count.set(self.count.get() + 1);
    }

    pub fn get(&self) -> usize {
        self.count.get()
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
        std::hint::black_box(counter.get());
    }
}

