//! # Benchmark Fixture R2 Unsync Buffer Safe: Sound Mutex protected buffer
//!
//! Safe counterpart to r2_unsync_buffer.rs. Uses Mutex to serialize writes.
//! Under Miri, concurrent execution passes cleanly with exit code 0.

use std::sync::Mutex;

pub struct UnsyncBuffer {
    data: Mutex<Vec<u8>>,
}

impl UnsyncBuffer {
    pub fn new(slice: &[u8]) -> Self {
        Self {
            data: Mutex::new(slice.to_vec()),
        }
    }

    pub fn write_at(&self, idx: usize, val: u8) {
        let mut lock = self.data.lock().unwrap();
        if idx < lock.len() {
            lock[idx] = val;
        }
    }

    pub fn read_at(&self, idx: usize) -> Option<u8> {
        let lock = self.data.lock().unwrap();
        lock.get(idx).copied()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_unsync_buffer_data_race() {
        let buf = UnsyncBuffer::new(&[0u8; 10]);

        std::thread::scope(|s| {
            s.spawn(|| {
                for _ in 0..100 {
                    buf.write_at(0, 42);
                }
            });
            s.spawn(|| {
                for _ in 0..100 {
                    buf.write_at(0, 99);
                }
            });
        });

        assert!(buf.read_at(0).is_some());
    }
}
