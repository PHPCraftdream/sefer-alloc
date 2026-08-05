//! R17-8 (task #325) — deterministic, synchronous oracle for
//! `HeapCore::trim_for_recycle`'s large-cache eviction, added because the
//! external round-17 review judged `regression_r4_3_teardown_trim.rs`'s
//! documented (R16-6, task #316) load-sensitive flake insufficiently pinned
//! for allocator teardown: "a potential race is too dangerous to leave to a
//! multi-threaded, timing-dependent reproducer alone."
//!
//! ## Relationship to `regression_r4_3_teardown_trim.rs` — complements, does
//! ## not replace
//!
//! `regression_r4_3_teardown_trim.rs` covers the REAL thread-exit wiring: `N`
//! threads spawned via `thread::scope`, each exiting through the actual TLS
//! `AbandonGuard::drop` → `HeapCore::trim_for_recycle` path, proving the
//! production hook-up end to end (bind → claim → exit → recycle). That value
//! is real and this file does not attempt to duplicate it — TLS teardown
//! timing across 8 real OS threads is exactly the kind of coverage a
//! synchronous single-thread test structurally cannot provide. But that same
//! multi-thread `thread::scope` + TLS-destructor dependency is *also* the
//! documented source of R16-6's rare, unreproduced flake (450+ direct
//! reruns green, one failure observed only under heavy concurrent background
//! CPU load) — the flakiness is in the TLS-teardown TIMING, not in the
//! release mechanism itself.
//!
//! This file isolates the release mechanism from that timing dependency. It
//! calls `SeferAlloc::dbg_trim_current_thread` (`#[doc(hidden)] pub fn`,
//! `src/global/sefer_alloc.rs`) — a test/bench-only hook that invokes the
//! EXACT SAME production primitive (`HeapCore::trim_for_recycle`, task
//! #95/N1: flush tcache → drain small-segment pool → evict large cache)
//! directly on the calling thread, with no thread spawn, no `join`, no TLS
//! `Drop`, no registry-slot recycle — a plain synchronous call that returns
//! before the next line runs. There is no scheduler, no OS thread-exit
//! sequencing, and therefore no timing window left to be flaky: the
//! before/after `segments_released_total` read is separated by exactly one
//! deterministic function call on one thread.
//!
//! What this buys: a fast, 100%-reproducible regression oracle for the
//! release mechanism itself (`trim_for_recycle` → `evict_all` →
//! `os::release_segment`) — if a future change breaks eviction, THIS test
//! fails every single time, with no flake to chase. It does not, and is not
//! meant to, prove anything about TLS destructor ordering or multi-thread
//! recycle correctness; that remains
//! `regression_r4_3_teardown_trim.rs`'s job.
//!
//! ## Red/green counterfactual (verified by hand before commit)
//!
//! RED: with `HeapCore::trim_for_recycle`'s body temporarily replaced with a
//! no-op (both the `flush_all_tcache()` call and the `drain_small_pool()` /
//! `evict_all()` block removed), this test's assertion fails deterministically
//! on every run — `segments_released_total` delta is 0 because the freed
//! Large span stays parked in `large_cache` instead of being released.
//!
//! GREEN: with the real body restored, the delta is > 0 on every run, with
//! no observed flake across repeated invocations (unlike the
//! `thread::scope`-based sibling test, there is no timing window to flake
//! on).
//!
//! ## Feature gating
//!
//! `alloc-global` — required for `SeferAlloc`/`global` module to exist at
//! all (`#[cfg(feature = "alloc-global")]` on `pub mod global` in
//! `src/lib.rs`). `alloc-decommit` — required for the large-cache deposit/
//! evict path itself (`HeapCore::trim_for_recycle`'s
//! `drain_small_pool()`/`evict_all()` call is itself
//! `#[cfg(feature = "alloc-decommit")]`; without it a freed Large span is
//! released immediately on `dealloc`, never cached, making the before/after
//! delta this test asserts on structurally untestable — see the same
//! reasoning in `regression_r4_3_teardown_trim.rs`'s file-level
//! `#![cfg(feature = "alloc-decommit")]`).
//!
//! Run directly:
//!   cargo test --release --features "production alloc-decommit" \
//!       --test r17_8_deterministic_trim_releases_cached_large_span

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

#[test]
fn dbg_trim_current_thread_releases_cached_large_span_deterministically() {
    let a = SeferAlloc::new();

    // 2 MiB: comfortably Large under every feature combination this crate
    // can build. Mirrors `regression_r4_3_teardown_trim.rs`'s
    // `drive_activity` sizing rationale (R6-OPT-P0-3a): under
    // `medium-classes`, `SMALL_MAX` is raised to exactly 1 MiB, so a 1 MiB
    // block would reclassify from Large to Small and never touch
    // `large_cache` at all. `2 << 20` stays genuinely Large regardless of
    // which class-widening features are active.
    let large = Layout::from_size_align(2 << 20, 8).unwrap(); // 2 MiB

    // Warm-up round on THIS thread: establishes the primordial segment /
    // baseline heap state before the measured before/after snapshot, exactly
    // as `regression_r4_3_teardown_trim.rs` does — without this, the first
    // Large alloc below could reserve the primordial's overflow segment,
    // muddying the delta.
    // SAFETY: valid, non-zero-size layout.
    let warmup = unsafe { a.alloc(large) };
    assert!(!warmup.is_null(), "warm-up alloc failed");
    // SAFETY: warmup valid for `large`, freed exactly once.
    unsafe { a.dealloc(warmup, large) };
    a.dbg_trim_current_thread();

    let before = a.stats().segments_released_total;

    // Allocate then free the Large block: the freed span is deposited into
    // this thread's large cache (budget unbounded by default) rather than
    // released immediately — the exact condition `trim_for_recycle`'s evict
    // step exists to reclaim.
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(large) };
    assert!(!p.is_null(), "measured alloc failed");
    // SAFETY: p valid for `large`, freed exactly once.
    unsafe { a.dealloc(p, large) };

    // Deterministic, synchronous call — same production primitive
    // `AbandonGuard::drop` runs on real thread exit, invoked directly here
    // with no thread spawn/join and no TLS teardown in between.
    a.dbg_trim_current_thread();

    let after = a.stats().segments_released_total;

    // RED without a working trim_for_recycle: delta == 0 (the span stays
    // cached, no release happens). GREEN: the cached span is evicted, delta
    // > 0. No timing window exists between the two stats reads and the
    // trim call, so this must be reproducible on every run.
    assert!(
        after > before,
        "dbg_trim_current_thread() did not release the cached large span: \
         segments_released_total before={before} after={after} (expected \
         after > before from evicting the one span deposited above)"
    );
}
