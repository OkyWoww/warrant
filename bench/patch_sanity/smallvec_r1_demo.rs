#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_smallvec_r1_demo() {
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.push(111);
        sv.insert_many(0, &[999]);
        assert_eq!(sv.get(0), Some(999));
        assert_eq!(sv.get(1), Some(111));
        assert_eq!(sv.len(), 2);
    }
}

