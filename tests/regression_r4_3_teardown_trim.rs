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
//!
//! ## Known flakiness under heavy system load (R16-6, task #316 — open question)
//!
//! On 2026-07-22, a personal-verification run of the FULL suite
//! (`cargo test --release --features production`) with two unrelated
//! `cargo clippy` builds racing in the background (high CPU contention) hit
//! this assertion once: `segments_released_total before=0 after=0`. Follow-up
//! investigation (R16-6, task #316) could not turn this into a reliable
//! reproducer:
//!
//! - `git stash` + the same run on the clean pre-Round-16 commit (`afa6b1d`)
//!   reproduced the same failure — so it predates Round 16's changes (#311–315),
//!   none of which touch the teardown-trim / large-cache production code.
//! - An isolated bisect run on `52bbb8a` (the commit before Round 16, where
//!   `npm run check` was last all-green) passed.
//! - 450+ direct invocations of this test's own binary in a tight loop, under
//!   sustained background `cargo clippy --all-features` load (two continuously
//!   re-running loaders, 16 logical CPUs, verified actually consuming CPU via
//!   their own build logs), all passed. Two additional full
//!   `cargo test --release --features production` runs under the same
//!   sustained load also passed.
//! - The production code was read end-to-end (`AbandonGuard::drop` →
//!   `HeapCore::trim_for_recycle` → `AllocCore::evict_all` →
//!   `evict_one_oldest` → `os::release_segment`): every step that could
//!   plausibly swallow a release was checked and ruled out. `release_segment`
//!   increments `SEGMENTS_RELEASED_TOTAL` unconditionally whenever
//!   `reservation` is non-null — there is no budget/policy branch that
//!   decommits-instead-of-releases on this path, and no early return that
//!   skips the counter. `MAX_HEAPS = 4096` rules out registry exhaustion from
//!   only 8 threads. `thread::scope`'s `join` is a full OS-level join (not
//!   just a return from the closure), so it cannot itself explain a trim that
//!   ran "too late" to be observed — the observation happens strictly after
//!   `scope` returns.
//! - One theoretical (unconfirmed, not reproduced) mechanism was identified
//!   in `src/global/tls_heap.rs`'s `finish_bind`: if `GUARD`'s
//!   `thread_local!` initialisation itself fails (`GUARD.try_with` returns
//!   `Err`, e.g. because this thread is already deep into its own teardown
//!   when a `HeapRegistry::claim` is triggered from another thread-local's
//!   `Drop`), the just-claimed slot is rolled back via `HeapRegistry::recycle`
//!   WITHOUT ever running `trim_for_recycle` (no guard was armed to call it).
//!   That is a narrow, pre-existing, and separately-reasoned-about edge case
//!   (see `finish_bind`'s own "UBFIX-10 (L-6)" doc comment) that does not
//!   match this test's straightforward `thread::scope` spawn pattern (no
//!   other thread-locals with `Drop` impls are in play here), so it was not
//!   pursued further within this task's time-box.
//!
//! **Honesty about what this means:** this looks like a genuine, rare,
//! load-sensitive flake rather than a reproducible defect — but the ~30–45
//! minute investigation window for this P3 task closed without a confirmed
//! root cause. If this assertion fails again, the useful next step is to
//! capture the failing run's exact conditions (concurrent load, thread
//! count, OS) rather than assume the mechanism above; do not silently loosen
//! the assertion (e.g. adding a retry) without first understanding why a
//! single well-defined trim-on-exit could ever legitimately fail to release
//! at least one of 8 independently cached spans.

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
    //
    // R6-OPT-P0-3a note: this was originally `1 << 20` (1 MiB) — under the
    // `medium-classes` feature (which raises SMALL_MAX to exactly 1 MiB, with
    // 1 MiB itself being the LARGEST new medium class), 1 MiB reclassifies
    // from Large to Small and never touches `large_cache` at all, making this
    // whole test vacuous (`segments_released_total` never advances via the
    // large-cache evict path this test exists to verify). Caught by running
    // `cargo test --all-features` against this file, which failed with delta
    // 0. `2 << 20` (2 MiB) stays genuinely Large in every feature
    // combination this crate can build.
    let large = Layout::from_size_align(2 << 20, 8).unwrap(); // 2 MiB
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
