#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_01() {
        let mut buf = CompactBuffer::new();
        buf.push(10u32);
        buf.push(20);
        buf.push(30);
        // Inserting 8 items exceeds capacity and entire struct allocation (total 11 > 4), triggering OOB
        buf.insert_slice(1, &[100, 200, 300, 400, 500, 600, 700, 800]);
        std::hint::black_box(buf.get(0));
    }
}