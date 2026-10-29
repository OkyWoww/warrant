//! # Pair 03 (R1 Mutant): Unchecked `from_raw_parts` length (`r1_03_from_raw_parts_mutant.rs`)
//!
//! Mutation: Removed `end <= self.buf.len()` check before `std::slice::from_raw_parts`.
//! Fails under Miri with `dereferencing pointer failed: ... is out-of-bounds`.

pub struct FrameView {
    buf: Vec<u8>,
}

impl FrameView {
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf }
    }

    /// UNSOUND MUTANT: Constructs slice of arbitrary caller-supplied length `len`.
    pub fn slice_window(&self, start: usize, len: usize) -> Option<&[u8]> {
        let _end = start.checked_add(len)?;
        // BUG: Missing `end <= self.buf.len()` check!
        Some(unsafe { std::slice::from_raw_parts(self.buf.as_ptr().add(start), len) })
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
        std::hint::black_box(win);
    }
}

