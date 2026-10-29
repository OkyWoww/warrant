#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_02() {
        let mut cursor = ByteCursor::new(vec![1, 2, 3, 4]);
        cursor.advance(100);
        let val = cursor.peek_current();
        std::hint::black_box(val);
    }
}