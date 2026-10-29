//! # Pair 05 (R1 Safe): Bounds-checked `copy_nonoverlapping` (`r1_05_copy_nonoverlapping_safe.rs`)
//!
//! Staged packet assembler checking destination capacity `self.write_pos + src.len() <= 8`
//! before calling `std::ptr::copy_nonoverlapping`. Passes cleanly under Miri (Exit Code 0).

pub struct PacketAssembler {
    dst: [u8; 8],
    write_pos: usize,
}

impl PacketAssembler {
    pub fn new() -> Self {
        Self {
            dst: [0u8; 8],
            write_pos: 0,
        }
    }

    /// SOUND: Checks remaining capacity before `copy_nonoverlapping`.
    pub fn append_payload(&mut self, src: &[u8]) -> bool {
        let Some(end) = self.write_pos.checked_add(src.len()) else {
            return false;
        };
        if end > self.dst.len() {
            return false;
        }
        // SAFETY: `self.write_pos + src.len() <= self.dst.len()`, and `src` is disjoint from `self.dst`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.as_ptr(),
                self.dst.as_mut_ptr().add(self.write_pos),
                src.len(),
            );
        }
        self.write_pos = end;
        true
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_05_copy_nonoverlapping() {
        let mut pkt = PacketAssembler::new();
        assert!(pkt.append_payload(&[1, 2, 3, 4]));
        assert!(!pkt.append_payload(&[5u8; 16]));
    }
}

