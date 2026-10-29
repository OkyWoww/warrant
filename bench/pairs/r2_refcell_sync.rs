//! # Benchmark Fixture R2 RefCell Sync: Unsound unsafe impl Sync on RefCell
//!
//! Target pattern: A type wrapping `RefCell<u64>` implements `Sync`.
//! Because `RefCell` borrow counters are non-atomic, borrowing concurrently from multiple threads
//! triggers a data race on the borrow count.
//! Under Miri, concurrent borrows trigger data race.

use core::cell::RefCell;

pub struct SharedRefCell {
    counter: RefCell<u64>,
}

impl SharedRefCell {
    pub fn new(val: u64) -> Self {
        Self {
            counter: RefCell::new(val),
        }
    }

    pub fn add(&self, delta: u64) {
        *self.counter.borrow_mut() += delta;
    }

    pub fn value(&self) -> u64 {
        *self.counter.borrow()
    }
}

// UNSOUND: RefCell is not Sync
unsafe impl Sync for SharedRefCell {}

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

        std::hint::black_box(sc.value());
    }
}
