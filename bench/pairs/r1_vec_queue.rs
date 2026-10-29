//! # Benchmark Fixture R1 Vec Queue: Unsound mutable indexing
//!
//! Target pattern: A ring queue calling `get_unchecked_mut` without checking capacity.
//! Under Miri, accessing an unallocated index triggers precondition violation.

pub struct FastQueue {
    storage: Vec<u64>,
}

impl FastQueue {
    pub fn new(storage: Vec<u64>) -> Self {
        Self { storage }
    }

    /// UNSOUND: Indexes without verifying idx < storage.len().
    pub fn peek_unchecked(&mut self, idx: usize) -> u64 {
        // SAFETY: Invariant violated! Missing bounds check.
        unsafe { *self.storage.get_unchecked_mut(idx) }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_vec_queue_oob() {
        let mut q = FastQueue::new(vec![100, 200]);
        let val = q.peek_unchecked(50);
        std::hint::black_box(val);
    }
}
