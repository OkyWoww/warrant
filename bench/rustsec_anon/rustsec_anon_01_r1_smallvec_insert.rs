//! # RustSec Anonymized 01: Buffer expansion and element shifting bug (R1 UbOob)
//!
//! Self-contained host-compilable reproduction adapted from historical RustSec advisory patterns (RUSTSEC-2021-0003, `smallvec` `>= 0.6.3, < 0.6.14`, MIT/Apache-2.0).
//! In `CompactBuffer`, `insert_slice` shifts elements to the right before ensuring
//! sufficient backing capacity has been allocated.
//! Fails under Miri with memory access / pointer arithmetic out-of-bounds.

pub struct CompactBuffer<T> {
    data: [T; 4],
    len: usize,
}

impl<T: Copy + Default> CompactBuffer<T> {
    pub fn new() -> Self {
        Self {
            data: [T::default(); 4],
            len: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.len < 4 {
            self.data[self.len] = item;
            self.len += 1;
        }
    }

    /// UNSOUND HISTORICAL PATTERN: Shifts elements without checking if `self.len + items.len() <= 4`.
    pub fn insert_slice(&mut self, index: usize, items: &[T]) {
        if index <= self.len {
            let shift_count = self.len - index;
            let insert_count = items.len();
            // BUG: Elements are shifted past the 4-element inline buffer allocation!
            unsafe {
                let base = self.data.as_mut_ptr();
                std::ptr::copy(base.add(index), base.add(index + insert_count), shift_count);
                std::ptr::copy_nonoverlapping(items.as_ptr(), base.add(index), insert_count);
            }
            self.len += insert_count;
        }
    }

    pub fn get(&self, idx: usize) -> Option<T> {
        if idx < self.len {
            Some(self.data[idx])
        } else {
            None
        }
    }
}

impl<T: Copy + Default> Default for CompactBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_rustsec_anon_01() {
        let mut buf = CompactBuffer::new();
        buf.push(10u32);
        buf.push(20);
        buf.push(30);
        // Inserting 8 items exceeds capacity and entire struct allocation (total 11 > 4), triggering OOB
        buf.insert_slice(1, &[100, 200, 300, 400, 500, 600, 700, 800]);
        std::hint::black_box(buf.get(0));
    }
}
