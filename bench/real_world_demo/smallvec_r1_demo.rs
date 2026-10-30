//! # Real-World Historical RustSec Benchmark: `smallvec` (RUSTSEC-2021-0003 / CVE-2021-25900)
//!
//! Self-contained simplified reproduction adapted from the buffer overflow class behind
//! RUSTSEC-2021-0003 / CVE-2021-25900 in SmallVec::insert_many (used as advisory pattern
//! inspiration; implemented as an inline 2-element fixed buffer without dynamic heap growth).
//! Original License: MIT / Apache-2.0 (Dual License)
//! Advisory: https://rustsec.org/advisories/RUSTSEC-2021-0003.html
//!
//! In affected versions of `smallvec`, calling `insert_many` or inserting elements
//! into an inline `SmallVec` calculated the copy offset before verifying that the
//! inline storage buffer had sufficient capacity to hold the additional elements,
//! leading to an out-of-bounds memory write via `ptr::copy`.

pub struct SmallVec<T> {
    data: [T; 2],
    len: usize,
}

impl<T: Copy + Default> SmallVec<T> {
    pub fn new() -> Self {
        Self {
            data: [T::default(); 2],
            len: 0,
        }
    }

    pub fn push(&mut self, val: T) {
        if self.len < 2 {
            self.data[self.len] = val;
            self.len += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// HISTORICAL VULNERABILITY (RUSTSEC-2021-0003):
    /// Shifts elements using raw pointer copy without expanding capacity when `len + elements.len() > 2`.
    pub fn insert_many(&mut self, index: usize, elements: &[T]) {
        assert!(index <= self.len);
        let num_items = elements.len();
        // BUG: `ptr::copy` shifts elements past the 2-element inline allocation!
        unsafe {
            let ptr = self.data.as_mut_ptr();
            std::ptr::copy(ptr.add(index), ptr.add(index + num_items), self.len - index);
            std::ptr::copy_nonoverlapping(elements.as_ptr(), ptr.add(index), num_items);
        }
        self.len += num_items;
    }

    pub fn get(&self, index: usize) -> Option<T> {
        if index < self.len {
            Some(self.data[index])
        } else {
            None
        }
    }
}

impl<T: Copy + Default> Default for SmallVec<T> {
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
    fn proof_smallvec_insert_many_oob() {
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.push(111);
        sv.push(222);
        // Inserting 6 elements exceeds capacity and entire struct allocation (total 8 > 2), writing OOB
        sv.insert_many(1, &[333, 444, 555, 666, 777, 888]);
        std::hint::black_box(sv.get(0));
    }

    #[test]
    fn sanity_smallvec_r1_demo() {
        // Multi-element insertion: all inserted elements must be preserved intact without silent truncation
        let mut sv: SmallVec<u64> = SmallVec::new();
        sv.insert_many(0, &[111, 222]);
        assert_eq!(sv.len(), 2);
        assert_eq!(sv.get(0), Some(111));
        assert_eq!(sv.get(1), Some(222));

        // Shift and insert single element into partially filled SmallVec
        let mut sv2: SmallVec<u64> = SmallVec::new();
        sv2.push(999);
        sv2.insert_many(0, &[888]);
        assert_eq!(sv2.len(), 2);
        assert_eq!(sv2.get(0), Some(888));
        assert_eq!(sv2.get(1), Some(999));
    }
}
