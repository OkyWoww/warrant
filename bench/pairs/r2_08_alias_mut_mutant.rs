//! Pair r2_08_alias_mut (Mutant): Unsynchronized mutable aliasing from `&self` via UnsafeCell.
use core::cell::UnsafeCell;

pub struct SharedAccumulator {
    val: UnsafeCell<u64>,
}

// MUTATION: Unsound `unsafe impl Sync` allowing concurrent `*self.val.get() += delta` without atomics.
unsafe impl Sync for SharedAccumulator {}

impl SharedAccumulator {
    pub const fn new(init: u64) -> Self {
        Self {
            val: UnsafeCell::new(init),
        }
    }

    pub fn add(&self, delta: u64) {
        // BUG: Concurrent read-modify-write via raw pointer causes a Miri data race.
        unsafe {
            let p = self.val.get();
            *p += delta;
        }
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
