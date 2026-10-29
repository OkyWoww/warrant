//! # Pair 08 (R1 Mutant): Unmasked lookup table index (`r1_08_lookup_table_mutant.rs`)
//!
//! Mutation: Removed `& 0x0F` bitmask sanitization before `self.table.get_unchecked(idx)`.
//! Fails under Miri with `slice::get_unchecked requires that the index is within the slice`.

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

    /// UNSOUND MUTANT: Passes unsanitized `raw_index` directly to `get_unchecked`.
    pub fn lookup(&self, raw_index: usize) -> u32 {
        // BUG: Missing `& 0x0F` mask!
        unsafe { *self.table.get_unchecked(raw_index) }
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
        std::hint::black_box(val);
    }
}

