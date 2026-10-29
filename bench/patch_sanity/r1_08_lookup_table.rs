#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_08_lookup_table() {
        let lut = NibbleLookup::new();
        assert_eq!(lut.lookup(0), 0x00);
        assert_eq!(lut.lookup(15), 0xFF);
    }
}

