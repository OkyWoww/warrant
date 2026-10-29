//! # Benchmark Fixture R2 Reference: Unsound `unsafe impl Sync` on `Cell<usize>`
//!
//! Target pattern: A type wrapping `Cell<T>` manually implements `Sync`.
//! Because `Cell` does not synchronize unsynchronized concurrent writes and reads,
//! this violates Rust's thread-safety guarantees.
//! Under Miri, safe multithreaded harness fails deterministically with `data_race`.

#![no_std]
extern crate alloc;

pub struct SharedCounter {
    counter: core::cell::Cell<usize>,
}

impl SharedCounter {
    pub fn new(init: usize) -> Self {
        Self {
            counter: core::cell::Cell::new(init),
        }
    }

    pub fn increment(&self) {
        let cur = self.counter.get();
        self.counter.set(cur + 1);
    }

    pub fn get(&self) -> usize {
        self.counter.get()
    }
}

// UNSOUND: Cell is not Sync
unsafe impl Sync for SharedCounter {}

// ==============================================================================
// Safe Verification Harness
// Strictly adheres to WARRANT invariant: #![forbid(unsafe_code)] + extern crate std;
// ==============================================================================
#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_sync_data_race() {
        let counter = SharedCounter::new(0);

        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..100 {
                    counter.increment();
                }
            });
            s.spawn(|| {
                for _ in 0..100 {
                    counter.increment();
                }
            });
        });

        std::hint::black_box(counter.get());
    }
}
