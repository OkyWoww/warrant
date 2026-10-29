#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_06_atomic_relaxed_race() {
        let chan = StatusChannel::new(0);
        chan.publish(111);
        chan.publish(222);
    }
}

