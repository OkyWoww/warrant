//! # Pair 02 (R1 Safe): Bounds-checked pointer arithmetic (`r1_02_ptr_arithmetic_safe.rs`)
//!
//! Validates pointer offset against slice length before `.add()` and dereference.
//! Passes cleanly under Miri (Exit Code 0).

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

    /// SOUND: Verifies `target < self.buf.len()` prior to pointer offset and read.
    pub fn peek_offset(&self, offset: usize) -> Option<u32> {
        let target = self.pos.checked_add(offset)?;
        if target < self.buf.len() {
            // SAFETY: `target` is strictly within the allocated slice bounds.
            Some(unsafe { *self.buf.as_ptr().add(target) })
        } else {
            None
        }
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
        assert_eq!(val, None);
    }
}

