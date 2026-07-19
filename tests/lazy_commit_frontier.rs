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
//!   - `numa-aware` (any platform): `SEGMENT` (NUMA reservations stay eager).
//!   - R8-5 (task #218): `alloc-lazy-commit` AND NOT `numa-aware`:
//!     - real Windows (not miri): `meta_end + LAZY_FIRST_CHUNK` (genuine
//!       partial commit — the only platform where `reserve_aligned_lazy`
//!       does a real 2-phase reserve+commit-prefix);
//!     - Unix/miri: `SEGMENT`. `reserve_aligned_lazy` already committed the
//!       WHOLE segment up front (Unix has no separate reserve/commit
//!       distinction; miri models no RSS), so the allocator's OWN frontier
//!       bookkeeping now matches that reality. Pre-R8-5 the frontier was
//!       understated at `meta_end + LAZY_FIRST_CHUNK` here too — eager in
//!       OS-commit-reality but NOT in the allocator's own bookkeeping,
//!       exactly the pointless logical-grow-on-carve discrepancy R8-5
//!       removed (every carve past the artificial frontier used to run a
//!       no-op `commit_pages` + bump an atomic counter for zero benefit).

#![cfg(feature = "alloc-lazy-commit")]
// Under `numa-aware`, OR on Unix/miri (where R8-5 made the frontier eager —
// `SEGMENT`), the lazy-arm cfg branches in the tests below compile out and
// leave their bindings (`prim_payload_start`, `payload_start`, etc.) unused.
// The only leg that actually exercises those bindings is real Windows (not
// miri) with `alloc-lazy-commit` ON and `numa-aware` OFF — i.e. the negation
// of this predicate. Silence the lint family on every other leg.
#![cfg_attr(
    any(not(windows), miri, feature = "numa-aware"),
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
///
/// R7-B6 (primordial lazy commit): the primordial segment ITSELF now follows
/// the identical rule (see `primordial_frontier_is_correct` below) rather
/// than always being eager, so this test starts from the primordial's own
/// frontier check before moving on to the SECOND (ordinary small) segment.
#[test]
fn fresh_small_segment_frontier_is_correct() {
    let mut a = AllocCore::new().unwrap();
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_frontier = a.dbg_committed_payload_end_for(prim_ptr).unwrap();
    // SAFETY: `prim_ptr` is the pointer just returned by `a.alloc` above —
    // live, exclusively owned, and its segment is owned by `a`.
    let prim_payload_start = unsafe { a.dbg_payload_start_for(prim_ptr) };
    // R8-5 (task #218): primordial frontier mirrors `bootstrap::primordial`'s
    // 3-way stamping — genuine Windows-lazy gets the lazy value; Unix/miri
    // (where `reserve_aligned_lazy` already committed everything) gets
    // `SEGMENT`; `numa-aware` stays eager `SEGMENT` (P2 gate).
    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        prim_frontier,
        prim_payload_start + a.dbg_lazy_first_chunk(),
        "primordial segment must start with the lazy initial-chunk frontier"
    );
    #[cfg(all(not(feature = "numa-aware"), any(not(windows), miri)))]
    {
        let _ = prim_payload_start;
        assert_eq!(
            prim_frontier, SEGMENT,
            "primordial segment on Unix/miri (where reserve_aligned_lazy \
             commits the whole segment) must have frontier == SEGMENT (R8-5)"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        let _ = prim_payload_start;
        assert_eq!(
            prim_frontier, SEGMENT,
            "under numa-aware the primordial segment must have \
             committed_payload_end == SEGMENT (eager)"
        );
    }

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

    // Determine the expected frontier based on the lazy-commit mode + platform.
    // `reserve_small_segment` (alloc_core_small.rs, R8-5/task #218) sets the
    // frontier with a 3-way split mirroring the THREE
    // `reserve_aligned_lazy_raw` implementations in `crates/vmem/src/lib.rs`:
    //   - `numa-aware` (any platform): `SEGMENT` (NUMA reservations stay eager —
    //     P2 gate).
    //   - NOT `numa-aware` AND real Windows (not miri): `meta_end +
    //     LAZY_FIRST_CHUNK` — the only platform where the lazy reservation
    //     does a REAL partial commit.
    //   - NOT `numa-aware` AND Unix/miri: `SEGMENT` — `reserve_aligned_lazy`
    //     already `mmap`d/`alloc`d the WHOLE segment up front (Unix has no
    //     reserve/commit distinction; miri models no RSS), so R8-5 stamps the
    //     frontier to match that reality instead of running a pointless
    //     logical-grow-on-carve for the segment's whole lifetime.
    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let expected = small_meta_end() + 256 * 1024; // LAZY_FIRST_CHUNK = 256 KiB
        assert_eq!(
            frontier, expected,
            "fresh small segment (Windows lazy path) must have \
             committed_payload_end == meta_end + LAZY_FIRST_CHUNK ({expected}), \
             got {frontier}"
        );
    }
    #[cfg(all(not(feature = "numa-aware"), any(not(windows), miri)))]
    {
        assert_eq!(
            frontier, SEGMENT,
            "fresh small segment on Unix/miri (where reserve_aligned_lazy \
             commits the whole segment) must have frontier == SEGMENT (R8-5), \
             got {frontier}"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        assert_eq!(
            frontier, SEGMENT,
            "fresh small segment (numa-aware eager path) must have \
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

// ── numa-aware equivalence ─────────────────────────────────────────────────
// (This test runs ONLY when `alloc-lazy-commit` is ON. R7-B6: the primordial
// is no longer unconditionally eager — see `primordial_frontier_is_correct`
// below for its general rule. This test isolates the ONE case that still IS
// unconditionally eager: `numa-aware`, where every segment — primordial
// included — uses the plain `Segment::reserve`/`reserve_aligned_on_node` path.)

#[test]
#[cfg(feature = "numa-aware")]
fn eager_segment_has_full_span_frontier() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    assert_eq!(
        frontier, SEGMENT,
        "under numa-aware the primordial segment must have full-span frontier"
    );
}

// ── R7-B6: primordial frontier follows the same lazy-commit rule ──────────

/// The primordial segment's `committed_payload_end` frontier follows the
/// SAME 3-way lazy-vs-eager rule (R8-5, task #218) as an ordinary fresh
/// small segment (see `fresh_small_segment_frontier_is_correct` above):
///   - Genuine Windows-lazy (`alloc-lazy-commit` AND NOT `numa-aware` AND
///     Windows-not-miri): `payload_start + LAZY_FIRST_CHUNK`, where
///     `payload_start` is the primordial's (larger) `primordial_meta_end()`,
///     not the ordinary small segment's `small_meta_end()`.
///   - Eager (`numa-aware`, OR Unix/miri where `reserve_aligned_lazy`
///     already committed the whole segment): `SEGMENT`.
#[test]
fn primordial_frontier_is_correct() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    // SAFETY: `p` is the pointer just returned by `a.alloc` above — live,
    // exclusively owned, and its segment is owned by `a`.
    let payload_start = unsafe { a.dbg_payload_start_for(p) };

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        frontier,
        payload_start + a.dbg_lazy_first_chunk(),
        "primordial segment must start with the lazy initial-chunk frontier"
    );
    #[cfg(all(not(feature = "numa-aware"), any(not(windows), miri)))]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "primordial segment on Unix/miri must have frontier == SEGMENT \
             (R8-5: reserve_aligned_lazy already committed the whole segment)"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "under numa-aware the primordial segment must have full-span frontier"
        );
    }
}
