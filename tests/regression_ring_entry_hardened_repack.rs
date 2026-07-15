//! X7 Ф2 (task #190) — hardened ring-entry repack: round-trip + layout tests.
//!
//! Ф2 adds a SEPARATE, hardened-only packing scheme for the per-segment
//! `RemoteFreeRing`'s `u32` slot entries: `[gen:8|class:6|off16:18]` (X7 plan
//! §2.4). The generation counter (Ф1's per-granule `u8` table) is threaded
//! into the ring note so a future Ф3 drain can drop a note whose generation no
//! longer matches the block's current life. This file pins Ф2's deliverables —
//! the bit layout, the `pack_entry_hardened` / `unpack_entry_hardened`
//! round-trip, the `RING_SLOT_EMPTY` non-collision, and the alignment guard —
//! BEFORE any allocator path consults them (that is Ф3).
//!
//! ## Test placement (why a new file, not extending `regression_gen_table_layout.rs`)
//!
//! The repo's "one file, one export/concept" convention (CLAUDE.md) puts each
//! distinct concept in its own file. `regression_gen_table_layout.rs` (Ф1)
//! covers the **generation table** — a segment-metadata byte TABLE plus its
//! `gen_at`/`bump_gen` byte-level accessors (concept: "per-granule generation
//! storage in segment metadata"). This file covers the **ring entry bit
//! layout** — the `u32` PACKING SCHEME in `remote_free_ring` (concept: "how a
//! (gen,class,off) triple packs into one ring slot word"). These are distinct
//! concepts in distinct modules (`segment_header` vs `remote_free_ring`); a
//! new file is the convention-faithful choice. Mirroring the existing ring
//! regression-test naming (`regression_ring_cursor_wrap.rs`,
//! `regression_ring_overflow_counter.rs`), this file is
//! `regression_ring_entry_hardened_repack.rs`.
//!
//! ## What these tests cover
//!
//! - **Round-trip** over a boundary spread of `(gen, class_idx, off)` triples:
//!   `unpack_entry_hardened(pack_entry_hardened(g, c, o)) == (g, c, o)` for the
//!   field minima, maxima, and a mid spread. Pins that the pack/unpack pair is
//!   an exact inverse (the `off16 = off >> MIN_BLOCK_SHIFT` internal rep does
//!   not lose information for `MIN_BLOCK`-aligned offsets).
//! - **Bit-width sum == 32** (the plan's exact layout, no wasted/overlapping
//!   bits) — asserted both at the source level (const `_` in
//!   `remote_free_ring.rs`) and here as a `#[test]` reading the public
//!   `ENTRY_*_BITS` constants.
//! - **`RING_SLOT_EMPTY` non-collision**: the maximum packed word over real
//!   ranges (gen∈[0,256), class∈[0,SMALL_CLASS_COUNT), off16∈[0,2^18)) is
//!   strictly below `u32::MAX` — computed from the LIVE `SMALL_CLASS_COUNT`
//!   (49 normally, 55 under `medium-classes`, R6-OPT-P0-3a), not a hardcoded
//!   literal (an earlier version of this test hardcoded `0xFFC3_FFFF`, which
//!   only held for `SMALL_CLASS_COUNT=49` and broke under `--all-features`
//!   once `medium-classes` changed the max real class from 48 to 54). The
//!   collision `(255, 63, max-off16)` is unreachable because
//!   `class=63 >= SMALL_CLASS_COUNT` in every feature configuration this
//!   crate builds. Pinned here AND by a load-bearing const-assert in the
//!   source (so a future bump of `SMALL_CLASS_COUNT` past 62 fails to
//!   compile, not silently collides).
//! - **Misaligned-offset guard**: a non-`MIN_BLOCK`-multiple `off`
//!   debug-asserts in `pack_entry_hardened` (mirrors Ф1's `gen_at`/`bump_gen`
//!   precedent, which document `MIN_BLOCK`-alignment as a caller contract but
//!   do NOT runtime-check it — the gen-table index arithmetic
//!   `off >> MIN_BLOCK_SHIFT` silently aliases distinct byte offsets onto the
//!   same cell. Ф2 chose to be STRICTER than Ф1 here: because the hardened
//!   packer is the sole stamp point for the ring note and a misaligned offset
//!   would silently corrupt BOTH the offset AND the gen lookup at drain time,
//!   Ф2 adds a `debug_assert!(off % MIN_BLOCK == 0)` where Ф1 left it to the
//!   caller. This asymmetry is deliberate and flagged for the reviewer — see
//!   the report's "ambiguity" section).
//!
//! ## Counterfactual (non-vacuity)
//!
//! - If `pack_entry_hardened` shifted `off` by the WRONG amount (e.g. `>> 3`
//!   instead of `>> MIN_BLOCK_SHIFT = 4`), the round-trip over the boundary
//!   spread would fail: `unpack_entry_hardened(pack_entry_hardened(g, c, o)).2`
//!   would not equal `o` for `o != 0`.
//! - If the bit fields OVERLAPPED (e.g. `ENTRY_GEN_SHIFT` computed from
//!   `ENTRY_OFF16_BITS` alone, dropping `ENTRY_CLASS_BITS`), the round-trip at
//!   `gen=255, class=48, off=SEGMENT-MIN_BLOCK` would fail: the high class
//!   bits would bleed into the gen field.
//! - If the layout LOOSER than the plan (e.g. `off16` given 19 bits, gen 7),
//!   `bit_widths_sum_to_exactly_32` would fail (sum != 32) — but only if the
//!   plan's exactness is encoded; it is, both in source const-asserts and here.
//!
//! ## Gates
//!
//! The hardened pack/unpack pair compiles ONLY under `#[cfg(feature =
//! "hardened")]` (which pulls `fastbin` → `alloc-global` → `alloc` →
//! `alloc-core`, so `sefer_alloc::alloc_core::remote_free_ring` is reachable).
//! The whole file is gated to `hardened`: there is no non-hardened companion
//! test here (unlike `regression_gen_table_layout.rs`'s Test 5) because the
//! non-hardened production path is BYTE-IDENTICAL to before this diff by
//! construction — the new functions are free, uncalled, and behind a cfg gate
//! that is off under `production`. The byte-identical Ir neutrality is pinned
//! by the iai judge (run under plain `production`), not by a runtime test.

#![cfg(feature = "hardened")]

use sefer_alloc::alloc_core::remote_free_ring::{
    pack_entry_hardened, unpack_entry_hardened, ENTRY_CLASS_BITS, ENTRY_GEN_BITS, ENTRY_OFF16_BITS,
    RING_SLOT_EMPTY,
};
use sefer_alloc::SegmentLayout;

/// The largest `MIN_BLOCK`-aligned segment-relative offset strictly below
/// `SEGMENT` — the maximum `off` a real ring note can carry. `(SEGMENT -
/// MIN_BLOCK)` is `MIN_BLOCK`-aligned (both powers of two) and `< SEGMENT`, so
/// it is a valid last-block start; `off16 = (SEGMENT - MIN_BLOCK) >>
/// MIN_BLOCK_SHIFT = 2^18 - 1` (the all-ones 18-bit value).
fn max_aligned_off() -> u32 {
    (SegmentLayout::SEGMENT - SegmentLayout::MIN_BLOCK) as u32
}

/// **Test 1 — round-trip over a boundary spread.** For the field minima, maxima,
/// and a mid spread, `unpack_entry_hardened(pack_entry_hardened(g, c, o))` must
/// equal `(g, c, o)` exactly. The spread covers:
/// - `gen`: 0 (fresh), 1 (first bump), 255 (the u8 max / wrap predecessor).
/// - `class_idx`: 0 (smallest class), `SMALL_CLASS_COUNT - 1` (largest real
///   class — 48 for the default geometry; the largest value the field can hold
///   for a real block).
/// - `off`: 0 (segment start), `MIN_BLOCK` (first payload granule),
///   `SEGMENT - MIN_BLOCK` (the largest `MIN_BLOCK`-aligned offset below
///   `SEGMENT`).
///
/// Non-vacuous: a wrong shift, an overlapping field, or a sign error in the
/// pack/unpack pair would fail at least one of these combinations.
#[test]
fn roundtrip_boundary_spread() {
    let mb = SegmentLayout::MIN_BLOCK as u32;
    let max_off = max_aligned_off();

    let gens: [u8; 3] = [0, 1, 255];
    // SMALL_CLASS_COUNT is `usize`; the largest real class index is count - 1.
    // We confirm the count at runtime (defensive: if the table ever grew past
    // 64 this test would still compile but the const-assert in the source would
    // have already failed the build).
    let max_class = small_class_count() - 1;
    assert!(
        max_class < 64,
        "SMALL_CLASS_COUNT must stay < 64 (const-asserted in source)"
    );
    let classes: [u32; 3] = [0, max_class as u32 / 2, max_class as u32];
    let offs: [u32; 3] = [0, mb, max_off];

    for &g in &gens {
        for &c in &classes {
            for &o in &offs {
                let packed = pack_entry_hardened(g, c, o);
                let (ug, uc, uo) = unpack_entry_hardened(packed);
                assert_eq!(
                    (ug, uc, uo),
                    (g, c, o),
                    "round-trip failed for (gen={g}, class={c}, off={o:#x}): \
                     packed={packed:#010x} unpacked=(gen={ug}, class={uc}, off={uo:#x})"
                );
            }
        }
    }
}

/// **Test 2 — exhaustive round-trip over a dense mid-range grid.** A finer
/// sweep than Test 1 to catch any off-by-one in the field boundaries that the
/// coarse spread might miss (e.g. a class-field mask one bit too narrow would
/// pass Test 1's `{0, 24, 48}` but fail at e.g. class=32). Dense over `gen` (a
/// few values across the u8 range), `class` (every value in `[0,
/// SMALL_CLASS_COUNT)`), and `off` (every `MIN_BLOCK` multiple in a small
/// window plus the boundary). Still fast (a few thousand iterations of pure
/// arithmetic).
#[test]
fn roundtrip_dense_grid() {
    let mb = SegmentLayout::MIN_BLOCK as u32;
    let max_class = (small_class_count() - 1) as u32;
    let max_off = max_aligned_off();

    // gen: a representative spread across the u8 range (not all 256 — keeps the
    // test sub-millisecond while still exercising the bit-7 boundary).
    let gens: [u8; 6] = [0, 1, 64, 128, 200, 255];
    for &g in &gens {
        for c in 0..=max_class {
            // off: 0, every MIN_BLOCK multiple up to 8 blocks, plus the max.
            let mut offs: Vec<u32> = (0..=8).map(|k| k * mb).collect();
            offs.push(max_off);
            for &o in &offs {
                let packed = pack_entry_hardened(g, c, o);
                let (ug, uc, uo) = unpack_entry_hardened(packed);
                assert_eq!(
                    (ug, uc, uo),
                    (g, c, o),
                    "dense round-trip failed for (gen={g}, class={c}, off={o:#x})"
                );
            }
        }
    }
}

/// **Test 3 — bit widths sum to exactly 32.** The plan's layout
/// `[gen:8|class:6|off16:18]` uses all 32 bits with no waste and no overlap.
/// This is also pinned by a `const _: () = assert!(...)` in
/// `remote_free_ring.rs` (compile-time), but this runtime assert surfaces the
/// exact values in a test failure message and documents the layout for readers
/// who skip the source const-asserts.
#[test]
fn bit_widths_sum_to_exactly_32() {
    assert_eq!(ENTRY_GEN_BITS, 8, "gen field must be 8 bits (the Ф1 u8)");
    assert_eq!(
        ENTRY_CLASS_BITS, 6,
        "class field must be 6 bits (SMALL_CLASS_COUNT=49 < 64)"
    );
    assert_eq!(
        ENTRY_OFF16_BITS, 18,
        "off16 field must be 18 bits (SEGMENT/MIN_BLOCK = 2^18)"
    );
    assert_eq!(
        ENTRY_GEN_BITS + ENTRY_CLASS_BITS + ENTRY_OFF16_BITS,
        32,
        "hardened ring entry fields must sum to exactly 32 (X7 §2.4 layout, no waste)"
    );
}

/// **Test 4 — `RING_SLOT_EMPTY` non-collision over real ranges.** A hardened
/// ring entry must never equal `RING_SLOT_EMPTY` (`u32::MAX`) for any real
/// `(gen, class, off)` triple, so the drain's `if off == RING_SLOT_EMPTY`
/// sentinel check (the not-yet-published / already-drained marker) stays
/// unambiguous. The packed word is `u32::MAX` only when ALL three fields are
/// simultaneously all-ones: `gen=0xFF`, `class=0x3F` (=63), `off16=0x3_FFFF`.
/// `gen=0xFF` is reachable (u8 wrap), `off16=0x3_FFFF` is reachable (the last
/// block start), but `class=63` is NOT (`SMALL_CLASS_COUNT=49`, max real class
/// = 48 = `0x30`). The maximum packed word over real ranges is therefore
/// `0xFFC3_FFFF < u32::MAX`.
///
/// This test computes the absolute maximum over real ranges and asserts it is
/// strictly below the sentinel. It ALSO scans the full product of the
/// boundary-value combinations to catch any combo that individually collides
/// (there is none, by the argument above — but the scan is cheap and pins the
/// property empirically, not just by reasoning).
#[test]
fn entry_never_collides_with_ring_slot_empty() {
    let mb = SegmentLayout::MIN_BLOCK as u32;
    let max_off = max_aligned_off();

    // The absolute maximum over real ranges: gen=255, class=<max real>, off
    // = SEGMENT - MIN_BLOCK (max off16). Any other real triple is <= this.
    //
    // R6-OPT-P0-3a note: the expected packed word used to be hardcoded as the
    // literal `0xFFC3_FFFF` (computed for `SMALL_CLASS_COUNT=49`, max real
    // class 48 = 0x30). The `medium-classes` feature raises
    // `SMALL_CLASS_COUNT` to 55 (max real class 54 = 0x36), which changes
    // this literal — the hardcoded assertion silently assumed exactly 49
    // classes and broke under `cargo test --all-features` once
    // `medium-classes` was added (confirmed: this was the actual failure
    // symptom during R6-OPT-P0-3a implementation). Recomputed from
    // `max_real_class` directly (the same bit arithmetic
    // `pack_entry_hardened` itself performs) instead of a hardcoded literal,
    // so this test stays correct across any future `SMALL_CLASS_COUNT`
    // change, not just the two values seen so far.
    let max_real_class = (small_class_count() - 1) as u32;
    let max_packed = pack_entry_hardened(255, max_real_class, max_off);
    let off16_at_max = max_off >> SegmentLayout::MIN_BLOCK_SHIFT;
    let expected_max_packed = off16_at_max | (max_real_class << 18) | (255u32 << 24);
    assert_eq!(
        max_packed, expected_max_packed,
        "max real packed word should be gen=0xFF, class={max_real_class:#04x}, off16={off16_at_max:#07x} \
         packed as {expected_max_packed:#010x}"
    );
    assert_ne!(
        max_packed, RING_SLOT_EMPTY,
        "max real packed word must not equal the ring-slot sentinel"
    );

    // Boundary-value scan: every combo of field min/max for gen, class, off.
    // None should collide. (If any did, the argument above would be wrong.)
    for &g in &[0u8, 1, 127, 128, 200, 255] {
        for c in [0u32, max_real_class / 2, max_real_class] {
            for &o in &[0u32, mb, mb * 128, max_off] {
                let packed = pack_entry_hardened(g, c, o);
                assert_ne!(
                    packed, RING_SLOT_EMPTY,
                    "packed (gen={g}, class={c}, off={o:#x}) = {packed:#010x} collides with sentinel"
                );
            }
        }
    }

    // The ONLY triple that produces u32::MAX is (255, 63, SEGMENT-MIN_BLOCK),
    // and class=63 is unreachable (>= SMALL_CLASS_COUNT). Confirm by packing it
    // DIRECTLY via the bit arithmetic (bypassing pack_entry_hardened's
    // debug_assert on class range) — this documents the theoretical collision
    // and why it cannot occur in practice.
    let off16_max = max_off >> SegmentLayout::MIN_BLOCK_SHIFT;
    let theoretical = (off16_max) | (63u32 << 18) | (255u32 << 24);
    assert_eq!(
        theoretical,
        u32::MAX,
        "the all-ones triple (gen=255, class=63, off16=max) is the unique u32::MAX packing"
    );
    assert!(
        63 >= small_class_count(),
        "class=63 (the collision value) is >= SMALL_CLASS_COUNT, so no real block reaches it"
    );
}

/// **Test 5 — misaligned offset is caller's-responsibility-guarded.** A
/// non-`MIN_BLOCK`-multiple `off` would silently alias onto the wrong `off16`
/// value (losing the low bits) AND, at drain time, index the wrong generation
/// cell. Ф2 adds a `debug_assert!(off % MIN_BLOCK == 0)` in
/// `pack_entry_hardened` (STRICTER than Ф1's `gen_at`/`bump_gen`, which document
/// alignment as a caller contract without runtime-checking it). In a debug
/// build this test confirms the guard PANICS on a misaligned offset; in a
/// release build (`--release`, debug-asserts off) the guard is inert and the
/// misaligned offset is silently truncated — documented as caller's
/// responsibility, same as Ф1.
///
/// We use `std::panic::catch_unwind` to confirm the debug-build panic WITHOUT
/// aborting the test process. Under `--release` the closure does not panic and
/// the test asserts the truncation behaviour instead (documenting the
/// release-build contract).
#[test]
fn misaligned_offset_guard() {
    let mb = SegmentLayout::MIN_BLOCK as u32;
    let misaligned = mb + 1; // MIN_BLOCK + 1 — not a multiple of MIN_BLOCK.

    // The guard fires only in debug builds (debug_assert!). We detect the build
    // cfg at compile time so this test is meaningful in both profiles.
    if cfg!(debug_assertions) {
        // Debug build: pack_entry_hardened MUST panic on a misaligned offset.
        let result = std::panic::catch_unwind(|| {
            pack_entry_hardened(0, 0, misaligned);
        });
        assert!(
            result.is_err(),
            "debug build: pack_entry_hardened must panic on a misaligned offset (off={misaligned}), \
             got {:#010x} instead",
            pack_entry_hardened(0, 0, misaligned)
        );
    } else {
        // Release build: the debug_assert is inert; the misaligned offset is
        // silently truncated to off16 = (mb+1) >> MIN_BLOCK_SHIFT = 0 (since
        // mb+1 < 2*mb). Documented as caller's responsibility. We assert the
        // truncation so the release-build behaviour is pinned, not implicit.
        let packed = pack_entry_hardened(0, 0, misaligned);
        let (_, _, off) = unpack_entry_hardened(packed);
        assert_eq!(
            off, 0,
            "release build: misaligned off={misaligned} truncates to off16=0 (caller's responsibility)"
        );
        // And the silent truncation is a LOSS: round-trip does NOT recover the
        // original misaligned offset. This is the COST of the caller's contract
        // in release builds — pinned here so a future change to the guard
        // policy (e.g. promoting to a runtime assert) surfaces as a test delta.
        assert_ne!(
            off, misaligned,
            "release build: misaligned offset is NOT recoverable (truncated to a MIN_BLOCK boundary)"
        );
    }

    // Sanity: the guard does NOT fire for a MIN_BLOCK-aligned offset adjacent to
    // the misaligned one (confirms the guard is on alignment, not on the value).
    let _ = pack_entry_hardened(0, 0, mb);
}

/// **Test 6 — field independence (no cross-field bleed).** Setting one field to
/// its maximum must not corrupt the other two. This is the bit-layout analogue
/// of Ф1's `distinct_granules_are_independent`: a packing bug where a field
/// mask is too wide (or a shift off-by-one) would let one field's bits bleed
/// into a neighbour.
#[test]
fn fields_are_independent() {
    let mb = SegmentLayout::MIN_BLOCK as u32;
    let max_off = max_aligned_off();
    let max_real_class = (small_class_count() - 1) as u32;

    // Vary gen alone (class, off fixed at non-trivial values); recovered gen
    // must track the input exactly.
    let fixed_c = max_real_class;
    let fixed_o = mb * 17;
    for g in [0u8, 1, 64, 128, 200, 255] {
        let (ug, uc, uo) = unpack_entry_hardened(pack_entry_hardened(g, fixed_c, fixed_o));
        assert_eq!(ug, g, "gen field corrupted by fixed class/off");
        assert_eq!(uc, fixed_c, "class field bled into by varying gen");
        assert_eq!(uo, fixed_o, "off field bled into by varying gen");
    }

    // Vary class alone (gen, off fixed).
    let fixed_g = 200u8;
    for c in [0u32, 1, max_real_class / 3, max_real_class] {
        let (ug, uc, uo) = unpack_entry_hardened(pack_entry_hardened(fixed_g, c, fixed_o));
        assert_eq!(ug, fixed_g, "gen field bled into by varying class");
        assert_eq!(uc, c, "class field corrupted by fixed gen/off");
        assert_eq!(uo, fixed_o, "off field bled into by varying class");
    }

    // Vary off alone (gen, class fixed). Use the boundary max to exercise the
    // full off16 field.
    for o in [0u32, mb, mb * 256, max_off] {
        let (ug, uc, uo) = unpack_entry_hardened(pack_entry_hardened(fixed_g, fixed_c, o));
        assert_eq!(ug, fixed_g, "gen field bled into by varying off");
        assert_eq!(uc, fixed_c, "class field bled into by varying off");
        assert_eq!(uo, o, "off field corrupted by fixed gen/class");
    }
}

/// Helper: read `SMALL_CLASS_COUNT` at runtime via the public
/// `SegmentLayout::SIZE_CLASS_TABLE` slice (the table is `pub`, its length IS
/// `SMALL_CLASS_COUNT`). Stronger than hardcoding 49: if `SIZE_CLASS_TABLE`
/// ever changes length, this tracks it automatically, and the source
/// const-assert in `remote_free_ring.rs` (which sees the `pub(crate)` constant
/// directly) fails the BUILD if the new length would overflow the 6-bit class
/// field or reintroduce a `RING_SLOT_EMPTY` collision.
fn small_class_count() -> usize {
    SegmentLayout::SIZE_CLASS_TABLE.len()
}
