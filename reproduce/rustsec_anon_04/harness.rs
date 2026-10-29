#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_04() {
        let container = SharedContainer::new(Cell::new(0u64));
        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..10 {
                    container.set_value(i);
                }
            });
            s.spawn(|| {
                for i in 0..10 {
                    container.set_value(i + 100);
                }
            });
        });
        std::hint::black_box(container.get_value());
    }
}