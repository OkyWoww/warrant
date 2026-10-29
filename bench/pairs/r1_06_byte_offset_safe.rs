//! # Pair 06 (R1 Safe): Bounds-checked struct byte offset (`r1_06_byte_offset_safe.rs`)
//!
//! Validates `byte_offset < core::mem::size_of::<RawHeader>()` before byte-level
//! pointer arithmetic on a `#[repr(C)]` struct. Passes cleanly under Miri (Exit Code 0).

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

    /// SOUND: Checks `byte_offset < size_of::<RawHeader>()` before `.add(byte_offset)`.
    pub fn read_header_byte(&self, byte_offset: usize) -> Option<u8> {
        if byte_offset < core::mem::size_of::<RawHeader>() {
            let base = (&raw const self.header).cast::<u8>();
            // SAFETY: `byte_offset < 8` and `RawHeader` has no uninitialized padding bytes.
            Some(unsafe { *base.add(byte_offset) })
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
    fn proof_r1_06_byte_offset() {
        let inspector = HeaderInspector::new(0xDEADBEEF, 1, 0);
        let b = inspector.read_header_byte(32);
        assert_eq!(b, None);
    }
}

