//! N4 (word-wide bitmap access) proof-of-boundaries tests.
//!
//! The bitmap accessors moved from byte-wide to `u64`-word-wide loads/stores
//! (`SegmentBitmap::locate` now returns `(word = bit / 64, 1 << bit % 64)`).
//! The in-memory byte layout is REQUIRED to stay identical to the byte-wide
//! predecessor: bit `b` = byte `b >> 3`, bit `b & 7` (little-endian word
//! numbering). These tests read the raw bitmap bytes back through the
//! test-only dump hook (`dbg_alloc_bitmap_bytes_for`) and compare them to an
//! independent BYTE-form reference bit-set, so a word-shift bug (wrong 8-byte
//! group or wrong bit inside the word) fails immediately.
//!
//! Covered boundaries:
//! * bit 7/8  — the first byte→byte crossing inside word 0;
//! * bit 63/64 — the first word→word crossing (the whole point of N4);
//! * last bits of the LAST `u64` word of the segment footprint
//!   (262141..262143 = byte 32767 bits 5..7), in-footprint;
//! * full byte-form equivalence: every freed bit index is compared against a
//!   reference byte image built independently (byte `b >> 3`, mask
//!   `1 << (b & 7)`) over the whole touched span, plus explicit
//!   neighbour-preservation checks.
//!
//! Mutant contract: shifting the word index by one bit (e.g. `(bit >> 6) + 1`
//! or addressing byte `bit >> 3 & !7 + 8`) makes every assertion here fail.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

const BLOCK: usize = 16; // MIN_BLOCK for the default configuration
const SEGMENT: usize = 4 * 1024 * 1024;
const BLOCKS_PER_SEG: usize = SEGMENT / BLOCK; // 262_144
const FOOTPRINT: usize = BLOCKS_PER_SEG / 8; // 32_768 bitmap bytes

/// The bitmap bit index of an issued 16-byte block pointer.
fn bit_of(p: *mut u8) -> usize {
    let addr = p as usize;
    let off = addr & (SEGMENT - 1);
    off >> 4 // MIN_BLOCK_SHIFT = 4
}

/// Independent byte-form reference: bit `b` lives in byte `b >> 3`, bit
/// `b & 7` (little-endian byte numbering — exactly the pre-N4 layout).
fn reference_bytes(bits: &[usize], out: &mut [u8]) {
    for b in out.iter_mut() {
        *b = 0;
    }
    for &bit in bits {
        out[bit >> 3] |= 1u8 << (bit & 7);
    }
}

/// Allocate `n` 16-byte blocks; the issued set covers bitmap bits
/// `first_bit..first_bit+n` contiguously (the substrate carves contiguous
/// blocks; issue order is LIFO per refill batch, which does not matter here —
/// only the ADDRESS set does). Returns the pointers.
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

/// Free the blocks whose bit indices appear in `set` (they must all be part of
/// `blocks`), dump the raw AllocBitmap bytes and compare against the byte-form
/// reference across the span from the lowest to the highest chosen bit plus a
/// one-byte margin on each side. A free of a 16-byte block `mark_free`s
/// exactly its own bit; the substrate's own refill-batch frees all sit in the
/// first-carved prefix which we keep fully allocated, outside that span.
fn free_and_verify(ac: &mut AllocCore, blocks: &[*mut u8], mut set: Vec<usize>, label: &str) {
    set.sort_unstable();

    // Baseline: the substrate's own refill-batch population legitimately
    // `mark_free`s up to 31 same-class blocks per carve batch at ALLOC time,
    // so the bitmap is NOT all-zero before our chosen frees. Capture it and
    // require our frees to only ever ADD their own bits on top of it.
    let mut before = vec![0u8; FOOTPRINT];
    // SAFETY: `blocks[0]` is a live allocation owned by `ac`.
    unsafe { ac.dbg_alloc_bitmap_bytes_for(blocks[0], &mut before) };
    for &bit in &set {
        assert_eq!(
            before[bit >> 3] & (1u8 << (bit & 7)),
            0,
            "{label}: bit {bit} already free before the harness free"
        );
    }

    let layout = Layout::from_size_align(BLOCK, BLOCK).unwrap();
    // bit index -> pointer (issued blocks cover bits contiguously).
    let by_bit: std::collections::HashMap<usize, *mut u8> =
        blocks.iter().map(|&p| (bit_of(p), p)).collect();
    for &bit in &set {
        let p = by_bit[&bit];
        // SAFETY: live allocation owned by `ac`, exact original layout.
        unsafe { ac.dealloc(p, layout) };
    }

    let mut buf = vec![0u8; FOOTPRINT];
    // SAFETY: `blocks[0]` is a live allocation owned by `ac`.
    unsafe { ac.dbg_alloc_bitmap_bytes_for(blocks[0], &mut buf) };

    let mut want = vec![0u8; FOOTPRINT];
    reference_bytes(&set, &mut want);
    for i in 0..FOOTPRINT {
        want[i] |= before[i];
    }

    let lo = set[0];
    let hi = *set.last().unwrap();
    let lo_byte = (lo >> 3).saturating_sub(1);
    let hi_byte = ((hi >> 3) + 2).min(FOOTPRINT);
    for (i, (got, exp)) in buf[lo_byte..hi_byte]
        .iter()
        .zip(want[lo_byte..hi_byte].iter())
        .enumerate()
    {
        assert_eq!(
            got,
            exp,
            "{label}: bitmap byte {} mismatch (bit range {lo}..={hi}); \
             word-shift or RMW clobber",
            i + lo_byte
        );
    }

    // Neighbour preservation: every bit adjacent to a chosen bit but not
    // chosen itself must read 0.
    for &bit in &set {
        for nb in bit.saturating_sub(1)..=(bit + 1).min(BLOCKS_PER_SEG - 1) {
            if !set.contains(&nb) && before[nb >> 3] & (1u8 << (nb & 7)) == 0 {
                let byte = buf[nb >> 3];
                assert_eq!(
                    byte & (1 << (nb & 7)),
                    0,
                    "{label}: neighbour bit {nb} clobbered by set/clear of {bit}"
                );
            }
        }
    }
}

/// The first bitmap bit covered by issued payload blocks (the payload start
/// depends on the metadata geometry; the tests below anchor to it).
fn first_bit(blocks: &[*mut u8]) -> usize {
    blocks.iter().map(|&p| bit_of(p)).min().unwrap()
}

#[test]
fn n4_bit_7_8_byte_crossing() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 32);
    let b0 = first_bit(&blocks);
    // Bits b0 and b0+1 (payload block 0 is byte-aligned): they straddle byte
    // b0>>3 / byte (b0>>3)+1, both inside the same u64 word.
    // Bits b0+7 and b0+8 additionally straddle the word's FIRST byte
    // boundary (byte bit-grid positions 7/8).
    free_and_verify(&mut ac, &blocks, vec![b0, b0 + 1, b0 + 7, b0 + 8], "bit7/8");
}

#[test]
fn n4_bit_63_64_word_crossing() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 72);
    let b0 = first_bit(&blocks);
    // The first word->word crossing at or after b0: b1 = start of the next
    // u64 word; bits b1-1 (last of its word) and b1 (first of the next). A
    // one-bit word-index shift addresses the wrong word for exactly one.
    #[allow(clippy::manual_is_multiple_of)] // keep % form compatible with older toolchains
    let b1 = if b0 % 64 == 0 {
        b0 + 64
    } else {
        b0 + 64 - (b0 % 64)
    };
    free_and_verify(&mut ac, &blocks, vec![b1 - 1, b1], "bit63/64-style");
}

#[test]
fn n4_last_bits_of_last_word_in_footprint() {
    let mut ac = AllocCore::new().expect("primordial");
    let last = BLOCKS_PER_SEG - 1;
    // The 16-byte class spans the whole segment, so the carved payload covers
    // every bit of the footprint; the final three bits are the last bits of
    // the LAST u64 word (byte 32767 bits 5,6,7).
    let blocks = alloc_blocks(&mut ac, BLOCKS_PER_SEG);
    free_and_verify(
        &mut ac,
        &blocks,
        vec![last - 2, last - 1, last],
        "last-word",
    );
}

#[test]
fn n4_full_byte_form_equivalence_first_two_words() {
    // Every-bit sweep of bits 0..128: free ONLY the even bits, verify the byte
    // image equals the reference pattern (0x55 repeating). A word-index
    // off-by-one-bit moves odd bits into even slots and inverts this image.
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_blocks(&mut ac, 128);
    let b0 = first_bit(&blocks);
    let set: Vec<usize> = (b0..b0 + 128).step_by(2).collect();
    free_and_verify(&mut ac, &blocks, set, "even-sweep-0..128");
}
