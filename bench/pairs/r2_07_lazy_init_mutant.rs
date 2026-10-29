//! Pair r2_07_lazy_init (Mutant): Unsynchronized lazy initialization race on UnsafeCell.
use core::cell::UnsafeCell;

pub struct LazyValue {
    slot: UnsafeCell<Option<u64>>,
}

// MUTATION: Unsound `unsafe impl Sync` without lock or atomic state machine for initialization.
unsafe impl Sync for LazyValue {}

impl LazyValue {
    pub const fn new() -> Self {
        Self {
            slot: UnsafeCell::new(None),
        }
    }

    pub fn init_or_set(&self, val: u64) {
        // BUG: Data race when two threads initialize `slot` concurrently.
        unsafe {
            *self.slot.get() = Some(val);
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
