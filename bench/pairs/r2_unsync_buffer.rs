//! # Benchmark Fixture R2 Unsync Buffer: Unsound Sync on raw pointer
//!
//! Target pattern: A struct holding raw pointer implements `Sync` without internal locking.
//! Concurrent writes from multiple threads cause data race.
//! Under Miri, concurrent writes fail deterministically with data race.

pub struct UnsyncBuffer {
    ptr: *mut u8,
}

impl UnsyncBuffer {
    pub fn new(slice: &mut [u8]) -> Self {
        Self {
            ptr: slice.as_mut_ptr(),
        }
    }

    pub fn write_at(&self, idx: usize, val: u8) {
        unsafe {
            *self.ptr.add(idx) = val;
        }
    }

    pub fn read_at(&self, idx: usize) -> u8 {
        unsafe { *self.ptr.add(idx) }
    }
}

// UNSOUND: raw pointer without synchronization is not Sync
unsafe impl Sync for UnsyncBuffer {}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r2_unsync_buffer_data_race() {
        let mut data = vec![0u8; 10];
        let buf = UnsyncBuffer::new(&mut data);

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
    }
}
