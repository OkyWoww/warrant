//! # RustSec Anonymized 04: Unsound Sync on Cell container (R2 DataRace)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (RUSTSEC-2020-0071, `lock_api`/`atomic-cell`, MIT/Apache-2.0).
//! `SharedContainer<T>` implements `Sync` without requiring `T: Sync`.
//! When instantiated with `Cell<u64>`, concurrent calls to `set` and `get` trigger a data race.
//! Fails under Miri with `Data race detected between`.

use core::cell::Cell;
use std::sync::Arc;

pub struct SharedContainer<T> {
    inner: Arc<T>,
}

// BUG: Missing `where T: Sync` bound! Only `T: Send` or unrestricted.
unsafe impl<T> Sync for SharedContainer<T> {}

impl<T> SharedContainer<T> {
    pub fn new(val: T) -> Self {
        Self {
            inner: Arc::new(val),
        }
    }
}

impl SharedContainer<Cell<u64>> {
    pub fn set_value(&self, val: u64) {
        self.inner.set(val);
    }

    pub fn get_value(&self) -> u64 {
        self.inner.get()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_04() {
        let container = SharedContainer::new(Cell::new(0u64));
        std::thread::scope(|s| {
            s.spawn(|| {
                for i in 0..10 {
                    container.set_value(i);
                }
            });
            s.spawn(|| {
                for i in 0..10 {
                    container.set_value(i + 100);
                }
            });
        });
        std::hint::black_box(container.get_value());
    }
}

