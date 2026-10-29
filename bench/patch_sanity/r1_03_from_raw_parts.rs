#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r1_03_from_raw_parts() {
        let view = FrameView::new(vec![1, 2, 3, 4]);
        assert_eq!(view.slice_window(1, 2), Some(&[2u8, 3][..]));
        assert_eq!(view.slice_window(0, 4), Some(&[1u8, 2, 3, 4][..]));
    }
}
