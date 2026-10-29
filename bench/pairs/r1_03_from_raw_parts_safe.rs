//! # Pair 03 (R1 Safe): Bounds-checked `from_raw_parts` (`r1_03_from_raw_parts_safe.rs`)
//!
//! Validates `start + len <= self.buf.len()` before constructing slice via `from_raw_parts`.
//! Passes cleanly under Miri (Exit Code 0).

pub struct FrameView {
    buf: Vec<u8>,
}

impl FrameView {
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf }
    }

    /// SOUND: Verifies `start + len <= self.buf.len()` before `from_raw_parts`.
    pub fn slice_window(&self, start: usize, len: usize) -> Option<&[u8]> {
        let end = start.checked_add(len)?;
        if end <= self.buf.len() {
            // SAFETY: Range `start..end` is verified to lie within `self.buf`.
            Some(unsafe { std::slice::from_raw_parts(self.buf.as_ptr().add(start), len) })
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
    fn proof_r1_03_from_raw_parts() {
        let view = FrameView::new(vec![1, 2, 3, 4]);
        let win = view.slice_window(0, 64);
        assert_eq!(win, None);
    }
}

