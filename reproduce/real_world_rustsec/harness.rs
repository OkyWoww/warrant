#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_smallvec_insert_many_oob() {
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.push(111);
        sv.push(222);
        // Inserting 6 elements exceeds capacity and entire struct allocation (total 8 > 2), writing OOB
        sv.insert_many(1, &[333, 444, 555, 666, 777, 888]);
        std::hint::black_box(sv.get(0));
    }
}