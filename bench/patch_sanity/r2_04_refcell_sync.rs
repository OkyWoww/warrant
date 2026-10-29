#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_04_refcell_sync() {
        let cache = ConfigCache::new(5);
        cache.update(3);
        assert_eq!(cache.read(), 8);
    }
}
