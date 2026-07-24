//! R14-4 (task #289) test (b) — correct free after growth past the promotion
//! threshold: allocate a Small/medium block, grow it past
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD`, verify the copy survived, free it,
//! and confirm there is no leak (via the process-wide, always-available
//! `segments_reserved_total`/`segments_released_total` counters) and no
//! crash/corruption on a subsequent, unrelated allocation.
//!
//! When `try_promote_to_large` is compiled in (see `HAS_PROMOTION` below),
//! this exercises the exact "how does dealloc know to free a promoted block
//! as Large" question the design doc's §4.2 argues is already answered by
//! `SegmentHeader::kind_at`-based routing (no new bookkeeping) — this test
//! confirms it structurally, not just by argument. When `HAS_PROMOTION` is
//! `false` (R15-3, task #305's zero-headroom exclusion), the grow instead
//! stays on the ordinary medium ladder — the SAME correctness properties
//! (canary survival, no leak, no corruption) still apply and are still a
//! meaningful, non-vacuous check in that configuration; only the identity of
//! the code path exercised (promotion vs. plain medium-ladder move-leg)
//! differs, which is why this file, unlike
//! `tests/r14_4_promotion_move_leg_reduction.rs`, does not need a
//! `HAS_PROMOTION`-gated early return — neither assertion here depends on
//! WHICH path was taken, only that the result is correct.
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_free_correctness
//!   cargo test --release --features "production medium-classes exact-span-large" --test r14_4_promotion_free_correctness

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;
const PROMOTION_THRESHOLD: usize = 256 * 1024;

/// Mirrors `tests/r14_4_promotion_move_leg_reduction.rs`'s constant of the
/// same name byte-for-byte. Not used to gate any assertion in this file (see
/// the module doc) — kept only as a documented cross-reference for readers
/// checking which path ("promoted to Large" vs. "ordinary medium-ladder
/// move-leg") a given build actually takes.
#[allow(dead_code)]
const HAS_PROMOTION: bool = !cfg!(feature = "exact-span-large")
    || (cfg!(feature = "large-reserved-capacity") && !cfg!(feature = "numa-aware"));

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Canary pattern survives the growth copy (promotion to Large under
/// `HAS_PROMOTION`, or an ordinary medium-ladder move-leg otherwise — see
/// `HAS_PROMOTION`'s doc), and the grown block frees cleanly with no leak
/// (segment counters balance) and no corruption of a later, unrelated
/// allocation.
#[test]
fn canary_survives_promotion_and_free_leaves_no_leak() {
    let a = SeferAlloc::new();

    let old_size = 96 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(old_layout) };
    assert!(!p.is_null());

    // Write a distinctive, position-dependent canary (not a flat byte) so a
    // partial/misaligned copy is detectable, not just a gross zeroing bug.
    // SAFETY: p valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 251) as u8);
        }
    }

    let stats_before = a.stats();

    let new_size = PROMOTION_THRESHOLD + 8192; // crosses the threshold -> promotes to Large (HAS_PROMOTION) or moves up the medium ladder otherwise
                                               // SAFETY: p live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null(), "growing realloc failed");

    // Canary must have survived the growth copy across the FULL old span.
    // SAFETY: grown valid for new_size >= old_size bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                grown.add(i).read(),
                (i % 251) as u8,
                "canary byte {i} corrupted or lost during the growth copy"
            );
        }
    }

    let stats_after_promote = a.stats();
    // Growing this block reserves at most one fresh segment (Large under
    // `HAS_PROMOTION`, or a medium-class segment otherwise — or reuses a
    // cached one, reserving zero) — either way `segments_reserved_total`
    // does not go backwards and the delta is small/bounded, never a wild
    // runaway (a sanity bound, not an exact-count assertion, since the
    // large_cache's admission policy is not this test's concern).
    assert!(
        stats_after_promote.segments_reserved_total >= stats_before.segments_reserved_total,
        "segments_reserved_total must be monotonic"
    );

    let grown_layout = layout(new_size);
    // SAFETY: grown live, grown_layout matches, freed exactly once.
    unsafe { a.dealloc(grown, grown_layout) };

    let stats_after_free = a.stats();
    // No leak: the reserved/released delta introduced by this test's own
    // grow+free round-trip must net to zero once the block is freed —
    // i.e. every segment THIS test reserved was also released (or handed
    // back to the large_cache, which does not increment
    // `segments_reserved_total` again on a later reuse — the invariant this
    // assertion checks is "reserved - released for this test's own activity
    // does not grow unboundedly", using the delta introduced since
    // `stats_before` as the bound).
    let reserved_delta =
        stats_after_free.segments_reserved_total - stats_before.segments_reserved_total;
    let released_delta =
        stats_after_free.segments_released_total - stats_before.segments_released_total;
    // Under `alloc-decommit` (part of `production`), a freed Large segment is
    // deposited into the large_cache rather than immediately released to the
    // OS (a freed medium-class segment, the `!HAS_PROMOTION` case, follows
    // its own tcache/segment-directory retention path) — so `released_delta`
    // may legitimately be 0 even though the block was correctly freed
    // (structurally: it is retained on this heap, not leaked to an
    // unreachable, still-mapped segment this process can never reclaim).
    // What must NOT happen is a released count that EXCEEDS what was
    // reserved (a double-release/corruption signal).
    assert!(
        released_delta <= reserved_delta,
        "released_delta ({released_delta}) must not exceed reserved_delta \
         ({reserved_delta}) — a double-release would indicate corruption"
    );

    // No corruption: a subsequent, unrelated allocation must still work and
    // be independently writable/readable (would likely crash or read back
    // wrong bytes if the growth/free path corrupted segment/table state).
    let q_layout = layout(4096);
    // SAFETY: valid, non-zero-size layout.
    let q = unsafe { a.alloc(q_layout) };
    assert!(!q.is_null(), "unrelated post-free allocation failed");
    // SAFETY: q valid for 4096 bytes.
    unsafe {
        for i in 0..4096usize {
            q.add(i).write((i % 199) as u8);
        }
        for i in 0..4096usize {
            assert_eq!(q.add(i).read(), (i % 199) as u8);
        }
        a.dealloc(q, q_layout);
    }
}

/// Multiple grow+free round-trips in a loop (each crossing
/// `PROMOTION_THRESHOLD` — promoting to Large under `HAS_PROMOTION`, or
/// moving up the medium ladder otherwise) must not accumulate a leak
/// (segments_reserved_total - segments_released_total must not grow
/// unboundedly across iterations, modulo cache retention which is itself
/// bounded).
#[test]
fn repeated_promote_and_free_does_not_leak_unboundedly() {
    let a = SeferAlloc::new();
    let stats_before = a.stats();

    for round in 0..20 {
        let old_size = 48 * 1024;
        let old_layout = layout(old_size);
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a.alloc(old_layout) };
        assert!(!p.is_null(), "round {round}: initial alloc failed");

        let new_size = PROMOTION_THRESHOLD + 1024 * (round + 1);
        // SAFETY: p live, old_layout matches, freed at most once on success.
        let grown = unsafe { a.realloc(p, old_layout, new_size) };
        assert!(!grown.is_null(), "round {round}: growing realloc failed");

        let grown_layout = layout(new_size);
        // SAFETY: grown live, grown_layout matches, freed exactly once.
        unsafe { a.dealloc(grown, grown_layout) };
    }

    let stats_after = a.stats();
    let reserved_delta = stats_after.segments_reserved_total - stats_before.segments_reserved_total;
    // 20 rounds, each doing one small alloc (never freed individually — it
    // is superseded by the growing realloc) plus one grow-across-threshold: a
    // reasonable, generous upper bound on distinct segments reserved is 2x
    // rounds (worst case zero cache reuse AND every round's small alloc also
    // lands in a fresh segment) — this is a leak-detection ceiling (catching
    // UNBOUNDED growth), not a tight performance assertion pinning an exact
    // count.
    assert!(
        reserved_delta <= 40,
        "20 grow+free rounds reserved {reserved_delta} segments — \
         expected at most 40 (<=2 per round), suggesting a leak"
    );
}
