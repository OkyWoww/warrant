#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_05() {
        let q = UnsyncQueue::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..10 {
                    q.push(i);
                }
            });
            s.spawn(|| {
                for _ in 0..10 {
                    std::hint::black_box(q.pop());
                }
            });
        });
        std::hint::black_box(q.pop());
    }
}