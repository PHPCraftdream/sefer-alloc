//! R13-3 (task #273, N1 — resource-defect fix, Defect 2) regression: a heap
//! that calls **ONLY** [`HeapCore::alloc_zeroed`] — never the plain
//! [`HeapCore::alloc`] — must still opportunistically drain its
//! `HeapOverflow` second-chance ring (RAD-4b) and its deferred-large-free
//! stack (A1/UBFIX-10), exactly like a heap that calls plain `alloc` does.
//!
//! **The bug (pre-fix).** The `virgin-zero-skip` small arm of
//! `HeapCore::alloc_zeroed` bypassed `self.alloc()` entirely (to reach
//! `AllocCore::alloc_small_with_virgin` directly, carrying the virgin
//! signal) and called `AllocCore::alloc_small_with_virgin` with NO drain
//! prelude — unlike the Large branch of the SAME function (which explicitly
//! replicates `alloc`'s two drains, `heap_core_alloc.rs` ~ lines 373-379) and
//! unlike the ordinary Small path (which gets the drains for free via
//! `refill_magazine_slow` on every magazine miss, see the UBFIX-10/RAD-4b
//! comments there). A calloc-heavy workload — the EXACT target profile this
//! feature exists for — that never calls plain `alloc` therefore never
//! drained its overflow ring: cross-thread-freed blocks queued there stayed
//! unreclaimed indefinitely, and a segment that emptied entirely via such a
//! reclaim was never pooled/released — an unbounded resource-retention leak.
//!
//! **This test (mirrors `tests/r11_2_overflow_drain_pool_release.rs`'s
//! scenario exactly, substituting `alloc_zeroed` for `alloc` at every
//! allocation site AND at the trigger site — the substitution is the whole
//! point).** Drives a non-`small_cur` segment down to `live_count == 1` via
//! own-thread `alloc_zeroed`/`dealloc`, fills its `RemoteFreeRing` to force
//! the LAST live block's cross-thread free to overflow into `HeapOverflow`,
//! then triggers a magazine miss with `alloc_zeroed` (NOT `alloc`) on an
//! unrelated class. Asserts the target segment reaches `live_count == 0` and
//! gets pooled/released — i.e. the overflow entry WAS reclaimed by an
//! `alloc_zeroed`-only call sequence.
//!
//! **Red/green discipline.** Verified RED against the pre-fix
//! `HeapCore::alloc_zeroed` (the R12-10 magazine-bypass shape with no drain
//! prelude) by temporarily reverting the small arm to call
//! `self.core.alloc_small_with_virgin(class_idx)` directly with no drains —
//! the target segment's `live_count` stays 1 and `dbg_pooled_count()` never
//! increases, because nothing ever drains the ring. GREEN after the R13-3
//! fix (this file's HEAD state): the fixed `alloc_zeroed` small arm goes
//! back through `self.alloc`'s own hit/miss machinery under `fastbin`
//! (recovering `refill_magazine_slow`'s drains for free), so the SAME
//! `alloc_zeroed`-only sequence reclaims the overflow entry.
//!
//! **Feature gate.** `alloc-global`, `alloc-xthread`, `fastbin`,
//! `alloc-decommit`, `virgin-zero-skip` (the feature under test). Under
//! other configurations the file compiles as an empty test binary (0 tests,
//! pass by absence) — this defect is specific to the `virgin-zero-skip`
//! small arm, which does not exist without the feature.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-decommit",
    feature = "virgin-zero-skip"
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore in the process.
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

/// The class the OWNER allocates and frees, exclusively via `alloc_zeroed`.
/// Chosen above the materialisation carve range so it carves its OWN
/// segment(s), and large enough that `refill_n_for_class == 1`, keeping the
/// magazine-residual reasoning simple (mirrors the R11-2 sibling test).
const TARGET_CLASS: usize = 40;

/// A SEPARATE class, touched by nothing else in this test, used ONLY to
/// trigger `refill_magazine_slow` (via `alloc_zeroed`, not `alloc`) without
/// the SAME refill also reaching into `TARGET_CLASS`'s just-emptied segment.
/// See `tests/r11_2_overflow_drain_pool_release.rs`'s identical constant for
/// the full rationale (unchanged here).
const TRIGGER_CLASS: usize = 41;

/// Enough blocks of TARGET_CLASS to cross the directory-materialise
/// threshold and span multiple segments.
const FILL_BLOCKS: usize = 3500;

/// Capacity of the per-segment RemoteFreeRing (must match
/// `RemoteFreeRing::RING_CAP`).
const RING_CAP: usize = 256;

/// Allocate `count` blocks of `class_idx` via `HeapCore::alloc_zeroed` (NOT
/// plain `alloc` — the whole point of this test is that the caller never
/// touches the plain `alloc` path at all). Returns the live pointers.
fn alloc_zeroed_batch(heap: *mut HeapCore, class_idx: usize, count: usize) -> Vec<*mut u8> {
    let bs = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(bs, 8).expect("class block size is a valid layout");
    let mut v = Vec::with_capacity(count);
    for _ in 0..count {
        let p = unsafe { (*heap).alloc_zeroed(layout) };
        assert!(
            !p.is_null(),
            "alloc_zeroed returned null for class {class_idx}"
        );
        v.push(p);
    }
    v
}

/// Force the directory sidecar to materialise using `alloc_zeroed` for every
/// carve (mirrors `materialise_directory` in the R11-2 sibling test, with
/// `alloc` swapped for `alloc_zeroed`).
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
        let p = unsafe { (*heap).alloc_zeroed(layout) };
        assert!(
            !p.is_null(),
            "materialise alloc_zeroed for class {cls} returned null"
        );
        keep_alive.push(p);
    }
    keep_alive
}

/// THE test: an `alloc_zeroed`-only calling sequence must still reclaim a
/// cross-thread free that overflowed into `HeapOverflow`, and finalize
/// (pool/release) the segment that reclaim empties.
#[test]
fn alloc_zeroed_only_workload_drains_overflow_and_finalizes_segment() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(TARGET_CLASS) };
    assert_eq!(
        refill_n, 1,
        "TARGET_CLASS must have refill_n=1 for this test's magazine reasoning"
    );

    // Phase 0: materialise the directory sidecar via alloc_zeroed only.
    let _keep_alive = materialise_directory(heap);

    // Phase 1: allocate FILL_BLOCKS of TARGET_CLASS via alloc_zeroed only,
    // spanning multiple segments.
    let blocks = alloc_zeroed_batch(heap, TARGET_CLASS, FILL_BLOCKS);
    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    let small_cur = unsafe { (*heap).dbg_last_stamped_segment() };

    // Phase 2: pick a target segment that is NOT small_cur.
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
         this scenario; got {}",
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
    // through HeapOverflow.
    let p_overflow = blocks[*target_indices.last().unwrap()];
    target_indices.truncate(target_indices.len() - 1);

    // Phase 3: free every OTHER block of the target segment own-thread
    // (`dealloc` — freeing is unaffected by this defect; only the ALLOC side
    // is under test) and force the magazine's TARGET_CLASS entries back to
    // the substrate so `live_count` is exact and observable.
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
    // entries (same technique as the R11-2 sibling test — see its Phase 4
    // doc for the full rationale; `is_free`'s bitmap guard rejects every one
    // of these at drain time, they exist only to fill the ring).
    let ring_filler = blocks[target_indices[0]];
    for _ in 0..RING_CAP {
        let ok = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
        assert!(ok, "dbg_push_to_ring failed before reaching RING_CAP");
    }
    let overflow_check = unsafe { (*heap).dbg_push_to_ring(ring_filler, TARGET_CLASS) };
    assert!(!overflow_check, "ring should be full after RING_CAP pushes");

    // Phase 5: cross-thread free p_overflow (the LAST live block of the
    // target segment) from a producer thread. The ring is full, so this
    // overflows into HeapOverflow.
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

    // Phase 6 (THE LOAD-BEARING SUBSTITUTION): trigger the magazine-miss
    // drain via `alloc_zeroed` — NOT `alloc`. Under the pre-fix
    // magazine-bypass `alloc_zeroed` small arm, this call reached
    // `AllocCore::alloc_small_with_virgin` directly with NO drain prelude, so
    // it would NOT reclaim `p_overflow`'s entry. Under the R13-3 fix, this
    // call goes back through `self.alloc`'s own miss path
    // (`refill_magazine_slow`, which unconditionally drains
    // `HeapOverflow`/the deferred-large stack before refilling), reclaiming
    // it exactly like a plain `alloc` call would.
    let trigger_bs = AllocCore::dbg_block_size(TRIGGER_CLASS);
    let trigger_layout = Layout::from_size_align(trigger_bs, 8).expect("TRIGGER_CLASS layout");
    let _trigger = unsafe { (*heap).alloc_zeroed(trigger_layout) };
    assert!(!_trigger.is_null(), "trigger alloc_zeroed returned null");

    // Phase 7: assert the target segment reached live_count == 0 (the
    // overflow reclaim succeeded via an alloc_zeroed-only call sequence) AND
    // was finalized into the pool.
    let live_final = unsafe { (*heap).dbg_live_count_for(target_base) };
    assert_eq!(
        live_final,
        Some(0),
        "target segment must reach live_count == 0 after an alloc_zeroed-only \
         trigger reclaimed p_overflow (the last live block): got {live_final:?}. \
         If this fails, the alloc_zeroed small arm is NOT draining \
         HeapOverflow on a magazine miss (Defect 2 regressed)."
    );

    let pooled_after = unsafe { (*heap).dbg_pooled_count() };
    assert!(
        pooled_after > pooled_before,
        "pooled_count must increase after an alloc_zeroed-only trigger drained \
         HeapOverflow and emptied target_base: pooled_before={pooled_before}, \
         pooled_after={pooled_after}. A heap that calls ONLY alloc_zeroed must \
         still reclaim cross-thread-freed blocks and retire the segments they \
         empty — an alloc_zeroed-only workload that never drains is the R13-3 \
         Defect 2 resource leak this test guards against."
    );

    // Cleanup.
    unsafe { (*heap).dealloc(_trigger, trigger_layout) };
    unsafe { HeapRegistry::recycle(heap) };
}
