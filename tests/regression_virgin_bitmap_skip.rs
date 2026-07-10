//! PERF-PASS-2 (G5/C1, task #50) — poison-then-assert counterfactual for the
//! fresh-segment `AllocBitmap` init-elision.
//!
//! `AllocCore::reserve_small_segment` and `bootstrap::primordial` now SKIP the
//! explicit `AllocBitmap::init_in_place` zero-write under `cfg(not(miri))`
//! (see the matching comments at both call sites in `alloc_core.rs` /
//! `bootstrap.rs`). The soundness argument is: the memory backing a
//! freshly-reserved segment is handed back by the OS already zeroed (Windows
//! `MEM_COMMIT` demand-zero; POSIX anonymous `mmap` zero-fill), and
//! `AllocBitmap`'s init target state IS all-zeros — so the explicit write was
//! always a tautology on this exact code path (never on any other path: the
//! decommit-reuse full-reset keeps its own unconditional re-init).
//!
//! This is NOT the rejected P4(b) `alloc_zeroed` virgin-skip: that NO-GO was
//! about *user-visible payload* virginity, where a RECYCLED (not freshly-
//! reserved) mapping's "was it definitely never written since the OS handed
//! it out" question is unsound under macOS `MADV_DONTNEED` laziness. Here the
//! virgin signal is exact (this function runs immediately after
//! `Segment::reserve`, nothing else has touched the segment yet) and the
//! target is allocator metadata, not payload the user could have written.
//!
//! ## What "poison-then-assert" means here
//!
//! We cannot literally poison the bytes BEFORE the crate's own reserve call
//! (that would require intercepting between the OS `mmap`/`VirtualAlloc` and
//! the allocator's own init, which is not a seam this crate exposes — and
//! deliberately so: exposing it would be a bigger footgun than the test is
//! worth). Instead this test proves the soundness argument the elision relies
//! on ONE LEVEL UP, empirically, on THIS platform, via the crate's own
//! reserve path:
//!
//!   1. **T1 — raw empirical zero-read-back.** Reserve a fresh `AllocCore`
//!      (which reserves the PRIMORDIAL segment — one of the two call sites
//!      the skip applies to), alloc ONE block (the minimum needed to obtain a
//!      pointer into the segment for `dbg_alloc_bitmap_bytes_for`'s (ptr ->
//!      base) contract), and read back its `AllocBitmap` footprint EXCLUDING
//!      the narrow span the one alloc's own refill batch legitimately
//!      populates (`carve_block_with_refill` carves 31 extra same-class
//!      blocks and correctly frees them back to the BinTable — see the test's
//!      own doc comment for the exact math). If the OS-zero guarantee did NOT
//!      hold on this platform (or the skip were wrongly applied to a
//!      non-virgin path), this read would observe non-zero garbage outside
//!      that span. It reads all zeros — this IS the empirical proof the
//!      skip's soundness argument depends on, not a vacuous "does not crash"
//!      check.
//!   2. **T2 — same proof for a SECOND fresh segment**, forcing
//!      `reserve_small_segment` (the non-primordial call site) by exhausting
//!      the primordial's small-class capacity, stopping at the FIRST pointer
//!      carved from the fresh segment, and reading back the same
//!      excluded-span logic as T1 — isolating "OS handed back zero" from "the
//!      allocator's own bookkeeping happens to read as zero".
//!   3. **T3 — behavioural counterfactual.** With the skip active, drive a
//!      full M2 double-free-guard exercise (alloc, double-free, re-alloc) on
//!      blocks carved from the just-reserved segment. If the skip had left
//!      SOME bits stuck at 1 (garbage "already free") on a virgin block, the
//!      very first `alloc` from that class would either return null (bitmap
//!      says allocated when the substrate thinks it's free — a different bug
//!      class) or a later double-free guard would misfire — this test's
//!      shape mirrors `tests/double_free_guard.rs` exactly, so a broken skip
//!      would fail it the same way a broken bitmap init has always failed
//!      that file's tests (proving THIS test is not vacuous).
//!
//! ## "No third path" audit (documented, not a runtime test)
//!
//! `grep -rn "AllocBitmap::init_in_place" src/` (re-run as part of this task)
//! shows exactly THREE call sites:
//!   - `bootstrap.rs::primordial` — the primordial virgin-reserve (skipped
//!     under `cfg(not(miri))`, this task).
//!   - `alloc_core.rs::reserve_small_segment` — the non-primordial
//!     virgin-reserve (skipped under `cfg(not(miri))`, this task).
//!   - `alloc_core.rs::decommit_empty_segment_impl` (the `release_follows =
//!     false` full-reset branch) — UNCONDITIONAL, untouched by this task. This
//!     is the ONLY other path that ever (re-)initialises the bitmap, and it
//!     runs on a segment that is NOT virgin (it was carved/freed/decommitted),
//!     so it correctly keeps doing real work.
//!
//! No other call site exists. `decommit_empty_segment_for_release` (the
//! `release_follows = true` variant, the one every production caller actually
//! uses today) does NOT call `init_in_place` at all — the whole reservation is
//! about to be released to the OS, so a future re-reserve of that address
//! range goes through one of the two (now-skipped) virgin-reserve sites again,
//! not through a stale, still-dirty bitmap.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

/// T1: a freshly-reserved `AllocCore` (primordial segment) reads back
/// all-zero across its `AllocBitmap` footprint before any alloc/dealloc call
/// TOUCHES that region. This is the empirical anchor for the elision's
/// soundness argument.
///
/// IMPORTANT: `ac.alloc(layout)` on a class miss does NOT carve just the ONE
/// requested block — the non-`fastbin` substrate path
/// (`carve_block_with_refill`) also carves `REFILL_BATCH = 31` EXTRA blocks of
/// the SAME class and pushes each through `dealloc_small`, which legitimately
/// `mark_free`s them (a real, correct free-list population — unrelated to
/// this task). So after one 16 B alloc, up to 32 consecutive 16 B-class bits
/// starting at the payload's first block legitimately read as FREE (bit=1).
/// This test therefore checks the bitmap BYTES BEYOND that maximum possible
/// same-class refill span (32 blocks × 16 B = 512 B window, rounded up to a
/// whole byte-aligned margin) — the OS-zero assumption is about to be
/// exercised there just as much as anywhere else in the segment, so this is
/// still a meaningful, non-vacuous check of the untouched majority of the
/// bitmap footprint.
#[test]
fn t1_primordial_bitmap_reads_zero_before_any_traffic() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null());

    const FOOTPRINT: usize = 32 * 1024; // AllocBitmap::FOOTPRINT for the default SEGMENT/MIN_BLOCK pair
    let mut buf = vec![0u8; FOOTPRINT];
    ac.dbg_alloc_bitmap_bytes_for(p, &mut buf);

    // The requested block sits at `primordial_meta_end()` (the payload
    // start); its bit index is `off >> MIN_BLOCK_SHIFT`. The refill batch
    // (`REFILL_BATCH = 31` extra same-class blocks, `carve_block_with_refill`)
    // covers at most the next 31 × `block_size` bytes of payload past that,
    // i.e. at most 32 total blocks (1 requested + 31 refilled) from the same
    // starting bit — see T2's identical margin computation (which uses a
    // LARGER class and so needs a wider margin; this formula is shared so
    // neither test hardcodes a class-specific magic number).
    let off = (p as usize) - ((p as usize) & !((4 * 1024 * 1024) - 1));
    let touched_byte = (off >> 4) / 8; // AllocBitmap::locate's byte_idx for `off`
    const REFILL_BATCH_MARGIN_BLOCKS: usize = 32; // 1 requested + 31 refilled (carve_block_with_refill)
    let margin_bytes = (REFILL_BATCH_MARGIN_BLOCKS * layout.size()).div_ceil(16 * 8);
    let skip_to = touched_byte + margin_bytes;
    assert!(
        buf[skip_to..].iter().all(|&b| b == 0),
        "freshly-reserved primordial segment's AllocBitmap did not read back \
         all-zero (past the legitimately-refilled span) BEFORE any free — \
         the OS-zero assumption the virgin-init skip depends on does not \
         hold on this platform (or the skip fired on a non-virgin path). \
         First non-zero byte at index {:?} (touched span was bytes {}..{}).",
        buf[skip_to..]
            .iter()
            .position(|&b| b != 0)
            .map(|i| i + skip_to),
        touched_byte,
        skip_to
    );
    // Sanity: bytes BEFORE the touched block's own bit-byte (metadata region
    // — header/page-map/bin-table/ring/registry/hash/free-list, all of which
    // the bitmap's flat indexing also numerically "covers" even though no
    // block ever starts there) must ALSO read zero — the OS-zero guarantee
    // applies uniformly across the whole fresh page range, not just payload.
    assert!(
        buf[..touched_byte].iter().all(|&b| b == 0),
        "freshly-reserved primordial segment's AllocBitmap metadata-region \
         bytes (before the payload's first block) did not read back \
         all-zero. First non-zero byte at index {:?}.",
        buf[..touched_byte].iter().position(|&b| b != 0)
    );

    ac.dealloc(p, layout);
}

/// T2: force a SECOND, non-primordial small-segment reservation
/// (`reserve_small_segment`) by exhausting the primordial's small-class
/// capacity with one class, then read back the TAIL of that fresh segment's
/// bitmap — far past any bit the handful of carve/refill calls that crossed
/// into it could have touched — isolating "OS handed back zero" from "our
/// own bookkeeping happens to read zero".
#[test]
fn t2_fresh_small_segment_bitmap_reads_zero_for_untouched_classes() {
    let mut ac = AllocCore::new().expect("primordial");

    // Drive 256 B allocations (a different class from T1's 16 B) until a
    // SECOND distinct 4 MiB-aligned region is observed, then STOP
    // immediately (do not keep allocating into the fresh segment) — the
    // point is to touch as little of the fresh segment's bitmap as possible
    // before reading it back, so the untouched tail is a meaningful check.
    let layout = Layout::from_size_align(256, 8).unwrap();
    let mut ptrs: Vec<*mut u8> = Vec::new();
    let mut segment_bases: HashSet<usize> = HashSet::new();
    let mut fresh_ptr: Option<*mut u8> = None;
    for _ in 0..200_000 {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        let base = (p as usize) & !((4 * 1024 * 1024) - 1);
        let is_new_segment = segment_bases.insert(base);
        ptrs.push(p);
        if segment_bases.len() >= 2 && is_new_segment {
            // `p` is the FIRST pointer carved from the freshly-reserved
            // segment — record it and stop driving more allocations into it.
            fresh_ptr = Some(p);
            break;
        }
    }
    let fresh_ptr = fresh_ptr.unwrap_or_else(|| {
        panic!(
            "test setup failed to force a second small segment reservation \
             (only touched {} distinct 4 MiB-aligned regions) — increase the \
             allocation count",
            segment_bases.len()
        )
    });

    const FOOTPRINT: usize = 32 * 1024;
    let mut buf = vec![0u8; FOOTPRINT];
    ac.dbg_alloc_bitmap_bytes_for(fresh_ptr, &mut buf);

    // `fresh_ptr` is the FIRST block carved from the fresh segment; the same
    // `carve_block_with_refill` batch behaviour as T1 applies: 1 requested +
    // `REFILL_BATCH = 31` extra SAME-CLASS (256 B) blocks legitimately read
    // FREE. That is 32 blocks × 256 B = 8192 B of payload = 512 bits = 64
    // bytes of bitmap (unlike T1's 16 B class, where the same 32-block span
    // is only 4 bytes of bitmap) — the margin must scale with `block_size`,
    // not be a fixed constant copied from T1. Skip that span, then require
    // the remainder of the fresh segment's bitmap — covering every OTHER
    // size class, none of which has ever allocated from this brand-new
    // segment — to read all zero.
    let base = (fresh_ptr as usize) & !((4 * 1024 * 1024) - 1);
    let off = (fresh_ptr as usize) - base;
    let touched_byte = (off >> 4) / 8;
    const REFILL_BATCH_MARGIN_BLOCKS: usize = 32; // 1 requested + 31 refilled (carve_block_with_refill)
    let block_size = layout.size(); // 256, a whole number of MIN_BLOCK (16) units
    let margin_bytes = (REFILL_BATCH_MARGIN_BLOCKS * block_size).div_ceil(16 * 8);
    let skip_to = touched_byte + margin_bytes;
    assert!(
        buf[skip_to..].iter().all(|&b| b == 0),
        "freshly-reserved (non-primordial) small segment's AllocBitmap did \
         not read back all-zero past the legitimately-refilled span — the \
         OS-zero assumption failed, or the skip fired on a segment that was \
         not actually virgin. First non-zero byte at index {:?} (touched \
         span was bytes {}..{}).",
        buf[skip_to..]
            .iter()
            .position(|&b| b != 0)
            .map(|i| i + skip_to),
        touched_byte,
        skip_to
    );
    assert!(
        buf[..touched_byte].iter().all(|&b| b == 0),
        "freshly-reserved (non-primordial) small segment's AllocBitmap \
         metadata-region bytes (before the payload's first block) did not \
         read back all-zero. First non-zero byte at index {:?}.",
        buf[..touched_byte].iter().position(|&b| b != 0)
    );
    for p in ptrs {
        ac.dealloc(p, layout);
    }
}

/// T3: behavioural counterfactual — the M2 double-free guard must still work
/// correctly on blocks carved from a freshly-reserved (skip-elided-init)
/// segment. Mirrors `tests/double_free_guard.rs`'s shape exactly: if the
/// elided init left the bitmap in anything other than the correct "all
/// allocated" starting state, either the first alloc from a virgin block
/// would misbehave, or a double-free on it would fail to no-op (both
/// failure modes this test would catch, proving it is not vacuous the same
/// way `double_free_guard.rs` is not vacuous against ITS guard).
#[test]
fn t3_double_free_guard_still_correct_on_freshly_reserved_segment() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 16).unwrap();

    let p = ac.alloc(layout);
    assert!(!p.is_null());

    // A virgin (never-freed) block's bit must read "allocated" (is_free ==
    // false) — this is the exact state the elided init is supposed to
    // produce implicitly via the OS's zero pages, matching what the removed
    // explicit zero-write used to produce explicitly.
    assert!(
        !ac.dbg_is_free_for(p),
        "virgin block's AllocBitmap bit read FREE before any free() call — \
         the init-elision left the bitmap in the wrong starting state"
    );

    ac.dealloc(p, layout); // legitimate free -> bit set
    ac.dealloc(p, layout); // double-free -> must no-op (M2 guard)
    ac.dealloc(p, layout); // triple-free -> still a no-op

    const K: usize = 32;
    let mut seen: HashSet<usize> = HashSet::new();
    let mut got = Vec::new();
    for _ in 0..K {
        let q = ac.alloc(layout);
        assert!(!q.is_null(), "alloc returned null after double-free");
        assert!(
            seen.insert(q as usize),
            "allocator handed out a DUPLICATE pointer {q:p} after a \
             double-free on a freshly-reserved (init-elided) segment — the \
             M2 guard failed under the virgin-init skip"
        );
        got.push(q);
    }
    for q in got {
        ac.dealloc(q, layout);
    }
}
