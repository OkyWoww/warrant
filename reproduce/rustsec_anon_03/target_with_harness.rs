//! # RustSec Anonymized 03: Arena slice allocation beyond chunk bounds (R1 UbOob)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (RUSTSEC-2020-0056, `typed-arena`/`bumpalo`, MIT/Apache-2.0).
//! In `ChunkArena`, slice allocation calculates remaining capacity against an incorrect boundary,
//! constructing a slice reference that extends past the backing chunk allocation.
//! Fails under Miri with dangling reference going beyond bounds of allocation.

pub struct ChunkArena {
    storage: [u8; 32],
    allocated: usize,
}

impl ChunkArena {
    pub fn new() -> Self {
        Self {
            storage: [0u8; 32],
            allocated: 0,
        }
    }

    /// UNSOUND HISTORICAL PATTERN: Checks `self.allocated + len <= 64` instead of `<= 32`.
    pub fn alloc_slice(&mut self, len: usize) -> Option<&[u8]> {
        if self.allocated + len <= 64 {
            let start = self.allocated;
            self.allocated += len;
            // BUG: Constructs a slice extending past the 32-byte storage buffer!
            Some(unsafe { std::slice::from_raw_parts(self.storage.as_ptr().add(start), len) })
        } else {
            None
        }
    }
}

impl Default for ChunkArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_03() {
        let mut arena = ChunkArena::new();
        // Requesting 48 bytes exceeds the 32-byte storage buffer, triggering dangling slice UB
        let slice = arena.alloc_slice(48);
        std::hint::black_box(slice);
    }
}