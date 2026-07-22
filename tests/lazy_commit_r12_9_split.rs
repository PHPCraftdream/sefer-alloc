//! R12-9 (task #260): tests proving `primordial-lazy-commit` and
//! `small-segment-lazy-commit` are genuinely INDEPENDENT policies, not just
//! two names for the same behaviour.
//!
//! The two split features share ONE frontier mechanism (the
//! `committed_payload_end` field + B2 grow-on-carve + B3 decommit-aware
//! reuse — see `Cargo.toml`'s `alloc-lazy-commit` doc), but each one gates a
//! DIFFERENT reservation call site: `primordial-lazy-commit` gates
//! `bootstrap::primordial`'s `Segment::reserve_lazy` call;
//! `small-segment-lazy-commit` gates `reserve_small_segment`'s
//! `reserve_aligned_lazy` call. This file proves the isolation runs BOTH
//! ways:
//!   - `primordial-lazy-commit` ON, `small-segment-lazy-commit` OFF: the
//!     primordial segment is committed lazily, but a SECOND (ordinary small)
//!     segment is committed EAGERLY (full `SEGMENT` span) — exactly the
//!     pre-R12-9 `alloc-lazy-commit`-OFF behaviour for that segment.
//!   - `small-segment-lazy-commit` ON, `primordial-lazy-commit` OFF: the
//!     primordial segment is committed EAGERLY, but the second segment is
//!     committed lazily.
//!
//! Compiled ONLY under this exact combination (`primordial-lazy-commit` XOR
//! `small-segment-lazy-commit`, `alloc-decommit` OFF is irrelevant here so
//! not required) — running `cargo test --features "production
//! primordial-lazy-commit"` (without `small-segment-lazy-commit`) exercises
//! the first half; `--features "production small-segment-lazy-commit"`
//! (without `primordial-lazy-commit`) exercises the second half. Under the
//! combined `alloc-lazy-commit` alias (which enables both), this file is
//! entirely compiled out — `lazy_commit_frontier.rs` already covers the
//! "both together" case.
//!
//! Every assertion below is a no-op (never reached) unless the true genuine
//! Windows-lazy leg is live (`not(numa-aware)`, real `windows`, `not(miri)`)
//! — on every other leg `reserve_aligned_lazy` itself falls back to eager
//! (Unix/miri) or the caller forces eager (`numa-aware`), so BOTH policies
//! already collapse to `SEGMENT` and there is nothing to distinguish. The
//! `#[cfg_attr]` below silences the resulting unused-binding lints on those
//! legs, mirroring `lazy_commit_frontier.rs`'s identical discipline.

#![cfg(any(
    all(
        feature = "primordial-lazy-commit",
        not(feature = "small-segment-lazy-commit")
    ),
    all(
        feature = "small-segment-lazy-commit",
        not(feature = "primordial-lazy-commit")
    )
))]
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

/// Allocate from the primordial segment, then keep allocating until a SECOND
/// segment is reserved. Returns `(allocator, primordial_ptr,
/// primordial_frontier_snapshot, second_ptr)`.
///
/// The primordial frontier is snapshotted IMMEDIATELY after the very first
/// alloc — BEFORE the exhaustion loop below runs — because that loop's own
/// carving legitimately advances the frontier via B2 grow-on-carve as the
/// primordial segment fills up (expected behaviour, not a bug); reading the
/// frontier only AFTER exhaustion would observe that later, grown value
/// instead of the value `bootstrap::primordial` originally stamped.
fn alloc_primordial_then_second() -> (AllocCore, *mut u8, usize, *mut u8) {
    let mut a = AllocCore::new().unwrap();
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_base = (prim_ptr as usize) & !(SEGMENT - 1);
    let prim_frontier_snapshot = a.dbg_committed_payload_end_for(prim_ptr).unwrap();

    let mut second = core::ptr::null_mut();
    for _ in 0..500_000 {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        if (p as usize) & !(SEGMENT - 1) != prim_base {
            second = p;
            break;
        }
    }
    assert!(
        !second.is_null(),
        "failed to trigger a second segment reservation"
    );
    (a, prim_ptr, prim_frontier_snapshot, second)
}

/// `primordial-lazy-commit` ON, `small-segment-lazy-commit` OFF: the
/// primordial segment starts with the lazy initial-chunk frontier, but the
/// SECOND (ordinary small) segment is fully committed (`SEGMENT`) — proving
/// the two reservation call sites are genuinely independently gated, not
/// both controlled by one flag.
#[test]
#[cfg(all(
    feature = "primordial-lazy-commit",
    not(feature = "small-segment-lazy-commit")
))]
fn primordial_only_isolates_from_small_segment() {
    let (mut a, prim_ptr, prim_frontier, second_ptr) = alloc_primordial_then_second();

    // SAFETY: `prim_ptr` is a live pointer from the primordial segment,
    // exclusively owned, and its segment is owned by `a`.
    let prim_payload_start = unsafe { a.dbg_payload_start_for(prim_ptr) };

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        prim_frontier,
        prim_payload_start + a.dbg_lazy_first_chunk(),
        "primordial segment must be lazily committed under \
         primordial-lazy-commit (small-segment-lazy-commit OFF)"
    );
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = prim_payload_start;
        assert_eq!(
            prim_frontier, SEGMENT,
            "eager leg (Unix/miri/numa-aware): primordial frontier is SEGMENT \
             regardless of the primordial-lazy-commit feature"
        );
    }

    // The SECOND segment must be EAGERLY committed (SEGMENT), matching
    // pre-R12-9 `alloc-lazy-commit`-OFF behaviour for ordinary small
    // segments — small-segment-lazy-commit is OFF, so
    // `reserve_small_segment` must take the plain `Segment::reserve` path
    // regardless of platform.
    let second_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
    assert_eq!(
        second_frontier, SEGMENT,
        "ordinary small segment must stay EAGERLY committed when only \
         primordial-lazy-commit is on (small-segment-lazy-commit OFF) — \
         got {second_frontier}, expected SEGMENT ({SEGMENT})"
    );

    // The grow-on-carve commit counter must reflect ONLY the primordial's
    // own growth (if any, on the genuine Windows-lazy leg) — carving the
    // second (eagerly-committed) segment must never call `commit_pages`,
    // since its frontier starts (and stays) at SEGMENT.
    let count_before = a.dbg_grow_commit_count();
    for _ in 0..2000 {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
    }
    let count_after = a.dbg_grow_commit_count();
    assert_eq!(
        count_before, count_after,
        "carving further into the eagerly-committed second segment must \
         not trigger ANY grow-on-carve commit (its frontier is already \
         SEGMENT)"
    );
}

/// `small-segment-lazy-commit` ON, `primordial-lazy-commit` OFF: the
/// primordial segment is fully committed (`SEGMENT`) at bootstrap, but the
/// SECOND (ordinary small) segment starts with the lazy initial-chunk
/// frontier — the mirror-image isolation of the test above.
#[test]
#[cfg(all(
    feature = "small-segment-lazy-commit",
    not(feature = "primordial-lazy-commit")
))]
fn small_segment_only_isolates_from_primordial() {
    let (a, _prim_ptr, prim_frontier, second_ptr) = alloc_primordial_then_second();

    // The PRIMORDIAL segment must be EAGERLY committed (SEGMENT) — the
    // primordial-lazy-commit feature is OFF, so `bootstrap::primordial` must
    // take the plain `Segment::reserve` path regardless of platform. (Read
    // as the early snapshot the helper took right after the first alloc —
    // for this test's eager primordial, the value cannot change afterward
    // anyway, since grow-on-carve only fires when the frontier is BELOW
    // SEGMENT.)
    assert_eq!(
        prim_frontier, SEGMENT,
        "primordial segment must stay EAGERLY committed when only \
         small-segment-lazy-commit is on (primordial-lazy-commit OFF) — \
         got {prim_frontier}, expected SEGMENT ({SEGMENT})"
    );

    // The SECOND (ordinary small) segment follows the lazy rule.
    let second_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
    // SAFETY: `second_ptr` is the pointer returned by the helper above —
    // live, exclusively owned, and its segment is owned by `a`.
    let second_payload_start = unsafe { a.dbg_payload_start_for(second_ptr) };

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        second_frontier,
        second_payload_start + a.dbg_lazy_first_chunk(),
        "ordinary small segment must be lazily committed under \
         small-segment-lazy-commit (primordial-lazy-commit OFF)"
    );
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_payload_start;
        assert_eq!(
            second_frontier, SEGMENT,
            "eager leg (Unix/miri/numa-aware): second-segment frontier is \
             SEGMENT regardless of the small-segment-lazy-commit feature"
        );
    }
}
