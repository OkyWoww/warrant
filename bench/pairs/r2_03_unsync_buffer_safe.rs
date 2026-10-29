//! # Pair 11 (R2 Safe): Mutex-synchronized shared buffer (`r2_03_unsync_buffer_safe.rs`)
//!
//! Synchronizes concurrent buffer writes with `std::sync::Mutex`.
//! Passes cleanly under Miri (Exit Code 0).

use std::sync::Mutex;

pub struct SyncSlotBuffer {
    slots: Mutex<[u8; 8]>,
}

// SOUND: Access to `slots` is serialized by `std::sync::Mutex`.
unsafe impl Sync for SyncSlotBuffer {}

impl SyncSlotBuffer {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new([0u8; 8]),
        }
    }

    pub fn write_slot(&self, index: usize, val: u8) {
        if index < 8 {
            if let Ok(mut guard) = self.slots.lock() {
                guard[index] = val;
            }
        }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_03_unsync_buffer() {
        let buf = SyncSlotBuffer::new();
        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..10 {
                    buf.write_slot(0, i);
                }
            });
            s.spawn(|| {
                for i in 0..10 {
                    buf.write_slot(0, i + 10);
                }
            });
        });
    }
}

