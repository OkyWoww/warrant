//! # Benchmark Fixture R1 Vec Queue Safe: Sound checked indexing
//!
//! Safe counterpart to r1_vec_queue.rs. Uses `.get()`.
//! Under Miri, execution passes cleanly with exit code 0.

pub struct FastQueue {
    storage: Vec<u64>,
}

impl FastQueue {
    pub fn new(storage: Vec<u64>) -> Self {
        Self { storage }
    }

    /// SOUND: Performs checked retrieval returning Option.
    pub fn peek_unchecked(&mut self, idx: usize) -> Option<u64> {
        self.storage.get(idx).copied()
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
        assert_eq!(val, None);
    }
}
