#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ph3a magazine-refill witnesses. Under `fastbin` a `HeapCore` small miss
//! refills its magazine through `AllocCore::refill_class_bump` — the batch
//! producer that runs every fallible prepare (`try_drain_freelist_batch` /
//! `try_carve_batch`) BEFORE the first mutation. A prepare refusal therefore
//! either truncates the refill (a PARTIAL batch: everything drained commits,
//! the refused carve does not) or, when it hits the first prepare, returns 0
//! and the rescue retry decides the outcome. Either way the accounting
//! identity `segment live == caller-held + magazine-resident` must hold after
//! every step, and the retry after the one-shot disarm succeeds.
//!
//! Without `fastbin` the magazine (and with it the batch refill) is compiled
//! out: every small alloc reaches `AllocCore::alloc_small`, so the same
//! refusal surfaces on the scalar carve path as a plain OOM — there is no
//! partial refill to observe in that configuration (the second test below
//! asserts the identical rollback/retry contract).

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, SmallSidecar};
use sefer_alloc::registry::{HeapCore, HeapRegistry};

const CLASS_SIZE: usize = 32;

fn layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 16).unwrap()
}

/// Independent owner-state read of the segment serving `ptr`:
/// `(bump, live_count, class_head)`. A numeric observer only — no raw pointer
/// leaves this test.
fn owner_state(ptr: *mut u8, c: usize) -> (usize, u32, u32) {
    // SAFETY: the heap holding `ptr` stays claimed and exclusively owned by
    // this thread for the whole read, exactly as `support.rs` of
    // `r11_p3_small_sidecar_issue_oom` does.
    unsafe { SmallSidecar::owner_state_for_test(ptr.addr(), c) }.unwrap()
}

/// The accounting identity the whole test defends: every block that left the
/// segment substrate is either caller-held or magazine-resident, so the
/// segment's live count is exactly their sum. `held` is non-empty and its
/// first entry names the segment under observation.
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

#[cfg(feature = "fastbin")]
fn resident(heap: &HeapCore, c: usize) -> usize {
    heap.dbg_tcache_count(c) as usize
}

#[cfg(feature = "fastbin")]
fn take_one(heap: &mut HeapCore, held: &mut Vec<*mut u8>, layout: Layout) -> *mut u8 {
    let ptr = held.pop().expect("held stock");
    // SAFETY: `ptr` came from this heap with this exact layout and is freed once.
    unsafe { heap.dealloc(ptr, layout) };
    ptr
}

#[cfg(feature = "fastbin")]
#[test]
fn magazine_refill_prepare_refusal_commits_only_the_drained_blocks() {
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

    // Free exactly one block: it parks in the magazine (no overflow, so the
    // substrate stays empty), then drain the magazine with pure hits.
    take_one(heap, &mut held, layout());
    for _ in 0..resident(heap, c) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    assert_eq!(resident(heap, c), 0, "the magazine must be empty here");
    assert_eq!(owner_state(anchor, c).2, u32::MAX);
    assert_accounted(heap, &held, c);

    // Overflow the park cap by exactly one block, which is what puts blocks on
    // the substrate freelist: `FLUSH_N` of them under the overflow policy
    // (`FLUSH_N = TCACHE_CAP / 2 = want / 2`), exactly one under the
    // straight-to-substrate policy.
    let k = if overflow { want / 2 } else { 1 };
    for _ in 0..(want + 1) {
        take_one(heap, &mut held, layout());
    }
    // Drain the magazine back to zero; the substrate chain is untouched by hits.
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

    // Arm the refusal so it lands on the CARVE prepare of the next refill: the
    // drain's `k` prepares must still succeed, the carve's first one fails.
    let system = SmallSidecar::system_totals_for_test();
    let census = RouteDirectory::global().live_route_census_for_test();
    let table = heap.dbg_table_count();
    let (bump_before, live_before, _) = owner_state(anchor, c);
    SmallSidecar::fail_prepare_after_for_test(k + 1);
    let issued = heap.alloc(layout());
    assert!(
        !issued.is_null(),
        "a partial refill must still hand the caller one block",
    );
    held.push(issued);

    // The refill committed exactly the `k` drained blocks: `k - 1` stayed in
    // the magazine, `1` was issued, nothing was carved.
    let (bump_after, live_after, head_after) = owner_state(anchor, c);
    assert_eq!(
        resident(heap, c),
        k - 1,
        "a partial refill parks k-1 of the k drained blocks",
    );
    assert_eq!(
        live_after,
        live_before + k as u32,
        "live credits grow by exactly the drained batch size",
    );
    assert_eq!(bump_after, bump_before, "the refused carve moved no cursor");
    assert_eq!(
        head_after,
        u32::MAX,
        "the drained chain must end at FREE_LIST_NULL",
    );
    assert_accounted(heap, &held, c);
    assert_eq!(
        SmallSidecar::system_totals_for_test(),
        system,
        "the refused carve consumed no sidecar storage",
    );
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test(),
        census,
        "a partial refill registers no new route",
    );
    assert_eq!(
        heap.dbg_table_count(),
        table,
        "a partial refill reserves nothing",
    );

    // Disarm: drain the partial refill's leftovers, then a fresh magazine miss
    // must refill fully again (carve, since the substrate chain is empty).
    SmallSidecar::fail_prepare_after_for_test(0);
    for _ in 0..resident(heap, c) {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        held.push(ptr);
    }
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null(), "the retry after the disarm must succeed");
    held.push(ptr);
    assert_eq!(
        resident(heap, c),
        want - 1,
        "a full refill parks want-1 blocks",
    );
    assert_accounted(heap, &held, c);

    while let Some(ptr) = held.pop() {
        // SAFETY: each block came from this heap with this exact layout.
        unsafe { heap.dealloc(ptr, layout()) };
    }
    drop(lease);
}

/// Without `fastbin` the magazine refill is compiled out, so the same prepare
/// refusal surfaces on the scalar carve path: a plain OOM with a full
/// rollback, then a one-shot-disarmed retry that carves.
#[cfg(not(feature = "fastbin"))]
#[test]
fn magazine_refill_refusal_is_a_scalar_oom_with_a_full_rollback() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let c = 1usize; // the 32B class, per the shared fixture layout.
    let anchor = heap.alloc(layout());
    assert!(!anchor.is_null());
    let mut held = vec![anchor];

    let (bump, live, head) = owner_state(anchor, c);
    let system = SmallSidecar::system_totals_for_test();
    let census = RouteDirectory::global().live_route_census_for_test();
    let table = heap.dbg_table_count();

    SmallSidecar::fail_prepare_after_for_test(1);
    let refused = heap.alloc(layout());
    assert!(
        refused.is_null(),
        "without fastbin the refusal is a plain scalar OOM",
    );
    let (bump_after, live_after, head_after) = owner_state(anchor, c);
    assert_eq!(
        (bump_after, live_after, head_after),
        (bump, live, head),
        "the refusal must not move bump/live/head",
    );
    assert_eq!(SmallSidecar::system_totals_for_test(), system);
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test(),
        census,
    );
    assert_eq!(
        heap.dbg_table_count(),
        table,
        "the refusal reserves nothing"
    );

    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null(), "the retry after the one-shot disarm carves");
    held.push(ptr);
    assert_accounted(heap, &held, c);

    while let Some(ptr) = held.pop() {
        // SAFETY: each block came from this heap with this exact layout.
        unsafe { heap.dealloc(ptr, layout()) };
    }
    drop(lease);
}
