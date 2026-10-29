//! # Benchmark Fixture R1 from_raw_parts Safe: Sound slice clamping
//!
//! Safe counterpart to r1_from_raw_parts.rs. Clamps `len` to `data.len()`.
//! Under Miri, execution passes cleanly with exit code 0.

pub struct SliceWindow {
    data: Vec<u8>,
}

impl SliceWindow {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// SOUND: Clamps length to actual allocated length.
    pub fn window(&self, len: usize) -> &[u8] {
        let safe_len = len.min(self.data.len());
        unsafe { std::slice::from_raw_parts(self.data.as_ptr(), safe_len) }
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
        let s = sw.window(128);
        assert_eq!(s.len(), 4);
    }
}
