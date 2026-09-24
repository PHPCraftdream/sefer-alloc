//! R2-11 (independent src review round 2, task #2013) — `SeferAlloc::stats()`
//! must never materialise a registry chunk, spin-wait on one materialising,
//! or abort the process.
//!
//! ## The defect
//!
//! `walk_initialised_slots` (`registry::heap_registry::counters`, the shared
//! loop backing `stats()`'s hit-rate aggregation) used to resolve every
//! index in `0..count` via `Registry::slot`, on the (refuted) assumption
//! that every such index's owning chunk was already materialised by the
//! time the walk reached it. In fact `bump_count`
//! (`registry::heap_registry::stack`) mints a fresh index by bumping `count`
//! and returns *immediately* — it does NOT call `slot()` itself; the caller
//! (`claim`/`claim_with_config`) calls `reg.slot(idx)` as a SEPARATE, later
//! step. A `stats()` call racing that window used to reach `reg.slot(idx)`
//! for the just-bumped index too — driving `stats()` (meant to be a cheap,
//! read-only diagnostic snapshot) into a real OS chunk reservation, a
//! spin-wait on the claiming thread's in-flight initialisation, or (on
//! chunk-materialisation OOM) `std::process::abort()`.
//!
//! ## The fix
//!
//! `walk_initialised_slots` now resolves each index via the new
//! `Registry::slot_if_materialised` — a pure `Acquire` peek at the chunk's
//! already-published pointer (the exact fast-path check `ensure_chunk` /
//! `try_ensure_chunk` already perform before falling through to
//! `ensure_chunk_slow`), returning `None` — never materialising, never
//! spinning, never aborting — if the chunk is not yet `READY`. The walk
//! simply skips such an index for this snapshot, exactly as it already
//! skips a materialised-but-not-yet-`initialised` slot.
//!
//! ## Reproduction strategy
//!
//! A true two-thread timing race is not needed to prove the mechanism: the
//! exact hazardous window is "count bumped, owning chunk never touched",
//! which the test-only `dbg_bump_count_without_materialising` hook
//! (`internals`-gated) constructs deterministically and single-threaded by
//! calling `bump_count` alone — stopping short of the follow-up `slot()`
//! call `claim`/`claim_with_config` always perform immediately afterward in
//! production. This is the same "reproduce the defect with a targeted
//! test-only hook rather than hoping for scheduler luck" approach this
//! crate already uses for the sibling chunk-materialisation-OOM finding
//! (see `tests/regression_free_path_chunk_oom_graceful.rs`).
//!
//! This test:
//!
//! 1. Constructs that exact window (a freshly-minted index whose owning
//!    chunk is observably NOT materialised).
//! 2. Calls the real `SeferAlloc::stats()` entry point — the layer the fix
//!    actually ships at, and the layer R2-11's finding is specifically
//!    about — while that window is open, and asserts the chunk is STILL not
//!    materialised afterward (the primary, non-vacuous assertion).
//! 3. As a counterfactual proving the oracle (`dbg_chunk_is_materialised`)
//!    is not vacuous — i.e. that a `slot()`-family resolution really WOULD
//!    materialise an untouched chunk, which is exactly what the pre-fix
//!    `reg.slot(idx)` call inside the walk did — resolves a DIFFERENT
//!    untouched chunk's index through the existing `slot_or_none`-family
//!    test hook (`dbg_slot_or_none`, added for the sibling R34-15 finding;
//!    it shares the identical fast-path/slow-path structure with
//!    `slot`/`ensure_chunk` — see `Registry::slot_or_none`'s doc comment)
//!    and confirms THAT chunk transitions to materialised.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-stats",
    feature = "internals"
))]

use sefer_alloc::registry::bootstrap::{self, Registry};
use sefer_alloc::registry::heap_registry::dbg_bump_count_without_materialising;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static ALLOC: SeferAlloc = SeferAlloc::new();

/// Advance `count` (via `bump_count` alone, never `slot()`) until it lands on
/// a fresh index whose owning chunk is observably not yet materialised AND
/// (if `avoid_chunk` is given) is not that chunk either. `count` only grows
/// and chunks are visited in non-decreasing index order, so this always
/// makes forward progress; bounded generously by `2 * MAX_HEAPS` calls (far
/// more than the whole slot space) so a genuine bug fails the test instead
/// of hanging.
fn mint_index_with_unmaterialised_chunk(
    reg: &'static Registry,
    chunk_slots: usize,
    avoid_chunk: Option<usize>,
) -> (usize, usize) {
    for _ in 0..2 * bootstrap::MAX_HEAPS {
        let idx = dbg_bump_count_without_materialising()
            .expect("registry exhausted — suite claimed close to MAX_HEAPS slots")
            as usize;
        let chunk_idx = idx / chunk_slots;
        if !reg.dbg_chunk_is_materialised(chunk_idx) && Some(chunk_idx) != avoid_chunk {
            return (idx, chunk_idx);
        }
    }
    panic!(
        "could not find a suitable unmaterialised chunk within 2*MAX_HEAPS \
         bump_count calls — MAX_HEAPS/CHUNK_SLOTS assumption violated, or the \
         registry is close to exhaustion"
    );
}

#[test]
fn stats_does_not_materialise_a_not_yet_claimed_chunk() {
    let reg = bootstrap::ensure();
    let chunk_slots = bootstrap::MAX_HEAPS / bootstrap::dbg_num_chunks();

    // Make the walk genuinely non-trivial: bind this thread's own heap so at
    // least one real, `initialised` slot exists for `stats()` to aggregate
    // (non-vacuousness — the walk is not simply looping over nothing).
    let v: Vec<u8> = vec![1, 2, 3, 4];
    core::hint::black_box(&v);

    // ── Reproduce the exact R2-11 window ────────────────────────────────
    let (_idx, chunk_idx) = mint_index_with_unmaterialised_chunk(reg, chunk_slots, None);
    assert!(
        !reg.dbg_chunk_is_materialised(chunk_idx),
        "precondition: the target chunk must not be materialised before stats()"
    );

    // ── The actual entry point under test ───────────────────────────────
    let stats = ALLOC.stats();
    core::hint::black_box(&stats);

    assert!(
        !reg.dbg_chunk_is_materialised(chunk_idx),
        "R2-11 regression: SeferAlloc::stats() materialised a registry chunk \
         for a slot index that was only `count`-visible (freshly minted by \
         bump_count) but never resolved via slot() — stats() must never \
         perform this OS reservation, spin-wait, or (on OOM) abort."
    );

    // ── Counterfactual: prove the oracle above is not vacuous ───────────
    // A DIFFERENT untouched chunk really does materialise when resolved
    // through the slot()-family fast/slow path (the same protocol the
    // pre-fix walk used via `reg.slot(idx)`) — confirming
    // `dbg_chunk_is_materialised` genuinely tracks "did an OS reservation
    // happen" rather than trivially always reading `false`.
    let (idx2, chunk_idx2) =
        mint_index_with_unmaterialised_chunk(reg, chunk_slots, Some(chunk_idx));
    assert_ne!(
        chunk_idx, chunk_idx2,
        "test setup: need a second, distinct chunk for the counterfactual"
    );
    let resolved = bootstrap::dbg_slot_or_none(idx2);
    assert!(
        resolved,
        "slot_or_none should resolve a freshly-minted, in-range index"
    );
    assert!(
        reg.dbg_chunk_is_materialised(chunk_idx2),
        "counterfactual failed: slot_or_none (sharing the slot()-family \
         fast/slow path with `slot()`/`ensure_chunk`) did not materialise \
         its chunk — the oracle used above would be vacuous if this doesn't \
         hold"
    );
}
