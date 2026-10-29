#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_01_cell_sync() {
        let counter = TelemetryCounter::new(0);
        counter.increment();
        counter.increment();
        assert_eq!(counter.get(), 2);
    }
}
