//! # RustSec Anonymized 02: Unchecked cursor advance in slice buffer (R1 UbOob)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (RUSTSEC-2021-0124, `bytes`/`slice-deque`, MIT/Apache-2.0).
//! In `ByteCursor`, `advance` increments internal position without checking remaining capacity,
//! causing subsequent `peek_current` to dereference memory beyond buffer allocation.
//! Fails under Miri with pointer arithmetic / precondition failure.

pub struct ByteCursor {
    buf: Vec<u8>,
    pos: usize,
}

impl ByteCursor {
    pub fn new(data: Vec<u8>) -> Self {
        Self { buf: data, pos: 0 }
    }

    pub fn advance(&mut self, cnt: usize) {
        // BUG: Missing check `if self.pos + cnt <= self.buf.len()`!
        self.pos += cnt;
    }

    /// UNSOUND HISTORICAL PATTERN: Reads current byte using raw pointer without revalidating `self.pos < len`.
    pub fn peek_current(&self) -> u8 {
        unsafe { *self.buf.as_ptr().add(self.pos) }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_02() {
        let mut cursor = ByteCursor::new(vec![1, 2, 3, 4]);
        cursor.advance(100);
        let val = cursor.peek_current();
        std::hint::black_box(val);
    }
}