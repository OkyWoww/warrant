//! # Pair 12 (R2 Mutant): Unsound `Sync` on `RefCell<u64>` (`r2_04_refcell_sync_mutant.rs`)
//!
//! Mutation: Replaced `RwLock<u64>` with `core::cell::RefCell<u64>` while keeping `unsafe impl Sync`.
//! Fails under Miri with `Data race detected between` on `RefCell` borrow flag.

use core::cell::RefCell;

pub struct ConfigCache {
    cell: RefCell<u64>,
}

// UNSOUND MUTANT: `RefCell<u64>` tracks borrows with a non-atomic `Cell<isize>`.
unsafe impl Sync for ConfigCache {}

impl ConfigCache {
    pub fn new(initial: u64) -> Self {
        Self {
            cell: RefCell::new(initial),
        }
    }

    pub fn update(&self, delta: u64) {
        *self.cell.borrow_mut() += delta;
    }

    pub fn read(&self) -> u64 {
        *self.cell.borrow()
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
