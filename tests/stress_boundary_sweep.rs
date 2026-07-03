//! Task S2 (#166) — a DETERMINISTIC, EXHAUSTIVE, single-threaded boundary
//! sweep. The deterministic complement to the concurrent S1 stress
//! (`stress_concurrent_boundaries.rs`): where S1 throws randomized concurrent
//! pressure at the allocator, S2 walks a FIXED grid of every size seam crossed
//! with every valid power-of-two alignment plus a realloc matrix, so a bug that
//! a PRNG-driven test might miss on a given seed is hit every single run — and
//! the failing case is its own repro (the sweep order is fixed, so
//! "case (size=256, align=64)" fully identifies it; no seed needed).
//!
//! ## The contract (iron — identical to S1)
//!
//! This harness stays STRICTLY inside the legal `GlobalAlloc` envelope: every
//! allocation is freed EXACTLY ONCE with the SAME layout it was allocated with.
//! There is NO double-free, NO foreign pointer, NO use-after-free, NO
//! mismatched-layout free. It drives `AllocCore` directly (single-thread,
//! deterministic — no magazine/TLS layer needed); `AllocCore::alloc/dealloc/
//! realloc` are *safe* `&mut self` methods, so the only `unsafe` here is
//! touching the memory the allocator hands back (writing/reading the canary),
//! exactly as legitimate std code would. Contract *violations* (illegal
//! double-free / foreign free) are the caller's UB and are OUT of scope — the
//! M2 no-op guards for those are covered by `regression_magazine_oracles` and
//! friends; this test does NOT duplicate or trigger them.
//!
//! ## The goal
//!
//! Break the allocator's OWN invariants across an exhaustive size×align grid:
//!
//! 1. **CANARY** — after each successful alloc the ENTIRE requested `size`
//!    bytes are filled with a position-dependent pattern and read back before
//!    free. A mismatch means the block was too small (usable < size), aliased a
//!    live neighbour, or was corrupted — a real bug.
//! 2. **ALIGNMENT** — `(ptr as usize) % align == 0` on every alloc and realloc.
//! 3. **DISTINCTNESS** — a small rolling set of simultaneously-live blocks per
//!    (size, align) must never share a pointer.
//! 4. **REALLOC DATA SURVIVAL** — after a realloc the first `min(old, new)`
//!    bytes must survive verbatim (the canary keyed on the OLD block).
//!
//! ## Fast + deterministic
//!
//! No PRNG, no threads, no env knobs — a fixed sweep. Bounded so the total
//! alloc/free count stays in the low tens of thousands; the whole file finishes
//! well under ~1 s in the normal suite.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::{AllocCore, SegmentLayout};

const SEGMENT: usize = SegmentLayout::SEGMENT; // 4 MiB
const SMALL_MAX: usize = SegmentLayout::SMALL_MAX; // ~253 KiB (top small class)

// ── position-dependent canary (same scheme as S1) ────────────────────────────

/// Per-allocation canary base word, derived from (ptr addr, a per-op tag). Two
/// live blocks (different addr / tag) get different bases, so if one aliases the
/// other the read-back detects the clobber.
#[inline]
fn canary_base(addr: usize, tag: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    h ^= addr as u64;
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= tag;
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= h >> 29;
    h.wrapping_mul(0xff51_afd7_ed55_8ccd)
}

#[inline]
fn canary_word(base: u64, off: usize) -> u64 {
    base ^ (off as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[inline]
fn canary_tail_byte(base: u64, off: usize) -> u8 {
    let word_off = off & !7usize;
    let w = canary_word(base, word_off);
    (w >> (8 * (off - word_off))) as u8
}

/// Blocks up to this size are canary-covered over their FULL length. Larger
/// (Large/huge) blocks are covered by a bounded head+tail window instead: the
/// full-fill of a multi-MiB block dominates the sweep's runtime under the
/// unoptimized test profile while adding no new detection (the head catches a
/// too-small usable size / a base-address aliasing; the tail catches an
/// off-by-a-page short block / a tail overlap — the two failure shapes a Large
/// block can have). Every SMALL class (block_size <= SMALL_MAX) is still filled
/// end to end, so the exhaustive small-seam coverage is unchanged.
const FULL_CANARY_MAX: usize = SMALL_MAX;

/// The window covered at each end of a large block (a couple of pages). A
/// too-small usable size shows up as a fault/mismatch within the first page;
/// a tail-short block within the last.
const LARGE_WINDOW: usize = 8192;

/// The set of byte ranges the canary covers for a block of `size` bytes: the
/// whole thing when small, else a head + tail window (which may overlap into a
/// single range for a size just over the threshold — handled by the caller
/// clamping to `[0, size)`).
#[inline]
fn canary_ranges(size: usize) -> [(usize, usize); 2] {
    if size <= FULL_CANARY_MAX {
        [(0, size), (0, 0)]
    } else {
        let tail_start = size - LARGE_WINDOW;
        [(0, LARGE_WINDOW), (tail_start, size)]
    }
}

/// Fill the canary-covered byte ranges of a `size`-byte block at `ptr` with the
/// pattern for `base` (the whole block when small; a head+tail window when
/// Large — see [`canary_ranges`]).
///
/// # Safety
/// `ptr` must own at least `size` writable bytes and be 8-aligned (every block
/// here has align >= 8).
unsafe fn write_canary(ptr: *mut u8, size: usize, base: u64) {
    for (lo, hi) in canary_ranges(size) {
        if lo == hi {
            continue;
        }
        // Word-align the u64 stride: `ptr` is 8-aligned, so only offsets that
        // are multiples of 8 may be cast to `u64`. Fill the ragged bytes below
        // the first 8-boundary, the whole words, then the ragged tail.
        let word_lo = (lo + 7) & !7usize;
        let word_hi = hi & !7usize;
        let mut off = lo;
        while off < word_lo.min(hi) {
            ptr.add(off).write(canary_tail_byte(base, off));
            off += 1;
        }
        while off < word_hi {
            ptr.add(off).cast::<u64>().write(canary_word(base, off));
            off += 8;
        }
        while off < hi {
            ptr.add(off).write(canary_tail_byte(base, off));
            off += 1;
        }
    }
}

/// Read the canary back over `size` bytes; panic (with a deterministic,
/// self-describing case label) on any mismatch.
///
/// # Safety
/// `ptr` must be the same live block `write_canary(base)` was called on.
unsafe fn verify_canary(ptr: *mut u8, size: usize, base: u64, case: &str) {
    for (lo, hi) in canary_ranges(size) {
        if lo == hi {
            continue;
        }
        let word_lo = (lo + 7) & !7usize;
        let word_hi = hi & !7usize;
        let mut off = lo;
        while off < word_lo.min(hi) {
            let got = ptr.add(off).read();
            let want = canary_tail_byte(base, off);
            assert!(
                got == want,
                "CANARY CORRUPTION at byte {off}/{size}: got {got:#04x}, want \
                 {want:#04x}. CASE: {case}",
            );
            off += 1;
        }
        while off < word_hi {
            let got = ptr.add(off).cast::<u64>().read();
            let want = canary_word(base, off);
            assert!(
                got == want,
                "CANARY CORRUPTION at word off {off}/{size}: got {got:#018x}, \
                 want {want:#018x}. The block was too small (usable < size), \
                 aliased a live neighbour, or was corrupted. CASE: {case}",
            );
            off += 8;
        }
        while off < hi {
            let got = ptr.add(off).read();
            let want = canary_tail_byte(base, off);
            assert!(
                got == want,
                "CANARY CORRUPTION at tail byte {off}/{size}: got {got:#04x}, \
                 want {want:#04x}. CASE: {case}",
            );
            off += 1;
        }
    }
}

/// Verify that the surviving bytes of a realloc'd block still hold the OLD
/// block's canary (keyed on the old addr/tag). The ranges checked are exactly
/// the intersection of what the OLD canary was WRITTEN over
/// (`canary_ranges(old_size)`) with the surviving prefix `[0, survived)`, where
/// `survived = min(old_size, new_size)`. This is subtle for a Large SHRINK: the
/// old block's tail window `[old-8192, old)` lies BEYOND `survived` and was
/// truncated away by the realloc, so we must NOT check it (those bytes hold
/// whatever the smaller new block leaves there, not the canary) — the
/// intersection drops it automatically, leaving the head window as the survival
/// signal. On a GROW the old tail is inside `[0, survived)` and is verified.
///
/// # Safety
/// `ptr` owns at least `survived` bytes.
unsafe fn verify_survived(
    ptr: *mut u8,
    old_size: usize,
    survived: usize,
    old_base: u64,
    case: &str,
) {
    for (rlo, rhi) in canary_ranges(old_size) {
        // Intersect the written range with the surviving prefix [0, survived).
        let lo = rlo;
        let hi = rhi.min(survived);
        if lo >= hi {
            continue;
        }
        let word_lo = (lo + 7) & !7usize;
        let word_hi = hi & !7usize;
        let mut off = lo;
        while off < word_lo.min(hi) {
            let got = ptr.add(off).read();
            let want = canary_tail_byte(old_base, off);
            assert!(
                got == want,
                "REALLOC LOST DATA at byte {off}/{survived}: got {got:#04x}, \
                 want {want:#04x}. CASE: {case}",
            );
            off += 1;
        }
        while off < word_hi {
            let got = ptr.add(off).cast::<u64>().read();
            let want = canary_word(old_base, off);
            assert!(
                got == want,
                "REALLOC LOST DATA at word off {off}/{survived}: got \
                 {got:#018x}, want {want:#018x}. CASE: {case}",
            );
            off += 8;
        }
        while off < hi {
            let got = ptr.add(off).read();
            let want = canary_tail_byte(old_base, off);
            assert!(
                got == want,
                "REALLOC LOST DATA at tail byte {off}/{survived}: got \
                 {got:#04x}, want {want:#04x}. CASE: {case}",
            );
            off += 1;
        }
    }
}

// ── the size grid ─────────────────────────────────────────────────────────────

/// Build the exhaustive size seam list: for EACH of the 49 small size classes,
/// its exact `block_size` and `block_size ± 1` (the seams a class boundary can
/// straddle), plus `SMALL_MAX ± 1`, the large threshold, a couple of Large
/// (> `SMALL_MAX`) sizes and a couple of huge (multi-segment) sizes. The set is
/// deduplicated and sorted so the sweep order is fixed and reportable.
fn size_grid() -> Vec<usize> {
    let mut sizes: HashSet<usize> = HashSet::new();
    for &bs in SegmentLayout::SIZE_CLASS_TABLE {
        sizes.insert(bs);
        sizes.insert(bs + 1);
        if bs > 1 {
            sizes.insert(bs - 1);
        }
    }
    // MIN_BLOCK floor and the page-aligned classes are already in the table
    // (16, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384), so their ±1 seams
    // are covered by the loop above. Add the explicit boundary values:
    sizes.insert(1); // clamps up to MIN_BLOCK — the tiniest non-zero request
    sizes.insert(SMALL_MAX - 1);
    sizes.insert(SMALL_MAX);
    sizes.insert(SMALL_MAX + 1); // first Large size
                                 // A couple of Large (single-segment) sizes.
    sizes.insert(SMALL_MAX + 4096);
    sizes.insert(SEGMENT / 2);
    sizes.insert(SEGMENT - 4096);
    // A couple of huge (multi-segment) sizes.
    sizes.insert(SEGMENT + 1);
    sizes.insert(2 * SEGMENT + 4096);
    let mut v: Vec<usize> = sizes.into_iter().filter(|&s| s > 0).collect();
    v.sort_unstable();
    v
}

/// Alignment seams: every power of two from 8 up to 65536. The high end
/// (32768, 65536) exercises the `align > SMALL_ALIGN_MAX` divisibility-walk and
/// the over-align → dedicated-segment Large path; `SEGMENT` itself (4 MiB) is
/// handled by the dedicated `over_align_segment_returns_null` test, not the
/// main sweep grid, since it is a legitimate-null case.
const ALIGN_GRID: &[usize] = &[
    8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
];

/// How many blocks of a given (size, align) are kept simultaneously live to
/// check intra-set distinctness. Kept tiny so over-aligned Large blocks (a whole
/// 4 MiB segment each) never pile up.
const LIVE_PER_CELL: usize = 3;

// ── sweep 1: every size seam × every valid align ─────────────────────────────

/// The exhaustive alloc grid. For each valid (size, align):
///   * alloc → assert non-null (the large/huge path can legitimately fail under
///     memory pressure — skip a genuine null, don't deref),
///   * assert `ptr % align == 0`,
///   * canary-fill the FULL `size`, read it back,
///   * keep a small rolling live set to assert intra-cell distinctness (a
///     second live alloc of the same layout must be a DIFFERENT pointer),
///   * free each exactly once with its own layout.
#[test]
fn sweep1_size_align_grid() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    let sizes = size_grid();

    let mut n_cases: u64 = 0;
    let mut n_allocs: u64 = 0;
    let mut tag: u64 = 0;

    for &size in &sizes {
        for &align in ALIGN_GRID {
            let layout = match Layout::from_size_align(size, align) {
                Ok(l) => l,
                Err(_) => continue, // (size, align) is not a valid Layout — legal skip.
            };
            n_cases += 1;

            // A rolling set of live blocks of THIS exact layout.
            let mut live: Vec<(*mut u8, u64)> = Vec::with_capacity(LIVE_PER_CELL);
            let mut live_ptrs: HashSet<usize> = HashSet::with_capacity(LIVE_PER_CELL);

            for _ in 0..LIVE_PER_CELL {
                let ptr = core.alloc(layout);
                let case = format!("size={size}, align={align}");
                if ptr.is_null() {
                    // A legitimate large/huge allocation failure under memory
                    // pressure. Do NOT deref; stop growing this cell's live set.
                    break;
                }
                n_allocs += 1;
                let addr = ptr as usize;
                assert!(
                    addr.is_multiple_of(align),
                    "ALLOC MISALIGNED: ptr={addr:#x} not a multiple of align. CASE: {case}",
                );
                assert!(
                    live_ptrs.insert(addr),
                    "ALLOC ALIASED a simultaneously-live block: ptr={addr:#x}. Two \
                     live allocations of the same layout share memory. CASE: {case}",
                );
                tag = tag.wrapping_add(1);
                let base = canary_base(addr, tag);
                // SAFETY: `ptr` owns `size` writable bytes (usable >= size).
                unsafe { write_canary(ptr, size, base) };
                live.push((ptr, base));
            }

            // Verify every live block's canary, then free it once, then re-alloc
            // the same layout and confirm reuse is aligned + usable.
            for &(ptr, base) in &live {
                let case = format!("size={size}, align={align}");
                // SAFETY: `ptr` is a live block we filled with `base`.
                unsafe { verify_canary(ptr, size, base, &case) };
            }
            for (ptr, _) in live.drain(..) {
                core.dealloc(ptr, layout);
                live_ptrs.remove(&(ptr as usize));
            }

            // Re-alloc the same layout after the frees: reuse returning a
            // previously-live pointer is fine now (all freed); just re-check
            // alignment + usability.
            let ptr = core.alloc(layout);
            if !ptr.is_null() {
                n_allocs += 1;
                let addr = ptr as usize;
                let case = format!("size={size}, align={align} (reuse)");
                assert!(
                    addr.is_multiple_of(align),
                    "REALLOC-REUSE MISALIGNED: ptr={addr:#x}. CASE: {case}",
                );
                tag = tag.wrapping_add(1);
                let base = canary_base(addr, tag);
                // SAFETY: fresh live block of `size` bytes.
                unsafe {
                    write_canary(ptr, size, base);
                    verify_canary(ptr, size, base, &case);
                }
                core.dealloc(ptr, layout);
            }
        }
    }

    // Post-sweep sanity: the allocator still serves a fresh alloc.
    let l = Layout::from_size_align(64, 8).unwrap();
    let p = core.alloc(l);
    assert!(!p.is_null(), "allocator dead after sweep1");
    core.dealloc(p, l);

    eprintln!(
        "sweep1: {n_cases} valid (size,align) cases over {} sizes x {} aligns, \
         {n_allocs} allocs",
        sizes.len(),
        ALIGN_GRID.len(),
    );
}

// ── sweep 2: realloc matrix ──────────────────────────────────────────────────

/// A representative set of (from_size → to_size) transitions crossing class,
/// page, and small↔Large boundaries. Each pair exercises either the C2 in-place
/// fast path (same class), the alloc+copy+dealloc fallback (cross-class /
/// Large), or the cross-class-shrink correctness path. Data survival + alignment
/// are asserted on every step.
const REALLOC_PAIRS: &[(usize, usize)] = &[
    // Across the 256 seam (grow and shrink) — the exact-256 class (task #145).
    (240, 256),
    (256, 304),
    (128, 256),
    (256, 512),
    (256, 240),
    (304, 256),
    (512, 256),
    (256, 128),
    // Same-class grow (in-place fast path: 240 and 250 both classify to 256's
    // neighbours; 256→256-ish stays in class) and size-preserving.
    (256, 256),
    (300, 304),
    (16, 16),
    // Across page-aligned seams (512 / 1024 / 4096 classes, task B1).
    (512, 1024),
    (1024, 512),
    (4096, 8192),
    (8192, 4096),
    (500, 520),
    // small → Large and Large → small (alloc+copy+dealloc fallback both ways).
    (1024, SMALL_MAX + 4096),
    (SMALL_MAX + 4096, 1024),
    (200, SMALL_MAX + 1),
    (SMALL_MAX + 1, 200),
    // Large → larger Large and Large → huge (multi-segment).
    (SMALL_MAX + 1, SEGMENT / 2),
    (SEGMENT / 2, SMALL_MAX + 1),
    (SEGMENT / 2, SEGMENT + 4096),
    // Grow near a class top and shrink back across several classes.
    (16, 16384),
    (16384, 16),
];

/// The realloc matrix, run for a few representative alignments (8 = the common
/// case, plus 16/64/256 to exercise the align-preservation on the copy path and
/// the divisibility-walk classes). For each (from → to, align):
///   * alloc `from`, canary-fill it,
///   * realloc to `to`,
///   * assert the first `min(from, to)` bytes survived (canary keyed on OLD),
///   * assert the new pointer is still `align`-aligned,
///   * canary the whole new block (usability of the full `to`), then free once.
#[test]
fn sweep2_realloc_matrix() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    let aligns = [8usize, 16, 64, 256];

    let mut n_reallocs: u64 = 0;
    let mut tag: u64 = 0;

    for &(from, to) in REALLOC_PAIRS {
        for &align in &aligns {
            // Both endpoints must form a valid layout; align must not exceed the
            // size where it would over-align into a segment for this test's
            // intent (align <= 256 here, always valid for these sizes).
            let from_layout = match Layout::from_size_align(from, align) {
                Ok(l) => l,
                Err(_) => continue,
            };
            if Layout::from_size_align(to, align).is_err() {
                continue;
            }
            let case = format!("{from}->{to}, align={align}");

            let old = core.alloc(from_layout);
            if old.is_null() {
                continue; // legitimate large/huge failure — skip.
            }
            let old_addr = old as usize;
            assert!(
                old_addr.is_multiple_of(align),
                "REALLOC-SRC MISALIGNED: ptr={old_addr:#x}. CASE: {case}",
            );
            tag = tag.wrapping_add(1);
            let old_base = canary_base(old_addr, tag);
            // SAFETY: fresh live block of `from` bytes.
            unsafe { write_canary(old, from, old_base) };

            let np = core.realloc(old, from_layout, to);
            if np.is_null() {
                // realloc failed: the OLD block is still live and intact. Verify
                // then free it under the OLD layout (exactly once).
                // SAFETY: `old` is still the same live block.
                unsafe { verify_canary(old, from, old_base, &case) };
                core.dealloc(old, from_layout);
                continue;
            }
            n_reallocs += 1;

            // Data survival: first min(from, to) bytes keyed on the OLD block.
            let survived = from.min(to);
            // SAFETY: `np` owns at least `to` bytes; `survived <= to`.
            unsafe { verify_survived(np, from, survived, old_base, &case) };

            let np_addr = np as usize;
            assert!(
                np_addr.is_multiple_of(align),
                "REALLOC-DST MISALIGNED: ptr={np_addr:#x}. CASE: {case}",
            );

            // The full new block must be usable: re-canary all `to` bytes on the
            // new basis and read back.
            tag = tag.wrapping_add(1);
            let new_base = canary_base(np_addr, tag);
            let new_layout = Layout::from_size_align(to, align).unwrap();
            // SAFETY: `np` owns `to` writable bytes.
            unsafe {
                write_canary(np, to, new_base);
                verify_canary(np, to, new_base, &case);
            }
            // Free exactly once with the NEW layout (the GlobalAlloc contract).
            core.dealloc(np, new_layout);
        }
    }

    eprintln!(
        "sweep2: {n_reallocs} reallocs over {} pairs x 4 aligns",
        REALLOC_PAIRS.len()
    );
}

// ── sweep 3: legitimate edge cases from safe usage ───────────────────────────

/// `align >= SEGMENT` (4 MiB) must return null — a legal alloc-failure signal,
/// never a mis-aligned or mis-registered block (task #130). We assert THAT and
/// never dereference.
#[test]
fn over_align_segment_returns_null() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    for &align in &[SEGMENT, 2 * SEGMENT, 4 * SEGMENT] {
        // A small size with an align >= SEGMENT: the dedicated-segment large
        // path cannot honour it, so alloc must return null.
        for &size in &[16usize, 4096, SMALL_MAX + 1] {
            let layout = match Layout::from_size_align(size, align) {
                Ok(l) => l,
                Err(_) => continue,
            };
            let p = core.alloc(layout);
            assert!(
                p.is_null(),
                "align >= SEGMENT must return null (task #130), got {:#x} for \
                 size={size}, align={align}",
                p as usize,
            );
        }
    }
}

/// `align > SMALL_MAX` but `< SEGMENT` with a small size routes to a dedicated
/// over-aligned segment (valid). Assert the block is non-null, correctly
/// aligned, and usable over the full requested size.
#[test]
fn over_small_max_align_routes_to_segment() {
    let mut core = AllocCore::new().expect("AllocCore::new");
    // First power of two strictly above SMALL_MAX (258752) and below SEGMENT.
    let mut align = 8usize;
    while align <= SMALL_MAX {
        align <<= 1;
    }
    // align is now the smallest pow2 > SMALL_MAX (should be 262144, < SEGMENT).
    assert!(
        align < SEGMENT,
        "expected an over-SMALL_MAX align below SEGMENT"
    );

    let mut tag = 0u64;
    while align < SEGMENT {
        for &size in &[16usize, 4096] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let p = core.alloc(layout);
            let case = format!("size={size}, align={align} (over-SMALL_MAX)");
            assert!(
                !p.is_null(),
                "over-SMALL_MAX align should route to a segment. CASE: {case}"
            );
            let addr = p as usize;
            assert!(
                addr.is_multiple_of(align),
                "OVER-ALIGN MISALIGNED: ptr={addr:#x}. CASE: {case}",
            );
            tag += 1;
            let base = canary_base(addr, tag);
            // SAFETY: `p` owns `size` writable bytes.
            unsafe {
                write_canary(p, size, base);
                verify_canary(p, size, base, &case);
            }
            core.dealloc(p, layout);
        }
        align <<= 1;
    }
}
