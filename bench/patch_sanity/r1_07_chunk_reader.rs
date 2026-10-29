#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_07_chunk_reader() {
        let mut reader = ChunkReader::new();
        reader.feed(&[10, 20, 30, 40]);
        assert_eq!(reader.next_chunk(2), Some(&[10, 20][..]));
        assert_eq!(reader.next_chunk(2), Some(&[30, 40][..]));
    }
}

