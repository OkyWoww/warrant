//! # Benchmark Fixture R3 Anonymized: Aliasing violation via unconstrained slice lifetime
//!
//! Target pattern: Returning `&'a mut [T]` from raw pointer without coupling lifetime
//! to the backing allocation, allowing two mutable references to the same memory.

pub mod target_lib {
    pub struct SplitView {
        buffer: Vec<u8>,
    }

    impl SplitView {
        pub fn new(size: usize) -> Self {
            Self {
                buffer: vec![0u8; size],
            }
        }

        // UNSOUND: Elided/unconstrained lifetime on mutable slice permits overlapping &mut borrows.
        pub fn get_view_mut<'a>(&self) -> &'a mut [u8] {
            let ptr = self.buffer.as_ptr() as *mut u8;
            unsafe { std::slice::from_raw_parts_mut(ptr, self.buffer.len()) }
        }
    }
}

// ==============================================================================
// Safe Verification Harness
// Invariant: #![forbid(unsafe_code)] + extern crate std;
// ==============================================================================
#![forbid(unsafe_code)]
extern crate std;

use target_lib::SplitView;

fn main() {
    let view = SplitView::new(16);

    // Two active mutable slices to identical memory location created safely
    let s1: &mut [u8] = view.get_view_mut();
    let s2: &mut [u8] = view.get_view_mut();

    s1[0] = 42;
    s2[0] = 99;

    std::hint::black_box(s1[0]);
}
