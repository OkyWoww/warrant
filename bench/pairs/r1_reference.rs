//! # Benchmark Fixture R1 Reference: Out-of-bounds get_unchecked unsoundness
//!
//! Target pattern: A safe-facing function wrapping `get_unchecked` without
//! verifying that index < slice.len().
//! Under Miri, safe harness execution fails deterministically with `ub_oob`.

// Target under audit: Unsound safe function wrapping get_unchecked
pub struct FastBuffer {
    data: Vec<u8>,
}

impl FastBuffer {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// UNSOUND: Marked safe, but relies on get_unchecked without bounds checking.
    pub fn read_unchecked_elem(&self, index: usize) -> u8 {
        // SAFETY: Invariant violated! Author assumed caller would check index,
        // but function is not marked `unsafe fn`.
        unsafe { *self.data.get_unchecked(index) }
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
    fn proof_r1_next_frame_oob() {
        let buffer = FastBuffer::new(vec![10, 20, 30, 40]);

        // Provoke out-of-bounds read using completely safe caller code.
        // Length is 4; offset 8 is beyond allocation boundary.
        // Expected Miri failure:
        // "unsafe precondition(s) violated: slice::get_unchecked requires that the index is within the slice"
        let val = buffer.read_unchecked_elem(8);

        std::hint::black_box(val);
    }
}
