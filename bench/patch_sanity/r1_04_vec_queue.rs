#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_04_vec_queue() {
        let mut q = RingQueue::new(4);
        q.advance_head(1);
        q.write_at_offset(2, 7);
        assert_eq!(q.buf[3], 7);
    }
}
