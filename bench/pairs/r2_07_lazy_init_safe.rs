//! Pair r2_07_lazy_init (Safe): Thread-safe lazy initialization using Mutex.
use std::sync::Mutex;

pub struct LazyValue {
    slot: Mutex<Option<u64>>,
}

unsafe impl Sync for LazyValue {}

impl LazyValue {
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    pub fn init_or_set(&self, val: u64) {
        if let Ok(mut guard) = self.slot.lock() {
            *guard = Some(val);
        }
    }
}

impl Default for LazyValue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;

    use super::*;

    #[test]
    fn proof_r2_07_lazy_init() {
        let lazy = LazyValue::new();
        std::thread::scope(|s| {
            s.spawn(|| {
                lazy.init_or_set(10);
            });
            s.spawn(|| {
                lazy.init_or_set(20);
            });
        });
    }
}

