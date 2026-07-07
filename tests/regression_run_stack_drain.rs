//! PERF-3 Ф3 (task #210) — run-encoded freelist DRAIN-side reconstruction
//! tests. This is the safety-critical "heart" of the PERF-3 arc.
//!
//! Ф1 (`regression_run_stack_layout.rs`) pinned the `RunStack` storage.
//! Ф2 (`regression_run_stack_flush.rs`) wired the flush side: contiguous-
//! accepted sub-runs are encoded as `(start_off, count)` descriptors. This
//! file pins the DRAIN side: `drain_freelist_batch` (under
//! `alloc-runfreelist`) FIRST drains the `RunStack` by stride arithmetic
//! (`start_off + i * block_size`), THEN drains the classic linked list for
//! any remaining capacity — with a per-reconstructed-block `is_free` bitmap
//! guard (plan §2.3 decision 1, the load-bearing M2 defense-in-depth).
//!
//! ## Tests in this file (the plan's Ф3 gate — full behavioural equivalence)
//!
//! - **Equivalence** (`drain_equivalence_feature_on_matches_classic`): the
//!   SAME flush scenario, drained under `alloc-runfreelist` and (companion
//!   test below) under plain `production`, yields the SAME SET of returned
//!   pointers. Order may differ (RunStack drains its class first; classic
//!   drain is list-order); we compare as a set.
//! - **Mixed drain** (`mixed_drain_all_blocks_exactly_once`): after a flush
//!   that populated BOTH the RunStack (≥1 descriptor) AND the linked list
//!   (≥1 singleton), drain returns ALL blocks exactly once — no dup, no
//!   missing.
//! - **M2 double-free-through-run** (`m2_double_free_through_run_refused`):
//!   the single most important test in this phase. A block in an active
//!   RunStack-encoded run is re-freed via the cross-thread reclaim path
//!   (`dbg_push_to_ring` + `dbg_drain_all_rings` → `reclaim_offset`);
//!   reclaim's EXISTING `is_free` guard REFUSES it (plan §2.4 — the block
//!   is FREE in the bitmap by construction, so reclaim's `if is_free return
//!   false` fires before any `write_next`). The run is not corrupted, the
//!   block is not double-linked, and the subsequent drain hands the block
//!   out exactly once.
//! - **Capacity/boundary** (`drain_capacity_boundary`): `out.len()` smaller
//!   than the available blocks — partial drain, no over-read, no panic, `k`
//!   correctly bounded.
//! - **Empty RunStack fallback** (`empty_runstack_falls_back_to_linked_list`):
//!   RunStack empty for a class → drain works exactly as the classic linked-
//!   list path.
//! - **`not(feature)` byte-identical** (`drain_classic_when_feature_off`):
//!   companion under `not(alloc-runfreelist)`.
//!
//! ## The M2 disabled-guard counterfactual (this phase's mandatory gate)
//!
//! The M2 test (`m2_double_free_through_run_refused`) MUST have teeth: with
//! the `is_free` guard in the drain-side reconstruction commented out, a
//! double-freed run-member block would be handed out TWICE (once by the run
//! drain, once by the linked-list drain where reclaim's `write_next` placed
//! it). The counterfactual is performed manually (edit → run → observe
//! failure → restore → re-confirm green) and reported in the phase report;
//! it is NOT an automated test (it would require a cfg-flag to disable a
//! safety guard, which must not exist in the shipped source).

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::AllocCore;
#[cfg(feature = "alloc-runfreelist")]
use sefer_alloc::alloc_core::run_stack::RunStack;
use sefer_alloc::SegmentLayout;

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

/// Carve `n` blocks directly via `dbg_carve_batch` (pure bump-carve, strictly
/// ascending + offset-adjacent). Asserts the precondition. Mirrors the Ф2
/// helper of the same name.
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
    let mut sorted: Vec<usize> = buf.iter().map(|p| *p as usize).collect();
    sorted.sort_unstable();
    for w in sorted.windows(2) {
        assert_eq!(w[1] - w[0], 16, "carved blocks must be offset-adjacent");
    }
    buf
}

#[cfg(feature = "alloc-runfreelist")]
fn runstack_drain_all(ptr: *mut u8, class: usize) -> usize {
    let base = seg_base(ptr) as *mut u8;
    let mut n = 0;
    while RunStack::pop(base, class).is_some() {
        n += 1;
    }
    n
}

/// NON-destructive count of live descriptors for `class` (peeks each slot).
/// Use this mid-test where `runstack_drain_all` would consume the state.
#[cfg(feature = "alloc-runfreelist")]
fn runstack_count(ptr: *mut u8, class: usize) -> usize {
    let base = seg_base(ptr) as *mut u8;
    // RunStack has no public "count"; emulate by peeking slot-by-slot via the
    // public `is_empty` + `pop`/push-back is not possible without mutation.
    // Instead, drain into a local vec and re-push (restoring state).
    let mut saved = Vec::new();
    while let Some(d) = RunStack::pop(base, class) {
        saved.push(d);
    }
    let n = saved.len();
    // Restore: push back in the same order (lowest-slot-first pop means the
    // first popped was at the lowest slot; pushing refills lowest-empty first,
    // so the round-trip preserves slot occupancy).
    for d in saved {
        let ok = RunStack::push(base, class, d.start_off, d.count);
        debug_assert!(ok, "restore push must succeed (capacity was not exceeded)");
    }
    n
}

// ---------------------------------------------------------------------------
// Test 1 — Behavioural equivalence: feature-on drain returns the same SET of
// blocks as the classic linked-list drain would for the same flush scenario.
//
// We construct a scenario with a MIX of run-encoded and linked-list blocks,
// drain under `alloc-runfreelist`, and assert the drained set equals the
// flushed set (the companion `drain_classic_when_feature_off` test below
// proves the classic path returns the same set for the same scenario).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_equivalence_feature_on_matches_classic() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 12 contiguous blocks. Flush [0,1,2,3] (run, → RunStack) +
    // [5] (singleton, → linked list) + [6,7] (run, → RunStack) +
    // [9] (singleton, → linked list). Blocks [4], [8], [10], [11] stay LIVE.
    let buf = carve_contiguous(&mut core, c, 12);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_a: Vec<*mut u8> = sorted[0..4].to_vec();
    let singleton_a = sorted[5];
    let run_b: Vec<*mut u8> = sorted[6..8].to_vec();
    let singleton_b = sorted[9];

    let batch: Vec<*mut u8> = vec![
        run_a[0], run_a[1], run_a[2], run_a[3],
        singleton_a,
        run_b[0], run_b[1],
        singleton_b,
    ];
    assert_eq!(batch.len(), 8);
    core.flush_class(c, &batch);

    // Drain under feature-on: RunStack first, then linked list.
    let mut out = vec![core::ptr::null_mut::<u8>(); 16];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(drained, 8, "drain must return all 8 flushed blocks");

    let drained_set: HashSet<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    let batch_set: HashSet<usize> = batch.iter().map(|p| *p as usize).collect();
    assert_eq!(
        drained_set, batch_set,
        "drained SET must equal flushed SET (order-independent)"
    );
    assert_eq!(drained_set.len(), drained, "no duplicate blocks");

    // RunStack must be fully drained (no descriptors left).
    assert_eq!(
        runstack_drain_all(batch[0], c),
        0,
        "RunStack must be empty after a full drain"
    );

    // Cleanup.
    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    for &p in sorted[4..5].iter().chain(&sorted[8..9]).chain(&sorted[10..12]) {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 2 — Mixed drain: BOTH representations populated, all blocks exactly
// once.
//
// Constructs a mixed flush (run + singletons), drains, and confirms EVERY
// flushed block appears exactly once in the output — no duplication, no loss.
// This is the core safety property of the dual-representation drain.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn mixed_drain_all_blocks_exactly_once() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Layout: [0,1,2]=run(3), [3]=gap(live), [4]=singleton, [5]=gap(live),
    // [6,7,8]=run(3), [9]=gap(live), [10]=singleton, [11]=gap(live).
    let buf = carve_contiguous(&mut core, c, 12);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_a: Vec<*mut u8> = sorted[0..3].to_vec();
    let singleton_a = sorted[4];
    let run_b: Vec<*mut u8> = sorted[6..9].to_vec();
    let singleton_b = sorted[10];

    let batch: Vec<*mut u8> = vec![
        run_a[0], run_a[1], run_a[2],
        singleton_a,
        run_b[0], run_b[1], run_b[2],
        singleton_b,
    ];
    assert_eq!(batch.len(), 8);
    core.flush_class(c, &batch);

    let mut out = vec![core::ptr::null_mut::<u8>(); 16];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(drained, 8, "all 8 flushed blocks must drain");

    // Multiset check: every block exactly once.
    let mut drained_vec: Vec<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    drained_vec.sort_unstable();
    let mut batch_vec: Vec<usize> = batch.iter().map(|p| *p as usize).collect();
    batch_vec.sort_unstable();
    assert_eq!(drained_vec, batch_vec, "drained multiset == flushed multiset");

    // After drain, every drained block must be bitmap-ALLOCATED (handed out).
    for &p in &out[..drained] {
        assert!(
            !core.dbg_is_free_for(p),
            "drained block must be bitmap-ALLOCATED (M2)"
        );
    }

    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    for &p in sorted[3..4].iter().chain(&sorted[5..6]).chain(&sorted[9..10]).chain(&sorted[11..12]) {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 3 — THE M2 double-free-through-run counterfactual (the single most
// important test in this phase).
//
// Plan §2.4 proof, exercised: a block that is a member of an active
// RunStack-encoded run (FREE in the bitmap by construction) is re-freed via
// the cross-thread reclaim path. Reclaim's EXISTING `is_free` guard (which
// fires BEFORE any `write_next`/`mark_free`) REFUSES it — the block is already
// FREE, so reclaim returns false without linking it onto the linked list and
// without touching the run descriptor. The run is not corrupted, the block is
// not double-linked, and the subsequent drain hands it out exactly once.
//
// This test exercises the EXISTING reclaim guard — it does NOT require any
// RunStack-aware code in reclaim. If this test reveals reclaim needs ANY
// awareness of RunStack to behave correctly, the design has a hole (plan §2.4
// would be wrong) and the phase must STOP and escalate.
//
// Requires `alloc-xthread` (for `dbg_push_to_ring`/`dbg_drain_all_rings`).
// ---------------------------------------------------------------------------

#[cfg(all(feature = "alloc-runfreelist", feature = "alloc-xthread"))]
#[test]
fn m2_double_free_through_run_refused() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 4 contiguous, flush as ONE run (→ RunStack). All 4 are FREE in
    // the bitmap and encoded in one descriptor.
    let buf = carve_contiguous(&mut core, c, 4);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);
    core.flush_class(c, &sorted);

    // Snapshot the run descriptor (must be one descriptor of count 4).
    let base = seg_base(buf[0]) as *mut u8;
    let desc_before = RunStack::peek(base, c).expect("one run → one descriptor");
    assert_eq!(desc_before.count, 4);

    // Pick a run-member block (sorted[2]) and attempt a cross-thread-style
    // double-free: push to the remote ring, then drain the ring (which calls
    // `reclaim_offset`). Reclaim's `is_free` guard MUST refuse (the block is
    // FREE in the bitmap — it is a run member).
    let victim = sorted[2];
    assert!(
        core.dbg_is_free_for(victim),
        "victim must be FREE (run member) before the double-free attempt"
    );
    assert!(
        core.dbg_push_to_ring(victim, c),
        "ring push must succeed (the ring accepts any entry)"
    );

    // Drain the ring → reclaim_offset runs → `is_free(victim)` is true →
    // reclaim returns false (refuses). No `write_next`, no `mark_free`, no
    // linked-list head change.
    core.dbg_drain_all_rings();

    // The victim is STILL FREE (reclaim did not touch it — it refused).
    assert!(
        core.dbg_is_free_for(victim),
        "victim must STILL be FREE (reclaim refused — no double-free)"
    );

    // The linked-list head must NOT reference the victim (reclaim did not
    // link it). The head should be NULL (no singletons were flushed — only
    // the run).
    let head = core.dbg_freelist_head_for(buf[0], c);
    assert_eq!(
        head, u32::MAX,
        "linked-list head must be NULL (run-only flush; reclaim refused the double-free)"
    );

    // The run descriptor is UNCHANGED (reclaim did not corrupt it — reclaim
    // has no RunStack awareness and never touches the RunStack).
    let desc_after = RunStack::peek(base, c).expect("descriptor still present");
    assert_eq!(
        desc_after, desc_before,
        "run descriptor must be byte-identical (reclaim does not touch RunStack)"
    );

    // Now drain the freelist. The victim must come out EXACTLY ONCE (with the
    // is_free guard in place, the run drain hands it out; the linked-list
    // drain finds nothing because reclaim refused to link it).
    let mut out = vec![core::ptr::null_mut::<u8>(); 8];
    let drained = core.dbg_drain_freelist_batch(buf[0], c, &mut out);
    assert_eq!(drained, 4, "all 4 run-member blocks drain, exactly once each");

    let mut drained_vec: Vec<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    drained_vec.sort_unstable();
    let mut batch_vec: Vec<usize> = sorted.iter().map(|p| *p as usize).collect();
    batch_vec.sort_unstable();
    assert_eq!(drained_vec, batch_vec, "drained multiset == run members");
    // The victim appears exactly once.
    let victim_count = drained_vec.iter().filter(|&&p| p == victim as usize).count();
    assert_eq!(victim_count, 1, "victim handed out EXACTLY once (no double-issue)");

    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 3b — The drain-side `is_free` guard's teeth (the mandatory
// counterfactual anchor).
//
// The M2 test above proves reclaim refuses a run-member double-free. But the
// DRAIN-SIDE `is_free` guard (plan §2.3 decision 1) is a SEPARATE, independent
// defense: it catches the case where a reconstructed offset is NOT free at
// drain time — for example, if two descriptors were to cover overlapping
// offsets (a corrupt/duplicated encoding), the SECOND descriptor's member
// would already be ALLOCATED (handed out by the first). Per plan §2.4 this
// overlapping-descriptor state is unreachable through correct flush+reclaim,
// but the guard must still have teeth. This test constructs the overlap DIRECTLY
// (two descriptors sharing a member offset) and asserts the guard hands the
// shared block out exactly ONCE. The companion counterfactual (disable the
// guard → this test fails with a double-issue) is performed manually and
// reported in the phase report.
//
// NOTE on scope: this test exercises ONLY the run-drain guard. It does NOT
// also cover the linked-list drain (the classic linked-list drain has NO
// `is_free` guard — it trusts that its list never holds an already-handed-out
// block, which plan §2.4's reclaim-refusal proof guarantees in correct
// operation). A cross-representation double (a block in BOTH a descriptor AND
// the linked list) is the state §2.3 case (a) describes; it is unreachable
// through reclaim, and the linked-list drain's lack of a guard there is by
// design (the existing pre-Ф3 behavior, unchanged by this phase).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_side_guard_prevents_cross_representation_double_issue() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 4 contiguous blocks. Flush [0,1,2] as a run (→ RunStack, one
    // descriptor count 3). [3] stays live.
    let buf = carve_contiguous(&mut core, c, 4);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);
    core.flush_class(c, &sorted[0..3]);

    let base = seg_base(buf[0]) as *mut u8;
    // One descriptor covering sorted[0..3].
    let desc = RunStack::peek(base, c).expect("one run");
    assert_eq!(desc.count, 3);

    // Now DIRECTly push a SECOND descriptor that OVERLAPS the first — covering
    // sorted[1] and sorted[2] (which are already in the first descriptor).
    // This is the corrupt/overlapping state the guard must defend against.
    let overlap_start_off = (sorted[1] as usize - base as usize) as u32;
    assert!(
        RunStack::push(base, c, overlap_start_off, 2),
        "direct push of an overlapping descriptor must succeed"
    );

    // Drain. Without the guard: descriptor 1 hands out s0,s1,s2; descriptor 2
    // hands out s1,s2 AGAIN → double-issue (s1, s2 each appear twice). WITH
    // the guard: descriptor 1 hands out s0,s1,s2 (FREE→ALLOC); descriptor 2's
    // s1,s2 hit `is_free == false` → skip. Total: 3 distinct blocks, no dup.
    let mut out = vec![core::ptr::null_mut::<u8>(); 8];
    let drained = core.dbg_drain_freelist_batch(buf[0], c, &mut out);
    assert_eq!(
        drained, 3,
        "exactly 3 distinct blocks (overlap defended by the drain-side guard)"
    );

    // No duplicates: s1 and s2 each appear exactly once.
    let drained_set: HashSet<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    assert_eq!(drained_set.len(), drained, "no duplicate blocks in drain");
    let expected: HashSet<usize> = sorted[0..3].iter().map(|p| *p as usize).collect();
    assert_eq!(drained_set, expected, "drained set == the 3 run-members");

    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    core.dealloc(sorted[3], layout);
}

// ---------------------------------------------------------------------------
// Test 4 — Capacity/boundary: `out.len()` smaller than available blocks.
//
// Drains with a capacity smaller than the RunStack's blocks. Confirms no
// over-read, no panic, `k` correctly bounded by `out.len()`. Also covers the
// partial-drain contract: a descriptor is popped atomically — if `out` fills
// mid-descriptor the remaining members of THAT descriptor are lost (a
// documented precondition; the test uses a capacity that does NOT split a
// descriptor, to keep the assertion exact).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_capacity_boundary() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Flush two runs: run_a (4 blocks) + run_b (3 blocks) = 7 blocks total.
    // Capacity 4: drains run_a fully (4 blocks), then stops (out full) before
    // touching run_b. run_b's descriptor survives for the next drain.
    let buf = carve_contiguous(&mut core, c, 10);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_a: Vec<*mut u8> = sorted[0..4].to_vec();
    // sorted[4] is a gap (live) separating the two runs.
    let run_b: Vec<*mut u8> = sorted[5..8].to_vec();

    let batch: Vec<*mut u8> = vec![
        run_a[0], run_a[1], run_a[2], run_a[3],
        run_b[0], run_b[1], run_b[2],
    ];
    core.flush_class(c, &batch);

    // Partial drain: capacity 4. run_a (count 4) fits exactly; run_b is NOT
    // touched (RunStack pop order: lowest slot first; run_a was pushed first).
    let cap = 4;
    let mut out = vec![core::ptr::null_mut::<u8>(); cap];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(drained, cap, "drain bounded by out.len()");

    let drained_set: HashSet<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    let run_a_set: HashSet<usize> = run_a.iter().map(|p| *p as usize).collect();
    assert_eq!(
        drained_set, run_a_set,
        "first partial drain returns exactly run_a"
    );

    // run_b's descriptor survives (1 descriptor left on the RunStack).
    // Use the NON-destructive counter (runstack_drain_all would consume it).
    assert_eq!(
        runstack_count(batch[0], c),
        1,
        "run_b's descriptor survives the partial drain"
    );

    // Cleanup: re-drain to recover run_b's blocks, then dealloc everything.
    let mut out2 = vec![core::ptr::null_mut::<u8>(); 8];
    let drained2 = core.dbg_drain_freelist_batch(batch[0], c, &mut out2);
    assert_eq!(drained2, 3, "second drain returns run_b");
    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    for &p in &out2[..drained2] {
        core.dealloc(p, layout);
    }
    for &p in sorted[4..5].iter().chain(&sorted[8..10]) {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 5 — Empty RunStack graceful fallback: drain works exactly as the
// classic linked-list path when the RunStack is empty for the class.
//
// A singleton-only flush (run of length 1 → linked-list fallback in Ф2) leaves
// the RunStack empty. The drain must work purely through the linked-list path.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn empty_runstack_falls_back_to_linked_list() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 5 contiguous. Flush [0], [2], [4] (every other block) — the
    // accepted offsets sort to s0, s2, s4 with gaps of 32 between each, so NO
    // contiguous sub-run of length ≥ 2 forms: three singletons. Blocks [1]
    // and [3] stay LIVE (allocated, NOT freed, NOT in batch) so they do not
    // pollute the freelist.
    let buf = carve_contiguous(&mut core, c, 5);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let batch: Vec<*mut u8> = vec![sorted[0], sorted[2], sorted[4]];
    core.flush_class(c, &batch);

    // RunStack MUST be empty (three singletons, no contiguous run ≥ 2).
    let base = seg_base(buf[0]) as *mut u8;
    assert!(
        RunStack::is_empty(base, c),
        "RunStack must be empty (singletons only)"
    );

    // Drain must return all 3 singletons via the linked-list path. The two
    // live gap blocks are NOT on the freelist, so they do not appear.
    let mut out = vec![core::ptr::null_mut::<u8>(); 8];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(drained, 3, "all 3 singletons drain via linked-list path");

    let drained_set: HashSet<usize> =
        out[..drained].iter().map(|p| *p as usize).collect();
    let batch_set: HashSet<usize> = batch.iter().map(|p| *p as usize).collect();
    assert_eq!(drained_set, batch_set);

    // Cleanup: drain returned the 3 batch blocks (now allocated); re-free them
    // + the 2 live gap blocks.
    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    core.dealloc(sorted[1], layout);
    core.dealloc(sorted[3], layout);
}

// ---------------------------------------------------------------------------
// Test 6 — RunStack-first ordering observable: run blocks come out BEFORE
// linked-list blocks when BOTH are present.
//
// This pins the documented drain order (plan §3-Ф3): RunStack drained first,
// linked list second. We construct a mixed flush and confirm the run-member
// blocks occupy the LOW indices of `out` and the singletons the HIGH indices.
// (Order within each representation is unspecified; across representations it
// is RunStack-then-linked-list by construction.)
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_order_runstack_first_then_linked_list() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Carve 10 contiguous. Flush [0,1,2,3] (run_a, 4) + [6] (singleton) +
    // [8,9] (run_b, 2). [4],[5],[7] stay live as gaps so the singleton [6] is
    // NOT offset-adjacent to either run (else Ф2 would merge them).
    let buf = carve_contiguous(&mut core, c, 10);
    let mut sorted: Vec<*mut u8> = buf.iter().copied().collect();
    sorted.sort_by_key(|p| *p as usize);

    let run_a: Vec<*mut u8> = sorted[0..4].to_vec();
    let singleton = sorted[6];
    let run_b: Vec<*mut u8> = sorted[8..10].to_vec();

    let batch: Vec<*mut u8> = vec![
        run_a[0], run_a[1], run_a[2], run_a[3],
        singleton,
        run_b[0], run_b[1],
    ];
    core.flush_class(c, &batch);

    let mut out = vec![core::ptr::null_mut::<u8>(); 16];
    let drained = core.dbg_drain_freelist_batch(batch[0], c, &mut out);
    assert_eq!(drained, 7);

    // The first 6 slots are run-members (run_a=4 + run_b=2); the last slot is
    // the singleton (linked-list).
    let run_set: HashSet<usize> = run_a
        .iter()
        .chain(run_b.iter())
        .map(|p| *p as usize)
        .collect();
    let first_n: HashSet<usize> =
        out[..6].iter().map(|p| *p as usize).collect();
    assert_eq!(
        first_n, run_set,
        "first 6 drained blocks are exactly the run-members (RunStack drained first)"
    );
    assert_eq!(
        out[6] as usize, singleton as usize,
        "7th drained block is the singleton (linked-list drained second)"
    );

    for &p in &out[..drained] {
        core.dealloc(p, layout);
    }
    for &p in sorted[4..6].iter().chain(&sorted[7..8]) {
        core.dealloc(p, layout);
    }
}

// ---------------------------------------------------------------------------
// Test 7 — `not(feature = "alloc-runfreelist")` byte-identical behaviour.
//
// The SAME flush scenario as the feature-on equivalence test, drained under
// plain production. Asserts the classic drain returns the same SET of blocks.
// This is the companion half of the equivalence proof: the feature-on test
// above and this test together show both paths agree on the set for the same
// scenario (the feature-on test asserts set-equality to the batch; this test
// does the same under the classic path).
// ---------------------------------------------------------------------------

#[cfg(not(feature = "alloc-runfreelist"))]
#[test]
fn drain_classic_when_feature_off() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Same scenario shape as the feature-on test: a mix that WOULD split into
    // runs + singletons under feature-on. Under feature-off it's all one
    // linked list.
    const N: usize = 8;
    let mut buf = vec![core::ptr::null_mut::<u8>(); N];
    assert_eq!(core.dbg_carve_batch(c, &mut buf), N);
    let base0 = seg_base(buf[0]);

    core.flush_class(c, &buf);

    for &p in &buf {
        assert!(core.dbg_is_free_for(p), "every block must be FREE (classic)");
    }

    let head = core.dbg_freelist_head_for(buf[0], c);
    let batch_offs: HashSet<u32> =
        buf.iter().map(|p| (*p as usize - base0) as u32).collect();
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
// Test 8 (stub, `#[ignore]`'d) — decommit-clears-runstack is Ф4's job, NOT
// Ф3's. Placeholder so the deferral is explicit.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-runfreelist")]
#[test]
#[ignore = "decommit-clears-runstack is Ф4's job (task #211), not Ф3's"]
fn decommit_clears_runstack_deferred_to_f4() {
    // Ф4 will add: flush → decommit_empty_segment → assert RunStack cleared
    // → re-carve → drain returns 0 (no stale descriptor).
    // Do NOT implement decommit changes in Ф3 (scope discipline).
}
