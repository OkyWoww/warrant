//! # Pair 07 (R1 Mutant): Unchecked trailing chunk slice (`r1_07_chunk_reader_mutant.rs`)
//!
//! Mutation: Only checks `self.pos < self.buf.len()` instead of `end <= self.buf.len()`
//! on trailing incomplete frame before `get_unchecked(self.pos..end)`.
//! Fails under Miri with `slice::get_unchecked requires that the range is within the slice`.

pub struct ChunkReader {
    buf: Vec<u8>,
    pos: usize,
}

impl ChunkReader {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            pos: 0,
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// UNSOUND MUTANT: Checks `self.pos < self.buf.len()` instead of `end <= self.buf.len()`.
    pub fn next_chunk(&mut self, chunk_size: usize) -> Option<&[u8]> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let end = self.pos + chunk_size;
        // BUG: `end` can exceed `self.buf.len()` on the last incomplete chunk!
        let chunk = unsafe { self.buf.get_unchecked(self.pos..end) };
        self.pos = end;
        Some(chunk)
    }
}

#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn proof_r1_07_chunk_reader() {
        let mut reader = ChunkReader::new();
        reader.feed(&[1, 2, 3, 4, 5]);
        let _first = reader.next_chunk(4);
        let second = reader.next_chunk(4);
        std::hint::black_box(second);
    }
}

