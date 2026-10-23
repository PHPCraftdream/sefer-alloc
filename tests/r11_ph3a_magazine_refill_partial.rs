//! Ph3a magazine-refill partial-commit witnesses, restored for D1′ (round
//! fix 2). The former injection point (`SmallSidecar::fail_prepare_after_for_test`,
//! the class-leaf spill-OOM countdown) is GONE with the class leaves; the
//! only remaining fallible refill step is the OS (re)commit inside the carve
//! (`carve_block` / `try_carve_batch`: `recommit_pages` / grow-on-carve
//! `commit_pages`). The TEST-ONLY `lazy-commit-fault-injection` fixture
//! feature (the only edge to `aligned-vmem/fault-injection`, R2-19) arms the
//! real-path commit hook (`AllocCore::dbg_arm_commit_fail`), so the ONE-SHOT
//! refusal now lands on the refill carve's commit instead of the removed
//! prepare.
//!
//! Under `fastbin` a `HeapCore` small miss refills its magazine through the
//! batch producer that drains the substrate freelist BEFORE the carve. The
//! armed commit failure refuses the refill carve; the alloc-side
//! fresh-segment fallback (whose raw reserve initial commit is NOT on the
//! hooked `commit_range` path) recovers the call, so the deterministic
//! oracle is the ROLLBACK contract on the refused segment: bump, commit
//! frontier and freelist head of the anchor segment (whose batch commit was
//! refused) are byte-identical before and after — and the retry after the
//! disarm carves for real. Under the "carve does not roll the bump back on
//! commit failure" mutation the bump assert goes RED.
//!
//! Without `fastbin` the magazine is compiled out: the same commit refusal
//! surfaces on the scalar carve path of the anchor segment, with the same
//! rollback oracle and a disarmed retry that carves.
//!
//! NOTE (feature set): the run needs `small-segment-lazy-commit` so the
//! ordinary small segments' own grow commits travel the hooked
//! `commit_range` path (the fresh-segment rescue of a plain `production`
//! build reserves eagerly and can never be refused).

#![cfg(all(
    windows,
    not(miri),
    not(feature = "numa-aware"),
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals",
    feature = "small-segment-lazy-commit",
    feature = "lazy-commit-fault-injection"
))]
// Platform restriction rationale: the `frontier < SegmentLayout::SEGMENT`
// guard inside `fill_to_commit_frontier` is a non-vacuity witness, and
// `frontier == SEGMENT` holds BY CONSTRUCTION wherever the commit-refusal
// witness cannot exist — on every non-Windows target and under miri
// (`reserve.rs`: the lazy-commit frontier policy is not applied there) and
// under `numa-aware` on any OS (eager whole-segment commit). Outside
// `all(windows, not(miri), not(numa-aware))` this file would compile into a
// guaranteed-red assert on the ubuntu `--all-features` CI lines, where the
// witness is unreachable by design; the Windows CI line with
// `lazy-commit-fault-injection` (see `.github/workflows/ci.yml`) keeps the
// grep postcondition on the `... ok` line so a `#![cfg]`-emptied run cannot
// silently pass.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::SmallSidecar;
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

const CLASS_SIZE: usize = 32;

fn layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 16).unwrap()
}

/// Independent owner-state read of the segment serving `ptr`:
/// `(bump, live_count, class_head)`. A numeric observer only — no raw pointer
/// leaves this test.
fn owner_state(ptr: *mut u8, c: usize) -> (usize, u32, u32) {
    // SAFETY: the heap holding `ptr` stays claimed and exclusively owned by
    // this thread for the whole read, exactly as in the registry-sidecar
    // fixtures.
    unsafe { SmallSidecar::owner_state_for_test(ptr.addr(), c) }.unwrap()
}

/// The accounting identity the whole test defends: every block that left the
/// segment substrate is either caller-held or magazine-resident, so the
/// segment's live count is exactly their sum. `held` is non-empty and its
/// first entry names the segment under observation.
#[cfg(feature = "fastbin")]
fn assert_accounted(heap: &HeapCore, held: &[*mut u8], c: usize) {
    #[cfg(feature = "fastbin")]
    let resident = heap.dbg_tcache_count(c) as usize;
    #[cfg(not(feature = "fastbin"))]
    let resident = {
        let _ = heap; // no magazine exists without `fastbin`
        0
    };
    assert_eq!(
        owner_state(held[0], c).1 as usize,
        held.len() + resident,
        "live credits must equal caller-held plus magazine-resident blocks",
    );
}

/// Push the segment's bump cursor right up to the lazy-commit frontier so
/// the NEXT carve crosses it and must perform a real `commit_pages` call
/// (the armed fault hook's target). Without this, a carve inside the already
/// committed chunk would never touch the commit path and the one-shot
/// refusal would not fire.
fn fill_to_commit_frontier(
    heap: &mut HeapCore,
    anchor: *mut u8,
    c: usize,
    held: &mut Vec<*mut u8>,
) {
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard <= 1_000_000, "fill_to_commit_frontier: no frontier");
        let frontier = heap
            .dbg_committed_payload_end_for(anchor)
            .expect("anchor's small segment frontier");
        let (bump, _, _) = owner_state(anchor, c);
        assert!(bump <= frontier, "bump past the committed frontier");
        // A carve of one CLASS_SIZE block from `bump` needs a commit iff
        // `bump + CLASS_SIZE > frontier`.
        if bump + CLASS_SIZE > frontier {
            // P2-C vacuum guard: if the platform's lazy-commit policy
            // committed the whole segment up front (frontier == SEGMENT,
            // e.g. `numa-aware` or the Linux frontier choice), the armed
            // refusal can never land on a real `commit_pages` call and the
            // test would pass vacuously. Fail loudly instead.
            assert!(
                frontier < SegmentLayout::SEGMENT,
                "commit frontier reached the whole segment (frontier = \
                 {frontier} == SEGMENT): the lazy-commit refusal witness is \
                 vacuous on this platform/feature set — no commit can be \
                 refused",
            );
            return;
        }
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
}

#[cfg(feature = "fastbin")]
fn resident(heap: &HeapCore, c: usize) -> usize {
    heap.dbg_tcache_count(c) as usize
}

#[cfg(feature = "fastbin")]
fn take_one(heap: &mut HeapCore, held: &mut Vec<*mut u8>) -> *mut u8 {
    let ptr = held.pop().expect("held stock");
    // SAFETY: `ptr` came from this heap with this exact layout and is freed once.
    unsafe { heap.dealloc(ptr, layout()) };
    ptr
}

#[cfg(feature = "fastbin")]
#[test]
fn magazine_refill_commit_refusal_commits_only_the_drained_blocks() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let c = heap.dbg_class_for(layout()).expect("32B is a small class");
    let want = heap.dbg_refill_n_for_class(c);
    // `FREE_PARK_CAP[c] == refill_n_for_class(c)`, so `want` is also the
    // free-side park cap. The smallest class refills at TCACHE_CAP, which is
    // what distinguishes the "overflow" park policy (cap == TCACHE_CAP) from
    // the "one block straight to the substrate" policy (cap < TCACHE_CAP).
    let tcache_cap = heap.dbg_refill_n_for_class(
        heap.dbg_class_for(Layout::from_size_align(16, 16).unwrap())
            .expect("16B is a small class"),
    );
    assert!(
        want >= 2,
        "the partial-carve fixture needs a refill amount of at least 2 (got {want})",
    );
    let overflow = want == tcache_cap;

    // The anchor is never freed until cleanup; it only names the segment.
    let anchor = heap.alloc(layout());
    assert!(!anchor.is_null());
    let mut held: Vec<*mut u8> = vec![anchor];
    // Warm-up: enough held blocks that the magazine has been filled and
    // drained at least once, with an EMPTY substrate freelist.
    for _ in 0..(2 * want + 1) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    assert_accounted(heap, &held, c);
    assert_eq!(
        owner_state(anchor, c).2,
        u32::MAX,
        "the substrate freelist must start empty",
    );

    // Put the bump cursor within one block of the lazy-commit frontier so
    // the refill's carve MUST grow the frontier (one real `commit_pages`
    // call) — the exact point the one-shot refusal lands on. This MUST
    // happen BEFORE the substrate freelist is built: the fill allocs would
    // otherwise drain it, and the decisive refill would recover through a
    // fresh-segment fallback instead of committing only the drained batch.
    fill_to_commit_frontier(heap, anchor, c, &mut held);

    // Free exactly one block: it parks in the magazine (no overflow, so the
    // substrate stays empty), then drain the magazine with pure hits.
    take_one(heap, &mut held);
    for _ in 0..resident(heap, c) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    assert_eq!(resident(heap, c), 0, "the magazine must be empty here");
    assert_eq!(owner_state(anchor, c).2, u32::MAX);
    assert_accounted(heap, &held, c);

    // Overflow the park cap by exactly one block, which is what puts blocks
    // on the substrate freelist: `FLUSH_N` of them under the overflow policy
    // (`FLUSH_N = TCACHE_CAP / 2 = want / 2`), exactly one under the
    // straight-to-substrate policy.
    let k = if overflow { want / 2 } else { 1 };
    for _ in 0..(want + 1) {
        take_one(heap, &mut held);
    }
    // Drain the magazine back to zero; the substrate chain is untouched by
    // hits.
    for _ in 0..resident(heap, c) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    assert_eq!(resident(heap, c), 0, "the magazine must be empty here");
    assert_ne!(
        owner_state(anchor, c).2,
        u32::MAX,
        "the overflow must have put blocks on the substrate freelist",
    );
    assert!(
        (1..want).contains(&k),
        "the fixture must leave a short-but-nonempty freelist (k = {k}, want = {want})",
    );
    assert_accounted(heap, &held, c);

    // Arm ENOUGH refusals to also kill every fresh-segment rescue attempt:
    // each rescue reserve performs its own raw (unhooked) initial commit, so
    // no countdown depth can stop the refill from recovering through the
    // fresh-segment fallback. What IS deterministic — and is exactly the
    // rollback contract the refused commit must honor — is that the REFUSED
    // carve leaves its own segment byte-untouched: bump, frontier and the
    // freelist head of the anchor segment (whose batch commit failed) must
    // be identical before and after. Under the "carve does not roll the bump
    // back on commit failure" mutation the bump assert goes RED.
    let (bump_before, live_before, _) = owner_state(anchor, c);
    let frontier_before = heap.dbg_committed_payload_end_for(anchor);
    heap.dbg_arm_commit_fail(8);
    let fails_before = heap.dbg_commit_fail_count();
    let issued = heap.alloc(layout());
    assert!(
        !issued.is_null(),
        "the refill must still hand the caller one block (fresh-segment rescue)",
    );
    held.push(issued);

    // Rollback oracle on the refused segment: the batch carve's commit was
    // refused, so the anchor segment's bump cursor, commit frontier and
    // substrate freelist head are EXACTLY as before — the refused carve
    // moved nothing.
    let (bump_after, live_after, head_after) = owner_state(anchor, c);
    assert_eq!(bump_after, bump_before, "the refused carve moved no cursor");
    assert_eq!(
        heap.dbg_committed_payload_end_for(anchor),
        frontier_before,
        "the refused commit moved no frontier",
    );
    assert_eq!(
        head_after,
        u32::MAX,
        "the drained chain must end at FREE_LIST_NULL",
    );
    // The drained batch (k blocks from the ANCHOR segment's substrate
    // freelist) re-entered the live set on the anchor segment; the issued
    // block lives on whichever segment served it. (The fresh-segment rescue
    // legitimately registers a new route/sidecar, so the System-total
    // snapshot is deliberately NOT compared after the refusal.)
    //
    // Restored D1′ accounting witnesses (round fix 3): the live credits
    // grow by EXACTLY the drained batch size k, the partial refill parks
    // `want - 1` residents (the fresh-segment rescue carved the remaining
    // `want - k` blocks; `k - 1` of the drained blocks stayed parked, one
    // was issued to the caller), and the anchor-exact identity
    // `live == caller-held + its magazine residents` holds after the
    // refusal: `held` (the issued block included — the refill serves the
    // FIRST drained block, from the anchor) plus the anchor's `k - 1`
    // residents equals the anchor's live count. Under the "the refused
    // carve still takes its live credits" mutation (add_live before the
    // commit refusal) the first assert goes RED.
    assert_eq!(
        live_after,
        live_before + k as u32,
        "live credits grow by exactly the drained batch size",
    );
    assert_eq!(
        resident(heap, c),
        want - 1,
        "a partial refill parks want-1 blocks",
    );
    assert_eq!(
        live_after as usize,
        held.len() + (k - 1),
        "anchor live == caller-held + its magazine residents",
    );

    // P2-C witness: the injector must have ACTUALLY refused at least one
    // real commit; otherwise the rollback oracle above was asserted against
    // a call that never touched the commit path (vacuous green).
    assert!(
        heap.dbg_commit_fail_count() > fails_before,
        "the armed commit-failure injector never fired: the rollback oracle \
         above was vacuous",
    );

    // Disarm: drain the partial refill's leftovers, then a fresh magazine
    // miss must refill fully again (carve, since the substrate chain is
    // empty) — with the commit now proceeding for real.
    heap.dbg_arm_commit_fail(0);
    for _ in 0..resident(heap, c) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    // Per-segment accounting identity for BOTH segments the refusal spans
    // (restored D1′ witness, round fix 3): the magazine is empty here, so
    // each segment's live count must equal exactly its caller-held stock —
    // the anchor's and the fresh-segment rescue's alike. The final full
    // refill below then parks `want - 1` blocks again.
    {
        let base_anchor = anchor.addr() & !(SegmentLayout::SEGMENT - 1);
        let on = |seg: usize| {
            held.iter()
                .filter(|b| b.addr() & !(SegmentLayout::SEGMENT - 1) == seg)
                .count()
        };
        assert_eq!(
            owner_state(anchor, c).1 as usize,
            on(base_anchor),
            "anchor segment live == its caller-held stock",
        );
        let rescue = held
            .iter()
            .copied()
            .find(|b| b.addr() & !(SegmentLayout::SEGMENT - 1) != base_anchor)
            .expect("the rescue carved on a second segment");
        let base_rescue = rescue.addr() & !(SegmentLayout::SEGMENT - 1);
        assert_eq!(
            owner_state(rescue, c).1 as usize,
            on(base_rescue),
            "rescue segment live == its caller-held stock",
        );
    }
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null(), "the retry after the disarm must succeed");
    held.push(ptr);
    assert_eq!(
        resident(heap, c),
        want - 1,
        "a full refill parks want-1 blocks",
    );
    // The final full refill above parked `want - 1` blocks; the per-segment
    // identity for both refusal segments was asserted on the emptied
    // magazine just before it.

    while let Some(ptr) = held.pop() {
        // SAFETY: each block came from this heap with this exact layout.
        unsafe { heap.dealloc(ptr, layout()) };
    }
    drop(lease);
}

/// Without `fastbin` the magazine is compiled out, so the same commit
/// refusal surfaces on the scalar carve path of the anchor segment. The
/// alloc-side fresh-segment fallback recovers the call through an
/// unhooked raw-reserve segment (see the fastbin variant's header), so the
/// deterministic oracle is again the ROLLBACK contract: the refused carve
/// leaves the anchor segment's bump/live/head and commit frontier exactly
/// as before, and the disarmed retry carves for real.
#[cfg(not(feature = "fastbin"))]
#[test]
fn magazine_refill_commit_refusal_is_a_scalar_oom_with_a_full_rollback() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let c = 1usize; // the 32B class, per the shared fixture layout.
    let anchor = heap.alloc(layout());
    assert!(!anchor.is_null());
    let mut held = vec![anchor];

    // Put the bump cursor within one block of the lazy-commit frontier so
    // the decisive carve must call `commit_pages` — the armed refusal's
    // target.
    fill_to_commit_frontier(heap, anchor, c, &mut held);

    // Drain any stray substrate freelist block so the decisive alloc truly
    // CARVES (a freelist pop would bypass the commit path entirely).
    let mut drain_guard = 0;
    while owner_state(anchor, c).2 != u32::MAX {
        drain_guard += 1;
        assert!(drain_guard <= 100_000, "stray freelist never drained");
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }

    // Snapshot the rollback oracle AFTER the fill: the fill blocks live on
    // the anchor segment too. (The fresh-segment fallback legitimately
    // registers a new route/sidecar, so the System-total snapshot is
    // deliberately NOT compared after the refusal.)
    let (bump, live, head) = owner_state(anchor, c);
    let frontier_before = heap
        .dbg_committed_payload_end_for(anchor)
        .expect("anchor commit frontier");

    // Arm enough refusals for the decisive carve_block's grow commit and any
    // subsequent commit on the anchor segment.
    heap.dbg_arm_commit_fail(8);
    let fails_before = heap.dbg_commit_fail_count();
    let served = heap.alloc(layout());
    assert!(
        !served.is_null(),
        "the alloc must recover (fresh-segment fallback)",
    );
    held.push(served);

    // Rollback oracle on the refused segment: identical bump/live/head and
    // commit frontier — the refused carve moved nothing, and the recovered
    // block lives on the fallback segment. Under the "carve
    // does not roll the bump back on commit failure" mutation the bump
    // assert goes RED.
    let (bump_after, live_after, head_after) = owner_state(anchor, c);
    assert_eq!(
        (bump_after, live_after, head_after),
        (bump, live, head),
        "the refusal must not move bump/live/head on the refused segment",
    );
    assert_eq!(
        heap.dbg_committed_payload_end_for(anchor),
        Some(frontier_before),
        "отказ commit не изменяет frontier исходного сегмента",
    );
    // Restored D1′ accounting witness (round fix 3): per-segment identity
    // right after the refusal, for BOTH segments — the anchor's live
    // credits equal exactly its caller-held stock, and so does the rescue
    // segment's (the fresh-segment fallback legitimately registers a new
    // route, so the System-total snapshot is deliberately NOT compared).
    {
        let base_anchor = anchor.addr() & !(SegmentLayout::SEGMENT - 1);
        let base_rescue = served.addr() & !(SegmentLayout::SEGMENT - 1);
        assert_ne!(
            base_rescue, base_anchor,
            "the rescue must serve from a fresh segment",
        );
        let on = |seg: usize| {
            held.iter()
                .filter(|b| b.addr() & !(SegmentLayout::SEGMENT - 1) == seg)
                .count()
        };
        assert_eq!(
            live_after as usize,
            on(base_anchor),
            "anchor segment live == its caller-held stock",
        );
        assert_eq!(
            owner_state(served, c).1 as usize,
            on(base_rescue),
            "rescue segment live == its caller-held stock",
        );
    }

    // P2-C witness: the injector must have ACTUALLY refused at least one
    // real commit; otherwise the rollback oracle above was asserted against
    // a call that never touched the commit path (vacuous green).
    assert!(
        heap.dbg_commit_fail_count() > fails_before,
        "the armed commit-failure injector never fired: the rollback oracle \
         above was vacuous",
    );

    // Disarm: the next alloc carves for real (from the fallback segment the
    // rescue published as the cursor).
    heap.dbg_arm_commit_fail(0);
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null(), "the retry after the disarm carves");
    held.push(ptr);
    // Per-segment accounting identity: the anchor segment's live count is
    // exactly its caller-held stock (the fallback-segment blocks excluded).
    let base_anchor = anchor.addr() & !(SegmentLayout::SEGMENT - 1);
    let on_anchor = held
        .iter()
        .filter(|b| b.addr() & !(SegmentLayout::SEGMENT - 1) == base_anchor)
        .count();
    assert_eq!(
        owner_state(anchor, c).1 as usize,
        on_anchor,
        "anchor segment live == its caller-held stock",
    );

    while let Some(ptr) = held.pop() {
        // SAFETY: each block came from this heap with this exact layout.
        unsafe { heap.dealloc(ptr, layout()) };
    }
    drop(lease);
}
