//! R12-3 (P0-perf, EXPERIMENTAL A/B) — `exact-span-large` correctness tests.
//!
//! Without `exact-span-large`, `AllocCore::alloc_large` reserves a MINIMUM of
//! one whole `SEGMENT` (4 MiB) for every Large request, regardless of how
//! much is actually needed (a 260 KiB request pays for 4 MiB). With the
//! feature on, the physical `usable` span is `round_up(header + size, PAGE)`
//! instead of `round_up(header + size, SEGMENT)` — the segment's alignment
//! stays `SEGMENT` (so `segment_base_of_ptr`/`SegmentLayout::segment_base_of`
//! still resolves the correct base), only the physical byte count shrinks.
//!
//! This file exercises:
//!   (a) correctness at 5 control sizes under the feature — alloc, write,
//!       read-back, dealloc;
//!   (b) `segment_base_of_ptr` (via `SegmentLayout::segment_base_of`, the
//!       public equivalent) still resolves the correct SEGMENT-aligned base
//!       for an exact-span segment;
//!   (c) the physical `span_usable` really shrank below one whole `SEGMENT`
//!       for a sub-4-MiB request (the assertion that would catch a
//!       do-nothing / vacuous "fix");
//!   (d) large-cache interaction is not broken: alloc→dealloc→realloc at the
//!       same size reuses the cache, and OPT-G in-place realloc growth still
//!       works within the (now smaller) committed span;
//!   (e) WITHOUT the feature, behaviour round-trips identically to the
//!       pre-existing 4-MiB-rounded path (`span_usable` is always an exact
//!       multiple of `SEGMENT`).

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

const KIB: usize = 1024;
const SEGMENT: usize = SegmentLayout::SEGMENT;

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// The smallest control size: `SMALL_MAX` plus a small headroom margin.
/// R6-OPT-P0-3a note (mirrored from `regression_inplace_large_realloc.rs`'s
/// `shrink_large_does_not_pin`): `SMALL_MAX` is NOT a fixed ~253 KiB — it
/// varies by feature (`medium-classes` -> 1 MiB, `medium-classes-wide` ->
/// 1.75 MiB, both included in `--all-features`). A hardcoded literal like
/// `260 * KIB` silently stops classifying as Large once `SMALL_MAX` grows
/// past it, invalidating the whole test's premise without ever failing
/// loudly. Deriving from the constant keeps every control size genuinely
/// Large regardless of which small-class feature is active.
fn just_above_small_max() -> usize {
    SegmentLayout::SMALL_MAX + SegmentLayout::PAGE
}

/// The 5 control sizes from the R12-3 review, re-expressed as
/// SMALL_MAX-relative so they stay genuinely Large under every feature
/// combination (see `just_above_small_max`): comfortably above `SMALL_MAX`
/// so every one of these classifies as Large, and they straddle
/// sub-1-segment, ~1-segment, and multi-segment physical footprints.
fn control_sizes() -> [usize; 5] {
    let base = just_above_small_max();
    [
        base,
        base + 256 * KIB,
        base + SEGMENT / 4,
        base + SEGMENT,
        base + 4 * SEGMENT,
    ]
}

/// (a) Correctness at all 5 control sizes: alloc, write a distinguishing
/// byte pattern across the whole span, read it back, dealloc. Proves the
/// smaller physical reservation is still fully valid/writable memory for
/// its entire logical size — a too-small `usable` would fault or silently
/// clip long before this loop finishes.
#[test]
#[cfg(feature = "exact-span-large")]
fn exact_span_alloc_write_read_dealloc_all_control_sizes() {
    let mut ac = AllocCore::new().expect("primordial");
    for &size in &control_sizes() {
        let l = layout(size);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "OOM allocating {size} bytes under exact-span-large"
        );
        // SAFETY: `p` is valid for `size` bytes per the alloc contract; write
        // then read back a full-span pattern to prove the whole logical size
        // is real, accessible, correctly-committed memory.
        unsafe {
            p.write_bytes(0xA5, size);
            for &probe in &[0usize, size / 2, size - 1] {
                assert_eq!(
                    p.add(probe).read(),
                    0xA5,
                    "byte {probe} of a {size}-byte exact-span alloc did not read back"
                );
            }
        }
        // SAFETY (R6-MS-1/2): `p` is a live allocation from this `AllocCore`
        // made with `l`, freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }
}

/// (b) `segment_base_of_ptr` (public form: `SegmentLayout::segment_base_of`)
/// still resolves the correct SEGMENT-aligned base for an exact-span
/// segment, and `AllocCore::dbg_contains_base` confirms that base is a
/// registered segment. A regression here (e.g. a caller accidentally passing
/// a smaller `align` to `vmem::reserve_aligned`) would either misalign the
/// base or point `segment_base_of_ptr` at a foreign, unregistered base.
#[test]
#[cfg(feature = "exact-span-large")]
fn segment_base_of_ptr_resolves_correctly_for_exact_span() {
    let mut ac = AllocCore::new().expect("primordial");
    for &size in &control_sizes() {
        let l = layout(size);
        let p = ac.alloc(l);
        assert!(!p.is_null(), "OOM allocating {size} bytes");

        let base = SegmentLayout::segment_base_of(p as usize) as *mut u8;
        assert_eq!(
            base as usize % SEGMENT,
            0,
            "segment base must stay SEGMENT-aligned for size={size}"
        );
        assert!(
            ac.dbg_contains_base(p),
            "segment_base_of_ptr's result for a {size}-byte exact-span alloc \
             must be one of this AllocCore's registered segments"
        );

        // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
        unsafe { ac.dealloc(p, l) };
    }
}

/// (c) The physical `span_usable` genuinely shrank below one whole `SEGMENT`
/// for a request just above `SMALL_MAX` (well under 4 MiB in every feature
/// combination, since even `medium-classes-wide`'s 1.75 MiB `SMALL_MAX` is
/// less than half a `SEGMENT`) — the assertion that catches a vacuous "fix"
/// (i.e. code that compiles and passes (a)/(b) but still silently rounds up
/// to 4 MiB internally). Physically this should cost only single-digit-KiB
/// above the header+payload, nowhere near 4 MiB.
#[test]
#[cfg(feature = "exact-span-large")]
fn span_usable_shrinks_below_one_segment_for_small_large_request() {
    let mut ac = AllocCore::new().expect("primordial");
    let size = just_above_small_max();
    let l = layout(size);
    let p = ac.alloc(l);
    assert!(!p.is_null(), "OOM allocating {size} bytes");

    let span_usable = ac.dbg_span_usable_of(p);
    assert!(
        span_usable < SEGMENT,
        "just-above-SMALL_MAX large alloc's physical span_usable \
         ({span_usable} bytes) should be far below one whole SEGMENT \
         ({SEGMENT} bytes) under exact-span-large — got a value that looks \
         like the OLD round-up-to-4-MiB behaviour"
    );
    // Sanity upper/lower bound: must cover at least the requested payload,
    // and stay within a small page-rounding margin above it (header + one
    // page of slack is generous; this is nowhere near SEGMENT).
    assert!(
        span_usable >= size,
        "span_usable ({span_usable}) must cover the full requested size ({size})"
    );
    assert!(
        span_usable < size + 64 * KIB,
        "span_usable ({span_usable}) should be page-rounded close to the \
         request ({size}), not padded out to a whole segment"
    );

    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(p, l) };
}

/// (d) Large-cache interaction: alloc -> dealloc -> re-alloc the SAME size
/// must still hit the cache and produce valid, writable memory (mirrors
/// `tests/large_cache.rs`'s `alloc_dealloc_alloc_reuses_cached_large`, run
/// here specifically under `exact-span-large` to prove the smaller `usable`
/// does not desync the best-fit size-factor matching or the byte-budget
/// accounting).
#[test]
#[cfg(all(feature = "exact-span-large", feature = "alloc-decommit"))]
fn large_cache_reuse_works_with_exact_span() {
    let mut ac = AllocCore::new().expect("primordial");
    let size = just_above_small_max() + 256 * KIB;
    let l = layout(size);

    let p1 = ac.alloc(l);
    assert!(!p1.is_null());
    let span1 = ac.dbg_span_usable_of(p1);
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(p1, l) };

    // Cache should now hold one slot sized at span1 (< SEGMENT).
    let cached = ac.dbg_large_cache_slot_sizes();
    assert!(
        cached.contains(&Some(span1)),
        "the freed exact-span segment ({span1} bytes) should be cached, \
         not released to the OS"
    );

    let p2 = ac.alloc(l);
    assert!(!p2.is_null());
    // SAFETY: `p2` valid for `size` bytes; prove the recommit/reuse path
    // still yields fully writable memory.
    unsafe {
        p2.write_bytes(0x5A, size);
        assert_eq!(p2.read(), 0x5A);
        assert_eq!(p2.add(size - 1).read(), 0x5A);
    }
    // The reused segment's span_usable must be preserved verbatim (bug #134
    // discipline) — still span1, not recomputed/shrunk/grown.
    assert_eq!(
        ac.dbg_span_usable_of(p2),
        span1,
        "cache-hit reuse must carry forward the physical span_usable verbatim"
    );

    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(p2, l) };
}

/// (d, cont'd) `realloc` GROWTH correctness under exact-span sizing.
///
/// Unlike the old 4-MiB-rounded path (which typically has megabytes of
/// unused committed slack above the payload, so OPT-G's in-place fast path
/// almost always applies), an exact-span segment is sized to
/// `round_up(header + size, PAGE)` — at most one page (commonly far less) of
/// slack above the payload, versus megabytes under the default rounding. A
/// growth target that exceeds that thin margin correctly makes OPT-G's
/// `payload_off + new_eff <= span_usable` check decline the in-place path,
/// so `realloc` falls through to its alloc+copy+dealloc slow path. This is
/// the expected, documented trade-off of `exact-span-large` (see the
/// feature's `Cargo.toml` doc and task R12-4's proposed follow-up:
/// reserved-capacity growth) — NOT a bug. What this test actually verifies is
/// that growth still WORKS CORRECTLY (via whichever path realloc takes) and
/// PRESERVES the original data, regardless of which leg fires.
#[test]
#[cfg(feature = "exact-span-large")]
fn realloc_grow_preserves_data_even_without_inplace_slack() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_size = just_above_small_max();
    let old_layout = layout(old_size);
    let p = ac.alloc(old_layout);
    assert!(!p.is_null());

    // Stamp a recognisable pattern over the old payload so the move leg's
    // copy-correctness (if taken) is actually exercised, not just assumed.
    // SAFETY: `p` valid for `old_size` bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 251) as u8);
        }
    }

    let new_size = old_size + 1024;
    // SAFETY (R6-MS-1/2): `p` is a live allocation from this AllocCore made
    // with `old_layout`; on success the old `p` is consumed by this call
    // (either returned unchanged in-place or freed after a copy) — either
    // way it must not be used afterwards except through the returned
    // pointer, which this test does.
    let grown = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null(), "realloc growth must not fail");

    // Verify the ENTIRE preserved prefix survived (whether the same pointer
    // came back in place, or a fresh block was copied into).
    // SAFETY: `grown` valid for `new_size` bytes; the first `old_size` of
    // them must be the pattern written above (the preserved prefix).
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                grown.add(i).read(),
                (i % 251) as u8,
                "byte {i} of the preserved prefix was corrupted by realloc growth"
            );
        }
        // The grown tail is uninitialised per contract, but must at least be
        // writable/readable without faulting.
        grown.add(old_size).write_bytes(0x7E, new_size - old_size);
        assert_eq!(grown.add(new_size - 1).read(), 0x7E);
    }

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once with the
    // matching (new) layout, as `GlobalAlloc::realloc`'s contract requires.
    unsafe { ac.dealloc(grown, new_layout) };
}

/// (d, cont'd) `realloc` SHRINK correctness under exact-span sizing. OPT-G's
/// in-place fast path is grow-or-same-size ONLY (`new_eff >= old_eff`) —
/// per its own doc comment, "Shrinks fall through to the slow path (reclaims
/// RSS)" — so a shrink always takes the alloc+copy+dealloc leg regardless of
/// `exact-span-large`; this is unrelated to the feature. What this test
/// verifies is that shrinking a Large exact-span allocation still preserves
/// the retained prefix correctly through that slow path (the smaller
/// `span_usable` of an exact-span segment must not corrupt the move leg's
/// `safe_payload_read_span` bound).
#[test]
#[cfg(feature = "exact-span-large")]
fn realloc_shrink_preserves_data_with_exact_span() {
    let mut ac = AllocCore::new().expect("primordial");
    // Headroom above SMALL_MAX large enough that shrinking by 4 KiB still
    // stays comfortably Large (see `just_above_small_max`'s doc for why a
    // fixed literal like `512 * KIB` is unsafe under `medium-classes-wide`).
    let old_size = just_above_small_max() + 256 * KIB;
    let old_layout = layout(old_size);
    let p = ac.alloc(old_layout);
    assert!(!p.is_null());
    // SAFETY: `p` valid for `old_size` bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 197) as u8);
        }
    }

    let new_size = old_size - 4 * KIB;
    // SAFETY (R6-MS-1/2): `p` is a live allocation from this AllocCore made
    // with `old_layout`; consumed by this call exactly once.
    let shrunk = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(!shrunk.is_null(), "shrink realloc must not fail");
    // SAFETY: `shrunk` valid for `new_size` bytes; the retained prefix must
    // still read back the original pattern.
    unsafe {
        for i in 0..new_size {
            assert_eq!(
                shrunk.add(i).read(),
                (i % 197) as u8,
                "byte {i} of the retained prefix was corrupted by realloc shrink"
            );
        }
    }

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once with the
    // matching (new) layout.
    unsafe { ac.dealloc(shrunk, new_layout) };
}

/// (e) WITHOUT `exact-span-large`, behaviour must be byte-for-byte identical
/// to the pre-existing rounding: `span_usable` is always an EXACT multiple
/// of `SEGMENT` (never page-exact), for every control size — this is the
/// defensive round-trip proving the default/production path is untouched.
#[test]
#[cfg(not(feature = "exact-span-large"))]
fn without_feature_span_usable_is_always_a_whole_segment_multiple() {
    let mut ac = AllocCore::new().expect("primordial");
    for &size in &control_sizes() {
        let l = layout(size);
        let p = ac.alloc(l);
        assert!(!p.is_null(), "OOM allocating {size} bytes");

        let span_usable = ac.dbg_span_usable_of(p);
        assert_eq!(
            span_usable % SEGMENT,
            0,
            "without exact-span-large, span_usable ({span_usable}) for a \
             {size}-byte large alloc must remain an exact multiple of \
             SEGMENT ({SEGMENT}) — old rounding behaviour must be unchanged"
        );
        assert!(
            span_usable >= SEGMENT,
            "without exact-span-large, every large alloc must reserve at \
             least one whole SEGMENT"
        );

        // Round-trip: write/read across the full logical size, then dealloc.
        // SAFETY: `p` valid for `size` bytes.
        unsafe {
            p.write_bytes(0x3C, size);
            assert_eq!(p.read(), 0x3C);
        }
        // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
        unsafe { ac.dealloc(p, l) };
    }
}
