//! # Pair 08 (R1 Safe): Bitmask-sanitized lookup table (`r1_08_lookup_table_safe.rs`)
//!
//! Sanitizes caller index with `& 0x0F` bitmask before indexing 16-entry table
//! via `get_unchecked`. Passes cleanly under Miri (Exit Code 0).

pub struct NibbleLookup {
    table: [u32; 16],
}

impl NibbleLookup {
    pub fn new() -> Self {
        Self {
            table: [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
                0xEE, 0xFF,
            ],
        }
    }

    /// SOUND: Bitmask `& 0x0F` guarantees `idx < 16`.
    pub fn lookup(&self, raw_index: usize) -> u32 {
        let idx = raw_index & 0x0F;
        // SAFETY: `idx` is in `0..=15`, strictly within `[u32; 16]`.
        unsafe { *self.table.get_unchecked(idx) }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_08_lookup_table() {
        let lut = NibbleLookup::new();
        let val = lut.lookup(255);
        assert_eq!(val, 0xFF);
    }
}

