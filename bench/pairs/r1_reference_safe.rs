//! # Benchmark Fixture R1 Reference Safe: Sound bounds checked buffer
//!
//! Safe counterpart to r1_reference.rs. Uses `.get(index)` with bounds check.
//! Under Miri, execution passes cleanly with exit code 0.

pub struct FastBuffer {
    data: Vec<u8>,
}

impl FastBuffer {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// SOUND: Performs checked access via .get()
    pub fn read_unchecked_elem(&self, index: usize) -> Option<u8> {
        self.data.get(index).copied()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_next_frame_oob() {
        let buffer = FastBuffer::new(vec![10, 20, 30, 40]);
        let val = buffer.read_unchecked_elem(8);
        assert_eq!(val, None);
    }
}
