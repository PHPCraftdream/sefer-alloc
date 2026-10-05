//! Same-thread Box churn across function-frame boundaries through the production
//! global allocator. The returned Box remains alive after its creating frame
//! returns, then its exact address is checked for reuse after it is dropped.

#![cfg(feature = "alloc-global")]

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn make_pattern(size: usize, seed: u8) -> Box<[u8]> {
    let mut bytes = vec![0_u8; size].into_boxed_slice();
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = seed.wrapping_add((index as u8).wrapping_mul(17));
    }
    bytes
}

fn check_pattern(bytes: &[u8], size: usize, seed: u8) {
    assert_eq!(bytes.len(), size);
    for (index, &byte) in bytes.iter().enumerate() {
        assert_eq!(
            byte,
            seed.wrapping_add((index as u8).wrapping_mul(17)),
            "size={size}, byte={index}"
        );
    }
}

fn check_zeroed(bytes: &[u8], size: usize) {
    assert_eq!(bytes.len(), size);
    for (index, &byte) in bytes.iter().enumerate() {
        assert_eq!(byte, 0, "zeroed size={size}, byte={index}");
    }
}

#[test]
fn by_value_box_frame_return_churn_preserves_bytes_and_zeroing() {
    for &size in &[17, 64, 257, 1024] {
        let seed = (size as u8).wrapping_mul(3).wrapping_add(1);
        let old = make_pattern(size, seed);

        // `make_pattern` has returned, so this checks caller-side survival of a
        // by-value Box result rather than only bytes in its creating frame.
        check_pattern(&old, size, seed);
        let old_address = old.as_ptr() as usize;
        drop(old);

        // Same-size allocation has the same layout; this single-threaded test
        // binary makes immediate local-magazine reuse deterministic (LIFO).
        let fresh = vec![0_u8; size].into_boxed_slice();
        assert_eq!(fresh.as_ptr() as usize, old_address, "size={size}");
        check_zeroed(&fresh, size);
        drop(fresh);

        let next = make_pattern(size, seed.wrapping_add(1));
        check_pattern(&next, size, seed.wrapping_add(1));
        drop(next);
    }
}
