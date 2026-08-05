//! R32-12 (task #503, F8 sub-change (2)) — falsification-first invariant test
//! for `AllocCore::large_cache_occupied`, the occupancy bitmask that replaces
//! the free-slot-search linear scan (`large_cache_find_free_slot`) with a
//! `trailing_ones()` lookup.
//!
//! The invariant under test: bit `i` of `large_cache_occupied` is set
//! **if and only if** combined slot `i` (base `large_cache[i]` for
//! `i < LARGE_CACHE_SLOTS`, extension `slots[i - LARGE_CACHE_SLOTS]`
//! otherwise) currently holds `Some(CachedLarge)`. This is checked directly
//! against the actual per-slot occupancy (`dbg_large_cache_slot_sizes` /
//! `dbg_large_cache_extended_slot_sizes`), NOT re-derived from the bitmask
//! itself — mirroring `large_cache_budget.rs`'s pre-existing
//! `assert_used_bytes_invariant` pattern (compare the maintained counter
//! against an independently-computed ground truth) applied to the new
//! bitmask instead of `large_cache_used_bytes`.
//!
//! Exercises the SAME two maintenance sites the doc comment on
//! `large_cache_occupied` (`src/alloc_core/alloc_core.rs`) enumerates —
//! `large_cache_slot_set` (admission) and `large_cache_slot_take` (cache hit
//! AND eviction, both of which call it) — via public alloc/dealloc traffic
//! that drives deposit, cache-hit reuse, and FIFO eviction, so a lockstep
//! bug in either site would show up as a mismatch here.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;

fn layout(mib: usize) -> Layout {
    Layout::from_size_align(mib * MIB, 8).unwrap()
}

/// Ground truth: for each combined slot, is it occupied? Compares against
/// `dbg_large_cache_occupied_bits()` bit-for-bit.
fn assert_occupancy_bitmask_invariant(ac: &AllocCore) {
    let bits = ac.dbg_large_cache_occupied_bits();
    let base = ac.dbg_large_cache_slot_sizes();

    for (i, slot) in base.iter().enumerate() {
        let bit_set = (bits >> i) & 1 == 1;
        let occupied = slot.is_some();
        assert_eq!(
            bit_set, occupied,
            "base slot {i}: bit_set={bit_set} but occupied={occupied} (bits={bits:#x})"
        );
    }

    #[cfg(feature = "large-cache-extended")]
    {
        let ext = ac.dbg_large_cache_extended_slot_sizes();
        let base_len = base.len();
        for (j, slot) in ext.iter().enumerate() {
            let i = base_len + j;
            let bit_set = (bits >> i) & 1 == 1;
            let occupied = slot.is_some();
            assert_eq!(
                bit_set, occupied,
                "extension slot {i}: bit_set={bit_set} but occupied={occupied} (bits={bits:#x})"
            );
        }
    }
}

// ── test 1 — empty cache starts with an all-clear bitmask ──────────────────

#[test]
fn fresh_cache_bitmask_is_zero() {
    let ac = AllocCore::new().expect("primordial");
    assert_eq!(
        ac.dbg_large_cache_occupied_bits(),
        0,
        "a freshly constructed AllocCore's large cache must be entirely unoccupied"
    );
    assert_occupancy_bitmask_invariant(&ac);
}

// ── test 2 — single deposit sets exactly one bit, hit clears it ────────────

#[test]
fn single_deposit_and_hit_bitmask() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    assert_occupancy_bitmask_invariant(&ac);

    let l = layout(4);
    let ptr1 = ac.alloc(l);
    if ptr1.is_null() {
        eprintln!("OOM allocating 4 MiB — skip test (machine too small)");
        return;
    }
    assert_occupancy_bitmask_invariant(&ac);

    // SAFETY (R6-MS-1/2): ptr1 was returned by the alloc directly above with
    // the same layout, live, freed exactly once here.
    unsafe { ac.dealloc(ptr1, l) };

    // Deposit landed: exactly one bit set.
    assert_eq!(
        ac.dbg_large_cache_occupied_bits().count_ones(),
        1,
        "exactly one slot should be occupied after a single deposit"
    );
    assert_occupancy_bitmask_invariant(&ac);

    // Cache hit on re-alloc: the bit must clear again.
    let ptr2 = ac.alloc(l);
    assert!(!ptr2.is_null(), "re-alloc after cache deposit must succeed");
    assert_eq!(
        ac.dbg_large_cache_occupied_bits(),
        0,
        "the bit must clear once the cached slot is taken by a cache hit"
    );
    assert_occupancy_bitmask_invariant(&ac);

    // SAFETY (R6-MS-1/2): ptr2 was returned by the alloc directly above with
    // the same layout, live, freed exactly once here.
    unsafe { ac.dealloc(ptr2, l) };
    assert_occupancy_bitmask_invariant(&ac);
}

// ── test 3 — filling every base slot with distinct sizes sets all 8 bits ───

#[test]
fn base_slots_fill_sets_all_bits() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    // 8 distinct sizes so best-fit/size-factor matching never collapses two
    // deposits onto the same slot via a cache hit mid-loop; LARGE_CACHE_SIZE_FACTOR
    // is 2, so spacing sizes far apart (4, 8, 16, ... MiB) keeps every
    // allocation a genuine miss against every OTHER already-cached entry.
    let mut ptrs = Vec::new();
    for i in 0..8u32 {
        let mib = 4usize << i; // 4, 8, 16, 32, 64, 128, 256, 512 MiB
        let l = layout(mib);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {mib} MiB — skip test (machine too small)");
            return;
        }
        ptrs.push((p, l));
    }
    assert_occupancy_bitmask_invariant(&ac);
    assert_eq!(
        ac.dbg_large_cache_occupied_bits(),
        0,
        "no deposits yet (only allocs so far)"
    );

    for (p, l) in ptrs {
        // SAFETY (R6-MS-1/2): each pointer was returned by the matching
        // alloc above, live, freed exactly once here.
        unsafe { ac.dealloc(p, l) };
        assert_occupancy_bitmask_invariant(&ac);
    }

    // All 8 base slots occupied: low 8 bits set, nothing above (extension
    // never needed — the base has exactly 8 slots for these 8 deposits).
    assert_eq!(
        ac.dbg_large_cache_occupied_bits(),
        0xFF,
        "all 8 base slots should be occupied after 8 distinct-size deposits"
    );
    assert_occupancy_bitmask_invariant(&ac);
}

// ── test 4 — eviction clears the victim's bit ───────────────────────────────

#[test]
fn eviction_clears_bitmask_bit() {
    let mut ac = AllocCore::new().expect("primordial");
    // Unbounded budget for the FILL loop — with `large-cache-extended` on,
    // the resolved DEFAULT budget is finite
    // (`DEFAULT_EXTENDED_BUDGET_BYTES`, `large_cache_config.rs`), which
    // would trigger budget-driven eviction DURING the fill loop below and
    // leave fewer than 8 base slots occupied.
    ac.dbg_set_large_cache_budget(None);

    // Fill the base 8 slots with distinct sizes.
    let mut ptrs = Vec::new();
    for i in 0..8u32 {
        let mib = 4usize << i;
        let l = layout(mib);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {mib} MiB — skip test (machine too small)");
            return;
        }
        ptrs.push((p, l));
    }
    for (p, l) in &ptrs {
        // SAFETY (R6-MS-1/2): each pointer was returned by the matching
        // alloc above, live, freed exactly once here.
        unsafe { ac.dealloc(*p, *l) };
    }
    assert_eq!(ac.dbg_large_cache_occupied_bits(), 0xFF);
    assert_occupancy_bitmask_invariant(&ac);
    let used_after_fill = ac.dbg_large_cache_used();

    // Now clamp the budget to EXACTLY the current used total. This forces
    // `evict_one_oldest` to fire on the next deposit REGARDLESS of whether
    // `large-cache-extended` is on: `large_cache_find_free_slot` may still
    // find a free slot (via the extension, under that feature), but
    // `budget_ok` is false until enough is evicted to make room, so the
    // admission loop's `if !self.evict_one_oldest() { break; }` branch runs
    // at least once before admission — the property this test wants to
    // observe (a bitmask bit clearing due to eviction) independent of
    // whether the extension happens to be available as an alternative to
    // eviction.
    ac.dbg_set_large_cache_budget(Some(used_after_fill));

    // The NEXT dealloc of a distinct, non-matching size must evict the
    // FIFO-oldest (the first deposit, layout(4)) to admit the new one.
    let l9 = layout(1024); // 1024 MiB, far outside every existing slot's
                           // [usable_size, usable_size*2] compatibility
                           // window, so this cannot be satisfied by a hit.
    let p9 = ac.alloc(l9);
    if p9.is_null() {
        eprintln!("OOM allocating 1024 MiB — skip eviction leg of test");
        return;
    }
    // SAFETY (R6-MS-1/2): p9 was returned by the alloc directly above with
    // the same layout, live, freed exactly once here.
    unsafe { ac.dealloc(p9, l9) };

    // At least one eviction must have fired to respect the clamped budget:
    // the occupied count must NOT have simply grown to 9 (that would mean
    // eviction was skipped in favor of the extension, defeating the point
    // of this test), and the bitmask/actual-occupancy invariant must still
    // hold bit-for-bit either way.
    assert!(
        ac.dbg_large_cache_occupied_bits().count_ones() <= 8,
        "budget-clamped admission must evict, not grow past 8 occupied slots"
    );
    assert_occupancy_bitmask_invariant(&ac);
}
