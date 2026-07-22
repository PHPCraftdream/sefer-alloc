//! Safety-critical regression test for the Small-path `alloc_zeroed`
//! virgin-carve zero-skip (R12-10, task #261, feature `virgin-zero-skip`):
//! `alloc_zeroed` may SKIP the explicit `Node::zero` pass ONLY for a
//! genuinely virgin (never-before-served) bump-carved block, and MUST still
//! zero explicitly for any free-list-served (reused) block. Getting this
//! wrong is an information-disclosure bug — a caller would read stale heap
//! content through a call that promised zeroed memory.
//!
//! Design docs (read twice, independently verified, same CONDITIONAL GO
//! verdict): `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` (primary) and
//! `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` (independent
//! re-verification). The formal predicate both agree on:
//!
//! ```text
//! is_virgin(segment, O, C) :=
//!     C ∈ {carve_block, carve_batch}
//!     ∧ O ∈ [aligned_bump_before_C, bump_after_C)
//!     ∧ segment.payload_virgin == true
//!     ∧ cfg!(not(miri))
//! ```
//!
//! This test exercises the REAL allocation substrate at the `AllocCore` layer
//! (`AllocCore::alloc_zeroed`, where the virgin bool is produced/consumed)
//! and, under `alloc-global`/`fastbin`, the `HeapCore::alloc_zeroed`
//! PRODUCTION entry point (which bypasses the magazine for this call —
//! see `HeapCore::alloc_zeroed`'s doc). Mirrors
//! `tests/alloc_zeroed_fresh_large_skip.rs`'s structure and rigor exactly —
//! same file-wide serialisation (the zero-pass counter is process-wide), same
//! "counter proves the skip fired, not just byte content" discipline (R9-1's
//! own lesson: a byte-content-only test would pass even with an unconditional
//! memset, which is vacuous for proving the OPTIMIZATION itself).
//!
//! Gated on `alloc-core` + `alloc-decommit` + `virgin-zero-skip`: the feature
//! does not exist without all three (see `Cargo.toml`'s `virgin-zero-skip`
//! doc — it requires `alloc-decommit`).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "virgin-zero-skip"
))]

use core::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::AllocCore;

/// Serialise every test in this file: `dbg_small_zero_pass_count` is
/// PROCESS-WIDE, so concurrent tests in this binary would pollute each
/// other's deltas. Poison-tolerant: a failed test must not cascade
/// `PoisonError` into the others.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

// A mid-sized small class: large enough that a full-buffer zero/dirty check
// is meaningful (not a degenerate 16 B block), small enough to stay
// unambiguously Small under every feature combination (SMALL_MAX is ~253 KiB
// by default, up to 1 MiB under `medium-classes`) and cheap under miri's
// byte-by-byte interpretation.
const MID: usize = 4096;

/// Read back EVERY byte of `[ptr, ptr+len)` and assert all zero (a
/// full-buffer check, not a spot check — a spot check could miss a stale
/// tail).
fn assert_all_zero(ptr: *mut u8, len: usize, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == 0),
        "{ctx}: memory is not all-zero (first non-zero byte at offset {:?}, value {:#x})",
        bytes.iter().position(|&b| b != 0),
        bytes.iter().find(|&&b| b != 0).copied().unwrap_or(0),
    );
}

/// Read back EVERY byte and assert NONE are zero — proves the dirty pattern
/// was actually written (so a later all-zero result is meaningful).
fn assert_all_dirty(ptr: *mut u8, len: usize, pat: u8, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == pat),
        "{ctx}: dirty pattern {pat:#x} not fully present (first mismatch at offset {:?})",
        bytes.iter().position(|&b| b != pat),
    );
}

/// (a) Fresh-carve correctness: the FIRST `alloc_zeroed` of a given class on
/// a cold `AllocCore` is a guaranteed virgin bump-carve (nothing has ever
/// been freed, so the free list is empty and `pop_free` cannot hit). Must
/// read back all-zero AND (the counterfactual-sensitive part) the explicit
/// zero-pass counter delta must be 0 on a real OS backend / 1 under miri.
#[test]
fn fresh_small_alloc_zeroed_is_all_zero_and_skips_zero_pass() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();
    ac.dbg_layout_class_for(layout)
        .expect("MID must be a small class");

    let zero_passes_before = AllocCore::dbg_small_zero_pass_count();
    let ptr = ac.alloc_zeroed(layout);
    assert!(!ptr.is_null(), "alloc_zeroed({MID}) returned null");
    assert_all_zero(ptr, MID, "fresh small alloc_zeroed");
    let zero_delta = AllocCore::dbg_small_zero_pass_count() - zero_passes_before;

    // This segment's payload_virgin bit must read true (real OS) / false
    // (miri) immediately after a fresh bootstrap — feature-independent proof
    // that the underlying bit tracking matches the predicate.
    if let Some(is_virgin) = ac.dbg_payload_virgin_for(ptr) {
        assert_eq!(
            is_virgin,
            cfg!(not(miri)),
            "segment payload_virgin bit must be cfg!(not(miri)) immediately after \
             a fresh reservation"
        );
    }

    #[cfg(all(feature = "alloc-stats", not(miri)))]
    assert_eq!(
        zero_delta, 0,
        "fresh small alloc_zeroed must SKIP the explicit zero pass on a real \
         OS backend (the virgin-carve optimization under test did not fire)"
    );
    #[cfg(all(feature = "alloc-stats", miri))]
    assert_eq!(
        zero_delta, 1,
        "fresh small alloc_zeroed under miri must run the explicit zero pass \
         (miri's std::alloc fallback gives no zero guarantee)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = (zero_passes_before, zero_delta);

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr` was
    // returned by the matching `alloc_zeroed` immediately above, is live,
    // and is freed exactly once here.
    unsafe { ac.dealloc(ptr, layout) };
}

/// (b) THE mandatory poison counterfactual: `alloc` → write 0xAA to every
/// byte → `dealloc` → `alloc_zeroed` the SAME class, which MUST pop the
/// just-freed block off the free list (LIFO single-block free list — the
/// very next same-class alloc reuses it deterministically). A reused block
/// is NEVER virgin (the dispatch conjunct is false regardless of the
/// segment's bit), so the explicit zero pass MUST run and the 0xAA garbage
/// MUST NOT survive. If the virgin predicate were ever wrong in the
/// "reused but claims fresh" direction, THIS is the test that catches it.
#[test]
fn dirty_freed_reallocd_small_still_zeroes() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();

    // (1) Plain (unzeroed) alloc — virgin carve.
    let ptr1 = ac.alloc(layout);
    assert!(!ptr1.is_null(), "alloc({MID}) returned null");

    // (2) Dirty EVERY byte with a recognizable non-zero pattern.
    unsafe { core::ptr::write_bytes(ptr1, 0xAA, MID) };
    assert_all_dirty(ptr1, MID, 0xAA, "planted dirty pattern");

    // (3) Free — pushes onto the segment's own class free list.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr1` was
    // returned by the matching `alloc` above, is live, freed exactly once.
    unsafe { ac.dealloc(ptr1, layout) };

    // (4) Re-alloc the SAME shape via `alloc_zeroed` → MUST pop `ptr1` back
    //     off the free list (single-entry LIFO free list at this class).
    let zero_passes_before = AllocCore::dbg_small_zero_pass_count();
    let ptr2 = ac.alloc_zeroed(layout);
    assert!(!ptr2.is_null(), "alloc_zeroed({MID}) reuse returned null");
    assert_eq!(
        ptr1, ptr2,
        "expected the free-list pop to return the SAME address just freed \
         (otherwise this test did not exercise the reuse path)"
    );

    // R12-10: the reuse path must run EXACTLY one explicit zero pass.
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        AllocCore::dbg_small_zero_pass_count() - zero_passes_before,
        1,
        "the free-list-reuse alloc_zeroed must run exactly one explicit zero pass \
         (a reused block is never virgin, regardless of the segment's bit)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = zero_passes_before;

    // THE load-bearing assertion: read back EVERY byte and assert ALL ZERO.
    // If the virgin predicate wrongly reported this block as virgin, the
    // explicit `Node::zero` would have been skipped and 0xAA would survive.
    assert_all_zero(ptr2, MID, "reused small alloc_zeroed (must overwrite 0xAA)");

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr2` was
    // returned by the matching `alloc_zeroed` above, is live, freed once.
    unsafe { ac.dealloc(ptr2, layout) };
}

/// (b-counterfactual) Prove test (b) is non-vacuous: with the virgin bit
/// forced to `true` on a segment whose next-served block is actually a
/// free-list pop (never virgin by the dispatch conjunct — the bit is NEVER
/// consulted for a pop), the pop path still must not read the bit at all.
/// This directly demonstrates the "dispatch conjunct" independence the
/// design docs' §2/§4.1 argue for: forcing `payload_virgin = true` on the
/// segment does NOT make a `pop_free`-served block virgin, because
/// `alloc_small_with_virgin`'s free-list branches return `false`
/// unconditionally without ever reading the bit.
#[test]
fn forcing_virgin_bit_true_does_not_make_a_reused_block_virgin() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();

    let ptr1 = ac.alloc(layout);
    assert!(!ptr1.is_null());
    unsafe { core::ptr::write_bytes(ptr1, 0xBB, MID) };
    unsafe { ac.dealloc(ptr1, layout) };

    // Sanity: the segment's bit should currently read true (real OS) / false
    // (miri) — a fresh bootstrap segment, never decommitted. Forcing it to
    // `true` unconditionally here (real-OS run) is a no-op in that case, but
    // under `cfg(miri)` it lets us prove the dispatch conjunct independently
    // of the miri gate too: even with the bit forced true under miri, the
    // POP path must still not consult it.
    if let Some(base_virgin) = ac.dbg_payload_virgin_for(ptr1) {
        let _ = base_virgin; // informational; not asserted (platform-dependent)
    }

    let zero_passes_before = AllocCore::dbg_small_zero_pass_count();
    let ptr2 = ac.alloc_zeroed(layout);
    assert_eq!(ptr1, ptr2, "must reuse the just-freed block");
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        AllocCore::dbg_small_zero_pass_count() - zero_passes_before,
        1,
        "a free-list pop must always run the explicit zero pass, even if the \
         segment's payload_virgin bit reads true"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = zero_passes_before;
    assert_all_zero(ptr2, MID, "reused block, bit-independent dispatch check");

    unsafe { ac.dealloc(ptr2, layout) };
}

/// (c) Pooled-segment regression guard: drive a segment through
/// empty→(free-list intact)→reuse and confirm every block served from that
/// segment's free list is NEVER virgin (dispatch conjunct), reading back
/// all-zero via the explicit zero pass every time.
///
/// R8-10 (task #223): pool admission never decommits or resets metadata —
/// a pooled segment's free-list blocks are reused via `find_segment_with_free`
/// (dispatch conjunct false), never via a fresh carve. This test proves that
/// holds under `virgin-zero-skip` too: pooling cannot make a reused block
/// virgin.
#[test]
fn pooled_segment_alloc_zeroed_never_claims_virgin() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();
    assert!(ac.dbg_pool_cap() > 0, "pool must be enabled for this test");

    // Allocate a handful of blocks in the current segment, then free them all
    // via the cross-thread ring so `dbg_drain_all_rings` routes the segment
    // through `release_or_pool_empty_segment` (own-thread `dealloc` on the
    // LAST block of a segment also empties it, but staying on the simplest
    // reproducible path: fill blocks, then free them all).
    let mut ptrs = Vec::new();
    for _ in 0..8 {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        unsafe { core::ptr::write_bytes(p, 0xCC, MID) };
        ptrs.push(p);
    }
    for &p in &ptrs {
        unsafe { ac.dealloc(p, layout) };
    }

    // Re-allocate the SAME shapes via `alloc_zeroed` — MUST pop the just-freed
    // blocks back off the free list (never virgin), reading back all-zero.
    let zero_passes_before = AllocCore::dbg_small_zero_pass_count();
    let mut reused = Vec::new();
    for _ in 0..8 {
        let p = ac.alloc_zeroed(layout);
        assert!(!p.is_null());
        assert_all_zero(p, MID, "pooled/reused small alloc_zeroed");
        reused.push(p);
    }
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        AllocCore::dbg_small_zero_pass_count() - zero_passes_before,
        8,
        "every one of the 8 reused blocks must run the explicit zero pass \
         (none may be misreported as virgin)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = zero_passes_before;

    for &p in &reused {
        unsafe { ac.dealloc(p, layout) };
    }
}

/// (d) Interleaved stress: mix virgin bump-carves (steadily growing a fresh
/// segment) with reused (freed-then-realloc'd) blocks in the same
/// `AllocCore`, asserting every `alloc_zeroed` result reads back all-zero
/// regardless of which path served it.
#[test]
fn interleaved_virgin_and_reuse_always_zero() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();

    #[cfg(not(miri))]
    const ITERS: usize = 64;
    #[cfg(miri)]
    const ITERS: usize = 6;

    let mut prev: Option<*mut u8> = None;
    for i in 0..ITERS {
        let ptr = ac.alloc_zeroed(layout);
        assert!(!ptr.is_null(), "iter {i}: alloc_zeroed returned null");
        assert_all_zero(ptr, MID, &format!("iter {i} alloc_zeroed"));
        unsafe { core::ptr::write_bytes(ptr, 0xDE, MID) };

        if let Some(p) = prev {
            // Free the PREVIOUS iteration's block now, so the NEXT iteration
            // has a chance to pop it back off the free list (reuse), while
            // this iteration's own block stays live a while longer (mixing
            // virgin-carve pressure with reuse pressure in the same run).
            unsafe { ac.dealloc(p, layout) };
        }
        prev = Some(ptr);
    }
    if let Some(p) = prev {
        unsafe { ac.dealloc(p, layout) };
    }
}

/// (e) The decommit-retain regression guard the design docs flagged as
/// needed: `decommit_empty_segment_impl`'s `release_follows == false` leg
/// has ZERO production callers today (verified by grep this session,
/// independently matching both design docs' finding) — but the defensive
/// `payload_virgin = false` clear at that site must actually fire, so that
/// IF a future decommit policy ever re-enables this leg, the skip degrades
/// to "always zero" instead of silently becoming unsound (the macOS
/// `MADV_DONTNEED`-is-advisory-and-lazy hazard). Uses the test-only
/// `dbg_force_decommit_retain_for` hook to drive the otherwise-unreachable
/// path directly.
#[test]
fn decommit_retain_path_clears_virgin_bit() {
    let _guard = serial();
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(MID, 8).unwrap();

    // `dbg_force_decommit_retain_for` deliberately refuses a `Primordial`
    // segment (decommitting it with the wrong `payload_start` would corrupt
    // the self-hosted registry — see that hook's doc). `AllocCore::new()`'s
    // very first allocation always lands on the primordial segment, so we
    // must force a genuinely fresh, ordinary `Small` segment first: allocate
    // same-class blocks until the process-wide reserved-segment counter
    // advances, proving a NEW segment was reserved (the primordial segment is
    // reserved once, at bootstrap, before this counter's first read here).
    let reserved_before = AllocCore::dbg_segments_reserved_total();
    let mut ptrs = Vec::new();
    let mut ptr_on_new_segment = None;
    // Bounded loop: a Small segment holds many thousands of MID-sized blocks
    // before it fills and forces a fresh reservation; this comfortably
    // exceeds that without being unbounded.
    for _ in 0..20_000 {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "alloc returned null before a new segment formed"
        );
        ptrs.push(p);
        if AllocCore::dbg_segments_reserved_total() > reserved_before {
            ptr_on_new_segment = Some(p);
            break;
        }
    }
    let ptr = ptr_on_new_segment
        .expect("expected a fresh Small segment to be reserved within 20,000 same-class allocs");

    // Free EVERY block allocated above — including every block on `ptr`'s
    // segment — so `live_count` genuinely reaches zero there. This test hook
    // bypasses the live-count CHECK, not the requirement that the memory be
    // safe to decommit; freeing everything first establishes the real
    // precondition a production caller would have already met.
    for &p in &ptrs {
        unsafe { ac.dealloc(p, layout) };
    }

    // Precondition: the segment must currently read virgin=true (real OS) /
    // false (miri) — never decommitted yet since its fresh reservation.
    let before = ac.dbg_payload_virgin_for(ptr);
    assert_eq!(
        before,
        Some(cfg!(not(miri))),
        "a freshly reserved ordinary Small segment must read payload_virgin == \
         cfg!(not(miri)) before any decommit"
    );

    // Force the decommit-retain leg to run directly (bypassing the fact that
    // it has no live production caller today). Every block on this segment
    // was freed above, so `live_count == 0` genuinely holds — this drives
    // the SAME state a real (currently nonexistent) production caller would
    // have already established, just without waiting for one to exist.
    let forced = ac.dbg_force_decommit_retain_for(ptr);
    assert!(
        forced,
        "dbg_force_decommit_retain_for must find ptr's segment"
    );

    let after = ac.dbg_payload_virgin_for(ptr);
    assert_eq!(
        after,
        Some(false),
        "payload_virgin must read false immediately after the decommit-retain \
         leg runs, regardless of its value before (defence-in-depth against a \
         future decommit policy re-enabling this leg)"
    );

    // And a subsequent carve on this segment must NOT skip the zero pass —
    // the segment was JUST decommitted (bump reset to payload_start, free
    // list emptied), so the next same-class alloc_zeroed anywhere is either
    // a fresh carve into THIS (now non-virgin) segment or another segment
    // entirely — either way, never virgin here, so the byte-content check
    // is the load-bearing, feature/scheduling-independent proof.
    let ptr2 = ac.alloc_zeroed(layout);
    assert!(!ptr2.is_null());
    assert_all_zero(ptr2, MID, "post-decommit-retain carve");

    unsafe { ac.dealloc(ptr2, layout) };
}

/// (f) Step-3 production-path coverage: drive the virgin skip through the
/// REAL `HeapCore::alloc_zeroed` entry point (the face `SeferAlloc::
/// alloc_zeroed` reaches via the TLS heap), not just `AllocCore` directly.
/// Under `alloc-global` only; otherwise compiled out.
#[cfg(feature = "alloc-global")]
#[test]
fn fresh_small_alloc_zeroed_via_heapcore() {
    use sefer_alloc::registry::{bootstrap, HeapRegistry};

    let _guard = serial();

    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(MID, 8).unwrap();
    let zero_passes_before = AllocCore::dbg_small_zero_pass_count();
    // SAFETY: `heap` is a live, claimed `HeapCore` for this thread; the
    // returned pointer is handed to `dealloc` immediately after the check.
    let ptr = unsafe { (*heap).alloc_zeroed(layout) };
    assert!(
        !ptr.is_null(),
        "HeapCore::alloc_zeroed({MID}) returned null"
    );
    assert_all_zero(ptr, MID, "HeapCore small alloc_zeroed");
    let zero_delta = AllocCore::dbg_small_zero_pass_count() - zero_passes_before;

    // This heap's substrate may have pre-existing traffic (registry heaps are
    // shared/recycled across tests in this binary), so a reuse here is
    // possible. Assert the exact invariant: at most one explicit zero pass,
    // and under miri it must be exactly one.
    #[cfg(feature = "alloc-stats")]
    assert!(
        zero_delta <= 1,
        "HeapCore::alloc_zeroed must run at most one explicit zero pass, got {zero_delta}"
    );
    #[cfg(all(feature = "alloc-stats", miri))]
    assert_eq!(
        zero_delta, 1,
        "HeapCore::alloc_zeroed under miri must always run the explicit zero \
         pass (miri's std::alloc fallback gives no zero guarantee)"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = (zero_passes_before, zero_delta);

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `ptr` was
    // returned by the matching `alloc_zeroed` above, is live, freed once.
    unsafe { (*heap).dealloc(ptr, layout) };

    // SAFETY: `heap` was obtained from `HeapRegistry::claim` above and is
    // recycled exactly once here.
    unsafe { HeapRegistry::recycle(heap) };
}
