//! # Pair 01 (R1 Mutant): Unchecked staged slice access (`r1_01_slice_get_mutant.rs`)
//!
//! Mutation: Removed the `if idx < self.data.len()` bounds check before `get_unchecked(idx)`.
//! Fails under Miri with `ub_oob` precondition violation.

pub struct SliceBuffer {
    data: Vec<u8>,
    cursor: usize,
}

impl SliceBuffer {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            cursor: 0,
        }
    }

    pub fn push_frame(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn advance(&mut self, steps: usize) {
        self.cursor = self.cursor.saturating_add(steps);
    }

    /// UNSOUND MUTANT: Bounds check removed before `get_unchecked`.
    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        // BUG: Missing `idx < self.data.len()` check!
        Some(unsafe { *self.data.get_unchecked(idx) })
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_01_slice_get() {
        let mut buf = SliceBuffer::new();
        buf.push_frame(&[10, 20, 30, 40]);
        buf.advance(2);
        let val = buf.read_at_offset(6);
        std::hint::black_box(val);
    }
}

