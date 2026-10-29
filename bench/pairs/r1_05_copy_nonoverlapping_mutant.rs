//! # Pair 05 (R1 Mutant): Unchecked `copy_nonoverlapping` (`r1_05_copy_nonoverlapping_mutant.rs`)
//!
//! Mutation: Removed the `if end > self.dst.len()` capacity check before `copy_nonoverlapping`.
//! Fails under Miri with `memory access failed`.

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

    /// UNSOUND MUTANT: Copies `src.len()` bytes without checking destination buffer capacity.
    pub fn append_payload(&mut self, src: &[u8]) -> bool {
        let Some(end) = self.write_pos.checked_add(src.len()) else {
            return false;
        };
        // BUG: Missing `if end > self.dst.len() { return false; }` check!
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
        pkt.append_payload(&[1, 2, 3, 4]);
        pkt.append_payload(&[5u8; 16]);
    }
}

