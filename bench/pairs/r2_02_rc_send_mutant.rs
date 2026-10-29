//! # Pair 10 (R2 Mutant): Unsound `Send`/`Sync` on `Rc<T>` (`r2_02_rc_send_mutant.rs`)
//!
//! Mutation: Replaced `Arc<usize>` with `std::rc::Rc<usize>` while keeping `unsafe impl Send/Sync`.
//! Fails under Miri with `Data race detected between` on non-atomic reference count updates.

use std::rc::Rc;

pub struct SharedHandle {
    inner: Rc<usize>,
}

// UNSOUND MUTANT: `Rc<usize>` mutates strong_count via non-atomic `Cell<usize>`.
unsafe impl Send for SharedHandle {}
unsafe impl Sync for SharedHandle {}

impl SharedHandle {
    pub fn new(val: usize) -> Self {
        Self {
            inner: Rc::new(val),
        }
    }

    pub fn clone_handle(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    pub fn read(&self) -> usize {
        *self.inner
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_02_rc_send() {
        let handle = SharedHandle::new(42);
        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..10 {
                    let h = handle.clone_handle();
                    std::hint::black_box(h.read());
                }
            });
            s.spawn(|| {
                for _ in 0..10 {
                    let h = handle.clone_handle();
                    std::hint::black_box(h.read());
                }
            });
        });
    }
}

