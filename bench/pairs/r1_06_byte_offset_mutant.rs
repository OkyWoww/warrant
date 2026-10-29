//! # Pair 06 (R1 Mutant): Unchecked struct byte offset (`r1_06_byte_offset_mutant.rs`)
//!
//! Mutation: Removed `byte_offset < core::mem::size_of::<RawHeader>()` check before `.add(byte_offset)`.
//! Fails under Miri with `in-bounds pointer arithmetic failed`.

#[repr(C)]
pub struct RawHeader {
    pub magic: u32,
    pub version: u16,
    pub flags: u16,
}

pub struct HeaderInspector {
    header: RawHeader,
}

impl HeaderInspector {
    pub fn new(magic: u32, version: u16, flags: u16) -> Self {
        Self {
            header: RawHeader {
                magic,
                version,
                flags,
            },
        }
    }

    /// UNSOUND MUTANT: Missing `byte_offset < size_of::<RawHeader>()` check.
    pub fn read_header_byte(&self, byte_offset: usize) -> Option<u8> {
        let base = (&raw const self.header).cast::<u8>();
        // BUG: Offsets beyond the 8-byte struct allocation!
        Some(unsafe { *base.add(byte_offset) })
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_06_byte_offset() {
        let inspector = HeaderInspector::new(0xDEADBEEF, 1, 0);
        let b = inspector.read_header_byte(32);
        std::hint::black_box(b);
    }
}

