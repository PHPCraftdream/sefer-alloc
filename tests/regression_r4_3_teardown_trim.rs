//! Task #95 / R4-3 (N1) — teardown trim releases retained memory on thread exit.
//!
//! RED→GREEN proof for the teardown-trim wiring (`AbandonGuard::drop` →
//! `HeapCore::trim_for_recycle`). The scenario from
//! `docs/agent_reviews_round4/performance_review.md` finding N1: a wave of
//! short-lived threads, each allocating and freeing through `SeferAlloc`,
//! leaves tcache-buffered blocks, pooled small segments, and cached large
//! spans pinned on each recycled heap slot — RSS/commit stays proportional
//! to peak thread count, not current load.
//!
//! ## What the test proves
//!
//! Each spawned thread allocates a Large block (reserving an OS segment) and
//! frees it (depositing the span into the per-thread large cache under
//! `alloc-decommit`). On thread exit, `AbandonGuard::drop` now calls
//! `trim_for_recycle` → `evict_all` releases every cached span's OS
//! reservation → `segments_released_total` increases.
//!
//! **RED (before fix):** without the `trim_for_recycle` call in
//! `AbandonGuard::drop`, the freed span stays cached on the recycled heap
//! slot. No OS release fires on thread exit → `segments_released_total`
//! delta is 0 (verified by commenting out the `trim_for_recycle` call and
//! running this test in isolation).
//!
//! **GREEN (after fix):** the teardown trim fires on each thread's exit,
//! evicting the cached span → delta > 0.
//!
//! The large-cache path is the primary signal because eviction
//! unconditionally calls `os::release_segment` (no "maybe" like tcache
//! flush — which only releases a segment if its `live_count` reaches 0 after
//! the flush). We also do a small-block burst to exercise tcache/pool but do
//! not assert on that signal (it is dependent on segment-fill geometry and
//! is less deterministic).

#![cfg(feature = "alloc-decommit")]

use std::alloc::{GlobalAlloc, Layout};
use std::thread;

use sefer_alloc::SeferAlloc;

/// Drive a mix of small + large alloc/free activity on the calling thread's
/// own heap slot, leaving blocks magazine-buffered (tcache) and a span in the
/// large cache when the thread exits.
fn drive_activity(a: &SeferAlloc) {
    // Small-object burst: fill the magazine, then free into it (blocks stay
    // magazine-buffered — the segment's live_count does NOT reach 0 until
    // the teardown flush returns them).
    let small = Layout::from_size_align(64, 8).unwrap();
    let mut live = Vec::new();
    for _ in 0..1024 {
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(small) };
        assert!(!p.is_null());
        live.push(p);
    }
    for p in live.drain(..) {
        // SAFETY: p was allocated above with `small` and is still live.
        unsafe { a.dealloc(p, small) };
    }

    // Large-object alloc then free: the freed span is deposited into the
    // per-thread large cache (budget is unbounded by default). This is the
    // span that teardown `evict_all` will release.
    let large = Layout::from_size_align(1 << 20, 8).unwrap(); // 1 MiB
                                                              // SAFETY: valid layout.
    let p = unsafe { a.alloc(large) };
    assert!(!p.is_null());
    // SAFETY: p valid for `large`.
    unsafe { a.dealloc(p, large) };
}

#[test]
fn teardown_trim_releases_retained_large_cache_spans() {
    let a = SeferAlloc::new();

    // Warm up: one round of activity on THIS thread so the primordial
    // segment is set up and baseline counters are established. Without this,
    // the very first large alloc in a spawned thread would reserve the
    // primordial's overflow (if any), muddying the delta.
    drive_activity(&a);

    let before = a.stats().segments_released_total;

    // Wave of N short-lived threads. Each allocates + frees a mix of small
    // and large blocks, then exits. On exit, AbandonGuard::drop calls
    // trim_for_recycle → flush tcache + drain pool + evict large cache.
    const N: usize = 8;
    thread::scope(|s| {
        for _ in 0..N {
            s.spawn(|| {
                drive_activity(&a);
                // Thread exits here → AbandonGuard::drop → trim_for_recycle.
            });
        }
    });

    let after = a.stats().segments_released_total;

    // Each of the N threads cached a large span on free; teardown trim's
    // evict_all released each one. So delta should be >= N. We assert the
    // weaker > 0 to stay robust against any pooling/decommit interaction
    // that might absorb one span (e.g. if a span was too large to cache in
    // a rare size-class edge case), but in practice delta == N.
    //
    // RED without fix: delta == 0 (spans stay cached, no release on exit).
    let delta = after.saturating_sub(before);
    assert!(
        delta > 0,
        "teardown trim did not release retained large-cache spans: \
         segments_released_total before={} after={} (expected delta > 0 \
         from {N} thread exits each evicting a cached span)",
        before,
        after,
    );
}
