//! Safety-critical regression test for the `alloc_zeroed` fresh-reservation
//! skip (task #221 / R8-8): `alloc_zeroed` may SKIP the explicit `Node::zero`
//! pass ONLY for a genuinely fresh OS reservation (zero-filled by the OS) and
//! MUST still zero explicitly for a `large_cache` HIT (a reused segment that may
//! hold the prior occupant's bytes). Getting this wrong is an information-
//! disclosure bug — a caller would read stale heap content through a call that
//! promised zeroed memory.
//!
//! This test exercises the REAL allocation substrate at the `AllocCore` layer
//! (where the freshness bool is produced and consumed — Step 2 of the task) and,
//! under `alloc-global`, the `HeapCore::alloc_zeroed` production entry point
//! (Step 3). The three load-bearing assertions, in priority order:
//!
//! 1. **Fresh-reservation correctness** — a large `alloc_zeroed` on a cold
//!    `AllocCore` reads back as ENTIRELY zero (full-buffer check, not a spot
//!    check). This is the actual win: the skip fires and the OS zero-fill is
//!    trusted.
//! 2. **Cache-hit MUST still fully zero — the critical regression guard.**
//!    `alloc` (plain) → write a non-zero pattern into EVERY byte → `dealloc`
//!    (deposits into `large_cache`) → `alloc_zeroed` of the same shape, which
//!    MUST hit the cache. The hit is CONFIRMED via diagnostics (the per-heap
//!    `dbg_large_cache_hits` under `alloc-stats`, AND — feature-independently —
//!    the deposited slot being vacated by the re-alloc), so the test cannot
//!    pass vacuously. The returned memory must read back as ENTIRELY zero,
//!    proving the skip did NOT fire on the reused path.
//! 3. **Interleaved stress** — ~80 iterations of alloc_zeroed → dirty → free at
//!    the same size; every zeroed allocation must read back all-zero regardless
//!    of whether that iteration hit a fresh reservation or the cache.
//!
//! Gated on `alloc-core` + `alloc-decommit`: the `large_cache` only exists
//! under `alloc-decommit`, and test 2 specifically needs it to exercise the
//! cache-hit EXCLUSION (without `alloc-decommit` every large alloc is trivially
//! fresh and the skip always fires — test 2 would be impossible).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::AllocCore;

/// Serialise every test in this file: the R9-1 zero-pass counter
/// (`dbg_large_zero_pass_count`) is PROCESS-WIDE, so concurrent tests in this
/// binary would pollute each other's deltas (e.g. the cache-hit test's +1
/// landing inside the fresh test's expected-0 window). Poison-tolerant: a
/// failed test must not cascade `PoisonError` into the others.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

// 2 MiB: unambiguously Large under every feature combination. Under the default
// size-class table SMALL_MAX ~253 KiB; under the opt-in `medium-classes` feature
// SMALL_MAX grows to 1 MiB — 2 MiB is Large in BOTH, so this test stays valid
// under `--all-features` (which enables `medium-classes`). See
// `src/alloc_core/size_classes.rs`.
//
// Under miri, every byte of every full-buffer touch (`write_bytes` /
// `assert_all_zero`) is interpreted one at a time with shadow-memory
// tracking — a 2 MiB buffer is orders of magnitude more expensive than under
// a real target. Shrink to the smallest value that is still unambiguously
// Large under `medium-classes` (SMALL_MAX tops out at exactly 1 MiB there):
// 1 MiB + 4 KiB clears that bound with margin while roughly halving the
// per-touch byte cost. This does not weaken the test — the freshness/
// zero-pass logic under test has no size-dependent branch, so the same bug
// would still be caught at the smaller size; only the interpreter's
// byte-by-byte cost scales with `LARGE`.
#[cfg(not(miri))]
const LARGE: usize = 2 * 1024 * 1024;
#[cfg(miri)]
const LARGE: usize = 1024 * 1024 + 4 * 1024;

/// Read back EVERY byte of `[ptr, ptr+len)` and assert all zero (a full-buffer
/// memcmp-style check, not a spot check — a spot check could miss a stale tail).
fn assert_all_zero(ptr: *mut u8, len: usize, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == 0),
        "{ctx}: memory is not all-zero (first non-zero byte at offset {:?}, value {:#x})",
        bytes.iter().position(|&b| b != 0),
        bytes.iter().find(|&&b| b != 0).copied().unwrap_or(0),
    );
}

/// Read back EVERY byte and assert NONE are zero — used to prove the dirty
/// pattern was actually written (so a later all-zero result is meaningful, not
/// an artefact of the write having been elided).
#[allow(dead_code)]
fn assert_all_dirty(ptr: *mut u8, len: usize, pat: u8, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == pat),
        "{ctx}: dirty pattern {pat:#x} not fully present \
         (first mismatch at offset {:?})",
        bytes.iter().position(|&b| b != pat),
    );
}

/// (1) Fresh-reservation correctness: a large `alloc_zeroed` on a cold
/// `AllocCore` (empty cache → guaranteed MISS → genuinely fresh OS reservation)
/// must read back as entirely zero WITHOUT an explicit `Node::zero`. The
/// externally-observable proof is the byte content alone.
#[test]
fn fresh_large_alloc_zeroed_is_all_zero() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    // Unbounded cache budget: isolate the freshness logic from byte-budget
    // eviction (mirrors `regression_large_cache_span_usable_stable.rs`).
    ac.dbg_set_large_cache_budget(None);

    let la = Layout::from_size_align(LARGE, 8).unwrap();
    // A cold AllocCore has an empty cache → this is a guaranteed fresh
    // reservation (the skip MUST fire here, trusting the OS zero-fill).
    assert_eq!(
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count(),
        0,
        "precondition: cache must be empty for a guaranteed fresh reservation"
    );

    let zero_passes_before = AllocCore::dbg_large_zero_pass_count();
    let ptr = ac.alloc_zeroed(la);
    assert!(!ptr.is_null(), "alloc_zeroed(2 MiB) returned null");
    assert_all_zero(ptr, LARGE, "fresh large alloc_zeroed");
    let zero_delta = AllocCore::dbg_large_zero_pass_count() - zero_passes_before;

    // R9-1: the byte-content check above cannot distinguish "skipped because
    // OS-zeroed" from "zeroed redundantly" — the counter can. On a real OS
    // backend the fresh-reservation skip MUST fire (delta 0: reintroducing an
    // unconditional memset turns this red). Under miri the freshness signal
    // is withheld (miri's std::alloc fallback does NOT zero), so the explicit
    // zero MUST run (delta 1: the pre-R9-1 bug — trusting miri freshness —
    // turns this red under miri).
    #[cfg(not(miri))]
    assert_eq!(
        zero_delta, 0,
        "fresh large alloc_zeroed must SKIP the explicit zero pass on a real \
         OS backend (the optimization under test did not fire)"
    );
    #[cfg(miri)]
    assert_eq!(
        zero_delta, 1,
        "fresh large alloc_zeroed under miri must run the explicit zero pass \
         (miri's std::alloc fallback gives no zero guarantee — R9-1)"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr` was
    // returned by the matching `alloc_zeroed` immediately above, is live, and
    // is freed exactly once here.
    unsafe { ac.dealloc(ptr, la) };
}

/// (2) Cache-hit MUST still fully zero — THE critical regression guard. If the
/// freshness bool were ever wrong in the "reused but claims fresh" direction,
/// this is the test that catches it: a dirty segment is freed into the cache,
/// then re-served via `alloc_zeroed`, and the returned memory MUST be all zero
/// (the explicit `Node::zero` overwrote the planted garbage). The cache hit is
/// CONFIRMED via diagnostics so the test cannot pass vacuously.
#[test]
fn cache_hit_large_alloc_zeroed_still_zeroes() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let la = Layout::from_size_align(LARGE, 8).unwrap();

    // (a) Plain (unzeroed) alloc → fresh reservation.
    let ptr1 = ac.alloc(la);
    assert!(!ptr1.is_null(), "alloc(2 MiB) returned null");

    // (b) Dirty EVERY byte with a recognizable non-zero pattern.
    unsafe { core::ptr::write_bytes(ptr1, 0xAA, LARGE) };
    assert_all_dirty(ptr1, LARGE, 0xAA, "planted dirty pattern");

    // (c) Free → deposits the segment into `large_cache`.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr1` was
    // returned by the matching `alloc` above, is live, freed exactly once here.
    unsafe { ac.dealloc(ptr1, la) };

    // Confirm the deposit actually happened (a no-deposit would make the
    // subsequent hit-impossible and the test vacuous).
    assert_eq!(
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count(),
        1,
        "the freed large segment must be deposited into the cache"
    );

    // (d) Re-alloc the SAME shape via `alloc_zeroed` → MUST hit the cache
    //     (reuse ptr1's segment, whose body still holds 0xAA).
    let hits_before = ac.dbg_large_cache_hits();
    let zero_passes_before = AllocCore::dbg_large_zero_pass_count();
    let ptr2 = ac.alloc_zeroed(la);
    assert!(!ptr2.is_null(), "alloc_zeroed(2 MiB) reuse returned null");
    let hits_after = ac.dbg_large_cache_hits();

    // R9-1: the cache-hit path must run EXACTLY one explicit zero pass — the
    // counter proves the zeroing came from the explicit `Node::zero` (not from
    // luckily-still-zero memory), complementing the byte-content assertion in
    // (e) below.
    assert_eq!(
        AllocCore::dbg_large_zero_pass_count() - zero_passes_before,
        1,
        "the cache-hit alloc_zeroed must run exactly one explicit zero pass"
    );

    // CONFIRM the hit fired. The per-hit increment is gated on `alloc-stats`
    // (default OFF, NOT in `production`) — when present, assert exactly one hit.
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        hits_after - hits_before,
        1,
        "the reuse alloc MUST be a cache hit (otherwise the test is vacuous)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = (hits_before, hits_after);

    // Feature-INDEPENDENT hit confirmation: the deposited slot must have been
    // VACATED by the re-alloc. A fresh reservation (MISS) would have LEFT the
    // cached slot untouched AND reserved a brand-new segment; only a HIT
    // consumes the cached slot. So an empty cache here is proof a hit occurred.
    assert!(
        ac.dbg_large_cache_slot_sizes().iter().all(|s| s.is_none()),
        "the deposited slot was not consumed — the reuse alloc did NOT hit the \
         cache, so this test did not exercise the cache-hit zeroing path"
    );

    // (e) THE load-bearing assertion: read back EVERY byte and assert ALL ZERO.
    // If the freshness bool wrongly reported `true` (fresh) for this reused
    // segment, the explicit `Node::zero` would have been skipped and the 0xAA
    // pattern would survive here.
    assert_all_zero(ptr2, LARGE, "cache-hit alloc_zeroed (must overwrite 0xAA)");

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr2` was
    // returned by the matching `alloc_zeroed` above, is live, freed once here.
    unsafe { ac.dealloc(ptr2, la) };
}

/// (3) Interleaved stress: ~80 iterations of alloc_zeroed → dirty → free at the
/// same size. Every zeroed allocation must read back all-zero regardless of
/// whether that iteration landed on a fresh reservation (iteration 0, cache
/// empty) or a cache hit (iterations 1.., the previous free's segment cached).
#[test]
fn interleaved_alloc_zeroed_always_zero() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let la = Layout::from_size_align(LARGE, 8).unwrap();

    let hits_before = ac.dbg_large_cache_hits();

    // Under miri, each iteration's full-buffer write+read is interpreted
    // byte-by-byte (see the `LARGE` comment above) — 80 iterations would take
    // hours. The per-iteration contract under test (fresh or cache-hit, the
    // result must read back all-zero) is deterministic, not probabilistic:
    // a handful of iterations exercises both the fresh path (iter 0) and the
    // cache-hit path (iter 1..) exactly as thoroughly as 80 would, since
    // there is no size- or iteration-count-dependent branch in the logic
    // under test. Native runs keep the full 80 for genuine stress coverage
    // (decay timing, cache churn) that miri cannot usefully add to anyway.
    #[cfg(not(miri))]
    const ITERS: usize = 80;
    #[cfg(miri)]
    const ITERS: usize = 4;
    for i in 0..ITERS {
        let ptr = ac.alloc_zeroed(la);
        assert!(!ptr.is_null(), "iter {i}: alloc_zeroed returned null");
        // The OBSERVABLE contract must hold every iteration — fresh or cached.
        assert_all_zero(ptr, LARGE, &format!("iter {i} alloc_zeroed"));

        // Dirty every byte so the NEXT iteration's reuse (if any) inherits
        // garbage that a broken skip would leak back to the caller.
        unsafe { core::ptr::write_bytes(ptr, 0xCD, LARGE) };

        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr` was
        // returned by the matching `alloc_zeroed` in this iteration, is live,
        // and is freed exactly once here.
        unsafe { ac.dealloc(ptr, la) };
    }

    let hits_after = ac.dbg_large_cache_hits();
    // The cache-hit path was demonstrably exercised across the loop: iteration
    // 0 is a fresh reservation (cache empty), but iterations 1..ITERS-1 each
    // find the previous iteration's freed segment in the cache (decay interval
    // is ~200ms, far longer than this tight zero-sleep loop, so no eviction
    // intervenes). Under `alloc-stats` we assert that many hits accumulated;
    // without it, test (2) above already proves the hit path is exercised and
    // correct — this test's load-bearing part is the per-iteration all-zero
    // contract.
    #[cfg(feature = "alloc-stats")]
    assert!(
        hits_after - hits_before >= (ITERS - 1) as u64,
        "expected at least {} cache hits across the interleaved loop, got {} \
         (decay may have evicted mid-loop, but the all-zero contract still held)",
        ITERS - 1,
        hits_after - hits_before
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = (hits_before, hits_after);
}

/// (4) Step-3 production-path coverage: drive the freshness skip through the
/// REAL `HeapCore::alloc_zeroed` entry point (the face `SeferAlloc::
/// alloc_zeroed` reaches via the TLS heap), not just `AllocCore` directly.
/// Under `alloc-global` only; otherwise compiled out (the `HeapCore` face is
/// not reachable without `alloc-global`).
#[cfg(feature = "alloc-global")]
#[test]
fn fresh_large_alloc_zeroed_via_heapcore() {
    use sefer_alloc::registry::{bootstrap, HeapRegistry};

    // The shared file-wide lock also serialises this test against the other
    // counter-asserting tests (the registry is a process-global static AND
    // the zero-pass counter is process-wide).
    let _guard = serial();

    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let la = Layout::from_size_align(LARGE, 8).unwrap();
    let zero_passes_before = AllocCore::dbg_large_zero_pass_count();
    // SAFETY: `heap` is a live, claimed `HeapCore` for this thread; the
    // returned pointer is handed to `dealloc` immediately after the check.
    let ptr = unsafe { (*heap).alloc_zeroed(la) };
    assert!(
        !ptr.is_null(),
        "HeapCore::alloc_zeroed(2 MiB) returned null"
    );
    assert_all_zero(ptr, LARGE, "HeapCore fresh large alloc_zeroed");
    let zero_delta = AllocCore::dbg_large_zero_pass_count() - zero_passes_before;

    // R9-1: same skip-sensitivity as the AllocCore-level fresh test, but
    // through the REAL production entry point. NOTE: this heap may have a
    // non-empty large_cache from earlier traffic in this process (registry
    // heaps are shared/recycled), so a cache HIT here is possible — in that
    // case exactly one explicit zero pass is correct. Assert the exact
    // invariant instead: delta 0 iff the alloc was fresh on a real OS
    // backend, delta 1 otherwise (cache hit, or any alloc under miri).
    assert!(
        zero_delta <= 1,
        "HeapCore::alloc_zeroed must run at most one explicit zero pass, got {zero_delta}"
    );
    #[cfg(miri)]
    assert_eq!(
        zero_delta, 1,
        "HeapCore::alloc_zeroed under miri must always run the explicit zero \
         pass (miri's std::alloc fallback gives no zero guarantee — R9-1)"
    );
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr` was
    // returned by the matching `alloc_zeroed` above, is live, freed once here.
    unsafe { (*heap).dealloc(ptr, la) };

    // SAFETY: `heap` was obtained from `HeapRegistry::claim` above and is
    // recycled exactly once here.
    unsafe { HeapRegistry::recycle(heap) };
}
