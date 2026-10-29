//! # Pair 10 (R2 Safe): Thread-safe `Arc<T>` handle (`r2_02_rc_send_safe.rs`)
//!
//! Uses `std::sync::Arc<usize>` for thread-safe reference counting across threads.
//! Passes cleanly under Miri (Exit Code 0).

use std::sync::Arc;

pub struct SharedHandle {
    inner: Arc<usize>,
}

// SOUND: `Arc<usize>` uses atomic reference counting and is safely Send + Sync.
unsafe impl Send for SharedHandle {}
unsafe impl Sync for SharedHandle {}

impl SharedHandle {
    pub fn new(val: usize) -> Self {
        Self {
            inner: Arc::new(val),
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

