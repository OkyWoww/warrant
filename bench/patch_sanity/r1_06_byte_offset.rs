#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_06_byte_offset() {
        let inspector = HeaderInspector::new(0x01020304, 1, 0);
        assert!(inspector.read_header_byte(0).is_some());
        assert!(inspector.read_header_byte(7).is_some());
    }
}

