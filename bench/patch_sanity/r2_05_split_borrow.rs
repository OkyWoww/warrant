#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_05_split_borrow() {
        let pair = ParallelPair::new(0, 0);
        pair.write_left(100);
        pair.write_right(200);
    }
}

