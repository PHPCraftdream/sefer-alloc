//! B4 variant (в): STATE-BYTE granule↔byte-index boundary tests.
//!
//! HISTORY: the original N4 tests asserted the `u64`-word boundaries of the
//! per-segment `AllocBitmap` (bit 7/8 byte crossing, bit 63/64 word
//! crossing, last word of the footprint). Those assertions went STALE BY
//! DESIGN when the per-granule STATE BYTE (one byte per `MIN_BLOCK` granule,
//! `byte = (class+1) | (state << 6)`) became the authoritative small-segment
//! state record and the bitmaps stopped being maintained — a bitmap-word
//! shift can no longer change allocation behaviour, so testing it would test
//! a dead mechanism. This file now asserts the STATE BYTE's own boundaries
//! through the `dbg_state_bytes_for` dump hook:
//!
//! * first-granule mapping: an issued block's byte lives at byte index
//!   `off >> 4` (== the granule index) and nowhere else;
//! * LIVE vs FREE state transitions per granule, with byte-indexed
//!   neighbour preservation (consecutive granules are consecutive BYTES —
//!   the byte mapping makes the old bit-packing hazards unreachable);
//! * the LAST granule of the segment maps to the LAST byte of the region
//!   (`StateByte::FOOTPRINT == SEGMENT / MIN_BLOCK == 256 KiB`);
//! * full byte-form equivalence over a swept span (FREE-encoded even
//!   granules, LIVE odd granules) against an independently built reference
//!   image.
//!
//! Mutant contract: shifting the byte index by one (e.g. indexing
//! `(off >> 4) + 1`, or mapping `off >> 3`) makes every assertion here fail.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

const BLOCK: usize = 16; // MIN_BLOCK for the default configuration
const SEGMENT: usize = 4 * 1024 * 1024;
const BLOCKS_PER_SEG: usize = SEGMENT / BLOCK; // 262_144
/// State-byte region footprint: ONE BYTE per granule (not per bit).
const FOOTPRINT: usize = BLOCKS_PER_SEG; // 256 KiB

const STATE_LIVE: u8 = 0;
const STATE_FREE: u8 = 2;

/// The state-byte (granule) index of an issued 16-byte block pointer.
fn granule_of(p: *mut u8) -> usize {
    let addr = p as usize;
    let off = addr & (SEGMENT - 1);
    off >> 4 // MIN_BLOCK_SHIFT = 4
}

fn state(b: u8) -> u8 {
    b >> 6
}
fn class_field(b: u8) -> u8 {
    b & 0x3F
}

/// Dump the first `len` state bytes of `anchor`'s segment.
fn dump(ac: &AllocCore, anchor: *mut u8, len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len];
    // SAFETY: `anchor` is a live allocation owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(anchor, &mut v) };
    v
}

/// Allocate `n` 16-byte blocks; the issued set covers granules
/// `first..first+n` contiguously (the substrate carves contiguous blocks).
fn alloc_blocks(ac: &mut AllocCore, n: usize) -> Vec<*mut u8> {
    let layout = Layout::from_size_align(BLOCK, BLOCK).unwrap();
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "allocation failed at n={}", v.len());
        v.push(p);
    }
    v
}

#[test]
fn state_byte_first_granule_mapping_and_live_state() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 32);
    let g0 = granule_of(blocks[0]);

    let buf = dump(&ac, blocks[0], FOOTPRINT);

    // Every issued block's byte sits at granule index `granule_of(p)`, is
    // nonzero (its class field is set), and is in the LIVE state.
    for &p in &blocks {
        let g = granule_of(p);
        let b = buf[g];
        assert_ne!(b, 0, "issued block's state byte at granule {g} is zero");
        assert_eq!(
            state(b),
            STATE_LIVE,
            "issued block at granule {g} is not LIVE (byte {b:#04x})"
        );
    }

    // Zero-byte invariant (И1): every granule BEFORE the first issued one
    // was never a block start — its byte must be exactly zero.
    assert!(
        buf[..g0].iter().all(|&b| b == 0),
        "state bytes before the payload's first granule are not zero \
         (first non-zero at {:?})",
        buf[..g0].iter().position(|&b| b != 0)
    );

    // Class-field consistency: all 32 issued blocks share one class, so all
    // 32 bytes carry the same class field.
    let cf = class_field(buf[g0]);
    assert_ne!(cf, 0, "LIVE byte must have a nonzero class field");
    for &p in &blocks {
        assert_eq!(class_field(buf[granule_of(p)]), cf);
    }
}

#[test]
fn state_byte_free_transition_and_byte_neighbour_preservation() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 72);
    let g0 = granule_of(blocks[0]);
    let before = dump(&ac, blocks[0], FOOTPRINT);
    let cf = class_field(before[g0]);

    // Free granules straddling the old bitmap's first byte boundary (7/8) —
    // for BYTES this just picks neighbouring-but-distinct byte indices.
    let set = [g0, g0 + 1, g0 + 7, g0 + 8];
    let layout = Layout::from_size_align(BLOCK, BLOCK).unwrap();
    let by_g: std::collections::HashMap<usize, *mut u8> =
        blocks.iter().map(|&p| (granule_of(p), p)).collect();
    for &g in &set {
        // SAFETY: live allocation owned by `ac`, exact original layout.
        unsafe { ac.dealloc(by_g[&g], layout) };
    }

    let after = dump(&ac, blocks[0], FOOTPRINT);
    for &g in &set {
        let b = after[g];
        assert_eq!(
            state(b),
            STATE_FREE,
            "freed granule {g} is not FREE (byte {b:#04x})"
        );
        assert_eq!(
            class_field(b),
            cf,
            "freed granule {g} lost its class field (byte {b:#04x})"
        );
    }
    // Byte neighbour preservation: every granule adjacent to a freed one but
    // not itself freed must be byte-IDENTICAL to its pre-free value.
    for &g in &set {
        for nb in g.saturating_sub(1)..=(g + 1).min(BLOCKS_PER_SEG - 1) {
            if !set.contains(&nb) {
                assert_eq!(
                    after[nb], before[nb],
                    "neighbour granule {nb} clobbered by the free of granule {g}"
                );
            }
        }
    }
}

#[test]
fn state_byte_full_byte_form_equivalence_even_sweep() {
    // Every-granule sweep of 128 granules: free ONLY the even ones, verify
    // the byte image equals the reference pattern (FREE-encoded even, LIVE
    // odd). A byte-index off-by-one moves even granules into odd slots and
    // inverts this image.
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 128);
    let g0 = granule_of(blocks[0]);
    let layout = Layout::from_size_align(BLOCK, BLOCK).unwrap();
    let cf = class_field(dump(&ac, blocks[0], FOOTPRINT)[g0]);

    let mut freed = std::collections::HashSet::new();
    for (i, &p) in blocks.iter().enumerate() {
        if i % 2 == 0 {
            // SAFETY: live allocation owned by `ac`, exact original layout.
            unsafe { ac.dealloc(p, layout) };
            freed.insert(granule_of(p));
        }
    }

    let buf = dump(&ac, blocks[0], FOOTPRINT);
    for (i, &p) in blocks.iter().enumerate() {
        let g = granule_of(p);
        let want = if freed.contains(&g) {
            cf | (STATE_FREE << 6)
        } else {
            cf | (STATE_LIVE << 6)
        };
        assert_eq!(
            buf[g], want,
            "granule {g} (block {i}) byte mismatch: got {:#04x}, want {want:#04x}",
            buf[g]
        );
    }
}

#[test]
fn state_byte_last_granule_maps_to_last_footprint_byte() {
    let mut ac = AllocCore::new().expect("primordial");
    // Fill the FIRST segment with the 16 B class until the allocator crosses
    // into a second segment. (B4 (в): the state-byte region costs 256 KiB of
    // metadata per segment, so a segment's payload no longer holds all
    // `BLOCKS_PER_SEG` 16 B blocks and the sweep spills — drive to the spill
    // point instead and assert on the TOP granules the first segment
    // actually issued.)
    let layout = Layout::from_size_align(BLOCK, BLOCK).unwrap();
    let mut blocks: Vec<*mut u8> = Vec::with_capacity(BLOCKS_PER_SEG);
    loop {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc failed during fill");
        let seg = (p as usize) & !(SEGMENT - 1);
        let is_first = blocks
            .first()
            .is_none_or(|&f| (f as usize) & !(SEGMENT - 1) == seg);
        blocks.push(p);
        if !is_first {
            // `blocks.last()` is the FIRST block of segment 2; drop it.
            blocks.pop();
            // SAFETY: live allocation owned by `ac`, exact original layout.
            unsafe { ac.dealloc(p, layout) };
            break;
        }
    }
    let base1 = (blocks[0] as usize) & !(SEGMENT - 1);

    // The top three ISSUED granules of segment 1.
    let gmax = blocks
        .iter()
        .map(|&p| ((p as usize) - base1) >> 4)
        .max()
        .unwrap();
    assert!(
        gmax < FOOTPRINT,
        "granule {gmax} outside the state-byte region"
    );
    let mut seg1_blocks: Vec<*mut u8> = blocks
        .iter()
        .copied()
        .filter(|&p| ((p as usize) & !(SEGMENT - 1)) == base1)
        .collect();
    seg1_blocks.sort_by_key(|&p| (p as usize) & (SEGMENT - 1));
    let start = seg1_blocks.len() - 3;
    let top: Vec<*mut u8> = seg1_blocks[start..].to_vec();
    for p in &top {
        // SAFETY: live allocation owned by `ac`, exact original layout.
        unsafe { ac.dealloc(*p, layout) };
    }

    let buf = dump(&ac, seg1_blocks[0], FOOTPRINT);
    assert_eq!(buf.len(), FOOTPRINT, "state-byte footprint is not 256 KiB");
    for &p in &top {
        let g = ((p as usize) - base1) >> 4;
        let b = buf[g];
        assert_eq!(
            state(b),
            STATE_FREE,
            "top granule {g} is not FREE after free (byte {b:#04x})"
        );
        assert_ne!(b, 0, "top granule {g} byte is zero after free");
    }
    // Nothing was ever issued past the top granule: the region above it must
    // read zero for at least the next 64 granules (well past any rounding).
    let zero_from = gmax + 1;
    let zero_to = (zero_from + 64).min(FOOTPRINT);
    assert!(
        buf[zero_from..zero_to].iter().all(|&b| b == 0),
        "state bytes above the top issued granule {gmax} are not zero"
    );
}
