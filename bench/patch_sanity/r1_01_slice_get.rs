#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_01_slice_get() {
        let mut buf = SliceBuffer::new();
        buf.push_frame(&[10, 20, 30, 40]);
        buf.advance(1);
        assert_eq!(buf.read_at_offset(1), Some(30));
        assert_eq!(buf.read_at_offset(2), Some(40));
    }
}
