//! # RustSec Anonymized 05: Unsound Sync on SPMC queue slot (R2 DataRace)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (RUSTSEC-2020-0008, `spmc`/`crossbeam-queue`, MIT/Apache-2.0).
//! `UnsyncQueue` implements `Sync` on a shared queue slot backed by `UnsafeCell<u64>`
//! without atomic synchronization, allowing concurrent producer and consumer threads to race on the slot.
//! Fails under Miri with `Data race detected between`.

use core::cell::UnsafeCell;

pub struct UnsyncQueue {
    slot: UnsafeCell<u64>,
}

// BUG: `unsafe impl Sync` on `UnsafeCell` permits unsynchronized concurrent read/write across threads.
unsafe impl Sync for UnsyncQueue {}

impl UnsyncQueue {
    pub const fn new(initial: u64) -> Self {
        Self {
            slot: UnsafeCell::new(initial),
        }
    }

    pub fn push(&self, val: u64) {
        unsafe {
            *self.slot.get() = val;
        }
    }

    pub fn pop(&self) -> u64 {
        unsafe {
            *self.slot.get()
        }
    }
}

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
