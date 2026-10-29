#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_02_ptr_arithmetic() {
        let mut cursor = RawCursor::new(vec![100, 200, 300, 400]);
        cursor.seek(1);
        assert_eq!(cursor.peek_offset(2), Some(400));
        assert_eq!(cursor.peek_offset(0), Some(200));
    }
}
