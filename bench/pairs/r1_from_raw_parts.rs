//! # Benchmark Fixture R1 from_raw_parts: Unsound slice reconstruction
//!
//! Target pattern: A safe function calling `std::slice::from_raw_parts` with a caller-supplied
//! length without validating that `len <= allocation_capacity`.
//! Under Miri, reading past the slice bounds fails with Undefined Behavior.

pub struct SliceWindow {
    data: Vec<u8>,
}

impl SliceWindow {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// UNSOUND: Reconstructs slice with unchecked caller-supplied length.
    pub fn window(&self, len: usize) -> &[u8] {
        // SAFETY: Invariant violated! Missing check `len <= self.data.len()`.
        unsafe { std::slice::from_raw_parts(self.data.as_ptr(), len) }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_from_raw_parts_oob() {
        let sw = SliceWindow::new(vec![1, 2, 3, 4]);
        // Allocation is 4 bytes; requesting 128 bytes creates out-of-bounds slice
        let s = sw.window(128);
        std::hint::black_box(s[100]);
    }
}
