//! # Benchmark Fixture R2 Rc Sync: Unsound unsafe impl Send for Rc
//!
//! Target pattern: A type wrapping `Rc<T>` manually implements `Send`.
//! Because `Rc` refcounts are non-atomic, sharing across threads causes data race on refcount.
//! Under Miri, concurrent clones across thread boundary fail deterministically with data race.

use std::rc::Rc;

pub struct SharedRc {
    pub inner: Rc<String>,
}

impl SharedRc {
    pub fn new(val: String) -> Self {
        Self {
            inner: Rc::new(val),
        }
    }
}

// UNSOUND: Rc is not Send
unsafe impl Send for SharedRc {}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_rc_data_race() {
        let rc = SharedRc::new("hello".to_string());

        std::thread::spawn(move || {
            let _ = rc.inner.clone();
        });
    }
}
