//! # Pair 04 (R1 Safe): Modulo-wrapped ring queue (`r1_04_vec_queue_safe.rs`)
//!
//! Staged ring buffer that wraps `(self.head + offset) % self.buf.len()` before
//! `get_unchecked_mut`. Passes cleanly under Miri (Exit Code 0).

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

    /// SOUND: Modulo wrap guarantees `idx < self.buf.len()`.
    pub fn write_at_offset(&mut self, offset: usize, val: u64) {
        let idx = (self.head + offset) % self.buf.len();
        // SAFETY: `self.buf.len() >= 1` and `idx` is reduced modulo `self.buf.len()`.
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

