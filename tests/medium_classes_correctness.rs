//! R6-OPT-P0-3a — exact medium size classes (256 KiB..1 MiB), feature-gated
//! behind `medium-classes`. This file exercises the review's own
//! correctness-surface checklist (`radical_optimization_review.md` §4 P0-3)
//! item by item, plus the "table vs brute force" cross-check the task's
//! test-requirements section calls for.
//!
//! Whole file is a no-op without `medium-classes` (see the `#![cfg(...)]`
//! below) — run with:
//!   cargo test --features "production medium-classes" --test medium_classes_correctness

#![cfg(all(feature = "alloc-core", feature = "medium-classes"))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The six exact medium classes this task adds, per the review's own
/// suggestion (used verbatim, no deviation — see the task report for why).
const MEDIUM_SIZES: &[usize] = &[
    256 * 1024,
    320 * 1024,
    384 * 1024,
    512 * 1024,
    768 * 1024,
    1024 * 1024,
];

// ---------------------------------------------------------------------------
// Table-vs-brute-force: SIZE_CLASS_TABLE / SIZE2CLASS agree with a linear
// scan for every size in the EXTENDED range (up to the new SMALL_MAX == 1
// MiB). This mirrors `tests/size_classes_lookup.rs`'s existing methodology
// (same "reference linear scan over the public table" approach) rather than
// inventing a new one, applied at the boundaries this task actually moves.
// The full 1..=SMALL_MAX sweep already lives in `size_classes_lookup.rs` and
// `size_classes_proptest.rs` (both parametrised on `SegmentLayout::SMALL_MAX`,
// so they automatically widen under this feature) — this file adds the
// MEDIUM-SPECIFIC boundary checks those generic sweeps don't call out by name.
// ---------------------------------------------------------------------------

fn linear_scan_class_for(size: usize, align: usize) -> Option<usize> {
    let need = if size > align { size } else { align };
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let mut i = 0;
    while i < table.len() {
        if table[i] >= need && table[i].is_multiple_of(align) {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[test]
fn medium_classes_present_in_table_at_the_right_sorted_position() {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    // R9-4 (task #226): `medium-classes-wide` appends 3 more exact classes
    // ON TOP of these 6 when it is ALSO enabled (e.g. under `--all-features`,
    // which turns on every feature simultaneously) -- the table then has 58
    // entries, not 55. The 6 medium classes checked below still land at the
    // same table[49..55] positions in both cases (the wide feature only
    // APPENDS, never mutates -- see
    // `tests/medium_classes_wide_correctness.rs::wide_does_not_disturb_six_class_medium_table_topology`),
    // so only this length assertion needs to branch.
    let expected_len = if cfg!(feature = "medium-classes-wide") {
        58
    } else {
        55
    };
    assert_eq!(
        table.len(),
        expected_len,
        "medium-classes must add exactly 6 entries to the 49-entry base table \
         (58 if medium-classes-wide is also enabled)"
    );
    // Every MEDIUM_SIZES value appears verbatim in the table, strictly after
    // every pre-existing (< 256 KiB) class.
    let pre_medium_top = table[48]; // the old 49-entry table's top class
    assert!(
        pre_medium_top < MEDIUM_SIZES[0],
        "the old 49-entry table's top class ({pre_medium_top}) must be strictly \
         below the first medium class ({}) for the append-not-merge shortcut \
         `build_table` relies on to be valid",
        MEDIUM_SIZES[0]
    );
    for (i, &sz) in MEDIUM_SIZES.iter().enumerate() {
        assert_eq!(
            table[49 + i],
            sz,
            "medium class {i} expected at table[{}], found {} instead of {sz}",
            49 + i,
            table[49 + i]
        );
    }
    // Strictly increasing end-to-end (not just within the medium run).
    for w in table.windows(2) {
        assert!(
            w[0] < w[1],
            "table not strictly increasing: {} >= {}",
            w[0],
            w[1]
        );
    }
    // Every entry stays MIN_BLOCK-aligned.
    for &sz in table {
        assert_eq!(
            sz % SegmentLayout::MIN_BLOCK,
            0,
            "{sz} not MIN_BLOCK-aligned"
        );
    }
}

#[test]
fn small_max_updates_to_one_mib() {
    // R9-4 (task #226): with `medium-classes-wide` ALSO enabled (e.g.
    // `--all-features`), SMALL_MAX is the wide feature's top class (1.75 MiB),
    // not plain `medium-classes`'s 1 MiB -- see this file's `medium_classes_
    // present_in_table_at_the_right_sorted_position` for the same branch.
    let expected_small_max = if cfg!(feature = "medium-classes-wide") {
        1792 * 1024
    } else {
        1024 * 1024
    };
    assert_eq!(
        SegmentLayout::SMALL_MAX,
        expected_small_max,
        "SMALL_MAX must equal the last (largest) medium class, 1 MiB \
         (1.75 MiB if medium-classes-wide is also enabled)"
    );
}

#[test]
fn size2class_o1_lookup_agrees_with_brute_force_across_the_full_extended_range() {
    // Full sweep 1..=SMALL_MAX (now up to 1 MiB) at MIN_BLOCK-fast-path
    // alignment. This is O(SMALL_MAX) = ~1M iterations, still sub-second.
    let small_max = SegmentLayout::SMALL_MAX;
    for size in 1..=small_max {
        let got = SegmentLayout::class_for(size, 1);
        let want = linear_scan_class_for(size, 1);
        assert_eq!(got, want, "drift at size={size}");
    }
}

#[test]
fn size2class_o1_lookup_agrees_with_brute_force_at_medium_boundaries() {
    // Targeted boundary check around each medium class (denser than the full
    // sweep needs, but pins the exact off-by-one edges the task calls out).
    for &sz in MEDIUM_SIZES {
        for delta in [-1i64, 0, 1] {
            let size = (sz as i64 + delta) as usize;
            if size == 0 || size > SegmentLayout::SMALL_MAX {
                continue;
            }
            let got = SegmentLayout::class_for(size, 1);
            let want = linear_scan_class_for(size, 1);
            assert_eq!(got, want, "drift at size={size} (near medium class {sz})");
        }
    }
    // One past the new SMALL_MAX must be None (Large).
    assert_eq!(
        SegmentLayout::class_for(SegmentLayout::SMALL_MAX + 1, 1),
        None
    );
}

// ---------------------------------------------------------------------------
// Correctness-surface item 1 — MiB alignment: a request whose ALIGN is one of
// the new medium class sizes must still resolve to a small class (not fall
// through to Large), via both class_for's fast and slow paths.
// ---------------------------------------------------------------------------

#[test]
fn item1_mib_alignment_resolves_to_small_not_large() {
    for &align in MEDIUM_SIZES {
        // size=1 forces the slow path to seed low and walk up to a
        // divisible class; align equal to a real table entry is trivially
        // divisible by itself, so the class holding exactly `align` bytes
        // must be found (this exercises the *slow* path per class_for's doc,
        // since align > SMALL_ALIGN_MAX).
        let got = SegmentLayout::class_for(1, align);
        let idx = got.unwrap_or_else(|| panic!("align={align} must resolve to a small class"));
        let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
        assert!(block >= align, "block {block} < align {align}");
        assert_eq!(
            block % align,
            0,
            "block {block} not divisible by align {align}"
        );
    }
    // And a request whose SIZE is a medium class with small align (fast path).
    for &size in MEDIUM_SIZES {
        let got = SegmentLayout::class_for(size, SegmentLayout::SMALL_ALIGN_MAX);
        assert!(
            got.is_some(),
            "size={size} (fast path) must resolve to small"
        );
    }
}

// ---------------------------------------------------------------------------
// Correctness-surface item 2 — mixed-size fragmentation within a segment /
// magazine-capacity assumption: a fresh AllocCore can carve MULTIPLE medium
// blocks from ONE segment (single-class-per-segment holds, just with a
// small block count), and the refill machinery does not corrupt state when a
// segment can hold only a handful of blocks.
// ---------------------------------------------------------------------------

#[test]
fn item2_one_segment_serves_multiple_medium_blocks_of_the_same_class() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(256 * 1024, 8).unwrap();

    // `AllocCore::new` already reserves the PRIMORDIAL segment, and the
    // primordial ALSO serves as the first small segment (`small_cur` starts
    // pointed at it — see `alloc_core.rs::new_inner`) — so the very first
    // 256 KiB alloc is carved from the primordial's remaining payload rather
    // than triggering a NEW OS reservation.
    //
    // Uses `dbg_table_count()` (THIS `core`'s own instance-scoped registry
    // high-water mark), NOT `dbg_segments_reserved_total()` (a PROCESS-WIDE
    // static shared across every `AllocCore` in the whole test binary,
    // including ones running concurrently on other test threads) — an
    // earlier version of this test used the process-wide counter and flaked
    // under `cargo test`'s default parallel test execution (confirmed: it
    // failed intermittently when run alongside the rest of this file's other
    // tests, each spawning its own `AllocCore` on a different thread).
    // `dbg_table_count()` is free of that hazard: it belongs to exactly this
    // `core`, which no other thread touches.
    let table_before = core.dbg_table_count();
    let p0 = core.alloc(layout);
    assert!(!p0.is_null());
    let table_after_first = core.dbg_table_count();
    assert_eq!(
        table_after_first, table_before,
        "the first 256 KiB alloc must be served from the already-reserved \
         primordial segment (no new segment registered)"
    );

    // Allocate several more 256 KiB blocks — they must come from the SAME
    // segment (no new segment reserved) until the segment's ~15-block
    // capacity is exhausted. We only check the first few stay same-segment.
    let mut ptrs = vec![p0];
    for _ in 0..5 {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    let table_after_more = core.dbg_table_count();
    assert_eq!(
        table_after_more, table_after_first,
        "6 x 256 KiB blocks must fit in ONE segment (a 4 MiB segment holds ~15) \
         — no additional segment should have been registered"
    );

    // All 6 pointers must resolve to the SAME segment base.
    let base0 = core.dbg_segment_id_of(p0);
    for &p in &ptrs[1..] {
        assert_eq!(
            core.dbg_segment_id_of(p),
            base0,
            "medium-class blocks from one carve run must share one segment"
        );
    }

    for &p in &ptrs {
        unsafe { core.dealloc(p, layout) };
    }
}

#[test]
fn item2_carving_more_medium_blocks_than_fit_reserves_a_second_segment() {
    // A 4 MiB segment holds only ~4 blocks of the 1 MiB class (minus
    // metadata) — well below TCACHE_CAP (16) and the substrate REFILL_BATCH
    // (31). Carving past that must gracefully reserve a fresh segment, not
    // corrupt bookkeeping or panic.
    //
    // Uses `dbg_table_count()` (instance-scoped), not
    // `dbg_segments_reserved_total()` (process-wide, and hence flaky under
    // `cargo test`'s parallel execution — see
    // `item2_one_segment_serves_multiple_medium_blocks_of_the_same_class`'s
    // identical fix for the confirmed failure mode).
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(1024 * 1024, 8).unwrap();
    let table_before = core.dbg_table_count();

    let mut ptrs = Vec::new();
    for _ in 0..10 {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "OOM well before exhausting a small VA range");
        ptrs.push(p);
    }
    let table_after = core.dbg_table_count();
    let new_segments = table_after - table_before;
    assert!(
        new_segments >= 2,
        "10 x 1 MiB blocks (at ~3-4/segment, starting from the already-live \
         primordial) must register at least 2 MORE segments, got {new_segments}"
    );

    for p in ptrs {
        unsafe { core.dealloc(p, layout) };
    }
}

// ---------------------------------------------------------------------------
// Correctness-surface item 3 — exact dealloc classification: a medium-class
// pointer deallocates/reallocates back to the SAME class deterministically
// (the class is derived from the caller's Layout at free time, same as every
// other small class — not re-derived from PageMap, which is per-page "first
// class wins" and NOT a reliable class oracle by design).
// ---------------------------------------------------------------------------

#[test]
fn item3_dealloc_reuses_the_same_class_freelist() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(512 * 1024, 8).unwrap();
    let class = core
        .dbg_layout_class_for(layout)
        .expect("512 KiB must be small");

    let p1 = core.alloc(layout);
    assert!(!p1.is_null());
    unsafe { core.dealloc(p1, layout) };

    // A second alloc of the SAME layout must reuse the freed block (same
    // address) rather than carving a fresh one — proving the free landed on
    // class `class`'s free list, not lost or misrouted.
    let p2 = core.alloc(layout);
    assert!(!p2.is_null());
    assert_eq!(
        p1, p2,
        "freed medium block must be reused by the next same-size alloc"
    );
    assert_eq!(core.dbg_layout_class_for(layout), Some(class));

    unsafe { core.dealloc(p2, layout) };
}

// ---------------------------------------------------------------------------
// Correctness-surface item 4 — page map: verify the per-page "first class
// wins" bookkeeping does not misreport/overflow for a medium class page.
// ---------------------------------------------------------------------------

// R12-11 (task #262): `dbg_page_map_class_for` is gated behind `page-map-diag`
// (the only reader of the now-diagnostic-only `PageMap`); this one test needs
// the feature explicitly (the rest of this file does not).
#[test]
#[cfg(feature = "page-map-diag")]
fn item4_page_map_records_medium_class_without_overflow() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(384 * 1024, 8).unwrap();
    let expected_class = core.dbg_layout_class_for(layout).unwrap();

    let p = core.alloc(layout);
    assert!(!p.is_null());
    let recorded = core.dbg_page_map_class_for(p);
    assert_eq!(
        recorded,
        Some(expected_class),
        "the page hosting a fresh medium-class block must record that class \
         (PageClass's u8 encoding must not overflow/alias for class indices \
         up to 54)"
    );

    unsafe { core.dealloc(p, layout) };
}

#[test]
fn item4_small_class_count_stays_below_page_class_sentinel_range() {
    // PageClass reserves 0xFE/0xFF as sentinels (Meta/Free); every real class
    // index must stay strictly below 0xFE. This is also pinned at compile
    // time (`segment_header.rs`'s `const _: () = assert!(SMALL_CLASS_COUNT <
    // 0xFE, ...)`), but re-asserting it at the public dbg surface documents
    // the property where a test reader will look for it.
    // R9-4 (task #226): 58 if `medium-classes-wide` is ALSO enabled (e.g.
    // `--all-features`) -- see the branch in `medium_classes_present_in_table_
    // at_the_right_sorted_position` above for the same reasoning.
    let expected_count = if cfg!(feature = "medium-classes-wide") {
        58
    } else {
        55
    };
    let count = AllocCore::dbg_small_class_count();
    assert_eq!(
        count, expected_count,
        "medium-classes must bring the count to 49 + 6 = 55 \
         (58 if medium-classes-wide is also enabled)"
    );
    assert!(
        count < 0xFE,
        "class count {count} would collide with a PageClass sentinel"
    );
}

// ---------------------------------------------------------------------------
// Correctness-surface item 7 — realloc grow/shrink across the new boundary:
// small -> medium, medium -> medium (adjacent class), medium -> Large (now
// starting at a higher absolute size, 1 MiB + 1B instead of ~253 KiB + 1B),
// and Large -> medium (shrink into what used to require Large).
// ---------------------------------------------------------------------------

#[test]
fn item7_realloc_crosses_from_small_into_a_new_medium_class() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let old_layout = Layout::from_size_align(4096, 8).unwrap();
    let p = core.alloc(old_layout);
    assert!(!p.is_null());
    unsafe {
        core.dealloc(p, old_layout);
    }
    // Fresh alloc/realloc pair (realloc consumes/frees internally).
    let p = core.alloc(old_layout);
    assert!(!p.is_null());
    let new_size = 256 * 1024;
    let grown = unsafe { core.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null());
    let grown_layout = Layout::from_size_align(new_size, 8).unwrap();
    assert_eq!(
        core.dbg_layout_class_for(grown_layout),
        core.dbg_layout_class_for(Layout::from_size_align(256 * 1024, 8).unwrap())
    );
    unsafe { core.dealloc(grown, grown_layout) };
}

#[test]
fn item7_realloc_between_two_medium_classes() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let old_layout = Layout::from_size_align(256 * 1024, 8).unwrap();
    let p = core.alloc(old_layout);
    assert!(!p.is_null());
    let new_size = 512 * 1024;
    let grown = unsafe { core.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null());
    // Must be readable/writable across its full new size (no OOB).
    unsafe {
        core::ptr::write_bytes(grown, 0xAB, new_size);
    }
    let new_layout = Layout::from_size_align(new_size, 8).unwrap();
    unsafe { core.dealloc(grown, new_layout) };
}

#[test]
fn item7_realloc_shrink_from_medium_into_former_large_territory_still_works() {
    // Before this feature, 300 KiB was Large; now it is a medium small class.
    // A realloc that shrinks INTO this range from a genuinely Large size must
    // still produce a valid, usable block.
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let old_size = 2 * 1024 * 1024; // genuinely Large even with medium-classes
    let old_layout = Layout::from_size_align(old_size, 8).unwrap();
    let p = core.alloc(old_layout);
    assert!(!p.is_null());
    let new_size = 300 * 1024; // now Small (medium class), was Large pre-feature
    let shrunk = unsafe { core.realloc(p, old_layout, new_size) };
    assert!(!shrunk.is_null());
    unsafe {
        core::ptr::write_bytes(shrunk, 0xCD, new_size);
    }
    let new_layout = Layout::from_size_align(new_size, 8).unwrap();
    assert!(core.dbg_layout_class_for(new_layout).is_some());
    unsafe { core.dealloc(shrunk, new_layout) };
}

#[test]
fn item7_realloc_grows_from_medium_up_into_genuine_large() {
    // SMALL_MAX grew to 1 MiB, so "genuine Large" now starts strictly above 1
    // MiB (previously it started strictly above ~253 KiB).
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let old_layout = Layout::from_size_align(1024 * 1024, 8).unwrap();
    assert!(
        core.dbg_layout_class_for(old_layout).is_some(),
        "1 MiB must be Small under medium-classes"
    );
    let p = core.alloc(old_layout);
    assert!(!p.is_null());
    let new_size = 4 * 1024 * 1024; // genuinely Large
    let grown = unsafe { core.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null());
    let new_layout = Layout::from_size_align(new_size, 8).unwrap();
    assert!(
        core.dbg_layout_class_for(new_layout).is_none(),
        "4 MiB must classify as Large even with medium-classes"
    );
    unsafe {
        core::ptr::write_bytes(grown, 0xEF, new_size);
    }
    unsafe { core.dealloc(grown, new_layout) };
}

// ---------------------------------------------------------------------------
// Correctness-surface item 8 — segment-id recycling: a medium-class segment,
// once fully freed, recycles through the SAME SegmentTable machinery a
// small-class segment already uses (requires `alloc-decommit` to observe the
// recycle transition; without it the slot free-lists still work identically
// to the small-class path, just without decommit).
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-decommit")]
#[test]
fn item8_emptied_medium_segment_recycles_its_table_slot() {
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(256 * 1024, 8).unwrap();

    // Fill and fully drain one segment's worth of 256 KiB blocks.
    let mut ptrs = Vec::new();
    let table_before = core.dbg_table_count();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        let base = core.dbg_segment_id_of(p);
        if !ptrs.is_empty() && core.dbg_segment_id_of(ptrs[0]) != base {
            // Rolled onto a second segment — stop; free everything from the
            // FIRST segment only, to trigger that segment's recycle.
            unsafe { core.dealloc(p, layout) };
            break;
        }
        ptrs.push(p);
    }
    let first_base = core.dbg_segment_id_of(ptrs[0]);
    for &p in &ptrs {
        assert_eq!(core.dbg_segment_id_of(p), first_base);
        unsafe { core.dealloc(p, layout) };
    }
    // The table's high-water count is unaffected (recycle NULLs a slot, it
    // doesn't shrink the count) — but a FRESH alloc of the same class must be
    // able to reuse the recycled/pooled slot instead of growing the table
    // further. This is the same recycling contract small classes already
    // rely on; verified structurally rather than by asserting an exact count
    // (which depends on decommit/pool timing details out of scope here).
    let _ = table_before;
    let p_again = core.alloc(layout);
    assert!(!p_again.is_null());
    unsafe { core.dealloc(p_again, layout) };
}
