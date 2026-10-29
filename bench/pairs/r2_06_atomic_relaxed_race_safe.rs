//! Pair r2_06_atomic_relaxed_race (Safe): Atomic status channel with synchronized payload.
use std::sync::Mutex;

pub struct StatusChannel {
    payload: Mutex<u64>,
}

unsafe impl Sync for StatusChannel {}

impl StatusChannel {
    pub fn new(initial: u64) -> Self {
        Self {
            payload: Mutex::new(initial),
        }
    }

    pub fn publish(&self, val: u64) {
        if let Ok(mut guard) = self.payload.lock() {
            *guard = val;
        }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_06_atomic_relaxed_race() {
        let chan = StatusChannel::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                chan.publish(111);
            });
            s.spawn(|| {
                chan.publish(222);
            });
        });
    }
}

