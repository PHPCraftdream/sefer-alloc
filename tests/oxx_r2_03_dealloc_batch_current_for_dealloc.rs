//! oxx R2-03 counterfactual: `SeferAlloc::dealloc_batch` must resolve a
//! dealloc-only thread through the DEALLOC-ONLY `current_for_dealloc`
//! resolver — the same one `GlobalAlloc::dealloc` uses (R6-OPT-P0-1,
//! `tests/dealloc_only_no_bind.rs`) — and NOT `current_heap()`, under
//! `alloc-xthread`.
//!
//! ## The defect (`docs/reviews/2026-09-28-154558-src-review-oxx-round-2.md`,
//! §R2-03)
//!
//! Before this fix, `SeferAlloc::dealloc_batch` unconditionally resolved via
//! `self.current_heap()` (== `current_for_alloc[_with_config]`), which for a
//! thread whose TLS is null (never allocated anything of its own — e.g. a
//! worker that only ever receives pointers from a producer thread and frees
//! them in bulk) called `bind_slow_tagged()` -> `HeapRegistry::claim()` ->
//! materialised a FULL `HeapCore` (reserving/committing a primordial
//! segment) JUST to free a batch of foreign pointers — exactly the
//! R6-OPT-P0-1 defect already fixed for the SCALAR `dealloc` entry point,
//! left unfixed on the batch one. The fix routes `dealloc_batch` (under
//! `alloc-xthread`) through `global::tls_heap::current_for_dealloc` +
//! `HeapCore::dealloc_foreign_routing` per non-null block, mirroring
//! `SeferAlloc::dealloc`'s `ForeignNoBind` arm exactly.
//!
//! ## Non-vacuous / counterfactual
//!
//! Same evidence shape as `tests/dealloc_only_no_bind.rs`:
//! `heaps_claimed_high_water` is a monotonic, process-wide high-water mark of
//! every registry slot ever claimed (`HeapRegistry::claim`, via
//! `bump_count`). This test snapshots it immediately before spawning a fresh
//! worker thread whose FIRST EVER allocator call is `dealloc_batch` on a
//! batch of pointers produced by another (already-bound) thread, and asserts
//! the mark is unchanged after the worker joins. Under the pre-fix
//! `current_heap()` dispatch, the worker's `dealloc_batch` call binds a fresh
//! slot (`bump_count` fires inside `claim`) and this assertion fails —
//! verified during development of this fix by temporarily restoring the old
//! `self.current_heap()` dispatch in `dealloc_batch`.
//!
//! A second assertion proves the fix does not merely avoid claiming a slot by
//! LOSING the freed blocks: `AllocCore::refill_class_bump_impl`'s documented
//! "free-drain first" ordering (`alloc_core_small_magazine.rs`) drains a
//! segment's remote-free ring (and reclaims its entries into the free list)
//! BEFORE ever bump-carving fresh memory. With exactly `TCACHE_CAP` (16)
//! blocks of the same class freed and a `TCACHE_CAP`-sized magazine refill,
//! the owner's very next `N` allocations of that class are — deterministically
//! — served ENTIRELY out of the reclaimed set, with no bump-carve in between.
//! So if (and only if) `dealloc_foreign_routing` correctly pushed the whole
//! batch onto the owner's segment ring, every one of the owner's next `N`
//! allocations must return an address from that batch.
//!
//! Feature gate: `alloc-global`, `alloc-xthread`, `fastbin` (the magazine +
//! cross-thread ring substrate `dealloc_batch`'s Small fast path needs),
//! `internals` (`sefer_alloc::registry` is only public under
//! `alloc-global + internals`), `batch-api` (gates `dealloc_batch` itself —
//! the R10-7 follow-up API-boundary feature, NOT part of `production`).
//! Spelled out explicitly (not `production`/`hardened`) so this test keeps
//! running under the exact combination CI already exercises unfiltered:
//! `.github/workflows/ci.yml`'s "test (--features \"hardened batch-api\")"
//! step (`cargo test --features "hardened batch-api internals" --no-fail-fast`)
//! — `hardened = ["fastbin"]` and `fastbin = ["alloc-global", "alloc-xthread"]`
//! resolve to exactly this feature set.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "batch-api",
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;

use sefer_alloc::registry::heaps_claimed_high_water;
use sefer_alloc::SeferAlloc;

/// A never-before-bound thread whose first allocator call is `dealloc_batch`
/// on a batch of foreign pointers must not claim a registry slot (oxx
/// R2-03), and every freed block must be reclaimed by its owner.
#[test]
fn dealloc_batch_from_unbound_thread_does_not_claim_a_slot_and_returns_blocks() {
    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Owner thread (this test's own OS thread, driven directly through the
    // `SeferAlloc` value -- not `#[global_allocator]`, matching
    // `dealloc_only_no_bind.rs`'s established driving discipline) binds its
    // own heap and allocates the blocks the worker will batch-free. Exactly
    // `TCACHE_CAP` (16) blocks: the owner's very first alloc of this class
    // refills+pops from an empty magazine (nothing to free-drain yet), so
    // all 16 are freshly bump-carved from the same segment.
    const N: usize = 16;
    let mut addrs: Vec<usize> = Vec::with_capacity(N);
    for _ in 0..N {
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "owner alloc returned null");
        addrs.push(p as usize);
    }
    let freed_set: HashSet<usize> = addrs.iter().copied().collect();

    // Snapshot the high-water mark AFTER the owner's own bind (so the
    // owner's slot claim is not counted against the worker) and immediately
    // before spawning the worker.
    let before = heaps_claimed_high_water();

    // Handed over as `usize` addresses (not `*mut u8`, which is `!Send`) --
    // same discipline `r10_7_alloc_batch_xthread_double_free.rs` uses for its
    // cross-thread pointer handoff. `addrs` itself moves into the closure;
    // `freed_set` (already built above) is what the post-join check uses.
    let handle = std::thread::spawn(move || {
        // This worker's FIRST EVER call into the allocator (any allocator --
        // this thread has done nothing else) is the batch free below.
        let worker = SeferAlloc::new();
        let layout = Layout::from_size_align(64, 8).unwrap();
        let blocks: Vec<*mut u8> = addrs.iter().map(|&x| x as *mut u8).collect();
        // SAFETY: every `blocks[i]` was allocated by the owner thread above
        // with `layout` and handed over by value (never touched again by the
        // owner) -- a sound cross-thread batch free, and the exact
        // "dealloc_batch, never-before-bound thread" shape this test targets.
        unsafe { worker.dealloc_batch(layout, &blocks) };
    });
    handle.join().expect("worker thread panicked");

    let after = heaps_claimed_high_water();
    assert_eq!(
        after, before,
        "a never-before-bound thread's dealloc_batch-only first call must not \
         claim a registry slot (oxx R2-03): high-water before={before}, after={after}"
    );

    // The freed blocks must have actually reached the owner (not silently
    // dropped by the `ForeignNoBind` routing tail): the owner's own next `N`
    // allocations of the SAME class must be served entirely out of the freed
    // set -- `refill_class_bump_impl`'s free-drain-first ordering guarantees
    // this if (and only if) the batch really landed on the owner's segment
    // ring.
    for i in 0..N {
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "owner re-alloc [{i}] returned null");
        assert!(
            freed_set.contains(&(p as usize)),
            "owner re-alloc [{i}] returned {:p}, which is not one of the \
             dealloc_batch-freed blocks -- the batch did not reach the owner \
             (oxx R2-03)",
            p,
        );
    }
}
