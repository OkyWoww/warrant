#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_08_alias_mut() {
        let acc = SharedAccumulator::new(0);
        acc.add(5);
        acc.add(10);
    }
}

