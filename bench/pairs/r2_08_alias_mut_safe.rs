//! Pair r2_08_alias_mut (Safe): Atomic accumulator safe across concurrent threads.
use core::sync::atomic::{AtomicU64, Ordering};

pub struct SharedAccumulator {
    val: AtomicU64,
}

unsafe impl Sync for SharedAccumulator {}

impl SharedAccumulator {
    pub const fn new(init: u64) -> Self {
        Self {
            val: AtomicU64::new(init),
        }
    }

    pub fn add(&self, delta: u64) {
        self.val.fetch_add(delta, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_08_alias_mut() {
        let acc = SharedAccumulator::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                acc.add(5);
            });
            s.spawn(|| {
                acc.add(10);
            });
        });
    }
}
