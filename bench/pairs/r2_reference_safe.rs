//! # Benchmark Fixture R2 Reference Safe: Sound atomic shared counter
//!
//! Safe counterpart to r2_reference.rs. Uses AtomicUsize which is natively Sync.
//! Under Miri, concurrent execution passes cleanly with exit code 0.

use core::sync::atomic::{AtomicUsize, Ordering};

pub struct SharedCounter {
    counter: AtomicUsize,
}

impl SharedCounter {
    pub fn new(init: usize) -> Self {
        Self {
            counter: AtomicUsize::new(init),
        }
    }

    pub fn increment(&self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get(&self) -> usize {
        self.counter.load(Ordering::SeqCst)
    }
}

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

        assert_eq!(counter.get(), 200);
    }
}
