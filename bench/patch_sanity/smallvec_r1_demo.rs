#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_smallvec_r1_demo() {
        // 1. Multi-element insertion: all inserted elements must be preserved intact without silent truncation
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.insert_many(0, &[111, 222]);
        assert_eq!(sv.len(), 2);
        assert_eq!(sv.get(0), Some(111));
        assert_eq!(sv.get(1), Some(222));

        // 2. Shift and insert single element into partially filled SmallVec
        let mut sv2: SmallVec<u64> = SmallVec::new();
        sv2.push(999);
        sv2.insert_many(0, &[888]);
        assert_eq!(sv2.len(), 2);
        assert_eq!(sv2.get(0), Some(888));
        assert_eq!(sv2.get(1), Some(999));
    }
}
