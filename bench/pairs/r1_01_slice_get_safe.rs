//! # Pair 01 (R1 Safe): Bounds-checked staged slice access (`r1_01_slice_get_safe.rs`)
//!
//! Staged state buffer that checks `index < self.data.len()` before executing
//! `get_unchecked(index)`. Passes cleanly under Miri (Exit Code 0).

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

    /// SOUND: Validates `idx < self.data.len()` before `get_unchecked`.
    pub fn read_at_offset(&self, offset: usize) -> Option<u8> {
        let idx = self.cursor.checked_add(offset)?;
        if idx < self.data.len() {
            // SAFETY: `idx < self.data.len()` is explicitly verified above.
            Some(unsafe { *self.data.get_unchecked(idx) })
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
    fn proof_r1_01_slice_get() {
        let mut buf = SliceBuffer::new();
        buf.push_frame(&[10, 20, 30, 40]);
        buf.advance(2);
        let val = buf.read_at_offset(6);
        assert_eq!(val, None);
    }
}

