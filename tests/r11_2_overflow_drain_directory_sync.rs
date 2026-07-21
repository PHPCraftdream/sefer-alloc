//! R11-2 (Bug 1) regression: `drain_heap_overflow` must sync the segment
//! directory after reclaiming a cross-thread-freed block from the
//! `HeapOverflow` second-chance ring — mirroring the ESTABLISHED pattern in
//! `drain_dirty_segments` / `find_segment_with_free_impl`'s per-segment ring
//! drain.
//!
//! **Context.** Before R11-2, `drain_heap_overflow` called
//! `reclaim_offset_checked` / `reclaim_offset` on each overflow entry and, on
//! success, discarded the result without updating the segment directory at
//! all. A block reclaimed via `HeapOverflow` became truly free in the
//! BinTable, but the authoritative segment directory still read it as absent
//! for that class. Any directory-driven lookup treated the segment as a
//! stale-negative miss and reserved a new segment instead of reusing the
//! freed block.
//!
//! **Counterfactual test.** This test constructs a segment whose
//! `RemoteFreeRing` is full (256 entries, all double-frees pushed via
//! `dbg_push_to_ring` targeting a magazine-resident block — rejected by the
//! `is_in_magazine` guard). TWO live blocks of the SAME class are then freed
//! cross-thread; since the ring is full, both entries route into
//! `HeapOverflow`. The owner's next alloc triggers `refill_magazine_slow` →
//! `drain_heap_overflow`, which reclaims BOTH overflow entries onto the
//! BinTable in one drain pass — but (TARGET_CLASS has `refill_n_for_class ==
//! 1`) the SAME refill only pops ONE of them back out to satisfy the
//! triggering `alloc`, leaving the other genuinely free on the BinTable.
//! Under the fixed code, the directory bit for that (class, segment) pair is
//! set for the pointer that stayed free. Under the buggy code, the directory
//! is never synced, and the bit stays clear.
//!
//! **Why two overflow entries, not one.** An earlier version of this test
//! routed a single block through the overflow ring and checked ITS directory
//! bit after the triggering alloc. That is unsound: with `refill_n == 1`,
//! `refill_class_bump_checked` pulls exactly one block into the magazine and
//! immediately pops it out to satisfy the caller — the very block whose
//! overflow entry was just reclaimed is issued right back out, so it is
//! legitimately NOT free on the BinTable by the time the directory bit is
//! read (the bit would correctly read `false` even under the FIXED code,
//! making the test unable to distinguish fixed from buggy). Routing a SECOND
//! block through the overflow path means the drain reclaims both (drain is
//! not capped by `want`), but the refill only consumes one — the other
//! remains genuinely free and observable.
//!
//! **Why this isolates the overflow path.** The 256 ring entries are all
//! double-frees targeting a magazine-resident block, so `drain_dirty_segments`
//! (or `find_segment_with_free_impl`'s P1-a ring drain) rejects every one
//! without touching the BinTable or `changed_classes`. The directory bit for
//! the target class in this segment is set ONLY by the overflow drain's new
//! `sync_directory_for_segment_classes` call. Without that call, the bit
//! stays clear.
//!
//! **Cleanup correctness note.** `p_ring` is popped back out of the magazine
//! by Phase 5's first alloc (`_pop`) — `_pop` and `p_ring` are the SAME
//! address, just a later "life" of the block (reissued, not still the
//! original allocation). The cleanup loop below must therefore skip
//! `p_ring`'s slot in `blocks` (not just `p_overflow`/`p_overflow2`'s): `_pop`
//! is freed explicitly at the end, so freeing `blocks[..]`'s copy of that
//! same address in the loop would double-free it. Under non-`hardened` this
//! double-free is silently absorbed by the M2 `is_free` bitmap guard (a
//! benign no-op); under `hardened` the additional generation/interior-pointer
//! bookkeeping does not degrade as gracefully and the combination (256 stale,
//! same-generation ring entries still undrained for this segment, PLUS a
//! double-freed reissued block) corrupts allocator metadata badly enough to
//! crash the process (`STATUS_ACCESS_VIOLATION`) later in the same cleanup
//! pass. Fixed by excluding `p_ring`'s address from the loop, exactly like
//! `p_overflow`/`p_overflow2`.
//!
//! **Feature gate.** `alloc-global`, `alloc-xthread`, `fastbin`,
//! `alloc-segment-directory` (directory must be materialised for the bit to
//! be observable), `alloc-stats` (production bundle). Under other
//! configurations the file compiles as an empty test binary (0 tests, pass by
//! absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-segment-directory"
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
/// that `refill_n_for_class == 1` (block_size > REFILL_BYTE_BUDGET/2),
/// so the magazine holds at most 1 block at a time — simplifying the
/// live_count / magazine-residency reasoning.
const TARGET_CLASS: usize = 40;

/// Enough blocks of TARGET_CLASS to cross the directory-materialise
/// threshold (DIRECTORY_MATERIALIZE_THRESHOLD = 32 segments). Each 4 MiB
/// segment holds ~93 blocks of ~43 KB, so 3500 blocks span ~36 segments,
/// safely past the threshold.
const FILL_BLOCKS: usize = 3500;

/// Capacity of the per-segment RemoteFreeRing. Must match
/// `RemoteFreeRing::RING_CAP` (256) — we push exactly this many entries to
/// fill the ring so the next cross-thread free overflows into `HeapOverflow`.
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

/// Regression test for the R11-2 Bug 1 fix: `drain_heap_overflow` must sync
/// the segment directory after reclaiming a cross-thread-freed block from the
/// `HeapOverflow` ring.
///
/// **RED before the fix:** the overflow entry is reclaimed (block put on
/// BinTable) but the directory is never synced → the directory bit for the
/// target (class, segment) pair stays `Some(false)` (or `None` if not
/// materialised).
///
/// **GREEN after the fix:** `sync_directory_for_segment_classes` is called
/// inline per successful reclaim → the bit becomes `Some(true)`.
#[test]
fn overflow_drain_syncs_segment_directory() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Verify TARGET_CLASS has refill_n == 1 so the magazine holds ≤1 block.
    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(TARGET_CLASS) };
    assert_eq!(
        refill_n, 1,
        "TARGET_CLASS must have refill_n=1 for this test's magazine reasoning"
    );

    // Phase 0: materialise the directory sidecar.
    let _keep_alive = materialise_directory(heap);

    // Phase 1: allocate FILL_BLOCKS of TARGET_CLASS. These span multiple
    // segments. We use the LAST segment's blocks for the test (to avoid
    // the primordial segment, which `dec_live_and_maybe_decommit` excludes).
    let blocks = alloc_batch(heap, TARGET_CLASS, FILL_BLOCKS);
    let bs = AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    // Pick three blocks from the LAST segment (not small_cur, not
    // primordial). `small_cur` points at the most recently carved segment;
    // blocks near the end of `blocks` are in earlier segments.
    let p_ring = blocks[FILL_BLOCKS - 3]; // will be freed to magazine, used to fill the ring
    let p_overflow = blocks[FILL_BLOCKS - 2]; // freed cross-thread → HeapOverflow; reclaimed AND reissued by the trigger alloc
    let p_overflow2 = blocks[FILL_BLOCKS - 1]; // freed cross-thread → HeapOverflow; reclaimed but stays free (refill_n == 1)

    // Verify all three blocks are in the same segment. `os::segment_base_of_ptr`
    // itself is `pub(crate)` and unreachable from an integration test, so we
    // go through the `dbg_segment_base_of_ptr` test-only accessor instead.
    let base_ring = unsafe { (*heap).dbg_segment_base_of_ptr(p_ring) };
    let base_overflow = unsafe { (*heap).dbg_segment_base_of_ptr(p_overflow) };
    let base_overflow2 = unsafe { (*heap).dbg_segment_base_of_ptr(p_overflow2) };
    assert_eq!(
        base_ring, base_overflow,
        "p_ring and p_overflow must be in the same segment"
    );
    assert_eq!(
        base_ring, base_overflow2,
        "p_ring and p_overflow2 must be in the same segment"
    );
    // Verify this segment is NOT small_cur (it's an earlier segment).
    let small_cur = unsafe { (*heap).dbg_last_stamped_segment() };
    // small_cur is the last stamped segment — may or may not equal base_ring.
    // We only need base_ring != small_cur for dec_live_and_maybe_decommit to
    // be eligible. If they happen to be the same, pick earlier blocks.
    let p_ring = if base_ring == small_cur {
        blocks[FILL_BLOCKS - 13]
    } else {
        p_ring
    };
    let p_overflow = if base_overflow == small_cur {
        blocks[FILL_BLOCKS - 12]
    } else {
        p_overflow
    };
    let p_overflow2 = if base_overflow2 == small_cur {
        blocks[FILL_BLOCKS - 11]
    } else {
        p_overflow2
    };

    // Phase 2: free p_ring own-thread → magazine push (refill_n=1, so
    // magazine has capacity). p_ring becomes magazine-resident.
    // SAFETY: `p_ring` is a live allocation owned by `heap`; this dealloc is
    // its single logical free.
    unsafe { (*heap).dealloc(p_ring, layout) };

    // Phase 3: fill the segment's RemoteFreeRing with RING_CAP double-free
    // entries, all targeting p_ring (which is magazine-resident →
    // reclaim_offset_checked's is_in_magazine guard will reject each one).
    // This fills the ring so the NEXT cross-thread free overflows into
    // HeapOverflow.
    // SAFETY (R6-MS-4): p_ring is a live block in a segment owned by this
    // heap; these pushes are deliberate double-frees to fill the ring (the
    // magazine-residency guard rejects them at drain time — a defensive
    // no-op, same pattern as tests/regression_xthread_double_free_residual.rs).
    for _ in 0..RING_CAP {
        let ok = unsafe { (*heap).dbg_push_to_ring(p_ring, TARGET_CLASS) };
        assert!(ok, "dbg_push_to_ring failed before reaching RING_CAP");
    }
    // The 257th push should FAIL (ring is full).
    let overflow_check = unsafe { (*heap).dbg_push_to_ring(p_ring, TARGET_CLASS) };
    assert!(!overflow_check, "ring should be full after RING_CAP pushes");

    // Phase 4: cross-thread free BOTH p_overflow and p_overflow2 from a
    // producer thread. Since the segment's ring is full,
    // `push_with_overflow_retry`'s ring.push fails for each → falls through
    // to `push_to_heap_overflow` → both entries land in HeapOverflow.
    let x_addr = p_overflow as usize;
    let x_addr2 = p_overflow2 as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // SAFETY (R6-MS-1/2 + raw-deref): `remote` is a live heap; `x_addr`/
        // `x_addr2` are blocks previously allocated by `heap` (the owner).
        // These deallocs from a DIFFERENT thread route through
        // `dealloc_foreign_slow` → `push_with_overflow_retry`, which finds
        // the ring full and pushes into HeapOverflow.
        unsafe { (*remote).dealloc(x_addr as *mut u8, layout) };
        unsafe { (*remote).dealloc(x_addr2 as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    // Phase 5: allocate one block of TARGET_CLASS. With refill_n=1 and the
    // magazine empty (we popped the one block in the alloc that allocated
    // p_ring... wait, the magazine currently has p_ring from Phase 2).
    // We need the magazine EMPTY so the alloc triggers refill_magazine_slow
    // → drain_heap_overflow. Pop the magazine by allocating one block.
    // SAFETY: valid layout; heap is the calling thread's own slot.
    let _pop = unsafe { (*heap).alloc(layout) };
    assert!(!_pop.is_null(), "magazine pop returned null");
    // Now magazine is empty. The NEXT alloc will miss → refill_magazine_slow
    // → drain_heap_overflow, which drains BOTH overflow entries (drain is not
    // capped by `want`) onto the BinTable — but the refill itself only pulls
    // `want == 1` block back out to satisfy this call, so exactly one of
    // {p_overflow, p_overflow2} remains genuinely free on the BinTable.
    let _trigger = unsafe { (*heap).alloc(layout) };
    assert!(!_trigger.is_null(), "trigger alloc returned null");
    assert!(
        _trigger == p_overflow || _trigger == p_overflow2,
        "trigger alloc must be one of the two overflow-reclaimed blocks \
         (proves drain_heap_overflow's reclaim succeeded): got {_trigger:?}"
    );
    // Whichever of the two was NOT reissued is the one still free on the
    // BinTable — that is the pointer whose directory bit Phase 6 checks.
    let still_free = if _trigger == p_overflow {
        p_overflow2
    } else {
        p_overflow
    };

    // Phase 6: assert the directory bit for the target (class, segment) is
    // set for the block that stayed free. Under the fixed code,
    // drain_heap_overflow's sync_directory_for_segment_classes set it. Under
    // the buggy code, the overflow reclaim put the block on the BinTable but
    // never synced the directory → the bit stays clear.
    let bit = unsafe { (*heap).dbg_directory_bit_for_ptr(still_free, TARGET_CLASS) };
    assert!(
        bit == Some(true),
        "directory bit for TARGET_CLASS in still_free's segment must be \
         Some(true) after drain_heap_overflow reclaimed the overflow entries: \
         got {bit:?}. Before R11-2, the overflow drain never called \
         sync_directory_for_segment_classes, so the directory read the \
         segment as absent for this class even though the BinTable had a \
         free block."
    );

    // Cleanup: free everything we allocated. `p_ring` is skipped here in
    // ADDITION to `p_overflow`/`p_overflow2`: `p_ring` was popped back out of
    // the magazine by `_pop` (Phase 5's first alloc) and is freed explicitly
    // below via `dealloc(_pop, ...)` — freeing `blocks[..]`'s copy of that
    // same address here too would be a double-free (see the module doc's
    // "Cleanup correctness note").
    // SAFETY: all these pointers were allocated via `heap.alloc` with `layout`.
    for &p in &blocks {
        if p != p_overflow && p != p_overflow2 && p != p_ring {
            unsafe { (*heap).dealloc(p, layout) };
        }
    }
    unsafe { (*heap).dealloc(_pop, layout) };
    unsafe { (*heap).dealloc(_trigger, layout) };
    // `still_free` is still on the BinTable (never re-issued) — it was
    // already logically freed by its cross-thread dealloc in Phase 4 and
    // must NOT be dealloc'd again here.
    unsafe { HeapRegistry::recycle(heap) };
}
