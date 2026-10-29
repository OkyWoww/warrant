#[cfg(test)]
mod warrant_proof {
    #![forbid(unsafe_code)]
    extern crate std;
    use super::*;

    #[test]
    fn sanity_r2_02_rc_send() {
        let handle = SharedHandle::new(42);
        let other = handle.clone_handle();
        assert_eq!(other.read(), 42);
        assert_eq!(handle.read(), 42);
    }
}
