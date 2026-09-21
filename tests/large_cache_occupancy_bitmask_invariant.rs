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
//! `large_cache_occupied` (`src/alloc_core/alloc_core/`) enumerates —
//! `large_cache_slot_set` (admission) and `large_cache_slot_take` (cache hit
//! AND eviction, both of which call it) — via public alloc/dealloc traffic
//! that drives deposit, cache-hit reuse, and FIFO eviction, so a lockstep
//! bug in either site would show up as a mismatch here.
//!
//! P1-3 (#1985): the file also pins SCAN-SELECTION EQUIVALENCE for the two
//! large-cache scans converted to iterate the occupancy bitmask
//! (`alloc_large`'s best-fit hit scan and `oldest_occupied_slot`'s
//! FIFO-oldest eviction scan): an in-test shadow model reimplementing the
//! OLD linear-scan semantics over the independently-read dbg arrays must
//! agree with the real cache after every operation, so a bitmask↔array
//! desync — or either scan selecting the wrong entry — fails loudly.

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

/// Byte-precise variant of [`layout`]: `layout` takes WHOLE MiB, but the
/// scan-equivalence tests below need request sizes with byte precision —
/// `k*4 MiB - 4096` makes `needed = 4096 (hdr) + size` land EXACTLY on
/// `k*4 MiB`, so the deposited `usable_size` is exactly `k*4 MiB` (see each
/// test's calibration step, which guards that premise per host).
fn layout_bytes(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
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

// ── test 5 (P1-3 #1985) — best-fit hit must reuse the array-predicted
//    segment ─────────────────────────────────────────────────────────────────
//
// Pins the CONVERTED best-fit hit scan (bitmask-driven) against the old
// linear-scan semantics recomputed in-test from the independently read slot
// arrays. Pointer identity proves WHICH segment was reused: if the bitmask
// ever disagreed with the arrays, the real scan could select a different
// cached entry than the array-derived prediction and the identity assert
// below would fire.

#[test]
fn best_fit_hit_returns_array_predicted_segment() {
    // CALIBRATION — throwaway core `ac0`. Every request size below is
    // `k*4 MiB - 4096`, which on a 4096-byte-page host deposits a segment of
    // EXACTLY `k*4 MiB` usable (`hdr offset = align_up(sizeof_hdr, PAGE) =
    // 4096`, so `needed = 4096 + size = k*4 MiB` exactly) — under
    // `exact-span-large` ON and OFF alike. Prove that premise before relying
    // on it: a host page size above 4 KiB breaks it, and the test must skip
    // rather than assert against a broken size model.
    let mut ac0 = AllocCore::new().expect("primordial");
    ac0.dbg_set_large_cache_budget(None);
    ac0.dbg_set_decay_config(0, u64::MAX, usize::MAX); // deterministically disable background decay

    let usable_req = |k: usize| k * 4 * MIB - 4096;

    let cal_layout = layout_bytes(usable_req(2)); // → 8 MiB usable segment
    let cal_ptr = ac0.alloc(cal_layout);
    if cal_ptr.is_null() {
        eprintln!("OOM allocating 8 MiB — skip test (machine too small)");
        return;
    }
    // SAFETY (R6-MS-1/2): cal_ptr was returned by the alloc directly above
    // with the same layout, live, freed exactly once here.
    unsafe { ac0.dealloc(cal_ptr, cal_layout) };
    assert_occupancy_bitmask_invariant(&ac0);
    let cal = ac0.dbg_large_cache_slot_sizes();
    if cal.iter().flatten().count() != 1 || cal[0] != Some(8 * MIB) {
        eprintln!("page-size assumption violated — skip");
        return;
    }

    // Real test core: FRESH (the calibration deposit stays behind on `ac0`),
    // so the seq counter starts at 0 and the cache is genuinely empty.
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    ac.dbg_set_decay_config(0, u64::MAX, usize::MAX); // deterministically disable background decay

    // Two fresh misses: the cache is empty, so both go to the OS with usable
    // spans of exactly 16 MiB and 24 MiB.
    let l16 = layout_bytes(usable_req(4)); // 16 MiB usable
    let l24 = layout_bytes(usable_req(6)); // 24 MiB usable
    let p16 = ac.alloc(l16);
    if p16.is_null() {
        eprintln!("OOM allocating 16 MiB — skip test (machine too small)");
        return;
    }
    let p24 = ac.alloc(l24);
    if p24.is_null() {
        eprintln!("OOM allocating 24 MiB — skip test (machine too small)");
        return;
    }

    // Dealloc in order: deposits land at slot 0 (16 MiB, seq 0) then slot 1
    // (24 MiB, seq 1) — the first two admissions of this fresh core.
    // SAFETY (R6-MS-1/2): p16/p24 were returned by the matching allocs
    // directly above with the same layouts, live, each freed exactly once.
    unsafe { ac.dealloc(p16, l16) };
    unsafe { ac.dealloc(p24, l24) };

    let base = ac.dbg_large_cache_slot_sizes();
    let mut expected: [Option<usize>; 8] = [None; 8];
    expected[0] = Some(16 * MIB);
    expected[1] = Some(24 * MIB);
    assert_eq!(
        base, expected,
        "deposits must land at combined slots 0 (16 MiB) then 1 (24 MiB)"
    );
    #[cfg(feature = "large-cache-extended")]
    assert!(
        ac.dbg_large_cache_extended_slot_sizes()
            .iter()
            .all(Option::is_none),
        "the extension must stay untouched — the base had room"
    );
    assert_occupancy_bitmask_invariant(&ac);

    // PREDICTION — old-linear-scan semantics recomputed HERE from the
    // independently-read arrays (never from the bitmask): a request whose
    // usable span is 12 MiB is compatible with every cached entry in
    // [12 MiB, 24 MiB] (`usable_size >= usable` AND
    // `usable_size <= usable * LARGE_CACHE_SIZE_FACTOR (= 2)`); best-fit
    // picks the SMALLEST compatible usable, ties toward the LOWEST index
    // (strict `<`). Whatever slot this loop predicts, the real scan must
    // take.
    let req_usable = 12 * MIB;
    let mut pred_idx: Option<usize> = None;
    let mut pred_usable = usize::MAX;
    for (i, slot) in base.iter().enumerate() {
        if let Some(sz) = *slot {
            if sz >= req_usable && sz <= req_usable.saturating_mul(2) && sz < pred_usable {
                pred_usable = sz;
                pred_idx = Some(i);
            }
        }
    }
    assert_eq!(
        pred_idx,
        Some(0),
        "array-derived best-fit must predict combined slot 0"
    );
    assert_eq!(
        pred_usable,
        16 * MIB,
        "array-derived best-fit must predict the 16 MiB entry"
    );

    // The hit itself: pointer identity proves WHICH segment was reused.
    let l12 = layout_bytes(usable_req(3)); // 12 MiB usable request
    let p_hit = ac.alloc(l12);
    assert!(!p_hit.is_null(), "12 MiB alloc must succeed");
    assert_eq!(
        p_hit, p16,
        "best-fit scan must reuse the 16 MiB segment the array-predicted scan selects"
    );
    // The hit consumed slot 0; only the 24 MiB slot 1 remains occupied.
    let mut expected_after_hit: [Option<usize>; 8] = [None; 8];
    expected_after_hit[1] = Some(24 * MIB);
    assert_eq!(
        ac.dbg_large_cache_slot_sizes(),
        expected_after_hit,
        "the hit must take exactly the 16 MiB slot 0"
    );
    #[cfg(feature = "large-cache-extended")]
    assert!(
        ac.dbg_large_cache_extended_slot_sizes()
            .iter()
            .all(Option::is_none),
        "the extension must stay untouched by the hit"
    );
    assert_occupancy_bitmask_invariant(&ac);

    // SAFETY (R6-MS-1/2): p_hit was returned by the alloc directly above with
    // the same layout, live, freed exactly once here.
    unsafe { ac.dealloc(p_hit, l12) };
    assert_occupancy_bitmask_invariant(&ac);
}

// ── test 6 (P1-3 #1985) — FIFO eviction victim must be the seq-0 first
//    deposit ─────────────────────────────────────────────────────────────────
//
// Pins the CONVERTED `oldest_occupied_slot` (bitmask-driven, with the seq==0
// short-circuit) against the old linear-scan semantics: the eviction victim
// selected during a budget-forced admission must be the FIFO-oldest entry —
// the FIRST deposit (seq 0) — never a newer one. A bitmask↔array desync
// would make the scan read a stale/wrong slot set and evict the wrong entry.

#[test]
fn fifo_oldest_eviction_victim_is_seq_zero_first_deposit() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    ac.dbg_set_decay_config(0, u64::MAX, usize::MAX); // deterministically disable background decay

    let usable_req = |k: usize| k * 4 * MIB - 4096;

    // Three distinct sizes, allocated and freed IN ORDER: the deposits land
    // at combined slots 0/1/2 holding {16, 32, 48} MiB with seqs 0/1/2 (the
    // first three admissions of this fresh core — only successful admissions
    // consume a seq from the monotonic counter).
    let plan: [(usize, usize); 3] = [(4, 16 * MIB), (8, 32 * MIB), (12, 48 * MIB)];
    let mut ptrs = Vec::new();
    for (k, usable) in plan {
        let l = layout_bytes(usable_req(k));
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!(
                "OOM allocating {} MiB — skip test (machine too small)",
                usable / MIB
            );
            return;
        }
        ptrs.push((p, l));
    }
    for (p, l) in &ptrs {
        // SAFETY (R6-MS-1/2): each pointer was returned by the matching
        // alloc above, live, freed exactly once here.
        unsafe { ac.dealloc(*p, *l) };
    }
    let mut expected_filled: [Option<usize>; 8] = [None; 8];
    expected_filled[0] = Some(16 * MIB);
    expected_filled[1] = Some(32 * MIB);
    expected_filled[2] = Some(48 * MIB);
    assert_eq!(
        ac.dbg_large_cache_slot_sizes(),
        expected_filled,
        "first three deposits must occupy slots 0/1/2 in FIFO order"
    );
    assert_occupancy_bitmask_invariant(&ac);

    // Clamp the budget to EXACTLY the current used total (96 MiB): the next
    // deposit cannot fit without an eviction.
    ac.dbg_set_large_cache_budget(Some(ac.dbg_large_cache_used()));

    // A 4 MiB-usable request cannot hit (window [4, 8] MiB vs cached
    // {16, 32, 48}); its dealloc then deposits 4 MiB under the clamped
    // budget, forcing exactly one eviction: the MIN-SEQ entry — the 16 MiB
    // FIRST deposit (seq 0), i.e. the seq==0 short-circuit path of the
    // converted `oldest_occupied_slot` — after which the 4 MiB span is
    // admitted into the freed slot 0.
    let l4 = layout_bytes(usable_req(1)); // 4 MiB usable
    let p4 = ac.alloc(l4);
    if p4.is_null() {
        eprintln!("OOM allocating 4 MiB — skip eviction leg of test");
        return;
    }
    // SAFETY (R6-MS-1/2): p4 was returned by the alloc directly above with
    // the same layout, live, freed exactly once here.
    unsafe { ac.dealloc(p4, l4) };

    // CORE ASSERT — the victim was the FIRST deposit (FIFO-oldest), not a
    // newer one: the slots must now hold exactly {4, 32, 48} MiB.
    let mut expected_after: [Option<usize>; 8] = [None; 8];
    expected_after[0] = Some(4 * MIB);
    expected_after[1] = Some(32 * MIB);
    expected_after[2] = Some(48 * MIB);
    assert_eq!(
        ac.dbg_large_cache_slot_sizes(),
        expected_after,
        "eviction victim must be the FIRST deposit (16 MiB @ slot 0, seq 0) — \
         a wrong victim here means the bitmask-driven oldest_occupied_slot \
         desynced from the arrays"
    );
    assert_occupancy_bitmask_invariant(&ac);
}

// ── test 7 (P1-3 #1985) — randomised bitmask↔array scan-equivalence shadow ─
//
// A deterministic-seeded LCG drives alloc/dealloc traffic while a SHADOW
// MODEL reimplements the OLD linear-scan semantics over a per-combined-slot
// state vector (size + seq). After EVERY operation the real cache state —
// independently read via the dbg arrays — must equal the shadow prediction,
// and `dbg_large_cache_occupied_bits()` bit i must equal combined slot i's
// occupancy (the same pattern as `assert_occupancy_bitmask_invariant`,
// extended with the full shadow comparison). Any desync — a bitmask bit
// drifting from its slot array, or either converted scan (best-fit hit,
// FIFO-oldest victim) selecting a different entry than the array-derived
// ground truth — desynchronises the shadow and fails with the step number,
// the op, and full shadow+real dumps. Skips (eprintln + return) only on OOM;
// the calibration-on-throwaway-core step guards the 4 KiB-page size premise
// exactly like `best_fit_hit_returns_array_predicted_segment`.

#[test]
fn randomised_bitmask_scan_equivalence_shadow() {
    /// Sum of the shadow's occupied sizes (= the real `large_cache_used`).
    fn shadow_used(slots: &[Option<(usize, u64)>]) -> usize {
        slots.iter().flatten().map(|&(sz, _)| sz).sum()
    }

    /// Per-combined-slot shadow↔real equality plus the bitmask⟺array
    /// invariant, with the step number, op, and full shadow+real dumps in
    /// every message.
    fn assert_real_matches_shadow(
        ac: &AllocCore,
        slots: &[Option<(usize, u64)>],
        step: usize,
        op: &str,
    ) {
        let bits = ac.dbg_large_cache_occupied_bits();
        let base = ac.dbg_large_cache_slot_sizes();
        // `mut` is only exercised with `large-cache-extended` (the extension
        // append below).
        #[cfg_attr(not(feature = "large-cache-extended"), allow(unused_mut))]
        let mut real: Vec<Option<usize>> = base.to_vec();
        #[cfg(feature = "large-cache-extended")]
        real.extend(ac.dbg_large_cache_extended_slot_sizes().iter().copied());
        assert_eq!(
            real.len(),
            slots.len(),
            "step {step} ({op}): combined slot count changed under the test"
        );
        for (i, shadow_slot) in slots.iter().enumerate() {
            assert_eq!(
                real[i],
                shadow_slot.map(|(sz, _)| sz),
                "step {step} ({op}): combined slot {i} desync — full shadow={slots:?}, \
                 real={real:?}, bits={bits:#x}"
            );
        }
        for (i, slot) in real.iter().enumerate() {
            assert_eq!(
                (bits >> i) & 1 == 1,
                slot.is_some(),
                "step {step} ({op}): bit {i} ⟺ occupancy violated — bits={bits:#x}, \
                 real={real:?}"
            );
        }
    }

    /// ONE dealloc path for every dealloc (random victim or forced
    /// oldest-live overflow victim): simulate the admission loop with the
    /// old linear-scan semantics, then really dealloc, then assert the real
    /// cache equals the shadow prediction.
    fn do_dealloc(
        ac: &mut AllocCore,
        slots: &mut [Option<(usize, u64)>],
        live: &mut Vec<(*mut u8, usize, usize)>, // (ptr, request size, segment usable)
        next_seq: &mut u64,
        victim_i: usize,
        step: usize,
    ) {
        const BUDGET: usize = 96 * MIB;
        let (ptr, req_size, seg_usable) = live.remove(victim_i);
        if seg_usable <= BUDGET {
            // Old admission-loop semantics: lowest free slot + budget check;
            // evict the argmin-seq entry and retry until both hold.
            loop {
                let free = slots.iter().position(Option::is_none);
                let budget_ok = shadow_used(slots) + seg_usable <= BUDGET;
                if let Some(i) = free {
                    if budget_ok {
                        slots[i] = Some((seg_usable, *next_seq));
                        *next_seq += 1;
                        break;
                    }
                }
                let (evict_idx, _) = slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.map(|(_, seq)| (i, seq)))
                    .min_by_key(|&(_, seq)| seq)
                    .expect("shadow eviction with a non-empty cache (budget fits the deposit)");
                slots[evict_idx] = None;
            }
        }
        // Otherwise the real path skips the deposit ENTIRELY (segment
        // released, no state change, no seq consumed) — the shadow matches
        // by doing nothing.
        //
        // SAFETY (R6-MS-1/2): ptr was returned by `ac.alloc` with exactly
        // `layout_bytes(req_size)` (recorded at alloc time), stayed live in
        // `live` until the `remove` above, and is freed exactly once here.
        unsafe { ac.dealloc(ptr, layout_bytes(req_size)) };
        assert_real_matches_shadow(ac, slots, step, "dealloc");
    }

    // CALIBRATION — throwaway core, same premise as
    // `best_fit_hit_returns_array_predicted_segment` (skip-out if violated).
    let mut ac0 = AllocCore::new().expect("primordial");
    ac0.dbg_set_large_cache_budget(None);
    ac0.dbg_set_decay_config(0, u64::MAX, usize::MAX); // deterministically disable background decay

    let usable_req = |k: usize| k * 4 * MIB - 4096;

    let cal_layout = layout_bytes(usable_req(2)); // → 8 MiB usable segment
    let cal_ptr = ac0.alloc(cal_layout);
    if cal_ptr.is_null() {
        eprintln!("OOM allocating 8 MiB — skip test (machine too small)");
        return;
    }
    // SAFETY (R6-MS-1/2): cal_ptr was returned by the alloc directly above
    // with the same layout, live, freed exactly once here.
    unsafe { ac0.dealloc(cal_ptr, cal_layout) };
    assert_occupancy_bitmask_invariant(&ac0);
    let cal = ac0.dbg_large_cache_slot_sizes();
    if cal.iter().flatten().count() != 1 || cal[0] != Some(8 * MIB) {
        eprintln!("page-size assumption violated — skip");
        return;
    }

    // Real core under test.
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    ac.dbg_set_decay_config(0, u64::MAX, usize::MAX); // deterministically disable background decay
    ac.dbg_set_large_cache_budget(Some(96 * MIB));

    let total_slots: usize = if cfg!(feature = "large-cache-extended") {
        40
    } else {
        8
    };
    let mut slots: Vec<Option<(usize, u64)>> = vec![None; total_slots]; // (usable, seq)
    let mut next_seq: u64 = 0;
    let mut lcg: u64 = 0x243F_6A88_85A3_08D3;
    let mut live: Vec<(*mut u8, usize, usize)> = Vec::new(); // (ptr, request size, segment usable)

    for step in 0..400 {
        lcg = lcg
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let op = (lcg >> 33) % 2; // 0 = alloc, 1 = dealloc
        let k = 1 + (lcg >> 13) % 5; // → fresh-usable span 4..20 MiB, 4 MiB steps

        // Pre-op reconcile: shadow and real must ALREADY agree (the previous
        // step's post-op assertion proved it; this re-proves it).
        assert_real_matches_shadow(&ac, &slots, step, "reconcile");

        if op == 0 || live.is_empty() {
            // ── alloc ─────────────────────────────────────────────────────
            // `v` = the usable span a FRESH segment for this request gets
            // (needed = 4096 + (v - 4096) = v exactly, per the calibration).
            let v = k as usize * 4 * MIB;
            // Old best-fit semantics: ascending index, compatibility window
            // [v, v * LARGE_CACHE_SIZE_FACTOR (= 2v)], STRICT `<` on the
            // best size (ties → lowest index).
            let mut hit_idx: Option<usize> = None;
            let mut best_usable = usize::MAX;
            for (i, slot) in slots.iter().enumerate() {
                if let Some((sz, _)) = *slot {
                    if sz >= v && sz <= v.saturating_mul(2) && sz < best_usable {
                        best_usable = sz;
                        hit_idx = Some(i);
                    }
                }
            }
            let segment_usable = if let Some(i) = hit_idx {
                let (sz, _) = slots[i].expect("hit slot must still be occupied");
                slots[i] = None;
                sz
            } else {
                v
            };
            let req = v - 4096;
            let p = ac.alloc(layout_bytes(req));
            if p.is_null() {
                eprintln!("OOM allocating {req} bytes — skip test (machine too small)");
                return;
            }
            live.push((p, req, segment_usable));
            assert_real_matches_shadow(&ac, &slots, step, "alloc");
        } else {
            // ── dealloc (random live victim) ──────────────────────────────
            let victim_i = ((lcg >> 7) as usize) % live.len();
            do_dealloc(
                &mut ac,
                &mut slots,
                &mut live,
                &mut next_seq,
                victim_i,
                step,
            );
        }

        // Overflow valve: keep the live set bounded by force-freeing the
        // OLDEST live record (front of the vec — FIFO) through the SAME
        // single dealloc path.
        if live.len() > 12 {
            do_dealloc(&mut ac, &mut slots, &mut live, &mut next_seq, 0, step);
        }
    }
    // End: deliberately do NOT deallocate the remaining live ptrs — the core
    // is dropped whole, and `Drop` releases every registered segment and
    // every cached entry.
}
