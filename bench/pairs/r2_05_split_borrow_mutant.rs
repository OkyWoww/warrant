//! Pair r2_05_split_borrow (Mutant): Off-by-one in split borrow causes overlapping write race.
use core::cell::UnsafeCell;

pub struct ParallelPair {
    pair: UnsafeCell<[u64; 2]>,
}

// MUTATION: `Sync` implementation relies on `write_left` and `write_right` being disjoint,
// but `write_right` mistakenly writes to `.add(0)` instead of `.add(1)`.
unsafe impl Sync for ParallelPair {}

impl ParallelPair {
    pub const fn new(a: u64, b: u64) -> Self {
        Self {
            pair: UnsafeCell::new([a, b]),
        }
    }

    pub fn write_left(&self, val: u64) {
        unsafe {
            let base = self.pair.get().cast::<u64>();
            *base.add(0) = val;
        }
    }

    pub fn write_right(&self, val: u64) {
        // BUG: Off-by-one index `.add(0)` aliases with `write_left`!
        unsafe {
            let base = self.pair.get().cast::<u64>();
            *base.add(0) = val;
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

