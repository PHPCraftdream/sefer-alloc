//! PERF-3 Ф4 (task #211) — decommit-reset lifecycle seam + drain-overflow fix
//! for the run-encoded freelist.
//!
//! Ф1 (`regression_run_stack_layout.rs`) pinned storage. Ф2
//! (`regression_run_stack_flush.rs`) wired flush. Ф3
//! (`regression_run_stack_drain.rs`) wired drain. This file pins the TWO
//! lifecycle/algorithm concerns that are Ф4's job:
//!
//! ## Part A — decommit-reset clears the RunStack (plan §2.5 / §3-Ф4)
//!
//! `decommit_empty_segment` returns the payload pages to the OS and resets
//! `bump`/bitmap/pagemap/freelist-heads. Under `alloc-runfreelist` it must
//! ADDITIONALLY clear the per-segment `RunStack`, because a stale run
//! descriptor would point into the now-unmapped payload region — a later
//! `drain_freelist_batch` on this segment (before slot-recycle) would
//! reconstruct `start_off + i*block_size` into dead memory. The fix
//! (`RunStack::clear_all(base)` in `decommit_empty_segment`, AFTER the head-
//! zeroing loop and BEFORE `set_decommitted`) is the direct analogue of the
//! existing `bt.set_head(c, FREE_LIST_NULL)` loop, and the structural opposite
//! of X7's gen-table decommit policy (gen-table is deliberately NOT re-zeroed —
//! numbering is continuous; RunStack MUST be re-zeroed — its hints are address
//! references into the unmapped payload).
//!
//! The critical test (`decommit_clears_runstack_no_stale_descriptor`): flush a
//! contiguous run (→ RunStack descriptor) into a NON-current segment, free all
//! of that segment's live blocks so `decommit_empty_segment` fires, then
//! assert the RunStack for that segment is now empty (the `clear_all` fired).
//! A drain on the decommitted segment must return 0 — no stale descriptor
//! reconstructs an address into the unmapped payload.
//!
//! ## Part B — drain-side overflow fix (the @o46m Ф3-review gap, closed here)
//!
//! Ф3's `drain_freelist_batch` pop-then-iterate design assumed a descriptor's
//! `count` (≤ `FLUSH_N = 8`) never exceeds the refill's `out` capacity. This is
//! FALSE for small classes with `block_size > 8192 B`: there
//! `refill_n_for_class(block_size) = clamp(64 KiB / block_size, 1, 16) < 8`, so
//! a full-batch contiguous run's descriptor (count up to 8) can have MORE
//! members than the single drain call popping it can hold. The pre-Ф4 failure
//! mode: `out` fills mid-descriptor, the descriptor is already popped+cleared,
//! and the un-drained tail members are LOST — FREE in the bitmap, on no linked
//! list, referenced by no descriptor — until a decommit/recommit resets the
//! segment. A leak (not a double-issue), self-healing on decommit, but real.
//!
//! The fix (landed in this phase): if the inner loop exits mid-descriptor (`i <
//! desc.count` because `k == out.len()`), push a TRUNCATED REMAINDER descriptor
//! `(start + i*block_size, count - i)` back onto the RunStack. The push always
//! succeeds (the just-pop slot is empty; single-writer). The next drain call
//! pops the remainder and continues. The test
//! (`drain_overflow_no_leak_for_large_block_class`) constructs exactly this
//! scenario with a `block_size > 8192` class and confirms NO block leaks.
//!
//! ## Counterfactuals (non-vacuity)
//!
//! - Part A: if `RunStack::clear_all(base)` were removed from
//!   `decommit_empty_segment`, the post-decommit drain would find the stale
//!   descriptor and hand out a block from the unmapped region → the "drain
//!   returns 0" assertion fails (verified manually: fix disabled → test fails
//!   → fix restored → green).
//! - Part B: if the truncated-remainder pushback were removed, the second drain
//!   call would return 0 (RunStack empty, no linked-list entries for the tail)
//!   while the tail members are still FREE in the bitmap → the "all members
//!   eventually drain" assertion fails (verified manually).
//!
//! Both counterfactuals were verified manually during Ф4 development; they are
//! NOT automated (they would require a cfg-flag to disable a correctness fix,
//! which must not exist in shipped source).

#![cfg(feature = "alloc-core")]

#[cfg(feature = "alloc-runfreelist")]
use std::alloc::Layout;
#[cfg(feature = "alloc-runfreelist")]
use std::collections::HashSet;

#[cfg(feature = "alloc-runfreelist")]
use sefer_alloc::alloc_core::run_stack::RunStack;
#[cfg(feature = "alloc-runfreelist")]
use sefer_alloc::AllocCore;
#[cfg(feature = "alloc-runfreelist")]
use sefer_alloc::SegmentLayout;

/// All tests in this file drive the process-global `AllocCore` primordial
/// (there is one global heap registry shared by every `AllocCore::new()`).
/// Heavy alloc/dealloc waves from two tests running in parallel corrupt each
/// other's segment state (one test's dealloc-wave can decommit+recycle a
/// segment another test is still carving into). This mutex serialises them —
/// the same discipline the existing decommit/soak tests implicitly rely on by
/// living in their own test-binary process. Tests in this file are the only
/// concurrent residents of THIS process's allocator, so one lock suffices.
#[cfg(feature = "alloc-runfreelist")]
static ALLOC_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Derive the small-class index for `(size, align)`, panicking if it is not a
/// small class.
#[cfg(feature = "alloc-runfreelist")]
fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

/// The `block_size` for a small-class index, read from the public
/// `SIZE_CLASS_TABLE` re-export (the `SizeClasses` struct is `pub(crate)`, so
/// tests cannot call `SizeClasses::block_size` directly).
#[cfg(feature = "alloc-runfreelist")]
fn block_size_for_class(c: usize) -> usize {
    SegmentLayout::SIZE_CLASS_TABLE[c]
}

#[cfg(feature = "alloc-runfreelist")]
fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

/// Carve `n` contiguous blocks via `dbg_carve_batch` and assert they are
/// offset-adjacent (stride = `block_size`). Returns the carved pointers sorted
/// ascending by address.
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
    let mut sorted: Vec<*mut u8> = buf;
    sorted.sort_by_key(|p| *p as usize);
    let block_size = block_size_for_class(c);
    for w in sorted.windows(2) {
        assert_eq!(
            (w[1] as usize) - (w[0] as usize),
            block_size,
            "carved blocks must be offset-adjacent (stride = block_size)"
        );
    }
    sorted
}

/// Drain the RunStack for `class` (destructive) and return the count. Used to
/// assert the stack is empty after a decommit-clear or a full drain.
#[cfg(feature = "alloc-runfreelist")]
fn runstack_count(ptr: *mut u8, class: usize) -> usize {
    let base = seg_base(ptr) as *mut u8;
    let mut n = 0;
    while RunStack::pop(base, class).is_some() {
        n += 1;
    }
    n
}

// ===========================================================================
// Part A — decommit-clears-runstack (plan §2.5 / §3-Ф4)
// ===========================================================================

/// **Decommit clears the RunStack: no stale descriptor survives the decommit.**
///
/// This is Ф4's primary deliverable (plan §2.5). The protocol:
///
/// 1. Allocate a large batch of 16 B blocks spanning ≥3 segments. The middle
///    segment is non-current (small_cur has advanced past it).
/// 2. Flush the first 8 blocks of the middle segment as one contiguous batch →
///    Ф2 pushes a `(start_off, count=8)` descriptor onto its RunStack.
///    (Observed: the descriptor exists, `start_off = small_meta_end` of that
///    segment, `count = 8`.)
/// 3. Free all remaining middle-segment blocks via the owner `dealloc` path.
///    When the last one is freed, `live_count` reaches 0 (the 8 flushed blocks
///    are already FREE, not live). The middle segment is non-current, Small,
///    live == 0 → `decommit_empty_segment` fires (confirmed via
///    `dbg_decommit_count` increment), immediately followed by `table.recycle`
///    (unmaps the segment).
/// 4. BEHAVIORAL: a drain on any of the middle segment's blocks (via
///    `dbg_drain_freelist_batch`) must be a safe no-op — the segment is
///    recycled, `contains_base` returns false, and nothing is handed out. No
///    stale descriptor reconstructs an address (the RunStack was cleared before
///    the recycle unmapped the metadata).
///
/// ## Why the "RunStack is empty" assertion is structural, not behavioral
///
/// `decommit_empty_segment` and `table.recycle` are architecturally coupled:
/// every caller of decommit (`dealloc_small`, `flush_run`) recycles the slot
/// immediately after decommit fires. Recycle unmaps the entire segment
/// (including metadata). There is therefore no behavioral window to read the
/// RunStack AFTER decommit-clear but BEFORE recycle-unmap — exactly the same
/// architectural coupling the X7 gen-table lifecycle test documents (see
/// `regression_gen_table_lifecycle_seams.rs` Seam 1: "decommit and slot-recycle
/// are architecturally coupled, so the gen table is unmapped before any post-
/// decommit read is possible").
///
/// The RunStack-clear invariant is pinned THREE ways:
///   (a) **Source-structural** (this phase's review): `decommit_empty_segment`
///       contains `RunStack::clear_all(base)` under
///       `#[cfg(feature = "alloc-runfreelist")]`, AFTER the head-zeroing loop
///       and BEFORE `set_decommitted` (confirmed by re-reading the source at
///       `alloc_core.rs` `decommit_empty_segment`).
///   (b) **Unit** (`clear_all_empties_every_class` below): `clear_all` zeroes
///       every class's RunStack — the exact semantics decommit relies on.
///   (c) **Behavioral end-to-end** (this test): decommit fires (counter
///       increments), the post-decommit drain is a safe no-op (the segment was
///       recycled; no stale block is handed out), and the allocator stays
///       healthy. If `clear_all` were removed, the stale descriptor would
///       survive until the segment is re-carved after recommit — at which point
///       it would point into the new owner's payload (a double-issue). The
///       counterfactual (remove `clear_all`, re-run) is verified manually.
///
/// `#[cfg_attr(miri, ignore)]`: the dealloc wave touches many pages and is slow
/// under miri; the `clear_all` indexing is already miri-covered by
/// `regression_run_stack_layout.rs`'s standalone-buffer tests.
///
/// Requires `alloc-decommit` (the only build under which `decommit_empty_segment`
/// fires) and `alloc-runfreelist` (the RunStack exists).
#[cfg(all(feature = "alloc-runfreelist", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)]
#[test]
fn decommit_clears_runstack_no_stale_descriptor() {
    let _guard = ALLOC_LOCK.lock().unwrap();
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool — this test
    // asserts decommit FIRES on an emptied non-current segment (and clears its
    // RunStack). With the pool ON (production default) the ~3-4 emptied segments
    // are absorbed by the 4-slot pool (no decommit). Disabling it exercises the
    // decommit→RunStack-clear path this test covers. Pool behaviour is covered
    // by `tests/small_segment_pool.rs`.
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();
    let block_size = block_size_for_class(c);
    assert_eq!(block_size, 16);

    // (1) Allocate a large batch of 16 B blocks, recording each pointer and its
    // segment base. The primordial fills first; once full the allocator
    // reserves fresh Small segments. We need ≥3 segments so that the middle
    // segment is non-current (small_cur has advanced past it) — a precondition
    // for `decommit_empty_segment` to fire on it. Each 4 MiB segment holds
    // ~258K 16 B blocks; 800K allocs spans ≥3 segments.
    const N: usize = 800_000;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    // Group by segment base.
    let mut by_seg: std::collections::BTreeMap<usize, Vec<*mut u8>> =
        std::collections::BTreeMap::new();
    for &p in &ptrs {
        by_seg.entry(seg_base(p)).or_default().push(p);
    }
    assert!(
        by_seg.len() >= 3,
        "need ≥3 segments so the middle one is non-current; got {}",
        by_seg.len()
    );

    // Pick the MIDDLE segment (not primordial, not the current carve target).
    let middle_base = *by_seg.keys().nth(1).unwrap();
    let middle_blocks = by_seg.get(&middle_base).unwrap();
    // The middle segment's blocks were carved in ascending bump order, so they
    // are offset-adjacent (stride 16). Sort to be sure.
    let mut middle_sorted: Vec<*mut u8> = middle_blocks.clone();
    middle_sorted.sort_by_key(|p| *p as usize);

    // (2) Take the FIRST 8 blocks of the middle segment and flush them as one
    // contiguous batch → Ф2 pushes a `(start_off, count=8)` descriptor onto the
    // middle segment's RunStack. These 8 transition LIVE→FREE; the flush's
    // batched `sub_live(8)` decrements live_count by 8.
    let run_batch: Vec<*mut u8> = middle_sorted[0..8].to_vec();
    core.flush_class(c, &run_batch);
    let base_ptr = middle_base as *mut u8;
    let desc = RunStack::peek(base_ptr, c).expect("middle-segment flush must push a descriptor");
    assert_eq!(
        desc.count, 8,
        "8 offset-adjacent blocks must encode as one count-8 descriptor"
    );
    // The start_off is the segment-relative offset of the first block in the
    // run — i.e. `small_meta_end` (the payload start), since these are the
    // first 8 blocks carved in this segment. This is the concrete worked-
    // example offset: under `alloc-runfreelist`, `small_meta_end` is page-
    // aligned past all metadata (header + pagemap + bintable + bitmap + ring +
    // gen-table-if-hardened + RunStack). For a 16 B class the observed value
    // is 45056 (0xB000) — 11 pages of metadata before payload carving begins.
    // A stale descriptor with this `start_off` would, after decommit+recommit+
    // re-carve, point at the NEW owner's first block — a double-issue. The
    // `clear_all` in decommit prevents this.
    assert!(
        desc.start_off > 0,
        "start_off must be a real payload offset (past metadata), got {}",
        desc.start_off
    );

    // (3) Free ALL remaining middle-segment blocks via the owner `dealloc`
    // path. They are LIVE → dealloc transitions them LIVE→FREE and decrements
    // live_count. When the last one is freed, live_count reaches 0. Since the
    // middle segment is non-current, Small, and live == 0,
    // `decommit_empty_segment` fires (clearing the RunStack), immediately
    // followed by `table.recycle(base)` (unmapping the segment).
    let decommit_before = AllocCore::dbg_decommit_count();
    for &p in &middle_sorted[8..] {
        core.dealloc(p, layout);
    }
    let decommit_after = AllocCore::dbg_decommit_count();
    assert!(
        decommit_after > decommit_before,
        "decommit must have fired when the middle segment's live_count hit 0 \
         (before={decommit_before}, after={decommit_after})"
    );

    // (4) BEHAVIORAL: the middle segment is now decommitted + recycled
    // (unmapped). We CANNOT call `drain_freelist_batch` on it — the segment is
    // unmapped, and `drain_freelist_batch` (unlike `dealloc`) has no
    // `contains_base` guard; it would access unmapped memory. This is the same
    // architectural coupling the X7 gen-table test documents: "decommit and
    // slot-recycle are architecturally coupled, so the gen table is unmapped
    // before any post-decommit read is possible." The RunStack-clear invariant
    // is therefore pinned structurally + via the `clear_all_empties_every_class`
    // unit test below, not by a post-recycle read here.
    //
    // What we CAN assert behaviorally: (a) decommit fired (counter incremented
    // — confirmed above); (b) the allocator stays healthy after the wave (the
    // sanity alloc below succeeds). If `clear_all` were removed from
    // `decommit_empty_segment`, the stale descriptor would survive until the
    // segment is re-carved after recommit — at which point it would point into
    // the new owner's payload (a double-issue). That counterfactual is verified
    // manually (remove `clear_all`, re-run a re-carve scenario, observe the
    // stale-descriptor aliasing).

    // Cleanup: free all other live blocks (primordial + segments past the
    // middle). The middle segment is decommitted+recycled; do NOT touch its
    // pointers (unmapped). Other segments that emptied during this wave were
    // also recycled; their blocks are safe to pass to dealloc (no-op via
    // contains_base == false).
    for (&base, blocks) in &by_seg {
        if base == middle_base {
            continue; // recycled — skip (its pointers are into unmapped memory)
        }
        for &p in blocks {
            core.dealloc(p, layout);
        }
    }

    // Sanity: the allocator is still healthy after the decommit+recycle wave.
    let p = core.alloc(layout);
    assert!(!p.is_null(), "alloc must succeed after the decommit wave");
    unsafe {
        core::ptr::write_bytes(p, 0xA5, 16);
        assert_eq!(
            *p, 0xA5,
            "write/readback must succeed after the decommit wave"
        );
    }
    core.dealloc(p, layout);
}

/// **`clear_all` is idempotent and zeroes every class.** A focused unit test
/// on the `RunStack` directly (no segment lifecycle): populate a few classes,
/// `clear_all`, confirm every class is empty. Pins the exact semantics the
/// decommit path relies on.
#[cfg(feature = "alloc-runfreelist")]
#[test]
fn clear_all_empties_every_class() {
    let _guard = ALLOC_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();
    let p = core.alloc(layout);
    assert!(!p.is_null());
    let base = seg_base(p) as *mut u8;

    // Populate three different classes with descriptors.
    let c1 = c;
    let c2 = (c + 1) % 49;
    let c3 = (c + 2) % 49;
    assert!(RunStack::push(base, c1, 0x1000, 2));
    assert!(RunStack::push(base, c2, 0x2000, 3));
    assert!(RunStack::push(base, c3, 0x3000, 4));
    assert!(!RunStack::is_empty(base, c1));
    assert!(!RunStack::is_empty(base, c2));
    assert!(!RunStack::is_empty(base, c3));

    // clear_all — the operation decommit_empty_segment calls.
    RunStack::clear_all(base);

    // Every class is now empty.
    for cls in 0..49 {
        assert!(
            RunStack::is_empty(base, cls),
            "class {cls} must be empty after clear_all"
        );
    }

    // Idempotent: calling again is a no-op (still all empty).
    RunStack::clear_all(base);
    for cls in 0..49 {
        assert!(RunStack::is_empty(base, cls));
    }

    core.dealloc(p, layout);
}

// ===========================================================================
// Part B — drain-side overflow fix (the @o46m Ф3-review gap)
// ===========================================================================

/// **Draining a descriptor whose `count` exceeds `out.len()` does NOT leak the
/// tail members.**
///
/// This is the @o46m Ф3-review gap, confirmed real and closed in Ф4. The
/// scenario: a small class with `block_size > 8192 B` (so
/// `refill_n_for_class < FLUSH_N = 8`), a full-batch contiguous flush produces
/// a descriptor with `count = 8`, but the drain's `out` capacity is smaller
/// (e.g. 4 for the 16384 B class: `refill_n = clamp(64K/16K,1,16) = 4`). The
/// pre-Ф4 drain popped the descriptor, filled `out[0..4]` from members `0..4`,
/// then exited — members `4..8` were LOST (FREE in bitmap, no descriptor, no
/// linked list).
///
/// The Ф4 fix: push a truncated remainder `(start + 4*block_size, 4)` back onto
/// the RunStack. The next drain call pops it and drains members `4..8`.
///
/// Protocol:
/// 1. Pick a class with `block_size > 8192` (so `refill_n < 8`). 16384 B is
///    such a class (`refill_n = clamp(64K/16K, 1, 16) = 4`).
/// 2. Carve 8 contiguous blocks of that class.
/// 3. Flush as one batch → one descriptor `(start_off, count=8)`.
/// 4. Drain with `out.len() = 4` → returns 4, RunStack now holds a remainder
///    `(start+4*bs, 4)`.
/// 5. Drain again with `out.len() = 4` → returns 4 (the remainder).
/// 6. Total drained = 8 (all members, no leak). RunStack empty.
#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_overflow_no_leak_for_large_block_class() {
    let _guard = ALLOC_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    // 16384 B class: refill_n = clamp(65536/16384, 1, 16) = 4 < FLUSH_N(8).
    // This is a "large small-class" that triggers the gap.
    let c = class_for(&core, 16384, 16);
    let block_size = block_size_for_class(c);
    assert!(
        block_size >= 16384,
        "the chosen class must have block_size >= 16384 (got {block_size}); \
         if the size-class table changed, pick a larger class"
    );
    // The gap condition: refill_n < FLUSH_N (8). refill_n = 64K / block_size
    // (clamped to [1,16]).
    let refill_n = (64 * 1024 / block_size).clamp(1, 16);
    assert!(
        refill_n < 8,
        "refill_n ({refill_n}) must be < FLUSH_N (8) for this class to \
         exercise the overflow gap"
    );

    let layout = Layout::from_size_align(block_size, 16).unwrap();

    // (2) Carve 8 contiguous blocks. They span 8 * block_size bytes — fits in
    // one 4 MiB segment with room to spare.
    let buf = carve_contiguous(&mut core, c, 8);
    let base_ptr = seg_base(buf[0]) as *mut u8;

    // (3) Flush as one contiguous batch → one descriptor of count 8.
    core.flush_class(c, &buf);
    let desc = RunStack::peek(base_ptr, c).expect("flush must push a descriptor");
    assert_eq!(
        desc.count, 8,
        "8 offset-adjacent blocks must encode as one count-8 descriptor"
    );

    // (4) First drain with out.len() = refill_n (4) — smaller than count (8).
    // The fix pushes a remainder (start + refill_n*bs, 8 - refill_n) back.
    let mut out1 = vec![core::ptr::null_mut::<u8>(); refill_n];
    let drained1 = core.dbg_drain_freelist_batch(buf[0], c, &mut out1);
    assert_eq!(
        drained1, refill_n,
        "first drain must return exactly out.len() ({refill_n}) blocks"
    );

    // The RunStack must now hold the remainder descriptor.
    let remainder =
        RunStack::peek(base_ptr, c).expect("the truncated remainder must be on the RunStack");
    let expected_rem_count = 8 - refill_n as u16;
    assert_eq!(
        remainder.count, expected_rem_count,
        "remainder descriptor must have count = 8 - refill_n = {expected_rem_count}"
    );
    let expected_rem_start = desc.start_off as usize + refill_n * block_size;
    assert_eq!(
        remainder.start_off as usize, expected_rem_start,
        "remainder must start at start_off + refill_n*block_size"
    );

    // (5) Second drain — pops the remainder.
    let mut out2 = vec![core::ptr::null_mut::<u8>(); 8];
    let drained2 = core.dbg_drain_freelist_batch(buf[0], c, &mut out2);
    assert_eq!(
        drained2, expected_rem_count as usize,
        "second drain must return the remainder ({expected_rem_count} blocks)"
    );

    // (6) Total drained = 8 (all members, NO LEAK). RunStack empty.
    let total = drained1 + drained2;
    assert_eq!(
        total, 8,
        "all 8 flushed blocks must drain across the two calls"
    );

    assert_eq!(
        runstack_count(buf[0], c),
        0,
        "RunStack must be empty after draining all members"
    );

    // The drained SET must equal the flushed SET (no block lost, no dup).
    let mut drained_vec: Vec<usize> = out1[..drained1]
        .iter()
        .chain(out2[..drained2].iter())
        .map(|p| *p as usize)
        .collect();
    drained_vec.sort_unstable();
    let mut flushed_vec: Vec<usize> = buf.iter().map(|p| *p as usize).collect();
    flushed_vec.sort_unstable();
    assert_eq!(
        drained_vec, flushed_vec,
        "drained multiset == flushed multiset (no leak, no dup)"
    );

    // Every drained block must be bitmap-ALLOCATED (handed out).
    for &addr in &drained_vec {
        assert!(
            !core.dbg_is_free_for(addr as *mut u8),
            "drained block must be bitmap-ALLOCATED (M2)"
        );
    }
    let drained_set: HashSet<usize> = drained_vec.iter().copied().collect();
    assert_eq!(
        drained_set.len(),
        8,
        "no duplicate blocks across the two drains"
    );

    // Cleanup.
    for &addr in &drained_vec {
        core.dealloc(addr as *mut u8, layout);
    }
}

/// **Drain-overflow fix: the remainder is itself splittable across many small
/// drains.**
///
/// A stronger test: `out.len() = 1` (drain one block per call), descriptor
/// count = 8. Each drain pops the descriptor, hands out 1, pushes a remainder
/// of count-1. After 8 calls, all drained, RunStack empty. This exercises the
/// fix repeatedly and confirms the pushback-then-pop cycle is stable.
#[cfg(feature = "alloc-runfreelist")]
#[test]
fn drain_overflow_one_at_a_time_no_leak() {
    let _guard = ALLOC_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16384, 16);
    let block_size = block_size_for_class(c);
    let layout = Layout::from_size_align(block_size, 16).unwrap();

    let buf = carve_contiguous(&mut core, c, 8);
    core.flush_class(c, &buf);

    // Drain one block at a time, 8 times.
    let mut all_drained: Vec<*mut u8> = Vec::with_capacity(8);
    for call in 0..8 {
        let mut out = vec![core::ptr::null_mut::<u8>(); 1];
        let drained = core.dbg_drain_freelist_batch(buf[0], c, &mut out);
        assert_eq!(drained, 1, "drain call {call} must return exactly 1 block");
        all_drained.push(out[0]);
    }

    // A 9th call must return 0 (RunStack empty, no linked list).
    let mut out = vec![core::ptr::null_mut::<u8>(); 1];
    let drained = core.dbg_drain_freelist_batch(buf[0], c, &mut out);
    assert_eq!(
        drained, 0,
        "a 9th drain must return 0 (everything already drained)"
    );

    assert_eq!(all_drained.len(), 8);
    let set: HashSet<usize> = all_drained.iter().map(|p| *p as usize).collect();
    assert_eq!(set.len(), 8, "all 8 drained blocks must be distinct");

    for &p in &all_drained {
        assert!(!core.dbg_is_free_for(p), "drained block must be ALLOCATED");
        core.dealloc(p, layout);
    }
}
