//! R11-2 (Bug 2) regression: `drain_heap_overflow` must finalize (pool or
//! release) a small segment that reaches EXACTLY zero live blocks via an
//! overflow-ring reclaim — mirroring the finalization every other
//! empty-observing site (`dealloc_small`, the per-segment ring drain in
//! `find_segment_with_free_impl`, `flush_run`) already performs by calling
//! `AllocCore::release_or_pool_empty_segment` whenever
//! `dec_live_and_maybe_decommit` (or its batched sibling) returns `true`.
//!
//! **Context.** Before R11-2, `drain_heap_overflow` called
//! `AllocCore::dec_live_and_maybe_decommit(base, small_cur)` per successful
//! reclaim and DISCARDED its `bool` return (`let _ = ...`). That return value
//! is the load-bearing "this segment just went fully empty, you MUST call
//! `release_or_pool_empty_segment` on it" signal every other call site
//! honours. Dropping it meant a segment emptied entirely through
//! cross-thread frees that overflowed into `HeapOverflow` was left exactly as
//! it was the instant it emptied: still registered as an ordinary live
//! segment, never pushed into the empty-small-segment hysteresis pool and
//! never released — leaking it out of the pool-cap/RSS budget those two
//! mechanisms exist to enforce (it does not corrupt anything or become
//! unreachable — `find_segment_with_free` can still find and reuse its free
//! blocks — but it permanently escapes the accounting `dbg_pooled_count`
//! and the decay-tick/pool-cap machinery track).
//!
//! **Counterfactual test.** This test drives a non-`small_cur` segment down
//! to EXACTLY zero live blocks, with the LAST live block specifically freed
//! through the `HeapOverflow` second-chance path (not the ordinary
//! per-segment ring drain, and not an own-thread `dealloc`):
//!
//! 1. Allocate enough blocks of `TARGET_CLASS` to materialise the directory
//!    sidecar and to span multiple segments (mirrors
//!    `tests/r11_2_overflow_drain_directory_sync.rs`'s setup).
//! 2. Identify every live block in one non-`small_cur` segment (via
//!    `dbg_segment_base_of_ptr` + `dbg_live_count_for`).
//! 3. Free all-but-one of that segment's blocks own-thread — ordinary
//!    `dealloc`, decrementing `live_count` down to exactly 1 remaining
//!    (`live_count == 1` is confirmed via `dbg_live_count_for`).
//! 4. Own-thread-free ONE further magazine-filler block from elsewhere in the
//!    same segment... **not needed**: the ring-fill trick from the Bug 1 test
//!    only needs ONE magazine-resident block of the segment to fill the ring
//!    against; reuse the highest-still-live pointer for that role, then free
//!    the LAST remaining live block of the segment CROSS-THREAD, with the
//!    ring already full, so it routes into `HeapOverflow`.
//! 5. The owner's next `alloc` triggers `refill_magazine_slow` →
//!    `drain_heap_overflow`, which reclaims the overflow entry — bringing the
//!    segment's `live_count` to 0.
//!
//! Under the FIXED code, `dec_live_and_maybe_decommit`'s `true` return is
//! collected and `release_or_pool_empty_segment` finalizes the segment after
//! the drain returns — `dbg_pooled_count()` increases by (at least) one.
//! Under the buggy code, the segment is left as an ordinary registered
//! segment and `dbg_pooled_count()` does NOT change.
//!
//! **Feature gate.** `alloc-global`, `alloc-xthread`, `fastbin`,
//! `alloc-decommit` (gates `dec_live_and_maybe_decommit` /
//! `release_or_pool_empty_segment` themselves), `alloc-stats` (production
//! bundle). Under other configurations the file compiles as an empty test
//! binary (0 tests, pass by absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-decommit"
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore/HeapCore in the process.
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
/// carve range (0..~40) so it carves its OWN segment(s), and large enough
/// that `refill_n_for_class == 1` (block_size > REFILL_BYTE_BUDGET/2), so the
/// magazine holds at most 1 block at a time — simplifying the residual
/// reasoning (mirrors `tests/r11_2_overflow_drain_directory_sync.rs`).
const TARGET_CLASS: usize = 40;

/// A SEPARATE class, touched by nothing else in this test, used ONLY to
/// trigger `refill_magazine_slow` -> `drain_heap_overflow` without the
/// SAME refill also reaching into `TARGET_CLASS`'s just-emptied segment.
///
/// This matters because `drain_heap_overflow` drains the ENTIRE
/// `HeapOverflow` ring (every class, every segment) as a side effect of
/// ANY magazine-miss refill, not just a refill of the class whose overflow
/// entry is being reclaimed. If the SAME alloc call that triggers the drain
/// also refills TARGET_CLASS, `refill_class_bump_checked`'s free-scan
/// (`find_segment_with_free`/`find_segment_with_free_checked`) would
/// immediately find the just-reclaimed-and-pooled block (directory bit set
/// by the Bug 1 fix) and pull it straight back out, re-incrementing
/// `live_count` and un-pooling the segment inside the SAME call this test
/// is trying to observe. Triggering via an unrelated class's magazine miss
/// reclaims the overflow entry (bringing `live_count` to 0 and finalizing
/// the segment) as a pure side effect, while the actual refill this alloc
/// performs never touches TARGET_CLASS or its segment.
const TRIGGER_CLASS: usize = 41;

/// Enough blocks of TARGET_CLASS to cross the directory-materialise
/// threshold and span multiple segments (mirrors the sibling Bug 1 test).
const FILL_BLOCKS: usize = 3500;

/// Capacity of the per-segment RemoteFreeRing. Must match
/// `RemoteFreeRing::RING_CAP` (256) — we push exactly this many entries to
/// fill the ring so the segment's final cross-thread free overflows into
/// `HeapOverflow`.
const RING_CAP: usize = 256;

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
/// the first `threshold + slack` distinct classes. Returns the live pointers
/// (caller keeps them alive so the segments don't get recycled by decommit).
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

/// Regression test for the R11-2 Bug 2 fix: `drain_heap_overflow` must
/// finalize (pool/release) a segment that reaches `live_count == 0` via an
/// overflow-ring reclaim.
///
/// **RED before the fix:** the segment's LAST live block is reclaimed from
/// `HeapOverflow` (bringing `live_count` to 0), but
/// `release_or_pool_empty_segment` is never called → `dbg_pooled_count()`
/// stays unchanged.
///
/// **GREEN after the fix:** the emptied base is collected and finalized after
/// the drain returns → `dbg_pooled_count()` increases by (at least) one.
#[test]
fn overflow_drain_finalizes_emptied_segment() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Verify TARGET_CLASS has refill_n == 1 so the magazine holds <=1 block —
    // the same simplifying precondition the sibling Bug 1 test relies on.
    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(TARGET_CLASS) };
    assert_eq!(
        refill_n, 1,
        "TARGET_CLASS must have refill_n=1 for this test's magazine reasoning"
    );

    // Phase 0: materialise the directory sidecar (not strictly required for
    // Bug 2's own assertion, but mirrors the sibling test's setup and
    // exercises the SAME production alloc path so the two tests' segment
    // layouts are directly comparable).
    let _keep_alive = materialise_directory(heap);

    // Phase 1: allocate FILL_BLOCKS of TARGET_CLASS, spanning multiple
    // segments.
    let blocks = alloc_batch(heap, TARGET_CLASS, FILL_BLOCKS);
    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    let small_cur = unsafe { (*heap).dbg_last_stamped_segment() };

    // Phase 2: pick a target segment that is NOT small_cur — walk `blocks`
    // from the end (earlier-carved segments) until we find one whose base
    // differs from small_cur, then collect EVERY pointer in `blocks` that
    // belongs to that same segment.
    let target_base = blocks
        .iter()
        .rev()
        .map(|&p| unsafe { (*heap).dbg_segment_base_of_ptr(p) })
        .find(|&b| b != small_cur)
        .expect("must find a non-small_cur segment among FILL_BLOCKS allocations");

    let mut target_indices: Vec<usize> = (0..blocks.len())
        .filter(|&i| unsafe { (*heap).dbg_segment_base_of_ptr(blocks[i]) } == target_base)
        .collect();
    assert!(
        target_indices.len() >= 3,
        "target segment must hold at least 3 blocks of TARGET_CLASS to run \
         this scenario (own-thread drain-down + ring-filler + overflow \
         final free); got {}",
        target_indices.len()
    );

    let live_before = unsafe { (*heap).dbg_live_count_for(target_base) }
        .expect("dbg_live_count_for must resolve a live small/primordial segment");
    assert_eq!(
        live_before as usize,
        target_indices.len(),
        "every live block of target_base must be one we allocated into `blocks`"
    );

    // Reserve the LAST index as the final cross-thread free that must route
    // through HeapOverflow (the block that brings live_count to exactly 0).
    // Every OTHER block of this segment is freed own-thread first.
    let p_overflow = blocks[*target_indices.last().unwrap()];
    target_indices.truncate(target_indices.len() - 1);

    // Phase 3: free every OTHER block of the target segment own-thread. This
    // pushes freed blocks into the (class-wide, TCACHE_CAP=16) TARGET_CLASS
    // magazine; own-thread frees do NOT decrement `live_count` until the
    // magazine actually FLUSHES a run back to the substrate (`flush_run` ->
    // `dec_live_batch_and_maybe_decommit`) — so `live_count` does not track
    // 1:1 with frees issued here. `dbg_flush_all()` forces every class's
    // magazine (including TARGET_CLASS's) back to the substrate, making
    // `live_count` exact and observable afterward AND moving every freed
    // block from the magazine onto the segment's BinTable free list.
    // `p_overflow` is NOT in `target_indices` (reserved above) and is still
    // live at this point, so the flush cannot touch it.
    // SAFETY: each `blocks[i]` is a live allocation owned by `heap`, freed
    // here exactly once.
    for &i in &target_indices {
        unsafe { (*heap).dealloc(blocks[i], layout) };
    }
    unsafe { (*heap).dbg_flush_all() };
    let live_mid = unsafe { (*heap).dbg_live_count_for(target_base) }
        .expect("target segment must still be registered (not yet empty)");
    assert_eq!(
        live_mid, 1,
        "target segment must have exactly 1 live block left (p_overflow) \
         after freeing+flushing every other block of the segment"
    );

    // Phase 4: fill the segment's RemoteFreeRing with RING_CAP double-free
    // entries targeting an ALREADY-FREED block of this segment (any entry
    // from `target_indices` — all now sitting on the segment's BinTable free
    // list after the flush above, not merely magazine-resident). This does
    // NOT need magazine residency the way the sibling Bug 1 test's ring-fill
    // does (that test isolates the overflow path from the ordinary ring
    // drain specifically); here we only need the ring genuinely FULL, and
    // `reclaim_offset_checked`/`reclaim_offset`'s `is_free` bitmap guard
    // rejects a double-free of an already-free BinTable block regardless of
    // magazine residency, so these 256 pushes stay inert exactly like the
    // sibling test's.
    // SAFETY (R6-MS-4): `ring_filler` is a block in a segment owned by this
    // heap that was already freed (own-thread) above; these pushes are
    // deliberate double-frees to fill the ring (the `is_free` bitmap guard
    // rejects each one at drain time — a defensive no-op, same pattern as
    // tests/regression_xthread_double_free_residual.rs and the sibling Bug 1
    // test).
    let ring_filler = blocks[target_indices[0]];
    for _ in 0..RING_CAP {
        let ok = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
        assert!(ok, "dbg_push_to_ring failed before reaching RING_CAP");
    }
    let overflow_check = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
    assert!(!overflow_check, "ring should be full after RING_CAP pushes");

    // Phase 5: cross-thread free p_overflow — THE LAST LIVE BLOCK of the
    // target segment — from a producer thread. Since the ring is full,
    // `push_with_overflow_retry`'s ring.push fails -> falls through to
    // `push_to_heap_overflow` -> the entry lands in HeapOverflow. This free
    // is the one that will bring live_count to 0, and it happens ENTIRELY
    // through the overflow path (not the per-segment ring, not an own-thread
    // dealloc) -- the exact scenario Bug 2's fix must handle.
    let pooled_before = unsafe { (*heap).dbg_pooled_count() };

    let x_addr = p_overflow as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // SAFETY (R6-MS-1/2 + raw-deref): `remote` is a live heap; `x_addr`
        // is a block previously allocated by `heap` (the owner). This dealloc
        // from a DIFFERENT thread routes through `dealloc_foreign_slow` ->
        // `push_with_overflow_retry`, which finds the ring full and pushes
        // into HeapOverflow.
        unsafe { (*remote).dealloc(x_addr as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    // Phase 6: allocate one block of TRIGGER_CLASS (a class untouched by
    // anything else in this test, so its magazine starts empty). This is a
    // genuine magazine MISS -> `refill_magazine_slow` -> `drain_heap_overflow`,
    // which drains the ENTIRE HeapOverflow ring (all classes, all segments)
    // as a side effect -- reclaiming p_overflow's overflow entry and bringing
    // TARGET_CLASS's target segment down to live_count == 0 -- while the
    // refill THIS alloc call performs is for TRIGGER_CLASS, so it carves a
    // fresh block instead of reaching into (and un-pooling) the just-emptied
    // target segment. See TRIGGER_CLASS's doc comment for why this
    // class-separation is necessary.
    let trigger_bs = AllocCore::dbg_block_size(TRIGGER_CLASS);
    let trigger_layout = Layout::from_size_align(trigger_bs, 8).expect("TRIGGER_CLASS layout");
    let _trigger = unsafe { (*heap).alloc(trigger_layout) };
    assert!(!_trigger.is_null(), "trigger alloc returned null");

    // Phase 8: assert the target segment reached live_count == 0 (the
    // overflow reclaim succeeded) AND was finalized into the pool. Under the
    // fixed code, `dec_live_and_maybe_decommit`'s `true` return is collected
    // and `release_or_pool_empty_segment` runs after the drain -> pooled
    // count increases. Under the buggy code, the segment empties but is
    // never finalized -> pooled count is unchanged.
    //
    // live_count is checked BEFORE dbg_pooled_count: once pooled/released,
    // `dbg_live_count_for` may report differently for a released (recycled,
    // slot NULLed) segment — checking live_count first, while the segment is
    // guaranteed still registered (pooled segments stay registered; only a
    // release-path segment's slot is NULLed, and pool_cap default is > 0 so
    // admission is expected here), pins down that the emptying itself
    // happened via the overflow reclaim before asserting on the pool-side
    // effect.
    let live_final = unsafe { (*heap).dbg_live_count_for(target_base) };
    assert_eq!(
        live_final,
        Some(0),
        "target segment must reach live_count == 0 after drain_heap_overflow \
         reclaimed p_overflow (the last live block): got {live_final:?}"
    );

    let pooled_after = unsafe { (*heap).dbg_pooled_count() };
    assert!(
        pooled_after > pooled_before,
        "pooled_count must increase after drain_heap_overflow emptied \
         target_base: pooled_before={pooled_before}, pooled_after={pooled_after}. \
         Before R11-2, drain_heap_overflow discarded \
         dec_live_and_maybe_decommit's `true` return and never called \
         release_or_pool_empty_segment, so an overflow-emptied segment was \
         left as an ordinary registered segment instead of being \
         pooled/released."
    );

    // Cleanup: free the TRIGGER_CLASS block allocated in Phase 6.
    // `target_indices`' blocks were already freed in Phase 3; `p_overflow`
    // was already logically freed (cross-thread overflow reclaim) and must
    // NOT be dealloc'd again (it is either still on the target segment's
    // BinTable, or its segment has since been reused). `_keep_alive` (the
    // materialisation carve, mirroring the sibling Bug 1 test) is
    // intentionally left un-freed and simply drops: the whole heap is
    // recycled immediately below, exactly as the sibling test does.
    unsafe { (*heap).dealloc(_trigger, trigger_layout) };
    unsafe { HeapRegistry::recycle(heap) };
}
