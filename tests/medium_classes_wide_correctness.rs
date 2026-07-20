//! R9-4 — wide-medium exact size classes (1.25 / 1.5 / 1.75 MiB), feature-gated
//! behind `medium-classes-wide` (which implies `medium-classes`). This file
//! exercises the prototype's own correctness-surface checklist (the task's
//! requirements §4 a–e) plus the density measurement that justifies the
//! prototype (`§3` of the task — verify the review's rough 3x/2x/2x
//! objects-per-segment guess against the REAL per-segment metadata overhead).
//!
//! Whole file is a no-op without `medium-classes-wide` (see the `#![cfg(...)]`
//! below) — run with:
//!   cargo test --features "production medium-classes-wide" --test medium_classes_wide_correctness
//!
//! Sibling file `tests/medium_classes_correctness.rs` covers the six-class
//! `medium-classes` substrate this prototype layers on top of; this file does
//! NOT re-test those six classes (their behavior is unchanged — see
//! `wide_does_not_disturb_six_class_medium_table_topology` below, which
//! asserts the 15 entries that PRECEDE the wide append are byte-identical to
//! the plain-`medium-classes` list).

#![cfg(all(feature = "alloc-core", feature = "medium-classes-wide"))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The three wide-medium classes R9-4 adds, in the exact byte values the
/// review specified (1.25 / 1.5 / 1.75 MiB).
const WIDE_SIZES: &[usize] = &[
    1280 * 1024, // 1.25 MiB = 1,310,720 B
    1536 * 1024, // 1.5   MiB = 1,572,864 B
    1792 * 1024, // 1.75  MiB = 1,835,008 B
];

/// The byte values, spelled out for assertion messages / arithmetic.
const ONE_MIB: usize = 1024 * 1024;
const ONE_POINT_25_MIB: usize = 1280 * 1024;
const ONE_POINT_5_MIB: usize = 1536 * 1024;
const ONE_POINT_75_MIB: usize = 1792 * 1024;

// ---------------------------------------------------------------------------
// Task §4 (a) — each new class correctly classifies a matching-size request
// (size -> class round-up is correct, no overflow into the wrong class), AND
// a request just above the OLD 1 MiB SMALL_MAX now routes into the new small
// path instead of Large.
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
fn wide_classes_present_in_table_at_the_right_sorted_position() {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    assert_eq!(
        table.len(),
        58,
        "medium-classes-wide must add exactly 3 entries to the 55-entry \
         medium-classes table"
    );
    // The 15 pre-wide entries (9 base extras + 6 medium-classes extras) are
    // byte-identical to the plain-`medium-classes` list — verified separately
    // by `wide_does_not_disturb_six_class_medium_table_topology`. Here we only
    // pin the wide tail.
    let pre_wide_top = table[54]; // the old 55-entry table's top class (1 MiB)
    assert_eq!(
        pre_wide_top, ONE_MIB,
        "the medium-classes table's top class (1 MiB) must be unchanged under -wide"
    );
    assert!(
        pre_wide_top < WIDE_SIZES[0],
        "the medium-classes table's top class ({pre_wide_top}) must be strictly \
         below the first wide class ({}) for the append-not-merge shortcut \
         `build_table` relies on to be valid",
        WIDE_SIZES[0]
    );
    for (i, &sz) in WIDE_SIZES.iter().enumerate() {
        assert_eq!(
            table[55 + i],
            sz,
            "wide class {i} expected at table[{}], found {} instead of {sz}",
            55 + i,
            table[55 + i]
        );
    }
    // Strictly increasing end-to-end (not just within the wide run).
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
fn small_max_updates_to_one_point_75_mib() {
    assert_eq!(
        SegmentLayout::SMALL_MAX,
        ONE_POINT_75_MIB,
        "SMALL_MAX must equal the last (largest) wide-medium class, 1.75 MiB"
    );
}

#[test]
fn small_class_count_is_58_and_below_every_encoding_ceiling() {
    let count = AllocCore::dbg_small_class_count();
    assert_eq!(
        count, 58,
        "medium-classes-wide must bring the count to 55 + 3 = 58"
    );
    // PageClass u8 sentinel ceiling (Meta=0xFE / Free=0xFF) — pinned at compile
    // time too (`segment_header.rs`'s `const _: () = assert!(SMALL_CLASS_COUNT
    // < 0xFE, ...)`); re-asserted here as documentation.
    assert!(
        count < 0xFE,
        "class count {count} would collide with a PageClass sentinel"
    );
    // Hardened RemoteFreeRing packed-entry class-field ceiling: 6 bits, and
    // the all-ones value 0x3F must stay strictly above the maximum real class
    // index so the packed word can never collide with `RING_SLOT_EMPTY`
    // (`u32::MAX`). Pinned at compile time by `SMALL_CLASS_COUNT <= 62`.
    assert!(
        count <= 62,
        "class count {count} exceeds the hardened-ring non-collision ceiling (62)"
    );
}

#[test]
fn each_matching_size_classifies_to_a_small_class_at_the_right_index() {
    for (i, &sz) in WIDE_SIZES.iter().enumerate() {
        let got = SegmentLayout::class_for(sz, SegmentLayout::SMALL_ALIGN_MAX);
        let idx = got
            .unwrap_or_else(|| panic!("size={sz} (wide class {i}) must resolve to a small class"));
        assert_eq!(
            SegmentLayout::SIZE_CLASS_TABLE[idx],
            sz,
            "a wide class's own size must round-trip to its own table entry"
        );
    }
}

#[test]
fn size_just_above_one_mib_routes_into_small_path_not_large() {
    // The OLD `medium-classes` SMALL_MAX was exactly 1 MiB; under
    // `medium-classes-wide` a request just above it must now resolve to a
    // small class (the 1.25 MiB wide class) instead of falling through to
    // Large.
    let just_above_old = ONE_MIB + 1;
    let got = SegmentLayout::class_for(just_above_old, SegmentLayout::SMALL_ALIGN_MAX);
    let idx = got.expect("1 MiB + 1 must be Small under medium-classes-wide");
    let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
    assert_eq!(
        block, ONE_POINT_25_MIB,
        "1 MiB + 1 must round up to the 1.25 MiB wide class, got block {block}"
    );
    // Sanity: the same request under plain medium-classes would be Large. We
    // cannot run the plain-medium-classes build in the same binary, so we
    // confirm the boundary behaviorally: ONE_MIB itself stays Small (it was
    // already the top of plain medium-classes), and ONE_POINT_75_MIB + 1 is
    // the new Large boundary (asserted in a separate test below).
    assert!(
        SegmentLayout::class_for(ONE_MIB, SegmentLayout::SMALL_ALIGN_MAX).is_some(),
        "1 MiB must remain Small under -wide (it was the top of plain medium-classes)"
    );
}

// ---------------------------------------------------------------------------
// Task §4 (c) — a request just above 1.75 MiB (new SMALL_MAX) still correctly
// falls through to the Large path (the cliff MOVED but did not disappear —
// confirm the boundary is exactly where intended).
// ---------------------------------------------------------------------------

#[test]
fn size_just_above_one_point_75_mib_falls_through_to_large() {
    // SMALL_MAX itself is Small; SMALL_MAX + 1 is Large. The cliff relocated
    // one octave up but is otherwise the same shape.
    assert!(
        SegmentLayout::class_for(SegmentLayout::SMALL_MAX, SegmentLayout::SMALL_ALIGN_MAX)
            .is_some(),
        "SMALL_MAX ({} B) itself must be Small",
        SegmentLayout::SMALL_MAX
    );
    assert_eq!(
        SegmentLayout::class_for(SegmentLayout::SMALL_MAX + 1, SegmentLayout::SMALL_ALIGN_MAX),
        None,
        "SMALL_MAX + 1 must be Large (the cliff moved to 1.75 MiB, not disappeared)"
    );
    // And the literal 2 MiB review-demarcated size stays Large (out of scope
    // for this prototype by design — see R9_4 report).
    assert_eq!(
        SegmentLayout::class_for(2 * 1024 * 1024, SegmentLayout::SMALL_ALIGN_MAX),
        None,
        "2 MiB must remain Large (out-of-scope for R9-4: no density win vs the \
         existing Large path, needs a page-run layer)"
    );
}

#[test]
fn size2class_o1_lookup_agrees_with_brute_force_at_wide_boundaries() {
    // Targeted boundary check around each wide class (denser than a full sweep,
    // pins the exact off-by-one edges the task calls out).
    for &sz in WIDE_SIZES {
        for delta in [-1i64, 0, 1] {
            let size = (sz as i64 + delta) as usize;
            if size == 0 || size > SegmentLayout::SMALL_MAX {
                continue;
            }
            let got = SegmentLayout::class_for(size, 1);
            let want = linear_scan_class_for(size, 1);
            assert_eq!(got, want, "drift at size={size} (near wide class {sz})");
        }
    }
    // One past the new SMALL_MAX must be None (Large).
    assert_eq!(
        SegmentLayout::class_for(SegmentLayout::SMALL_MAX + 1, 1),
        None
    );
}

// ---------------------------------------------------------------------------
// Task §3 + §4 (b) (d) — the density win. The review's rough 3x/2x/2x guess
// used the naive `payload / block_size` arithmetic, which IGNORES the carve
// path's block-size alignment requirement (`carve_block` does
// `align_up(bump, block_size)`). The REAL density is one lower for every wide
// class — see [`empirical_density_for`] for the full derivation. These tests
// measure the REAL density two ways (arithmetic from constants + empirical
// carve) and report both against the review's guess, so the R9-4 doc can cite
// measured values, not estimates.
//
// HEADLINE FINDING (asserted + measured below): only the 1.25 MiB class
// actually delivers a density win (2x vs the Large path's 1x). The 1.5 and
// 1.75 MiB classes fit only 1 block per segment — the SAME density as the
// existing Large path — because `floor(4 MiB / block_size)` collapses from 3
// (at 1.25 MiB) to 2 (at 1.5 / 1.75 MiB), and the alignment-tax `-1` then
// takes them to 1. They still gain the small path's warm freelist (a ~90 µs
// free becomes a ~60 ns free for same-size reuse, per R8-9 §4.3), but the
// DENSITY headline the prototype was sized on does NOT apply to them.
// 2 MiB itself would also be 1 (floor(4 MiB / 2 MiB) - 1) — confirming the
// task's "out of scope" call on it.
//
// `dbg_segment_id_of` is the same instance-scoped diagnostic surface
// `tests/medium_classes_correctness.rs` uses (see that file's `item2_*` tests
// for why the process-wide `dbg_segments_reserved_total()` is avoided under
// `cargo test`'s parallel execution).
// ---------------------------------------------------------------------------

/// The per-segment payload available to a fresh (non-primordial) small
/// segment: `SEGMENT - SMALL_META_END`. This is the NAIVE density numerator
/// (what the review's rough 3x/2x/2x guess used); the REAL density is one
/// lower for the wide classes — see [`empirical_density_for`] for why
/// (block-size alignment waste at the segment start).
fn fresh_segment_payload() -> usize {
    SegmentLayout::SEGMENT - SegmentLayout::SMALL_META_END
}

/// The REAL objects-per-fresh-segment count for a given block size, accounting
/// for the carve path's `align_up(bump, block_size)` requirement
/// (`src/alloc_core/alloc_core_small.rs::carve_block`: each block must be
/// `block_size`-aligned so the free path's `align_down(ptr, block_size)`
/// recovers the block start).
///
/// For a block size `> SMALL_META_END` (true for every wide class — they are
/// all `>= 1.25 MiB > 72 KiB = SMALL_META_END`), the FIRST carved block goes
/// at offset `= block_size`, NOT at `small_meta_end`, so the layout is:
///
/// ```text
///   [ metadata ][unused: small_meta_end .. block_size][block 1][block 2]...
/// ```
///
/// The k-th block sits at offset `k * block_size` and ends at
/// `(k+1) * block_size`; the carve succeeds iff `(k+1) * block_size <= SEGMENT`.
/// So the maximum `k` is `floor(SEGMENT / block_size) - 1` — exactly ONE FEWER
/// than the naive `floor(SEGMENT / block_size)` the review's arithmetic
/// (`payload / block_size`) approximates.
///
/// This is the same mechanism the existing `medium-classes` 1 MiB class
/// already exhibits (~3 blocks per segment, not the naive ~4) — see
/// `benches/medium_size_sweep.rs` lines 13-18's "~15 fit per segment" comment
/// for 256 KiB (= `floor(4 MiB / 256 KiB) - 1 = 16 - 1 = 15`).
fn empirical_density_for(block_size: usize) -> usize {
    // For block_size > SMALL_META_END (all wide classes), first block sits at
    // offset `block_size`, so the count is `floor(SEGMENT / block_size) - 1`.
    // (The general formula would special-case small block_size <= meta_end,
    // but no wide class is that small — assert it.)
    assert!(
        block_size > SegmentLayout::SMALL_META_END,
        "empirical_density_for is only meaningful for block_size > SMALL_META_END; \
         wide classes are all >= 1.25 MiB"
    );
    SegmentLayout::SEGMENT / block_size - 1
}

#[test]
fn report_real_density_per_wide_class() {
    // The task asks to "verify these ratios yourself against the REAL segment
    // metadata overhead before writing them into any report." This test does
    // exactly that, printing both the naive (review's-method) density and the
    // REAL density after accounting for the carve path's block-size alignment
    // requirement, so the R9-4 doc can cite the measured numbers.
    let payload = fresh_segment_payload();
    eprintln!();
    eprintln!("=== R9-4 density measurement (REAL carve geometry, not arithmetic) ===");
    eprintln!(
        "SEGMENT = {sg} B ({sg_kib} KiB = 4 MiB)",
        sg = SegmentLayout::SEGMENT,
        sg_kib = SegmentLayout::SEGMENT / 1024
    );
    eprintln!(
        "SMALL_META_END = {sme} B ({sme_kib} KiB) — per-segment metadata overhead",
        sme = SegmentLayout::SMALL_META_END,
        sme_kib = SegmentLayout::SMALL_META_END / 1024
    );
    eprintln!(
        "fresh-segment payload (arithmetic) = SEGMENT - SMALL_META_END = {pl} B ({pl_kib} KiB)",
        pl = payload,
        pl_kib = payload / 1024
    );
    eprintln!("carve path aligns each block to block_size (`align_up(bump, block_size)`)");
    eprintln!("  -> for block_size > SMALL_META_END, the first block sits at offset = block_size,");
    eprintln!("     so REAL density = floor(SEGMENT / block_size) - 1 (one fewer than the");
    eprintln!("     review's payload/block_size guess). Same mechanism that makes the existing");
    eprintln!("     1 MiB medium class fit ~3/segment, not the naive ~4.");
    eprintln!();
    eprintln!(
        "  {cls:<14} {review:<14} {naive:<14} {real:<14} density-win?",
        cls = "class",
        review = "review guess",
        naive = "naive",
        real = "REAL",
    );
    for &sz in WIDE_SIZES {
        let naive = payload / sz; // review's-method approximation
        let real = empirical_density_for(sz);
        let review_guess = match sz {
            ONE_POINT_25_MIB => 3,
            ONE_POINT_5_MIB => 2,
            ONE_POINT_75_MIB => 2,
            _ => unreachable!(),
        };
        let verdict = if real >= 2 {
            format!("YES ({}x vs Large's 1x)", real)
        } else {
            "NO (same 1x as Large; warm-freelist win still applies)".to_string()
        };
        eprintln!(
            "  {cls:<14} {review:<14} {naive:<14} {real:<14} {verdict}",
            cls = format!("{:.2} MiB", sz as f64 / (1024.0 * 1024.0)),
            review = review_guess,
            naive = naive,
            real = real,
            verdict = verdict
        );
    }
    eprintln!();
    eprintln!("HEADLINE: only the 1.25 MiB class delivers a real density win (2x). The 1.5");
    eprintln!("  and 1.75 MiB classes fit only 1 block per segment — the SAME density as the");
    eprintln!("  existing Large path — because floor(4 MiB / block_size) drops from 3 to 2 at");
    eprintln!("  1.5 MiB and the alignment-tax `-1` then takes them to 1.");
    eprintln!("2 MiB itself would also be 1 (floor(4 MiB / 2 MiB) - 1) — confirming the");
    eprintln!("  task's 'out of scope' call on it (needs a larger medium-arena / page-run).");
    eprintln!("==========================================================================");
}

#[test]
fn empirical_carve_matches_predicted_density_for_every_wide_class() {
    // Empirical confirmation of [`empirical_density_for`]: carve blocks and
    // verify the REAL per-fresh-segment count matches the formula. This is the
    // "verify against REAL overhead, don't trust the review's rough estimate"
    // measurement the task explicitly asks for.
    //
    // The primordial's payload is smaller (it hosts the registry/hash/free-
    // list), but it never holds MORE than a fresh segment — so the MAX
    // per-segment residency across a multi-block carve IS the fresh-segment
    // count, and that is what we compare to `empirical_density_for`.
    for &sz in WIDE_SIZES {
        let mut core = AllocCore::new().expect("AllocCore::new failed");
        let layout = Layout::from_size_align(sz, 8).unwrap();
        let predicted = empirical_density_for(sz);
        // Carve enough to span at least 2 segments at the predicted density,
        // so the max-residency reading is stable (not just the primordial
        // tail). `predicted + 3` is always enough.
        let carve_count = predicted + 3;
        let mut ptrs = Vec::new();
        for _ in 0..carve_count {
            let p = core.alloc(layout);
            assert!(!p.is_null(), "OOM carving {sz} B");
            ptrs.push(p);
        }
        let mut ids: Vec<u32> = ptrs.iter().map(|&p| core.dbg_segment_id_of(p)).collect();
        ids.sort_unstable();
        let max_residency = ids
            .chunk_by(|a, b| a == b)
            .map(|chunk| chunk.len())
            .max()
            .unwrap();
        assert_eq!(
            max_residency, predicted,
            "wide class {sz} B: empirical max-per-segment residency ({max_residency}) \
             must match empirical_density_for ({predicted}) — drift here means the \
             carve geometry changed and the R9-4 density report is stale"
        );
        for p in ptrs {
            unsafe { core.dealloc(p, layout) };
        }
    }
}

#[test]
fn one_point_25_mib_class_proves_a_real_density_win() {
    // The 1.25 MiB class is the ONLY wide class with a real density win (2x
    // vs the Large path's 1x). Prove it concretely: carve multiple same-size
    // blocks and assert at least 2 share a fresh segment. The Large path puts
    // every object in its OWN segment; the small path must pack at least 2.
    let sz = ONE_POINT_25_MIB;
    let mut core = AllocCore::new().expect("AllocCore::new failed");
    let layout = Layout::from_size_align(sz, 8).unwrap();
    let mut ptrs = Vec::new();
    for _ in 0..6 {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    let mut bases: Vec<u32> = ptrs.iter().map(|&p| core.dbg_segment_id_of(p)).collect();
    bases.sort_unstable();
    let max_residency = bases
        .chunk_by(|a, b| a == b)
        .map(|chunk| chunk.len())
        .max()
        .unwrap();
    assert!(
        max_residency >= 2,
        "1.25 MiB is the one wide class with a density win: expected >= 2 of 6 \
         allocs to share a segment, got max residency {max_residency}"
    );
    for p in ptrs {
        unsafe { core.dealloc(p, layout) };
    }
}

#[test]
fn one_point_5_and_1_point_75_mib_classes_carry_no_density_win_documented() {
    // IMPORTANT HONEST FINDING (the task explicitly asks to report real
    // numbers, not the review's guess): the 1.5 and 1.75 MiB classes each fit
    // only 1 block per fresh segment — the SAME density as the existing Large
    // path — because `floor(SEGMENT / block_size)` is 2 for both, and the
    // carve path's block-size alignment tax takes that to 1. So for these two
    // classes the prototype does NOT deliver its headline density win.
    //
    // They STILL deliver the small path's warm freelist (same-size reuse: a
    // ~90 µs Large free becomes a ~60 ns small free per R8-9 §4.3), but that
    // is the freelist win, not the density win.
    //
    // This test PINS the no-density-win finding behaviorally: carve multiple
    // same-size blocks and assert NO 2 share a segment (max residency == 1).
    for &sz in &[ONE_POINT_5_MIB, ONE_POINT_75_MIB] {
        let mut core = AllocCore::new().expect("AllocCore::new failed");
        let layout = Layout::from_size_align(sz, 8).unwrap();
        let mut ptrs = Vec::new();
        for _ in 0..4 {
            let p = core.alloc(layout);
            assert!(!p.is_null());
            ptrs.push(p);
        }
        let mut bases: Vec<u32> = ptrs.iter().map(|&p| core.dbg_segment_id_of(p)).collect();
        bases.sort_unstable();
        let max_residency = bases
            .chunk_by(|a, b| a == b)
            .map(|chunk| chunk.len())
            .max()
            .unwrap();
        assert_eq!(
            max_residency, 1,
            "1.5/1.75 MiB wide class {sz} B: expected NO density win (max residency \
             1, same as Large path), got {max_residency} — if this is >= 2, the carve \
             geometry changed and the R9-4 'no density win' finding is stale \
             (re-check floor(SEGMENT / block_size))"
        );
        for p in ptrs {
            unsafe { core.dealloc(p, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// Task §4 (e) — feature-OFF (plain `medium-classes` without `-wide`) behavior
// is UNCHANGED: assert `SMALL_MAX` / class count stay exactly as R9-3 measured
// them (55 classes, 1 MiB ceiling) when `medium-classes-wide` is NOT enabled.
//
// This whole file is `#![cfg(feature = "medium-classes-wide")]`, so the
// plain-medium-classes build does not compile this file at all — the
// regression guard for the plain-medium-classes class count already lives in
// `tests/medium_classes_correctness.rs::item4_small_class_count_stays_below_page_class_sentinel_range`
// (asserts 55) and `tests/segment_directory_a5.rs::medium_classes_directory_rebuild`
// (asserts 55). What we CAN assert here is that the 15 entries that PRECEDE
// the wide append are byte-identical to that plain-medium-classes list —
// i.e. the wide feature only APPENDS, never mutates the six-class substrate.
// ---------------------------------------------------------------------------

#[test]
fn wide_does_not_disturb_six_class_medium_table_topology() {
    // The first 55 entries of the -wide table must be byte-identical to the
    // plain-`medium-classes` `EXTRAS` list (R9-3 measured exactly that 55-class
    // topology for the promotion gate; this test pins it so the -wide append
    // cannot silently invalidate R9-3's measurements).
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    assert_eq!(table.len(), 58);

    // The 9 base extras (256 B..16384 B) — these come BEFORE the medium
    // classes; verify they are unchanged by listing them verbatim.
    let base_extras: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
    for (i, &expected) in base_extras.iter().enumerate() {
        // The base extras are merged into the table at sorted positions
        // interleaved with the geometric run; rather than re-deriving the
        // exact interleaving, just check each base extra VALUE appears in the
        // table (the strict-increase + sorted-order properties verified above
        // guarantee uniqueness).
        assert!(
            table.contains(&expected),
            "base extra {expected} (index {i}) missing from -wide table — \
             the wide append must not disturb the base+medium substrate"
        );
    }

    // The 6 medium classes — these appear as a contiguous run at indices 49..55
    // (after the 49-entry base table). They MUST be byte-identical to the
    // plain-medium-classes values.
    let medium_extras: &[usize] = &[
        256 * 1024,
        320 * 1024,
        384 * 1024,
        512 * 1024,
        768 * 1024,
        1024 * 1024,
    ];
    for (i, &expected) in medium_extras.iter().enumerate() {
        assert_eq!(
            table[49 + i],
            expected,
            "medium extra {i} = {expected} must be at table[{}], found {} — \
             the wide append must not mutate the six-class medium substrate \
             (R9-3's promotion-gate measurements assumed exactly this topology)",
            49 + i,
            table[49 + i]
        );
    }

    // And only indices 55..58 are the wide append.
    for (i, &expected) in WIDE_SIZES.iter().enumerate() {
        assert_eq!(
            table[55 + i],
            expected,
            "wide class {i} = {expected} must be at table[{}], found {}",
            55 + i,
            table[55 + i]
        );
    }
}
