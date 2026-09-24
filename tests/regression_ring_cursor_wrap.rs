//! Cursor identity and exhaustion regressions for the real ring.
//! `u32::MAX` is now an ordinary value of the `u64` cursor, not a wrap.
//! A ring accepts at most `u64::MAX` reservations in its lifetime; it never
//! rebases or wraps, including after it has drained completely.

#![cfg(all(feature = "alloc-xthread", feature = "internals"))]

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT, RING_CAP};

fn ring_buffer() -> Box<[u8]> {
    let mut buf = vec![0u8; FOOTPRINT];
    assert!((buf.as_mut_ptr() as usize).is_multiple_of(8));
    buf.into_boxed_slice()
}

fn ring(buf: &mut [u8]) -> RemoteFreeRing {
    let base = buf.as_mut_ptr();
    // SAFETY: the caller owns a live, 8-byte-aligned FOOTPRINT-sized buffer.
    unsafe {
        RemoteFreeRing::init_test_buffer(base);
        RemoteFreeRing::over_test_buffer(base)
    }
}

#[test]
fn u32_boundary_is_not_a_cursor_wrap() {
    let mut buf = ring_buffer();
    let ring = ring(&mut buf);
    let start = u32::MAX as u64 - 2;
    ring.dbg_set_cursors(start, start);
    assert_eq!(ring.dbg_tail_guard_token(), start as u32);
    let entries = [16, 32, 48, 64, 80];
    for entry in entries {
        assert!(ring.push(entry).is_ok());
    }
    assert_eq!(ring.dbg_cursors(), (start, start + 5));
    assert_eq!(ring.dbg_tail_guard_token(), u32::MAX);
    let mut drained = Vec::new();
    // The u32 owner-cache shortcut is disabled beyond this boundary.
    let cache = ring.drain(|entry| drained.push(entry));
    assert_eq!(cache, 0);
    assert_ne!(ring.dbg_tail_guard_token(), cache);
    assert_eq!(drained, entries);
    assert_eq!(ring.dbg_cursors(), (start + 5, start + 5));
    assert!(ring.push(96).is_ok());
    assert_eq!(ring.drain(|entry| drained.push(entry)), 0);
    assert_eq!(drained.last(), Some(&96));
}

#[test]
fn capacity_is_exact_before_the_u32_boundary() {
    let mut buf = ring_buffer();
    let ring = ring(&mut buf);
    let start = u32::MAX as u64 - RING_CAP as u64 / 2;
    ring.dbg_set_cursors(start, start);
    for i in 0..RING_CAP {
        assert!(ring.push((i as u32 + 1) * 16).is_ok());
    }
    let (head, tail) = ring.dbg_cursors();
    assert_eq!(tail - head, RING_CAP as u64);
    assert!(ring.push(123_456).is_err());
    assert_eq!(ring.overflow_count(), 1);
    let mut got = Vec::new();
    ring.drain(|entry| got.push(entry));
    assert_eq!(got.len(), RING_CAP);
    assert_eq!(ring.dbg_cursors(), (tail, tail));
}

#[test]
fn cursor_exhaustion_never_wraps_or_reuses_a_stale_cas_value() {
    let mut buf = ring_buffer();
    let ring = ring(&mut buf);
    ring.dbg_set_cursors(u64::MAX - 2, u64::MAX - 2);
    assert!(ring.push(16).is_ok());
    assert!(ring.push(32).is_ok());
    assert_eq!(ring.dbg_cursors(), (u64::MAX - 2, u64::MAX));
    assert!(ring.push(48).is_err());
    assert!(ring.try_push_uncounted(48).is_err());
    let mut got = Vec::new();
    ring.drain(|entry| got.push(entry));
    assert_eq!(got, [16, 32]);
    assert_eq!(ring.dbg_cursors(), (u64::MAX, u64::MAX));
    assert!(ring.push(48).is_err(), "exhaustion persists after draining");
    assert_eq!(ring.dbg_cursors(), (u64::MAX, u64::MAX));
    assert_eq!(ring.overflow_count(), 2);
}
