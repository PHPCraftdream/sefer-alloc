//! R2-06 (independent src review round 2, task #2008) — miri data-race
//! regression: a diagnostic full-header snapshot (`SegmentHeader::read_at`,
//! reached here via `HeapCore::dbg_segment_state_reconciliation`, which walks
//! EVERY registered segment including Large ones this thread does not
//! exclusively own) running CONCURRENTLY with a remote thread's cross-thread
//! Large free (`dealloc` → `dealloc_routing` → `push_large_deferred_free`,
//! which CASes/stores that segment's `deferred_next` field via a REAL
//! `&AtomicU64` view).
//!
//! Before this fix, `SegmentHeader::read_at` was a plain
//! `Node::read_struct::<SegmentHeader>` — ONE non-atomic load over the WHOLE
//! struct, including `deferred_next`'s bytes. Racing that against
//! `push_large_deferred_free`'s atomic CAS/store on the SAME bytes is a data
//! race (undefined behavior) under Rust's memory model, independent of
//! whether the diagnostic caller ever uses the torn `deferred_next` value it
//! read (`dbg_segment_state_reconciliation` in fact never reads that field at
//! all — the UB is in the non-atomic READ touching racing memory, not in
//! what the caller does with the result). The fix
//! (`Node::read_struct_with_atomic_word`) never performs a non-atomic read
//! over `deferred_next`'s bytes: it splices in a REAL atomic load at that
//! offset instead, closing the race structurally for every `read_at` caller.
//!
//! Mirrors `regression_xthread_thread_free_alias_miri.rs`'s established
//! shape (a real, gated, CONCURRENT owner/remote overlap under plain-
//! provenance miri with an elevated preemption rate — see
//! `scripts/miri.mjs`'s `PLAIN_MIRIFLAGS` / `PLAIN_MATRIX`) — that test
//! targets a DIFFERENT racing field (`thread_free`); this one targets
//! `deferred_next` specifically, the field this task's review finding
//! (R2-06) names.
//!
//! If miri reports a data race here, the fix regressed (or this test
//! predates it and empirically PROVES the pre-fix hazard — verified via a
//! genuine counterfactual: stashing just the `src/` fix and re-running this
//! exact test under `node scripts/miri.mjs --plain
//! regression_r2_06_header_race_miri` reproduces a real miri "Data race
//! detected" report). If miri stays green, the concurrent snapshot is sound.

#![cfg(all(
    all(feature = "alloc-global", feature = "alloc-xthread"),
    all(feature = "alloc-decommit", feature = "bench-internals"),
    feature = "internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against the other xthread tests in the same binary: the registry
// is a process-global static. Mirrors the SerialGuard in
// `regression_xthread_thread_free_alias_miri.rs`.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

#[test]
fn owner_reconciliation_walk_overlaps_remote_large_free() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Large blocks the remote thread will free (each free = a real
    // `push_large_deferred_free` CAS/store on THAT segment's `deferred_next`).
    // Distinct bases so the double-push guard does not collapse them into a
    // single no-op — each contributes a real atomic write. Kept small (miri
    // is ~1e5x slower than native; each Large alloc reserves an OS segment).
    const LARGE_N: usize = 6;
    const LARGE_SIZE: usize = 2 * 1024 * 1024; // > SMALL_MAX -> routed to alloc_large
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();

    // Owner's diagnostic-walk iteration count: enough for miri's preemptive
    // scheduler (elevated preemption rate under the `--plain` job) to land a
    // remote deferred-free write inside a live owner `read_at` snapshot,
    // without blowing the per-test miri budget.
    const RECONCILIATION_ITERS: usize = 40;

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Owner pre-allocates the Large blocks so they are registered, owner-
    // stamped segments before the remote thread starts freeing them.
    let mut large_ptrs: Vec<*mut u8> = Vec::with_capacity(LARGE_N);
    for i in 0..LARGE_N {
        let p = unsafe { (*heap).alloc(large_layout) };
        assert!(!p.is_null(), "large alloc[{i}] returned null");
        large_ptrs.push(p);
    }

    // A gate so both threads start at (nearly) the same instant, maximising
    // the window in which a remote deferred-free write overlaps an owner
    // `read_at` snapshot.
    let start = Arc::new(AtomicBool::new(false));

    // Raw pointers are `!Send`; ship addresses.
    let addrs: Vec<usize> = large_ptrs.iter().map(|&p| p as usize).collect();
    let heap_addr = heap as usize;
    let start_remote = Arc::clone(&start);

    let remote = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        while !start_remote.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Each dealloc of a Large block owned by `heap` routes through
        // `dealloc_routing` -> `push_large_deferred_free` -> a real
        // CAS/store on that segment's `deferred_next` -- concurrently with
        // the owner's diagnostic reconciliation walk below.
        for &addr in &addrs {
            let p = addr as *mut u8;
            unsafe { (*remote_heap).dealloc(p, large_layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });

    // Owner: release the gate, then repeatedly snapshot the FULL segment-state
    // reconciliation -- this walks every registered segment (including the
    // Large ones the remote thread is concurrently freeing) and, for each
    // Large segment, reads its header via `SegmentMeta::header()` ->
    // `SegmentHeader::read_at`, exactly the call path this task's review
    // finding names.
    start.store(true, Ordering::Release);
    let heap_ptr = heap_addr as *mut sefer_alloc::registry::HeapCore;
    for _ in 0..RECONCILIATION_ITERS {
        let rec = unsafe { (*heap_ptr).dbg_segment_state_reconciliation() };
        // Not asserting on the numbers themselves (they are inherently
        // racy/momentary while the remote thread is mid-free) -- the
        // regression this test targets is a miri data-race REPORT, not a
        // wrong count. `recompute_total` having run without panicking is a
        // cheap sanity floor.
        let _ = rec;
    }

    remote.join().unwrap();

    // Drain the deferred-free stack the remote populated (own-thread large
    // alloc slow path runs the drain) and clean up, so miri's leak checker is
    // satisfied -- mirrors `regression_xthread_thread_free_alias_miri.rs`.
    let drain = unsafe { (*heap).alloc(large_layout) };
    assert!(!drain.is_null());
    unsafe { (*heap).dealloc(drain, large_layout) };
    unsafe { HeapRegistry::recycle(heap) };
}
