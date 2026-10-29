//! # Pair 11 (R2 Mutant): Unsynchronized `UnsafeCell` buffer (`r2_03_unsync_buffer_mutant.rs`)
//!
//! Mutation: Replaced `Mutex<[u8; 8]>` with `UnsafeCell<[u8; 8]>` while keeping `unsafe impl Sync`.
//! Fails under Miri with `Data race detected between`.

use core::cell::UnsafeCell;

pub struct SyncSlotBuffer {
    slots: UnsafeCell<[u8; 8]>,
}

// UNSOUND MUTANT: `UnsafeCell<[u8; 8]>` mutated concurrently without synchronization.
unsafe impl Sync for SyncSlotBuffer {}

impl SyncSlotBuffer {
    pub fn new() -> Self {
        Self {
            slots: UnsafeCell::new([0u8; 8]),
        }
    }

    pub fn write_slot(&self, index: usize, val: u8) {
        if index < 8 {
            // BUG: Unsynchronized concurrent write to shared slot!
            unsafe {
                let base = self.slots.get().cast::<u8>();
                *base.add(index) = val;
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

