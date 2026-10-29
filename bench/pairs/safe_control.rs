//! # Benchmark Fixture Safe Control: Verified sound bounds checking
//!
//! Control pattern: Safe function using `.get(index)` with Option check.
//! Under Miri, safe harness execution passes cleanly with exit code 0.
//! The verifier must deterministically classify this outcome as UNPROVEN (SoundSafe).

pub struct SafeBuffer {
    data: Vec<u8>,
}

impl SafeBuffer {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// SOUND: Bounds-checked access returning Option.
    pub fn read_elem(&self, index: usize) -> Option<u8> {
        self.data.get(index).copied()
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
    fn proof_safe_control() {
        let buffer = SafeBuffer::new(vec![10, 20, 30, 40]);

        // Attempt out-of-bounds index on safe implementation.
        // No Undefined Behavior or panics should occur; returns None.
        let val = buffer.read_elem(8);
        assert_eq!(val, None);
    }
}
