//! # Benchmark Fixture R1 Pointer Deref Safe: Sound pointer offset checking
//!
//! Safe counterpart to r1_ptr_deref.rs. Validates `offset < len`.
//! Under Miri, execution passes cleanly with exit code 0.

pub struct RawBuffer {
    ptr: *const u8,
    len: usize,
}

impl RawBuffer {
    pub fn new(slice: &[u8]) -> Self {
        Self {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }

    /// SOUND: Checks offset < self.len before accessing
    pub fn read_offset(&self, offset: usize) -> Option<u8> {
        if offset < self.len {
            Some(unsafe { *self.ptr.add(offset) })
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
    fn proof_r1_raw_ptr_oob() {
        let data = vec![10u8, 20, 30, 40];
        let buf = RawBuffer::new(&data);
        let val = buf.read_offset(100);
        assert_eq!(val, None);
    }
}
