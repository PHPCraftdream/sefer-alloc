//! R10-3 regression: a rejected ring entry must NOT set the `changed_classes`
//! bit for its class.
//!
//! **Context.** `drain_dirty_segments` accumulates a per-segment
//! `changed_classes: u64` bitmap of classes whose BinTable was actually
//! mutated by the drain pass. This bitmap feeds TWO consumers:
//! 1. `sync_directory_for_segment_classes` — incremental directory-bit sync.
//! 2. The R9-6 `WASTED_DIRTY_DRAINS` metric — a drain counts as "wasted" (from
//!    the calling class's perspective) only if the SOUGHT class's bit is ABSENT
//!    from `changed_classes`.
//!
//! Before the fix, `changed_classes |= 1u64 << entry_class_idx(off)` fired
//! **unconditionally** — even when `reclaim_offset[_checked]` returned `false`
//! (the entry was rejected: double-free guard, in-magazine duplicate, stale
//! generation, garbled offset). A rejected entry never mutated the BinTable
//! (every early `return false` precedes `set_head`/`mark_free`), so recording
//! it was both (a) a spurious directory-sync trigger and (b) a metric
//! under-count: a drain that rejected EVERY entry of the sought class still
//! looked "not wasted".
//!
//! **Counterfactual.** This test constructs a segment whose ring has exactly
//! ONE entry (a double-free: the block was freed own-thread, then freed
//! cross-thread to push it into the ring and set the dirty bit). At drain time
//! the block is on the BinTable (`is_free` → true) so `reclaim_offset` rejects
//! the entry (`reclaimed = false`). Under the fixed code, `changed_classes`
//! stays 0 for this class → the R9-6 wasted check fires →
//! `WASTED_DIRTY_DRAINS` increments. Under the buggy code, the bit was set
//! unconditionally → the wasted check did NOT fire → `WASTED_DIRTY_DRAINS`
//! stayed flat.
//!
//! **Why the production drain is exercised** (not `dbg_drain_all_rings`): the
//! `WASTED_DIRTY_DRAINS` counter lives ONLY inside `drain_dirty_segments`
//! (the production path). `dbg_drain_all_rings_impl` has the same
//! unconditional-bit bug (also fixed in this change for consistency) but does
//! NOT bump the counter. So the test MUST drive the production alloc path to
//! trigger `find_segment_with_free_impl` → `drain_dirty_segments`.
//!
//! **Feature gate.** Same as `tests/r9_6_class_aware_dirty_judge.rs`:
//! `alloc-global`, `alloc-xthread`, `alloc-segment-directory`, `alloc-stats`
//! (the counter increment site), `not(numa-aware)` (the drain is compiled out
//! under `numa-aware`). Under other configurations the file compiles as an
//! empty test binary (0 tests, pass by absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

// Serialise against other tests in this binary: the diagnostic counters are
// process-global statics shared across every AllocCore/HeapCore in the process.
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

/// The class the OWNER allocates and frees. Chosen above the materialisation
/// carve range (0..~40) so it carves its OWN segment(s), distinct from the
/// materialisation segments. Must be a valid small class under both the
/// baseline (49-class) and `medium-classes` (58-class) feature sets.
const TARGET_CLASS: usize = 40;

/// Enough blocks of TARGET_CLASS to cross the directory-materialise
/// threshold (DIRECTORY_MATERIALIZE_THRESHOLD = 32 segments). Each 4 MiB
/// segment holds ~98 blocks of ~43 KB, so 3500 blocks span ~36 segments,
/// safely past the threshold. This is the same order as the R9-6 judge
/// test's BLOCKS_PER_CLASS = 4000.
const FILL_BLOCKS: usize = 3500;

/// Allocate `count` blocks of `class_idx` via the production `HeapCore::alloc`
/// path. Returns the live pointers.
fn alloc_batch(heap: *mut HeapCore, class_idx: usize, count: usize) -> Vec<*mut u8> {
    let bs = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(bs, 8).expect("class block size is a valid layout");
    let mut v = Vec::with_capacity(count);
    for _ in 0..count {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null for class {class_idx}");
        v.push(p);
    }
    v
}

/// Force the directory sidecar to materialise by carving one block of each of
/// the first `threshold + slack` distinct classes (same approach as the R9-6
/// judge test). Returns the live pointers (caller keeps them alive so the
/// segments don't get recycled by decommit).
fn materialise_directory(heap: *mut HeapCore) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let target = (threshold + 8).min(TARGET_CLASS);
    assert!(
        target > threshold,
        "size-class table too small for materialisation carve"
    );
    let mut keep_alive = Vec::with_capacity(target);
    for cls in 0..target {
        let bs = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(bs, 8).expect("class block size is a valid layout");
        let p = unsafe { (*heap).alloc(layout) };
        assert!(
            !p.is_null(),
            "materialise alloc for class {cls} returned null"
        );
        keep_alive.push(p);
    }
    keep_alive
}

/// Regression test for the R10-3 fix: a ring entry that `reclaim_offset`
/// REJECTS must not set the `changed_classes` bit for its class. The test
/// constructs a segment whose ring has one rejected entry (a double-free:
/// freed own-thread → `mark_free`, then freed cross-thread → ring push + dirty
/// bit). When the production drain processes this segment, the entry is
/// rejected (`is_free` → true), and under the fixed code `changed_classes`
/// stays empty for TARGET_CLASS → `WASTED_DIRTY_DRAINS` increments.
///
/// **RED before the fix:** under the unconditional `changed_classes |=` the
/// bit was set even for rejected entries → the wasted check
/// `changed_classes & (1 << class_idx) == 0` was FALSE → no increment →
/// `delta == 0`.
///
/// **GREEN after the fix:** `changed_classes` stays 0 → wasted check is TRUE
/// → `delta >= 1`.
#[test]
fn rejected_ring_entry_does_not_set_changed_classes_bit() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Phase 0: materialise the directory sidecar (drain_dirty_segments is a
    // no-op until the sidecar exists).
    let _keep_alive = materialise_directory(heap);

    // Phase 1: pre-allocate FILL_BLOCKS blocks of TARGET_CLASS. These span
    // 2+ segments; blocks[0] is in the FIRST segment (S1), and `small_cur`
    // ends up pointing at the LAST segment (≠ S1).
    let blocks = alloc_batch(heap, TARGET_CLASS, FILL_BLOCKS);
    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    // Phase 2: free ALL blocks via own-thread. blocks[0] is freed FIRST so it
    // ends up deep in the LIFO freelist (popped LAST by subsequent refills).
    // Under fastbin, the first magazine overflow flushes blocks[0] to the
    // BinTable with `mark_free` — so `is_free(blocks[0])` is `true` at drain
    // time, regardless of the magazine predicate.
    for &p in &blocks {
        // SAFETY: `p` is a live allocation owned by `heap`; this dealloc is its
        // single logical free (the cross-thread free in Phase 4 deliberately
        // double-frees blocks[0] to exercise the drain's rejection guard — a
        // contract-stress of the defensive path, same pattern as
        // `tests/regression_xthread_double_free_residual.rs`).
        unsafe { (*heap).dealloc(p, layout) };
    }

    // Phase 3: cross-thread free blocks[0] (a DELIBERATE double-free — the
    // block was freed in Phase 2). The producer's `dealloc_foreign_slow`
    // pushes blocks[0]'s offset into S1's ring AND calls
    // `set_dirty_bit_for_segment(S1)`. The ring entry will be REJECTED at
    // drain time because `is_free(blocks[0])` is `true`.
    let x_addr = blocks[0] as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // SAFETY (R6-MS-1/2 + raw-deref): `remote` is a live heap; `x_addr` is
        // a block previously allocated by `heap` (the owner). This dealloc from
        // a DIFFERENT thread routes through `dealloc_foreign_slow`, which pushes
        // the offset into the owner's ring and sets the dirty bit. The block
        // was already freed in Phase 2 — this is a deliberate double-free to
        // exercise the drain's `is_free` rejection guard. The allocator handles
        // this defensively (the guard returns `false`, no corruption).
        unsafe { (*remote).dealloc(x_addr as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    // Phase 4: snapshot the counters BEFORE the measured window.
    let drained_before = AllocCore::dbg_dirty_segments_drained();
    let wasted_before = AllocCore::dbg_wasted_dirty_drains();

    // Phase 5: allocate TARGET_CLASS blocks continuously (accumulating, no
    // self-free) to exhaust the magazine + BinTable and trigger
    // `find_segment_with_free_checked(TARGET_CLASS)` → `drain_dirty_segments`.
    // The drain runs at the TOP of `find_segment_with_free_impl`, BEFORE the
    // directory scan pops any BinTable blocks — so blocks[0] is still
    // `is_free` when the drain processes S1's ring.
    let mut alloced = alloc_batch(heap, TARGET_CLASS, FILL_BLOCKS + 50);

    // Phase 6: assert the counter incremented.
    let wasted_after = AllocCore::dbg_wasted_dirty_drains();
    let drained_after = AllocCore::dbg_dirty_segments_drained();
    let delta = wasted_after.saturating_sub(wasted_before);
    let drained_delta = drained_after.saturating_sub(drained_before);
    eprintln!("R10-3: drained_delta={drained_delta}, wasted_delta={delta}");
    assert!(
        delta > 0,
        "WASTED_DIRTY_DRAINS delta = {delta} (expected > 0): a drain that \
         rejected every entry of the sought class must count as wasted under \
         the fixed code (changed_classes bit gated on `reclaimed`). Under the \
         pre-fix unconditional `changed_classes |=`, the rejected entry set \
         the bit and the drain looked 'not wasted' (delta == 0)."
    );

    eprintln!(
        "R10-3: WASTED_DIRTY_DRAINS delta = {delta} (wasted_before = \
         {wasted_before}, wasted_after = {wasted_after})"
    );

    // Cleanup.
    for &p in &alloced {
        unsafe { (*heap).dealloc(p, layout) };
    }
    alloced.clear();
    unsafe { HeapRegistry::recycle(heap) };
}
