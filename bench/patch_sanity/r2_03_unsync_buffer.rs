#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_03_unsync_buffer() {
        // The type exposes no public getter, so this only checks that valid and out-of-range
        // writes complete without aborting (documented limitation in docs/LIMITS.md).
        let buf = SyncSlotBuffer::new();
        buf.write_slot(0, 1);
        buf.write_slot(7, 2);
        buf.write_slot(8, 3);
    }
}
