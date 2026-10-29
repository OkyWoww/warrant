//! # Pair 02 (R1 Mutant): Unchecked pointer `.add()` dereference (`r1_02_ptr_arithmetic_mutant.rs`)
//!
//! Mutation: Removed `target < self.buf.len()` guard before `self.buf.as_ptr().add(target)`.
//! Fails under Miri with `in-bounds pointer arithmetic failed`.

pub struct RawCursor {
    buf: Vec<u32>,
    pos: usize,
}

impl RawCursor {
    pub fn new(buf: Vec<u32>) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// UNSOUND MUTANT: Unchecked pointer addition beyond allocation bounds.
    pub fn peek_offset(&self, offset: usize) -> Option<u32> {
        let target = self.pos.checked_add(offset)?;
        // BUG: Missing `target < self.buf.len()` check before `.add(target)`!
        Some(unsafe { *self.buf.as_ptr().add(target) })
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_02_ptr_arithmetic() {
        let mut cursor = RawCursor::new(vec![100, 200, 300, 400]);
        cursor.seek(2);
        let val = cursor.peek_offset(10);
        std::hint::black_box(val);
    }
}

