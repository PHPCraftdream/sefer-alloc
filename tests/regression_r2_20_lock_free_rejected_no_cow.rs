//! R2-20 (independent src review round 2, task #2022) — a REJECTED
//! `LockFreeRegion::remove` must be a true no-op: no copy-on-write snapshot,
//! no page clone, and (after the `contains` half of the fix) no transient
//! `Arc<T>` value refcount churn either.
//!
//! ## The defect
//!
//! `remove` used to begin mutation unconditionally — it cloned the whole
//! snapshot (an O(P) heap allocation for the page-table `Vec<Arc<Page>>`)
//! and then the target page (`Vec<Slot<T>>`, a second allocation) BEFORE it
//! looked at the handle. Only afterwards did the generation check reject a
//! stale or already-removed handle, or `pages.get(page_idx)?` bail on an
//! out-of-range one. A no-op therefore paid the full copy-on-write cost —
//! allocations and `Arc` refcount churn under the region's writer mutex —
//! for nothing. `contains` carried a milder form of the same shape: it
//! delegated to `get`, so answering a boolean cloned and dropped an
//! `Arc<T>` (pure refcount churn with no semantic effect).
//!
//! ## The fix
//!
//! `remove` now validates the generation, occupancy, and page range against
//! the BORROWED current snapshot first — a pure lookup through the new
//! allocation-free `Snapshot::slot` — and pays for the snapshot/page clones
//! only once the removal is known to succeed. `contains` inspects the slot's
//! occupancy directly instead of routing through `get`.
//!
//! ## Reproduction strategy
//!
//! Allocation-free-ness is directly observable: install a counting
//! `GlobalAlloc` wrapper (the same technique `tests/alloc_core_reentrancy.rs`
//! uses for its M5 reentrancy proof) and assert the per-call allocation
//! DELTA around each rejected operation is exactly zero. Three counterfactuals
//! pin the oracle's honesty:
//!
//! 1. **Negative control** — a genuinely SUCCESSFUL remove (live handle) must
//!    still allocate (its copy-on-write clones) and must preserve the
//!    published-snapshot visibility and retirement semantics. If the
//!    successful path did not trip the counter, every zero-delta assertion
//!    below would pass vacuously against an always-zero oracle.
//! 2. **Stale remove / out-of-range remove** — both rejected paths must
//!    allocate EXACTLY ZERO times. Pre-fix, the stale path cloned the page
//!    table AND the target page before the generation check (2 allocations),
//!    and the out-of-range path cloned the page table before `pages.get`
//!    bailed (1 allocation) — so these assertions fail against the pre-fix
//!    `remove`.
//! 3. **Structural guard** — the eliminated `get`-delegation in `contains`
//!    is unobservable from safe test code (it allocated nothing; a transient
//!    strong-count bump cannot be sampled race-free), so this test also reads
//!    `lock_free_region.rs` and pins the SHAPE of `contains`: it must inspect
//!    the published snapshot via `state.load()` and must not delegate to
//!    `self.get` (the same source-reading pattern `tests/no_panic_doc_accuracy.rs`
//!    uses).

#![allow(deprecated)]
#![cfg(feature = "experimental")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use sefer_alloc::{LockFreeHandle, LockFreeRegion};

/// A counting wrapper around the system allocator, installed as this test's
/// global allocator so every heap allocation the region performs (and none the
/// test itself performs inside a measured window) is observed exactly.
///
/// **Counter scope is thread-local, not process-wide.** A process-global
/// counter would be contaminated by the test harness, libstd, and libc
/// background work (e.g. libtest output capture, panic-runtime setup, and
/// thread-local bootstrap on first access from a background thread) touching
/// the global allocator on OTHER threads. The invariant under test — "this
/// thread's rejected operations allocate nothing" — is inherently per-thread,
/// so the counter must be too. (Same rationale as the counting allocator in
/// `tests/alloc_core_reentrancy.rs`.)
struct Counting;

std::thread_local! {
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static DEALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn alloc_count() -> usize {
    ALLOC_COUNT.try_with(|c| c.get()).unwrap_or(0)
}

fn dealloc_count() -> usize {
    DEALLOC_COUNT.try_with(|c| c.get()).unwrap_or(0)
}

// SAFETY: `Counting` has no shared mutable state; its counters are thread-local,
// and the delegated `System` allocator supports concurrent calls.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `Cell::set` does not allocate; thread-local key lookup with `const`
        // init is allocation-free, so the counting itself never disturbs what
        // it measures.
        let _ = ALLOC_COUNT.try_with(|c| c.set(c.get() + 1));
        // SAFETY: The caller supplies a valid `Layout`; forwarding it unchanged
        // preserves the `GlobalAlloc::alloc` contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = DEALLOC_COUNT.try_with(|c| c.set(c.get() + 1));
        // SAFETY: The caller supplies the pointer/layout pair allocated by this
        // allocator; forwarding it once to `System` preserves allocator pairing.
        unsafe { System.dealloc(ptr, layout) };
    }
}

// One test per binary: a single measured thread, no parallel test allocating
// on it. (The thread-local counter would isolate it anyway; the single-test
// shape just keeps the binary's own story simple.)
#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn rejected_lock_free_remove_and_contains_allocate_nothing() {
    // ── Setup ──────────────────────────────────────────────────────────
    // Three pages = 192 slots; 70 inserts (70 > 64 = PAGE, one page) leave
    // the live slots spanning two of the three pages — a page table deep
    // enough that the pre-fix page-table clone is a real, countable
    // allocation rather than a rounding artifact.
    let region = LockFreeRegion::<u64>::with_pages(3);
    let mut handles: Vec<LockFreeHandle<u64>> = Vec::with_capacity(70);
    for i in 0..70u64 {
        handles.push(region.insert(i));
    }
    assert_eq!(
        region.len(),
        70,
        "setup: 70 live inserts must be counted (I4)"
    );

    // ── Negative control: a genuinely successful write still allocates ──
    // The acceptance criterion's other half: the successful copy-on-write
    // remove really does hit the counter (guarding against a vacuous
    // always-zero oracle), and it preserves snapshot visibility and the
    // retirement semantics.
    let before = alloc_count();
    let removed = region.remove(handles[0]);
    let delta = alloc_count() - before;
    assert!(
        delta > 0,
        "R2-20 negative control FAILED: a successful remove must clone the \
         snapshot's page table and the target page (copy-on-write), but the \
         allocation delta was exactly 0 — the counting oracle is not wired to \
         real allocations"
    );
    let removed = removed.expect("remove of a live handle must return Some (I1)");
    assert_eq!(*removed, 0u64, "remove must return the stored value");
    // Post-remove semantics of the SUCCESSFUL write: the tombstone is visible
    // (I2), `len` dropped (I4), and every survivor still resolves against the
    // newly published snapshot.
    assert!(
        region.get(handles[0]).is_none(),
        "R2-20: get() of a just-removed handle must be None (I2 tombstone)"
    );
    assert_eq!(
        region.len(),
        69,
        "len must drop by one after a successful remove (I4)"
    );
    for (i, h) in handles.iter().enumerate().skip(1) {
        assert!(
            region.contains(*h),
            "R2-20: published-snapshot visibility broken — survivor handle \
             #{i} stopped resolving after an unrelated successful remove"
        );
    }

    // ── Stale remove: the primary counterfactual ───────────────────────
    // The same handle, already removed. Pre-fix, `remove` cloned the
    // snapshot's page-table `Vec<Arc<Page>>` (one allocation) AND the target
    // page `Vec<Slot<T>>` (a second allocation) before the generation check
    // rejected the stale handle. The fix validates against the borrowed
    // current snapshot first, so the reject path must allocate nothing.
    let before = alloc_count();
    let again = region.remove(handles[0]);
    let delta = alloc_count() - before;
    assert!(
        again.is_none(),
        "R2-20: a second remove of an already-removed handle must be a \
         no-op None (I2)"
    );
    assert_eq!(
        delta, 0,
        "R2-20 regression: a REJECTED remove (already-removed handle) \
         allocated {delta} time(s). Pre-fix the page-table snapshot clone AND \
         the target-page clone were both paid before the generation check \
         rejected it; the fix must validate against the borrowed current \
         snapshot and pay for the copy-on-write clones only on success."
    );

    // ── Stale contains: the same no-op guarantee on the read path ──────
    let before = alloc_count();
    let stale = region.contains(handles[0]);
    let delta = alloc_count() - before;
    assert!(
        !stale,
        "R2-20: contains() of a removed handle must be false (I2)"
    );
    assert_eq!(
        delta, 0,
        "R2-20 regression: contains() on an already-removed handle allocated \
         {delta} time(s) — answering a boolean must never touch the allocator."
    );

    // ── Live contains: stays allocation-free ───────────────────────────
    // NOTE: this held pre-fix too — the eliminated `get` delegation's
    // transient `Arc<T>` clone bumped a refcount, which the system allocator
    // never observes. Kept as a guard that the RESHAPED `contains` never
    // regresses into something that DOES allocate (e.g. building a temporary
    // collection or taking a debug-format detour to answer the boolean).
    // Same-generation vacant slot distinguishes the occupancy check from the
    // stale-generation check. Slot 70 is vacant after the 70 setup inserts.
    let vacant = region._forge_handle_for_tests(70, 0);
    let before = alloc_count();
    let vacant_removed = region.remove(vacant);
    let delta = alloc_count() - before;
    assert!(
        vacant_removed.is_none(),
        "a matching-generation vacant slot is not removable"
    );
    assert_eq!(
        delta, 0,
        "R2-20 regression: a same-generation vacant-slot remove allocated \
         {delta} time(s); occupancy must be checked before copy-on-write."
    );
    assert!(!region.contains(vacant));

    let before = alloc_count();
    let live = region.contains(handles[1]);
    let delta = alloc_count() - before;
    assert!(live, "R2-20: contains() of a live handle must be true (I1)");
    assert_eq!(
        delta, 0,
        "R2-20 regression: contains() on a live handle allocated {delta} \
         time(s) — a boolean answer must never touch the allocator."
    );

    // ── Out-of-range handle: the second allocation counterfactual ──────
    // `u32::MAX` names page index `u32::MAX >> 6`, far beyond the region's
    // three pages. Pre-fix, the page-table snapshot clone was paid BEFORE
    // `pages.get(page_idx)?` bailed out (one wasted allocation plus refcount
    // churn under the writer mutex).
    let oor = region._forge_handle_for_tests(u32::MAX, 0);
    let before = alloc_count();
    let oor_removed = region.remove(oor);
    let delta = alloc_count() - before;
    assert!(
        oor_removed.is_none(),
        "R2-20: remove of an out-of-range handle must be None"
    );
    assert_eq!(
        delta, 0,
        "R2-20 regression: an OUT-OF-RANGE remove allocated {delta} time(s). \
         Pre-fix the snapshot page-table clone was paid before the \
         `pages.get(page_idx)?` bail; the fix must reject before any clone."
    );
    let before = alloc_count();
    let oor_contains = region.contains(oor);
    let delta = alloc_count() - before;
    assert!(
        !oor_contains,
        "R2-20: contains() of an out-of-range handle must be false"
    );
    assert_eq!(
        delta, 0,
        "R2-20 regression: out-of-range contains allocated {delta} time(s) — \
         answering a boolean must never touch the allocator."
    );

    // ── Retirement, free-list reuse, and I3 preserved by the fix ───────
    // The slot freed in the negative control is recycled by the next insert
    // (free-list reuse); the OLD handle of that reused slot must never
    // resolve again (I3 — no ABA), and accounting must stay exact.
    let h_new = region.insert(7777);
    let got = region
        .get(h_new)
        .expect("a freshly inserted handle must resolve (I1)");
    assert_eq!(
        *got, 7777,
        "the inserted value must round-trip through get()"
    );
    assert!(
        region.get(handles[0]).is_none(),
        "R2-20: I3 violated — the OLD handle of the recycled slot resolved \
         with a fresh value"
    );
    assert_eq!(region.len(), 70, "len must track the recycled slot (I4)");
    let removed_new = region
        .remove(h_new)
        .expect("remove of a live recycled handle must succeed");
    assert_eq!(
        *removed_new, 7777,
        "the removed recycled value must round-trip"
    );
    assert_eq!(
        region.len(),
        69,
        "len must drop after the second successful remove (I4)"
    );

    // The dealloc half of the wrapper is pinned live for completeness; it is
    // deliberately NOT asserted on — snapshot retirement is refcount-timed,
    // not synchronously with the store.
    core::hint::black_box(dealloc_count());

    // ── Structural counterfactual for the `contains` half of the fix ────
    // The eliminated `get`-delegation cost an `Arc<T>` clone plus drop:
    // pure refcount churn, invisible to an allocation counter (a transient
    // strong-count bump cannot be sampled race-free from safe test code). So
    // pin the STRUCTURE of the shipped source instead — the same
    // source-reading pattern `tests/no_panic_doc_accuracy.rs` uses:
    // `contains` must inspect the published snapshot itself and must NOT be
    // reverted to delegating to `get`.
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/concurrent/lock_free/lock_free_region.rs"
    ))
    .expect("read src/concurrent/lock_free/lock_free_region.rs");
    let contains_start = src
        .find("pub fn contains(&self, handle: LockFreeHandle<T>) -> bool")
        .expect("R2-20 structural guard: the `contains` signature was not found");
    let after_sig = &src[contains_start..];
    let contains_end = after_sig
        .find("pub fn insert")
        .expect("R2-20 structural guard: the `insert` signature was not found");
    let contains_src = &after_sig[..contains_end];
    assert!(
        !contains_src.contains("self.get"),
        "R2-20 structural guard FAILED: `contains` was reverted to delegating \
         to `self.get` — the fix's whole point was that answering a boolean \
         must never clone and drop an Arc<T>."
    );
    assert!(
        contains_src.contains("state.load()"),
        "R2-20 structural guard FAILED: `contains` no longer reads the \
         published snapshot via `state.load()` — its lock-free read shape \
         changed under us."
    );
}
