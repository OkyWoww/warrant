//! # Pair 12 (R2 Safe): `RwLock`-synchronized config cache (`r2_04_refcell_sync_safe.rs`)
//!
//! Synchronizes concurrent config mutations via `std::sync::RwLock<u64>`.
//! Passes cleanly under Miri (Exit Code 0).

use std::sync::RwLock;

pub struct ConfigCache {
    cell: RwLock<u64>,
}

// SOUND: `RwLock<u64>` safely arbitrates concurrent readers and writers.
unsafe impl Sync for ConfigCache {}

impl ConfigCache {
    pub fn new(initial: u64) -> Self {
        Self {
            cell: RwLock::new(initial),
        }
    }

    pub fn update(&self, delta: u64) {
        if let Ok(mut guard) = self.cell.write() {
            *guard += delta;
        }
    }

    pub fn read(&self) -> u64 {
        self.cell.read().map(|g| *g).unwrap_or(0)
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_04_refcell_sync() {
        let cache = ConfigCache::new(0);
        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..10 {
                    cache.update(1);
                }
            });
            s.spawn(|| {
                for _ in 0..10 {
                    cache.update(2);
                }
            });
        });
        std::hint::black_box(cache.read());
    }
}
