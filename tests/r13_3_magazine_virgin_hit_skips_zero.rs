//! R13-3 (task #273, Defect 1) regression: a **magazine HIT** through
//! [`HeapCore::alloc_zeroed`] must still be able to skip the explicit
//! `Node::zero` pass when the popped block is genuinely virgin — proving the
//! virgin signal now survives the magazine (`PerClass::virgin_mask`) instead
//! of being lost, as it was under the R12-10 magazine-BYPASS design (which
//! this task fixes).
//!
//! **Class choice.** Uses a mid-range class index picked dynamically (see
//! `pick_target_class`, mirroring `tests/r11_2_overflow_drain_pool_release.rs`'s
//! "unused-by-anything-else" rationale) that nothing else in this process's
//! `HeapRegistry`-shared test binary touches — avoiding free-list pollution
//! from OTHER tests' traffic on a recycled heap slot. This matters
//! specifically for THIS scenario: the allocator's free-drain-first policy
//! (`refill_class_bump_impl`'s NON-NEGOTIABLE source order — existing free
//! blocks are always preferred over a fresh bump-carve within the same
//! refill call) means a low/shared class like "16-byte align-8" can have
//! LEFTOVER free blocks from unrelated earlier tests, and a miss-triggering
//! refill would drain those (correctly non-virgin) BEFORE reaching a
//! bump-carve — silently turning MOST of the retained magazine entries
//! non-virgin, not a uniform "all virgin" set. A dedicated
//! never-before-touched class sidesteps this: nothing has ever freed a block
//! of that class, so its free list is provably empty and a miss-refill is a
//! pure `carve_batch` run — genuinely all-virgin, as the design docs' §4.4
//! "no intra-run transition" argument requires. The two tests in this file
//! draw from DISJOINT candidate pools (`VIRGIN_TEST_CLASS_CANDIDATES` /
//! `REUSE_TEST_CLASS_CANDIDATES`) so they cannot pollute each other's class
//! regardless of `HeapRegistry` slot reuse or execution order.
//!
//! **Verification, not assumption.** Rather than assuming which resident
//! slots ended up virgin, this test reads back
//! [`HeapCore::dbg_tcache_virgin_mask`] directly (a `#[doc(hidden)]`
//! test-only hook added alongside this fix) and asserts against the ACTUAL
//! mask — the honest way to pin this invariant down given the free-drain
//! interaction above.
//!
//! **The complementary counterfactual (a magazine HIT of a NON-virgin
//! block)**: a block that was allocated, freed (pushed back into the
//! magazine), and then re-popped via `alloc_zeroed` MUST still be zeroed —
//! proving the mask is not just "always true" by construction, but tracks
//! real state through a push/pop cycle.
//!
//! **Feature gate.** `alloc-global`, `fastbin`, `virgin-zero-skip`,
//! `alloc-stats` (for the `SMALL_ZERO_PASS_CALLS` delta oracle — without it
//! the byte-content check alone still catches a correctness regression, just
//! not a "did the skip actually fire" regression).

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "virgin-zero-skip"
))]

use std::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// Serialise every test in this file against the process-wide
/// `SMALL_ZERO_PASS_CALLS` counter.
static TEST_LOCK: Mutex<()> = Mutex::new(());
fn serial() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

fn assert_all_zero(ptr: *mut u8, len: usize, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == 0),
        "{ctx}: memory is not all-zero (first non-zero byte at offset {:?})",
        bytes.iter().position(|&b| b != 0),
    );
}

/// A class touched by NOTHING else in this test binary (see module doc for
/// why this matters), AND small enough that `refill_n_for_class > 1` (a
/// large class refills only 1 block per miss — no retained blocks for a
/// later hit to pop). Mirrors `tests/r11_2_overflow_drain_pool_release.rs`'s
/// `TARGET_CLASS = 40` / `TRIGGER_CLASS = 41` "unused-by-anything-else"
/// convention, but scans for a class satisfying BOTH constraints instead of
/// hardcoding an index whose `refill_n` depends on the size-class table
/// (which differs under `medium-classes`).
///
/// DISJOINT candidate pools per test: `HeapRegistry` recycles/reuses heap
/// slots across tests WITHIN this same process (both tests in this file can
/// land on the SAME underlying `AllocCore`/segments), so if both tests
/// picked from the SAME candidate list, whichever ran second could inherit
/// the other's dirtied free list for that class -- exactly the
/// free-drain-first pollution this file's `pick_target_class` exists to
/// avoid in the first place. Two disjoint pools make the two tests mutually
/// non-interfering regardless of execution order.
const VIRGIN_TEST_CLASS_CANDIDATES: [usize; 3] = [20, 21, 22];
const REUSE_TEST_CLASS_CANDIDATES: [usize; 3] = [23, 24, 25];

fn pick_target_class(heap: *mut sefer_alloc::registry::HeapCore, candidates: &[usize]) -> usize {
    for &c in candidates {
        let refill_n = unsafe { (*heap).dbg_refill_n_for_class(c) };
        if refill_n >= 2 {
            return c;
        }
    }
    panic!(
        "none of {candidates:?} have refill_n >= 2 under this feature/size-class \
         configuration -- widen the candidate list"
    );
}

/// A magazine HIT (block already resident from an earlier refill) must still
/// skip the zero pass when the resident block is genuinely virgin.
#[test]
fn magazine_hit_of_virgin_block_skips_zero_pass() {
    let _guard = serial();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let target_class = pick_target_class(heap, &VIRGIN_TEST_CLASS_CANDIDATES);

    let bs = AllocCore::dbg_block_size(target_class);
    let layout = Layout::from_size_align(bs, 8).unwrap();
    let refill_n = unsafe { (*heap).dbg_refill_n_for_class(target_class) };
    assert!(
        refill_n >= 2,
        "this test requires refill_n >= 2 so a miss parks >=1 retained \
         virgin block for a later hit to pop; got refill_n={refill_n}"
    );

    unsafe { (*heap).dbg_flush_all() };
    assert_eq!(
        unsafe { (*heap).dbg_tcache_count(target_class) },
        0,
        "magazine must be empty for target_class after a forced flush \
         (target_class is untouched by any other test in this binary)"
    );

    // First call: a magazine MISS -> refill_magazine_slow_virgin ->
    // target_class's free list is provably empty (nothing has EVER freed a
    // block of this class in this process) -> the refill is a pure
    // carve_batch run -> genuinely all-virgin.
    let zero_before_1 = AllocCore::dbg_small_zero_pass_count();
    let p1 = unsafe { (*heap).alloc_zeroed(layout) };
    assert!(!p1.is_null(), "first alloc_zeroed returned null");
    assert_all_zero(p1, bs, "first (miss/virgin-carve) alloc_zeroed");
    let delta_1 = AllocCore::dbg_small_zero_pass_count() - zero_before_1;
    #[cfg(not(miri))]
    assert_eq!(
        delta_1, 0,
        "first alloc_zeroed (virgin carve via magazine miss) must skip the \
         explicit zero pass on a real OS backend"
    );

    // The magazine must now hold refill_n - 1 resident blocks, and (the
    // load-bearing verification, not an assumption) EVERY one of those bits
    // must read virgin: target_class's free list was empty, so the refill
    // was a pure bump-carve run sharing ONE segment's payload_virgin bit.
    let resident = unsafe { (*heap).dbg_tcache_count(target_class) };
    assert_eq!(
        resident,
        (refill_n - 1) as u16,
        "magazine must retain refill_n - 1 blocks after the miss-triggering refill"
    );
    let mask = unsafe { (*heap).dbg_tcache_virgin_mask(target_class) };
    let expect_all_virgin_mask: u16 = if resident >= 16 {
        u16::MAX
    } else {
        (1u16 << resident) - 1
    };
    #[cfg(not(miri))]
    assert_eq!(
        mask, expect_all_virgin_mask,
        "every retained resident block must be marked virgin (target_class's \
         free list was empty, so the refill was a pure carve_batch run): \
         got mask={mask:016b}, expected={expect_all_virgin_mask:016b}"
    );

    // Second call: a genuine magazine HIT (count > 0 going in) of a block
    // from the SAME carve_batch run as p1 — equally virgin per the mask
    // check above. THE core R13-3 assertion: the hit must ALSO skip the
    // zero pass.
    let zero_before_2 = AllocCore::dbg_small_zero_pass_count();
    let p2 = unsafe { (*heap).alloc_zeroed(layout) };
    assert!(!p2.is_null(), "second (hit) alloc_zeroed returned null");
    assert_ne!(p1, p2, "must be a DIFFERENT block from the first call");
    assert_all_zero(p2, bs, "second (magazine-HIT/virgin) alloc_zeroed");
    let delta_2 = AllocCore::dbg_small_zero_pass_count() - zero_before_2;
    #[cfg(not(miri))]
    assert_eq!(
        delta_2, 0,
        "a magazine HIT of a genuinely virgin resident block must ALSO skip \
         the explicit zero pass — this is the R13-3 Defect 1 fix: the virgin \
         signal must survive a magazine pop, not just a magazine bypass"
    );
    #[cfg(miri)]
    {
        assert_eq!(delta_1, 1, "under miri the virgin skip must never fire");
        assert_eq!(delta_2, 1, "under miri the virgin skip must never fire");
    }

    unsafe {
        (*heap).dealloc(p1, layout);
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

/// Complementary counterfactual: a magazine HIT of a block that was
/// allocated, freed (pushed back into the magazine), and re-popped MUST
/// still run the explicit zero pass — the mask correctly reports
/// non-virginity for a reused block, not merely "always skip" by construction.
#[test]
fn magazine_hit_of_reused_block_still_zeroes() {
    let _guard = serial();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let target_class = pick_target_class(heap, &REUSE_TEST_CLASS_CANDIDATES);

    let bs = AllocCore::dbg_block_size(target_class);
    let layout = Layout::from_size_align(bs, 8).unwrap();

    // Warm the magazine and get a live block via plain `alloc` (not
    // alloc_zeroed), then dirty it and free it -- this push puts a
    // NON-virgin block back into the magazine (dispatch conjunct: it was
    // already issued once).
    let p0 = unsafe { (*heap).alloc(layout) };
    assert!(!p0.is_null());
    unsafe { core::ptr::write_bytes(p0, 0xAA, bs) };
    unsafe { (*heap).dealloc(p0, layout) };

    assert!(
        unsafe { (*heap).dbg_tcache_count(target_class) } > 0,
        "magazine must hold the just-freed block"
    );
    // Verify the pushed-back slot's virgin bit reads false — proving the
    // push-clear logic (not just the pop-read logic) is exercised.
    let cnt = unsafe { (*heap).dbg_tcache_count(target_class) } as usize;
    let mask = unsafe { (*heap).dbg_tcache_virgin_mask(target_class) };
    assert_eq!(
        mask & (1u16 << (cnt - 1)),
        0,
        "the just-pushed-back (reused) block's slot must NOT be marked virgin"
    );

    // Re-popping it via alloc_zeroed must be a magazine HIT (LIFO -- this is
    // the most-recently-pushed block) that STILL zeroes (never virgin).
    let zero_before = AllocCore::dbg_small_zero_pass_count();
    let p1 = unsafe { (*heap).alloc_zeroed(layout) };
    assert!(!p1.is_null());
    assert_eq!(
        p0, p1,
        "expected the magazine LIFO pop to return the SAME just-freed address"
    );
    assert_all_zero(p1, bs, "reused magazine-HIT alloc_zeroed");
    #[cfg(feature = "alloc-stats")]
    assert_eq!(
        AllocCore::dbg_small_zero_pass_count() - zero_before,
        1,
        "a magazine HIT of a REUSED (previously-freed) block must run \
         exactly one explicit zero pass -- it is never virgin regardless of \
         the mask's state for any earlier occupant of that physical slot"
    );
    #[cfg(not(feature = "alloc-stats"))]
    let _ = zero_before;

    unsafe {
        (*heap).dealloc(p1, layout);
        HeapRegistry::recycle(heap);
    }
}

// Guard against accidental future reuse of the class-candidate pools above
// by another test file in this binary breaking the "untouched by anything
// else" precondition: documented here, not enforced at compile time
// (cross-file coordination is by convention, matching the R11-2 sibling
// tests' identical TARGET_CLASS/TRIGGER_CLASS discipline).
