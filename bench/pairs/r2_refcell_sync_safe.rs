//! # Benchmark Fixture R2 RefCell Sync Safe: Sound RwLock protected counter
//!
//! Safe counterpart to r2_refcell_sync.rs. Uses RwLock instead of RefCell.
//! Under Miri, concurrent execution passes cleanly with exit code 0.

use std::sync::RwLock;

pub struct SharedRefCell {
    counter: RwLock<u64>,
}

impl SharedRefCell {
    pub fn new(val: u64) -> Self {
        Self {
            counter: RwLock::new(val),
        }
    }

    pub fn add(&self, delta: u64) {
        let mut lock = self.counter.write().unwrap();
        *lock += delta;
    }

    pub fn value(&self) -> u64 {
        let lock = self.counter.read().unwrap();
        *lock
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_refcell_data_race() {
        let sc = SharedRefCell::new(0);

        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..100 {
                    sc.add(1);
                }
            });
            s.spawn(|| {
                for _ in 0..100 {
                    sc.add(1);
                }
            });
        });

        assert_eq!(sc.value(), 200);
    }
}
