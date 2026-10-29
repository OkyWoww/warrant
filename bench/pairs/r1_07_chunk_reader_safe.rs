//! # Pair 07 (R1 Safe): Bounds-checked staged chunk reader (`r1_07_chunk_reader_safe.rs`)
//!
//! Staged frame reader using `checked_add` and `end <= self.buf.len()` before slicing
//! with `get_unchecked(self.pos..end)`. Passes cleanly under Miri (Exit Code 0).

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

    /// SOUND: Verifies `end <= self.buf.len()` before slicing `self.pos..end`.
    pub fn next_chunk(&mut self, chunk_size: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(chunk_size)?;
        if end > self.buf.len() {
            return None;
        }
        // SAFETY: `self.pos <= end <= self.buf.len()`.
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
        assert_eq!(reader.next_chunk(4), Some(&[1, 2, 3, 4][..]));
        assert_eq!(reader.next_chunk(4), None);
    }
}

