//! # Benchmark Fixture R2 Rc Sync Safe: Sound Arc wrapper
//!
//! Safe counterpart to r2_rc_sync.rs. Uses Arc<String> which is natively Send + Sync.
//! Under Miri, concurrent execution passes cleanly with exit code 0.

use std::sync::Arc;

pub struct SharedRc {
    pub inner: Arc<String>,
}

impl SharedRc {
    pub fn new(val: String) -> Self {
        Self {
            inner: Arc::new(val),
        }
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_rc_data_race() {
        let rc = SharedRc::new("hello".to_string());

        let handle = std::thread::spawn(move || {
            let cloned = rc.inner.clone();
            cloned.len()
        });

        assert_eq!(handle.join().unwrap(), 5);
    }
}
