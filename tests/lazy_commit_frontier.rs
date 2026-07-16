//! B1 (R7 Workstream B): tests for the `committed_payload_end` frontier and
//! the lazy fresh-segment reservation path.
//!
//! Feature-gated: `alloc-lazy-commit` (which implies `alloc-core`).
//! All tests verify:
//!   - The frontier is set correctly at segment creation (lazy vs eager).
//!   - The header layout / size asserts hold (feature-OFF unchanged;
//!     feature-ON with the new field passes its own asserts).
//!   - An alloc from a freshly-lazy-reserved segment works (the first chunk
//!     is committed and writable) and does NOT fault.
//!   - Feature-OFF: `committed_payload_end == SEGMENT` (the entire payload).
//!   - Unix/miri: eager (identical to feature-OFF).

#![cfg(feature = "alloc-lazy-commit")]
#![cfg_attr(
    feature = "numa-aware",
    allow(
        unused_variables,
        unused_mut,
        dead_code,
        unused_imports,
        clippy::needless_return
    )
)]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The segment size constant (4 MiB).
const SEGMENT: usize = SegmentLayout::SEGMENT;

/// The metadata end offset (from `SegmentLayout`).
fn small_meta_end() -> usize {
    SegmentLayout::SMALL_META_END
}

// ── Frontier correctness at segment creation ──────────────────────────────

/// A fresh small segment (created by a carve that exhausts the primordial)
/// has `committed_payload_end` set to the expected value:
///   - On the lazy path (Windows, not numa-aware): `meta_end + LAZY_FIRST_CHUNK`
///   - On the eager path (Unix, miri, numa-aware): `SEGMENT`
#[test]
fn fresh_small_segment_frontier_is_correct() {
    let mut a = AllocCore::new().unwrap();
    // The primordial segment is always eager — its frontier must be SEGMENT.
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_frontier = a.dbg_committed_payload_end_for(prim_ptr).unwrap();
    assert_eq!(
        prim_frontier, SEGMENT,
        "primordial segment must have committed_payload_end == SEGMENT (always eager)"
    );

    // Exhaust the primordial by allocating many blocks until a new segment
    // is reserved. We detect the segment switch by checking when the segment
    // base changes.
    let prim_base = (prim_ptr as usize) & !(SEGMENT - 1);
    let mut second_seg_ptr = core::ptr::null_mut();
    for _ in 0..500_000 {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        let this_base = (p as usize) & !(SEGMENT - 1);
        if this_base != prim_base {
            second_seg_ptr = p;
            break;
        }
    }
    assert!(
        !second_seg_ptr.is_null(),
        "failed to trigger a second segment reservation"
    );

    let frontier = a.dbg_committed_payload_end_for(second_seg_ptr).unwrap();

    // Determine the expected frontier based on the platform:
    // - Windows (not miri): lazy path → meta_end + LAZY_FIRST_CHUNK
    // - Unix / miri: eager fallback → SEGMENT
    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let expected = small_meta_end() + 256 * 1024; // LAZY_FIRST_CHUNK = 256 KiB
        assert_eq!(
            frontier, expected,
            "fresh small segment (Windows lazy path) must have \
             committed_payload_end == meta_end + LAZY_FIRST_CHUNK ({expected}), \
             got {frontier}"
        );
    }
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        assert_eq!(
            frontier, SEGMENT,
            "fresh small segment (Unix/miri eager path) must have \
             committed_payload_end == SEGMENT, got {frontier}"
        );
    }
}

// ── Alloc from a lazily-reserved segment works ────────────────────────────

/// Allocating from a freshly-lazy-reserved segment must not fault: the first
/// chunk is committed and writable. We do a simple alloc + write + read
/// roundtrip.
#[test]
fn alloc_from_lazy_segment_is_writable() {
    let mut a = AllocCore::new().unwrap();
    // Exhaust the primordial to get a second segment.
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_base = (prim_ptr as usize) & !(SEGMENT - 1);

    let mut ptrs = Vec::new();
    loop {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        let this_base = (p as usize) & !(SEGMENT - 1);
        if this_base != prim_base {
            ptrs.push(p);
            break;
        }
    }

    // Allocate more blocks from the second segment to exercise the first chunk.
    for _ in 0..1000 {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        ptrs.push(p);
    }

    // Write + read each block — must not fault.
    for &p in &ptrs {
        // SAFETY: `p` is a valid 16-byte allocation.
        unsafe {
            p.write(0xAB);
            assert_eq!(p.read(), 0xAB, "block at {p:p} not writable/readable");
        }
    }
}

// ── Multiple size classes from a lazy segment ─────────────────────────────

/// Allocate several different size classes from a fresh lazy segment to
/// exercise the bump cursor across different block sizes within the first chunk.
#[test]
fn multiple_classes_from_lazy_segment() {
    let mut a = AllocCore::new().unwrap();
    // Exhaust the primordial.
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_base = (prim_ptr as usize) & !(SEGMENT - 1);

    // Allocate until we get a new segment.
    loop {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        if (p as usize) & !(SEGMENT - 1) != prim_base {
            break;
        }
    }

    // Now allocate various sizes from the new segment.
    let sizes = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
    let mut ptrs = Vec::new();
    for &size in &sizes {
        let p = a.alloc(Layout::from_size_align(size, 8).unwrap());
        assert!(!p.is_null(), "alloc({size}) returned null");
        ptrs.push((p, size));
    }

    // Write + read each block.
    for &(p, size) in &ptrs {
        // SAFETY: `p` is valid for `size` bytes.
        unsafe {
            for i in 0..size {
                p.add(i).write(0x5C);
            }
            for i in 0..size {
                assert_eq!(
                    p.add(i).read(),
                    0x5C,
                    "byte {i} of {size}-byte block at {p:p} corrupt"
                );
            }
        }
    }
}

// ── Header size / layout assert sanity ────────────────────────────────────

/// The header layout asserts in segment_header.rs are compile-time; this test
/// simply verifies at runtime that the observable values haven't drifted.
/// The compile-time asserts in segment_header.rs already guarantee these
/// invariants; the const asserts here pin them from the test side.
#[test]
fn header_layout_asserts_hold() {
    // Runtime checks that match the crate's compile-time asserts.
    let meta_end = SegmentLayout::SMALL_META_END;
    assert_ne!(meta_end, 0, "small_meta_end must be non-zero");
    assert!(
        meta_end + 4096 <= SEGMENT,
        "small_meta_end ({meta_end}) + PAGE must fit within SEGMENT ({SEGMENT})"
    );
}

// ── Feature-OFF equivalence ───────────────────────────────────────────────
// (This test runs ONLY when `alloc-lazy-commit` is ON, but verifies that the
// primordial — which is always eager — has the full-span frontier.)

#[test]
fn eager_segment_has_full_span_frontier() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    // The primordial is always eager.
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    assert_eq!(
        frontier, SEGMENT,
        "eager (primordial) segment must have full-span frontier"
    );
}
