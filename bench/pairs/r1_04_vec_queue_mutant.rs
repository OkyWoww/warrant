//! # Pair 04 (R1 Mutant): Unwrapped ring queue offset (`r1_04_vec_queue_mutant.rs`)
//!
//! Mutation: Replaced `(self.head + offset) % self.buf.len()` with unmasked `self.head + offset`
//! before `get_unchecked_mut`. Fails under Miri with `slice::get_unchecked_mut` precondition panic.

pub struct RingQueue {
    buf: Vec<u64>,
    head: usize,
}

impl RingQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0; capacity.max(1)],
            head: 0,
        }
    }

    pub fn advance_head(&mut self, steps: usize) {
        self.head = (self.head + steps) % self.buf.len();
    }

    /// UNSOUND MUTANT: Missing `% self.buf.len()` wrap on `self.head + offset`.
    pub fn write_at_offset(&mut self, offset: usize, val: u64) {
        let idx = self.head + offset;
        // BUG: `idx` can exceed `self.buf.len()`!
        unsafe {
            *self.buf.get_unchecked_mut(idx) = val;
        }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_04_vec_queue() {
        let mut q = RingQueue::new(4);
        q.advance_head(3);
        q.write_at_offset(5, 99);
    }
}

