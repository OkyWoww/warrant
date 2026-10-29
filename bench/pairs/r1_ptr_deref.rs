//! # Benchmark Fixture R1 Pointer Dereference OOB Unsoundness
//!
//! Target pattern: Safe-facing struct wrapping a raw pointer with pointer arithmetic
//! (`ptr.add(offset)`), dereferencing without verifying that `offset < len`.
//! Under Miri, safe harness execution fails deterministically with:
//! `error: Undefined Behavior: dereferencing pointer failed: alloc... has size ... so pointer to 1 byte starting at offset ... is out-of-bounds`

// Target under audit: Unsound safe function wrapping raw pointer arithmetic
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

    /// UNSOUND: Marked safe, but performs unchecked raw pointer offset and dereference.
    pub fn read_offset(&self, offset: usize) -> u8 {
        // SAFETY: Invariant violated! Missing bounds check (offset < self.len).
        unsafe { *self.ptr.add(offset) }
    }
}

// ==============================================================================
// Safe Verification Harness
// Strictly adheres to WARRANT invariant: #![forbid(unsafe_code)] + extern crate std;
// ==============================================================================
#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r1_raw_ptr_oob() {
        let data = vec![10u8, 20, 30, 40];
        let buf = RawBuffer::new(&data);

        // Provoke out-of-bounds read using completely safe caller code.
        // Slice length is 4; offset 100 is far beyond allocation boundary.
        let val = buf.read_offset(100);

        std::hint::black_box(val);
    }
}
