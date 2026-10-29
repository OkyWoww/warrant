#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_05_copy_nonoverlapping() {
        let mut pkt = PacketAssembler::new();
        assert!(pkt.append_payload(&[1, 2, 3, 4]));
        assert!(pkt.append_payload(&[5, 6, 7, 8]));
    }
}

