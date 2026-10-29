#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_03() {
        let mut arena = ChunkArena::new();
        // Requesting 48 bytes exceeds the 32-byte storage buffer, triggering dangling slice UB
        let slice = arena.alloc_slice(48);
        std::hint::black_box(slice);
    }
}