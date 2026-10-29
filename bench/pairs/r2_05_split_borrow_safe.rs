//! Pair r2_05_split_borrow (Safe): Disjoint index access across threads.
use core::cell::UnsafeCell;

pub struct ParallelPair {
    pair: UnsafeCell<[u64; 2]>,
}

unsafe impl Sync for ParallelPair {}

impl ParallelPair {
    pub const fn new(a: u64, b: u64) -> Self {
        Self {
            pair: UnsafeCell::new([a, b]),
        }
    }

    pub fn write_left(&self, val: u64) {
        // SAFETY: `write_left` only accesses index 0.
        unsafe {
            let base = self.pair.get().cast::<u64>();
            *base.add(0) = val;
        }
    }

    pub fn write_right(&self, val: u64) {
        // SAFETY: `write_right` only accesses index 1 (disjoint from index 0).
        unsafe {
            let base = self.pair.get().cast::<u64>();
            *base.add(1) = val;
        }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_05_split_borrow() {
        let pair = ParallelPair::new(0, 0);
        std::thread::scope(|s| {
            s.spawn(|| {
                pair.write_left(100);
            });
            s.spawn(|| {
                pair.write_right(200);
            });
        });
    }
}

