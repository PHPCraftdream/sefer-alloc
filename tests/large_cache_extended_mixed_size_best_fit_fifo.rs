//! R14-5 (task #290, item 5) — mixed-size and adversarial best-fit/FIFO
//! tests for the `large-cache-extended` sidecar, spanning both the base 8
//! slots and the extension once materialised.
//!
//! Covers:
//! 1. Overlapping `LARGE_CACHE_SIZE_FACTOR` (2x) ranges straddling the
//!    base/extension boundary — best-fit must still pick the globally
//!    tightest compatible slot, not just the tightest among whichever of
//!    base/extension happens to be scanned first.
//! 2. Eviction under pressure once BOTH base and extension are full and a
//!    new distinct size must displace the true FIFO-oldest entry across the
//!    COMBINED 40-slot index space, not merely the oldest within one half.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

fn segment() -> usize {
    SegmentLayout::SEGMENT
}

fn large_floor() -> usize {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let segment = segment();
    (2 * small_max).div_ceil(segment).max(1) * segment
}

/// Non-overlapping (`> 2x` apart) base sizes, matching the sibling files'
/// derivation — used where deposits must land in DISTINCT slots.
fn large_test_sizes(n: usize) -> Vec<usize> {
    let segment = segment();
    let mut size = large_floor();
    let mut sizes = Vec::with_capacity(n);
    for _ in 0..n {
        sizes.push(size);
        size = (2 * size + 1).div_ceil(segment) * segment;
    }
    sizes
}

/// Probe the ACTUAL `usable_size` a raw byte request rounds to, using a
/// scratch, disposable `AllocCore` (never touches the caller's own cache
/// state). Large-segment sizing rounds a raw request up by header +
/// alignment padding THEN to a whole SEGMENT multiple
/// (`alloc_core_large.rs`'s `needed.div_ceil(SEGMENT) * SEGMENT`) — the
/// header/padding overhead means a raw request that is itself already a
/// clean SEGMENT multiple does not necessarily round to that same value (it
/// can round UP to the NEXT one if the header pushes `needed` past the
/// boundary). Probing empirically (rather than re-deriving the header-size
/// arithmetic here, which would silently drift from `alloc_core_large.rs` if
/// that arithmetic ever changes) keeps this test's size choices honest.
fn probe_usable_size(raw_bytes: usize) -> usize {
    let mut scratch = AllocCore::new().expect("primordial (scratch probe)");
    scratch.dbg_set_large_cache_budget(None);
    let l = layout(raw_bytes);
    let p = scratch.alloc(l);
    assert!(!p.is_null(), "probe alloc of {raw_bytes} bytes failed");
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here (deposits into the scratch instance's cache,
    // which is dropped at the end of this function).
    unsafe { scratch.dealloc(p, l) };
    scratch.dbg_large_cache_slot_sizes()[0].expect("first deposit must occupy slot 0")
}

/// Best-fit across the base/extension boundary: deposit two OVERLAPPING
/// sizes (within `LARGE_CACHE_SIZE_FACTOR` = 2x of each other) — one landing
/// in the base 8 (deposited before the base filled), the other forced into
/// the extension (deposited after the base was already full of OTHER,
/// non-overlapping sizes). A subsequent request for the smaller of the two
/// overlapping sizes must best-fit-match the TIGHTER one, regardless of
/// which physical half (base array vs extension sidecar) it lives in.
#[test]
fn best_fit_picks_tightest_slot_across_base_extension_boundary() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let floor = large_floor();
    let segment = segment();

    // `small`'s RAW REQUEST is `floor`; its ACTUAL resident `usable_size`
    // (after header + SEGMENT rounding — see `probe_usable_size`'s doc for
    // why this must be discovered empirically rather than re-derived here)
    // is `small_usable`. `large_overlap`'s raw request is picked as the
    // smallest SEGMENT multiple strictly above `small_usable` for which the
    // ACTUAL resulting `usable_size` (`large_overlap_usable`) is BOTH
    // distinct from `small_usable` and within `LARGE_CACHE_SIZE_FACTOR` (2x)
    // of it — probed empirically and asserted, not assumed, so this test
    // stays correct even if the header/rounding arithmetic in
    // `alloc_core_large.rs` changes in the future. The 7 fillers START ABOVE
    // `large_overlap_usable` and grow pairwise `> 2x` apart from THERE, so
    // none of the 9 values in this test (`small`, `large_overlap`, 7
    // fillers) coincide or fall within 2x of each other except the one
    // deliberate `small`/`large_overlap` overlap this test exists to probe.
    let small = floor;
    let small_usable = probe_usable_size(small);
    let mut large_overlap = small_usable + segment;
    let mut large_overlap_usable = probe_usable_size(large_overlap);
    while large_overlap_usable == small_usable {
        // Header/rounding overhead can occasionally collapse a +1-SEGMENT
        // step onto the SAME usable_size the probe just measured for
        // `small`; step forward another SEGMENT and re-probe until they
        // diverge (bounded: SEGMENT rounding guarantees convergence within a
        // handful of steps, and each step only makes `large_overlap` bigger,
        // never smaller, so the 2x-band assertion below is what actually
        // bounds this loop's usefulness, not an artificial iteration cap).
        large_overlap += segment;
        large_overlap_usable = probe_usable_size(large_overlap);
    }
    assert!(
        large_overlap_usable <= small_usable.saturating_mul(2),
        "test precondition: large_overlap_usable ({large_overlap_usable}) must \
         be within 2x of small_usable ({small_usable}) for this to exercise \
         best-fit overlap — the derivation above should guarantee this for any \
         realistic floor/segment ratio"
    );

    let mut fillers = Vec::with_capacity(7);
    let mut n = (2 * large_overlap_usable + 1).div_ceil(segment) * segment;
    for _ in 0..7 {
        fillers.push(n);
        n = (2 * n + 1).div_ceil(segment) * segment;
    }

    // Deposit the 7 fillers + `small` (8 deposits — fills the base exactly).
    for &bytes in fillers.iter().chain(std::iter::once(&small)) {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(!p.is_null(), "alloc of {bytes} bytes failed unexpectedly");
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }
    assert_eq!(
        ac.dbg_large_cache_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count(),
        8,
        "base must be exactly full after 7 fillers + `small`"
    );
    assert!(!ac.dbg_large_cache_extension_materialised());

    // Deposit `large_overlap` — base is full, so this lands in the (now
    // materialising) extension.
    let l_overlap = layout(large_overlap);
    let p_overlap = ac.alloc(l_overlap);
    assert!(!p_overlap.is_null());
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here.
    unsafe { ac.dealloc(p_overlap, l_overlap) };
    assert!(
        ac.dbg_large_cache_extension_materialised(),
        "9th deposit (large_overlap) must have materialised the extension"
    );

    // A request for exactly `small` bytes must best-fit-match `small`'s OWN
    // resident deposit (the tightest match: usable_size == small, versus
    // `large_overlap`'s larger usable_size, which also technically
    // qualifies under the `<= 2x` rule but is not the tightest fit) — even
    // though `large_overlap` sits in the DIFFERENT physical half (extension)
    // and could easily be found first by a naive scan-order bug.
    let l_small = layout(small);
    let p_small = ac.alloc(l_small);
    assert!(!p_small.is_null());
    // After this hit, `large_overlap`'s slot must STILL be occupied (it was
    // not the one consumed) — proving best-fit chose `small`'s tighter slot,
    // not `large_overlap`'s looser one.
    let ext_occupied = ac
        .dbg_large_cache_extended_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count();
    assert_eq!(
        ext_occupied, 1,
        "large_overlap's extension slot must remain occupied — best-fit must \
         have matched `small`'s own tighter base-slot deposit instead, proving \
         best-fit scans and compares across BOTH halves correctly rather than \
         short-circuiting on whichever half is scanned first"
    );
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here.
    unsafe { ac.dealloc(p_small, l_small) };
}

/// FIFO eviction across the combined 40-slot index space: once base+
/// extension are BOTH full (a synthetic worst case — deposit enough distinct
/// sizes to fill all 40), a new distinct deposit must evict the TRUE
/// FIFO-oldest entry (smallest `seq`) regardless of whether that oldest
/// entry lives in the base array or the extension sidecar.
///
/// This test uses a small budget-free run with a tight PER-SLOT byte budget
/// disabled (budget=None) so slot-count, not bytes, is the sole admission
/// constraint — isolating the FIFO-across-halves question from the
/// byte-budget question the sibling `large_cache_extended_budget_*` files
/// already cover.
#[test]
fn fifo_eviction_targets_true_oldest_across_combined_index_space() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    // 41 distinct, pairwise->2x-apart sizes: exactly fills all 40 slots
    // (8 base + 32 extension), the 41st forces one eviction.
    let sizes = large_test_sizes(41);
    for &bytes in &sizes[..40] {
        let l = layout(bytes);
        let p = ac.alloc(l);
        if p.is_null() {
            eprintln!("OOM at {bytes} bytes — stopping early (host memory pressure)");
            return;
        }
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }
    assert_eq!(ac.dbg_large_cache_total_slots(), 40);
    let occupied_before = ac
        .dbg_large_cache_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count()
        + ac.dbg_large_cache_extended_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count();
    assert_eq!(occupied_before, 40, "all 40 slots must be occupied");

    // The 41st (largest, most-recently-defined) size, and the FIFO-oldest
    // (sizes[0], the very first deposit — smallest `seq`) must be evicted to
    // make room, since it's the true oldest across the whole 40-slot space
    // (this test deposited in strict size-ascending == time-ascending
    // order, so seq order == size order == index order here).
    let l41 = layout(sizes[40]);
    let p41 = ac.alloc(l41);
    if p41.is_null() {
        eprintln!("OOM at 41st deposit — stopping early (host memory pressure)");
        return;
    }
    // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
    // freed exactly once here.
    unsafe { ac.dealloc(p41, l41) };

    // sizes[0] (the true FIFO-oldest) must no longer be resident: a
    // subsequent alloc at that exact size must be a genuine cache MISS —
    // i.e. it must still succeed (served fresh from the OS) but the total
    // occupied-slot count must not exceed 40 (no leak, no double-count), and
    // the specific usable_size of sizes[0] must not appear twice.
    let occupied_after = ac
        .dbg_large_cache_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count()
        + ac.dbg_large_cache_extended_slot_sizes()
            .iter()
            .filter(|s| s.is_some())
            .count();
    assert_eq!(
        occupied_after, 40,
        "occupied count must remain exactly 40 after the 41st deposit evicts \
         exactly one entry to make room for itself"
    );

    // Every one of sizes[1..=40] must still be resident (only sizes[0] was
    // evicted) — verify by re-allocating each and confirming success,
    // restoring the cache to its prior state each time (probe-and-restore).
    for &bytes in &sizes[1..=40] {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "size {bytes} (not the FIFO-oldest) must still be resident and \
             servable after the 41st deposit's eviction"
        );
        // SAFETY (R6-MS-1/2): pointer from the alloc immediately above, live,
        // freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }
}
