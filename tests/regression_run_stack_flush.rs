//! PERF-3 Ф2 (task #209) — run-encoded freelist FLUSH-side detection tests.
//!
//! Ф1 (`regression_run_stack_layout.rs`) pinned the `RunStack` storage, layout,
//! and accessors. Ф2 wires the flush side: `flush_run` now (under
//! `alloc-runfreelist`) detects contiguous-accepted sub-runs in a flush batch,
//! encodes each as a compact `(start_off, count)` descriptor on the segment's
//! `RunStack`, and falls back to the classic linked-list path for singletons
//! and overflow. This file pins that behavioural wiring at the `AllocCore`
//! level (where `flush_class`/`flush_run` live), via the `dbg_*` seams.
//!
//! ## Detection strategy decision (evidence-based)
//!
//! The plan's §1 hypothesised that the cold-storm bench (`bench_direct_alloc`)
//! "produces long runs". Empirically measuring the ACTUAL offset-contiguity of
//! flush batches (a throwaway experiment against the real `SeferAlloc` alloc/
//! dealloc path, simulating the magazine's LIFO mechanics — see the Ф2 report)
//! showed the opposite for an in-place scan: the magazine's LIFO refill returns
//! blocks in DESCENDING address order within a refill batch, so the flush batch
//! (the oldest 8 slots) is also descending → **0% offset-adjacent in flush
//! order** (only `off[i+1] == off[i] - block_size`, not `+`). Sorting the
//! accepted offsets ASCENDING before detection turns that same batch into a
//! ~100%-contiguous run. So Ф2 uses **Approach B (sort-then-detect)**, not
//! Approach A. The sort is on ≤16 elements (the magazine cap) — cheap and
//! one-time-per-flush.
//!
//! ## Test-buffer construction note
//!
//! `refill_class` first DRAINS the segment's freelist (LIFO: most-recently-
//! freed first) THEN carves — so the blocks it returns are NOT necessarily a
//! single contiguous ascending run. For tests that need a GUARANTEED-
//! contiguous batch we carve directly via `dbg_carve_batch` (which is pure
//! bump-carve: `off = aligned_start + i * block_size`, strictly ascending and
//! offset-adjacent). Gaps between sub-runs are constructed by deallocating
//! specific carved blocks (so they read `is_free` and are skipped by the guard
//! on flush, NOT included in the batch).
//!
//! ## What these tests cover (the plan's Ф2 gate — flush-side unit checks)
//!
//! - Contiguous accepted → one RunStack descriptor.
//! - Non-contiguous (a gap) → correct split.
//! - Overflow → linked-list fallback (no panic, no data loss).
//! - Mixed flush → both representations populated, head intact.
//! - Guards still fire per-block (ring-DF'd block skipped).
//! - `not(feature)` byte-identical classic behaviour.
//! - Run-member blocks NOT on the linked-list drain (Ф2 intermediate state).
//!
//! ## IMPORTANT — Ф2's gate is NARROWER than full behavioural equivalence
//!
//! Between Ф2 and Ф3, run-encoded blocks are honestly "leaked": FREE in the
//! bitmap AND in the RunStack, but NOT on the linked list and NOT yet
//! drainable. Full equivalence is Ф3's gate. `runstack_drain_all` (below) is
//! DESTRUCTIVE, so it is used only as a TERMINAL assertion.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::AllocCore;
#[cfg(feature = "alloc-runfreelist")]
use sefer_alloc::alloc_core::run_stack::{RunDesc, RunStack, RUNSTACK_CAPACITY};
use sefer_alloc::SegmentLayout;

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

#[cfg(feature = "alloc-runfreelist")]
fn runstack_peek(ptr: *mut u8, class: usize) -> Option<RunDesc> {
    RunStack::peek(seg_base(ptr) as *mut u8, class)
}

#[cfg(feature = "alloc-runfreelist")]
fn runstack_drain_all(ptr: *mut u8, class: usize) -> Vec<RunDesc> {
    let base = seg_base(ptr) as *mut u8;
    let mut out = Vec::new();
    while let Some(desc) = RunStack::pop(base, class) {
        out.push(desc);
    }
    out
}

/// Carve `n` blocks directly via `dbg_carve_batch` (pure bump-carve, strictly
/// ascending + offset-adjacent). Asserts the precondition.
#[cfg(feature = "alloc-runfreelist")]
fn carve_contiguous(core: &mut AllocCore, c: usize, n: usize) -> Vec<*mut u8> {
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.dbg_carve_batch(c, &mut buf);
    assert_eq!(got, n, "carve_batch must carve all n blocks");
    let base0 = seg_base(buf[0]);
    assert!(
        buf.iter().all(|&p| seg_base(p) == base0),
        "all carved blocks share one segment"
    );
    // Verify strictly ascending + offset-adjacent.
    let mut sorted: Vec<usize> = buf.iter().map(|p| *p as usize).collect();
    sorted.sort_unstable();
    for w in sorted.windows(2) {
        assert_eq!(w[1] - w[0], 16, "carved blocks must be offset-adjacent");
    }
    buf
}

// ---------------------------------------------------------------------------
// Test 1 — Contiguous accepted → exactly one RunStack descriptor.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn contiguous_accepted_pushes_one_descriptor() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();
    const N: usize = 8;

    let buf = carve_contiguous(&mut core, c, N);
    let base0 = seg_base(buf[0]);
    let expected_start_off = (buf.iter().map(|p| *p as usize).min().unwrap() - base0) as u32;

    core.flush_class(c, &buf);

    let desc = runstack_peek(buf[0], c).expect("a contiguous run must produce a descriptor");
    assert_eq!(desc.start_off, expected_start_off);
    assert_eq!(desc.count, N as u16);

    for &p in &buf {
        assert!(core.dbg_is_free_for(p), "every accepted block must be FREE");
    }

    // No singletons → linked-list head must be NULL.
    let head = core.dbg_freelist_head_for(buf[0], c);
    assert_eq!(head, u32::MAX, "linked-list head must be NULL (no singletons)");

    let drained = runstack_drain_all(buf[0], c);
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].count, N as u16);

    for &p in &buf {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 2 — Non-contiguous (a gap) → correct split into multiple descriptors.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn non_contiguous_gap_produces_split_descriptors() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 16 contiguous, then dealloc the middle 4 to create a gap. The
    // remaining 12 form two sub-runs of 6 each.
    let buf = carve_contiguous(&mut core, c, 16);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    // Dealloc sorted[6..10] — those become gap blocks (is_free → skipped).
    for &p in &sorted[6..10] {
        core.dealloc(p, layout);
    }
    let mut batch: Vec<*mut u8> = Vec::with_capacity(12);
    batch.extend_from_slice(&sorted[0..6]);
    batch.extend_from_slice(&sorted[10..16]);
    assert_eq!(batch.len(), 12);

    let gap = (sorted[10] as usize) - (sorted[5] as usize);
    assert!(gap > 16, "gap must exist between the two groups");

    core.flush_class(c, &batch);

    for &p in &batch {
        assert!(core.dbg_is_free_for(p), "every batch block must be FREE");
    }

    let drained = runstack_drain_all(batch[0], c);
    assert_eq!(drained.len(), 2, "two contiguous sub-runs → two descriptors");
    assert!(drained.iter().all(|d| d.count == 6), "each sub-run has 6 blocks");

    for &p in &batch {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 3 — Overflow: fill RUNSTACK_CAPACITY descriptors, then flush a 9th
// contiguous run → falls back to linked-list (no panic, no data loss).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn overflow_falls_back_to_linked_list() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve enough for GROUPS runs of length 2 + GROUPS gap blocks.
    const GROUPS: usize = RUNSTACK_CAPACITY + 1; // 9
    let total_needed = GROUPS * 3; // 2 per group + 1 gap = 27
    let buf = carve_contiguous(&mut core, c, total_needed);
    let base0 = seg_base(buf[0]);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    // groups[i] = sorted[3i..3i+2], gap = sorted[3i+2].
    let mut groups: Vec<Vec<*mut u8>> = Vec::with_capacity(GROUPS);
    let mut gap_blocks: Vec<*mut u8> = Vec::new();
    for i in 0..GROUPS {
        groups.push(vec![sorted[3 * i], sorted[3 * i + 1]]);
        gap_blocks.push(sorted[3 * i + 2]);
    }
    for &p in &gap_blocks {
        core.dealloc(p, layout);
    }
    for g in &groups {
        assert_eq!((g[1] as usize) - (g[0] as usize), 16);
    }

    // Flush first RUNSTACK_CAPACITY groups → fills RunStack.
    let mut first_batch: Vec<*mut u8> = Vec::new();
    for g in &groups[..RUNSTACK_CAPACITY] {
        first_batch.extend_from_slice(g);
    }
    core.flush_class(c, &first_batch);

    // Flush 9th group — MUST overflow → linked-list fallback.
    let overflow_batch: Vec<*mut u8> = groups[RUNSTACK_CAPACITY].clone();
    core.flush_class(c, &overflow_batch);

    for &p in &overflow_batch {
        assert!(core.dbg_is_free_for(p), "overflow block must be FREE");
    }

    let head = core.dbg_freelist_head_for(overflow_batch[0], c);
    let overflow_offs: HashSet<u32> = overflow_batch
        .iter()
        .map(|p| (*p as usize - base0) as u32)
        .collect();
    assert!(
        overflow_offs.contains(&head),
        "linked-list head ({head:#x}) must reference an overflow block"
    );

    let drained = runstack_drain_all(overflow_batch[0], c);
    assert_eq!(
        drained.len(),
        RUNSTACK_CAPACITY,
        "RunStack at capacity (9th overflowed to linked-list)"
    );

    for &p in &first_batch {
        core.dealloc(p, layout);
    }
    for &p in &overflow_batch {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 4 — Mixed flush: contiguous (→ RunStack) + singletons (→ linked-list).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn mixed_flush_populates_both_representations() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Layout (carve contiguous, then partition):
    // [0,1,2]=run_a(3), [3]=gap, [4]=singleton_a, [5]=gap, [6,7]=run_b(2),
    // [8]=gap, [9]=singleton_b, [10..14]=unused(dealloc).
    let buf = carve_contiguous(&mut core, c, 14);
    let base0 = seg_base(buf[0]);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_a: Vec<*mut u8> = sorted[0..3].to_vec();
    let singleton_a = sorted[4];
    let run_b: Vec<*mut u8> = sorted[6..8].to_vec();
    let singleton_b = sorted[9];
    for &p in sorted[3..4].iter().chain(&sorted[5..6]).chain(&sorted[8..9]).chain(&sorted[10..14]) {
        core.dealloc(p, layout);
    }

    let batch: Vec<*mut u8> = vec![
        run_a[0], run_a[1], run_a[2],
        singleton_a,
        run_b[0], run_b[1],
        singleton_b,
    ];
    core.flush_class(c, &batch);

    for &p in &batch {
        assert!(core.dbg_is_free_for(p), "every batch block must be FREE");
    }

    let head = core.dbg_freelist_head_for(batch[0], c);
    let singleton_offs: HashSet<u32> = [singleton_a, singleton_b]
        .iter()
        .map(|p| (*p as usize - base0) as u32)
        .collect();
    assert!(
        singleton_offs.contains(&head),
        "linked-list head ({head:#x}) must reference a singleton, not a run-member"
    );

    let drained = runstack_drain_all(batch[0], c);
    assert_eq!(drained.len(), 2, "two contiguous runs → two descriptors");
    let mut counts: Vec<u16> = drained.iter().map(|d| d.count).collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![2, 3], "run_a=3, run_b=2");

    for &p in &batch {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 5 — Guards still fire per-block (ring-DF'd block skipped).
//
// Carve 8 contiguous blocks. Ring-DF the middle one (sorted[3]) so its bitmap
// shows FREE while it's still in our batch. Flush the batch. The guard must
// SKIP sorted[3]; the remaining 7 split into two sub-runs ([0,1,2] and
// [4,5,6,7]) around the gap. The other blocks are NOT pre-freed, so the linked
// list contains only the blocks we just flushed (no pre-existing freelist
// entries to confuse the drain).
// ---------------------------------------------------------------------------

#[cfg(all(feature = "alloc-runfreelist", feature = "alloc-xthread"))]
#[test]
fn guards_skip_already_free_block() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    let buf = carve_contiguous(&mut core, c, 8);
    let base0 = seg_base(buf[0]);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    // Ring-DF sorted[3]: bitmap FREE, still in batch. No other frees happen,
    // so the only freelist entries after flush are the ones we just flushed.
    let already_free = sorted[3];
    assert!(core.dbg_push_to_ring(already_free, c), "ring push must succeed");
    core.dbg_drain_all_rings();
    assert!(core.dbg_is_free_for(already_free), "block must be FREE before flush");

    core.flush_class(c, &sorted);

    // The already-free block was SKIPPED by the flush guard (it did not go
    // through mark_free/write_next again, nor into any RunStack descriptor).
    // It IS legitimately on the freelist (the ring drain's reclaim put it
    // there), so the head MAY point at it — that is correct. The load-bearing
    // assertion is that it does NOT appear in any RunStack descriptor (it was
    // not re-accepted) and did not corrupt the run-detection for its neighbours.
    let already_free_off = (already_free as usize - base0) as u32;

    // Terminal: drain descriptors. Two sub-runs around the gap; the skipped
    // block must NOT be inside any descriptor's range.
    let descriptors = runstack_drain_all(already_free, c);
    assert_eq!(descriptors.len(), 2, "two sub-runs around the skipped block");
    for desc in &descriptors {
        let start = desc.start_off as usize;
        let end = start + (desc.count as usize) * 16;
        assert!(
            !(start..end).contains(&(already_free_off as usize)),
            "skipped block (off={already_free_off}) must NOT be in descriptor range [{start}..{end})"
        );
    }

    for &p in &buf {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 6 — `not(feature = "alloc-runfreelist")` byte-identical behaviour.
// ---------------------------------------------------------------------------

#[cfg(not(feature = "alloc-runfreelist"))]
#[test]
fn flush_run_classic_when_feature_off() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve directly (same as the feature-on tests) for a comparable batch.
    const N: usize = 8;
    let mut buf = vec![core::ptr::null_mut::<u8>(); N];
    assert_eq!(core.dbg_carve_batch(c, &mut buf), N);
    let base0 = seg_base(buf[0]);

    core.flush_class(c, &buf);

    for &p in &buf {
        assert!(core.dbg_is_free_for(p), "every block must be FREE (classic)");
    }

    let head = core.dbg_freelist_head_for(buf[0], c);
    let batch_offs: HashSet<u32> = buf.iter().map(|p| (*p as usize - base0) as u32).collect();
    assert!(batch_offs.contains(&head), "head must reference a batch block");

    let mut out = vec![core::ptr::null_mut::<u8>(); N + 4];
    let drained = core.dbg_drain_freelist_batch(buf[0], c, &mut out);
    assert_eq!(drained, N, "classic drain must return exactly N blocks");
    let unique: HashSet<usize> = out[..drained].iter().map(|p| *p as usize).collect();
    assert_eq!(unique.len(), N);
    let batch_set: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(unique, batch_set, "drained set == flushed set");

    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 7 — Run-member blocks NOT on linked-list drain (Ф2 intermediate state).
//
// Carve 6 contiguous blocks. Flush [0,1,2] (run, → RunStack) + [4] (singleton,
// → linked list). Blocks [3] and [5] are NOT freed and NOT in the batch — they
// stay LIVE, so they don't pollute the freelist. The classic drain should
// return ONLY the singleton (1 block); the run blocks are in the RunStack,
// undrainable until Ф3.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn run_member_blocks_not_on_linked_list_drain() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    let buf = carve_contiguous(&mut core, c, 6);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_blocks: Vec<*mut u8> = sorted[0..3].to_vec();
    let singleton = sorted[4];
    // sorted[3] and sorted[5] stay LIVE (allocated) — NOT freed, NOT in batch.

    let batch: Vec<*mut u8> = vec![run_blocks[0], run_blocks[1], run_blocks[2], singleton];
    core.flush_class(c, &batch);

    let peeked = runstack_peek(batch[0], c);
    assert!(peeked.is_some(), "one run → one descriptor");
    assert_eq!(peeked.unwrap().count, 3);

    // Classic drain returns ONLY the singleton (no pre-existing freelist
    // entries; the run blocks are in RunStack, not linked-list, until Ф3).
    let mut out = vec![core::ptr::null_mut::<u8>(); 8];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(
        drained, 1,
        "classic drain returns only the singleton (run blocks undrainable until Ф3)"
    );
    assert_eq!(out[0] as usize, singleton as usize);

    // Cleanup: drain gave us the singleton back (now allocated); re-free all
    // batch blocks + the leftover live blocks.
    core.dealloc(out[0], layout);
    for &p in &run_blocks {
        core.dealloc(p, layout);
    }
    core.dealloc(sorted[3], layout);
    core.dealloc(sorted[5], layout);
}
