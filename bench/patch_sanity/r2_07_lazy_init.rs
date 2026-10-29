#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_07_lazy_init() {
        let lazy = LazyValue::new();
        lazy.init_or_set(10);
        lazy.init_or_set(20);
    }
}

