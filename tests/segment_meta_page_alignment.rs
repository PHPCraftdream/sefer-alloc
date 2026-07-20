//! R8-6 (task #219) — platform-portability test for the split between the
//! TIGHT payload/metadata boundary and the REAL-OS-page-aligned decommit
//! boundary.
//!
//! ## Lineage
//!
//! This test descends from task #205 (PLAT-1), which fixed a latent bug:
//! `Layout::small_meta_end`/`Layout::primordial_meta_end` are passed straight
//! to `os::decommit_pages`/`os::recommit_pages`/`os::commit_pages` as
//! page-range boundaries. Those syscalls (`madvise`/`VirtualFree`/
//! `VirtualAlloc`) silently round a non-page-aligned boundary to the nearest
//! REAL OS page — so if the boundary were aligned only to the 4 KiB
//! compile-time `PAGE` constant, on a 16 KiB- or 64 KiB-page machine it would
//! land mid-real-page and the OS would reclaim/recommit the wrong byte range,
//! silently breaking the M6 invariant with no red signal. Task #205 fixed this
//! by over-aligning BOTH offsets to `MAX_REALISTIC_PAGE_SIZE` (64 KiB) — a
//! compile-time superset bound — at the cost of ~56–64 KiB of wasted payload
//! per 4 MiB segment on ordinary 4 KiB-page systems.
//!
//! ## R8-6 split
//!
//! R8-6 (task #219) recognizes that #205 conflated TWO distinct concepts:
//!
//! 1. **TIGHT payload/metadata boundary** (`small_meta_end`/
//!    `primordial_meta_end`): where bump initialization, the H-1
//!    defense-in-depth "is this offset in metadata" guard, the primordial
//!    registry/hash/free-list placement, and the page-map re-marking loop
//!    operate. These have NO OS-page interaction and want the tightest
//!    possible value (4 KiB aligned, matching every other offset in the
//!    layout file) — the exact value that maximizes recoverable payload.
//!
//! 2. **DECOMMIT boundary** (`small_decommit_start`/`primordial_decommit_start`):
//!    the REAL-OS-page-aligned boundary ONLY the `os::decommit_pages`/
//!    `os::recommit_pages` syscall sites need. These are runtime functions
//!    (they call `aligned_vmem::page_size()`, which a `const fn` cannot).
//!
//! This split recovers the payload #205's over-alignment cost on 4 KiB-page
//! systems (where the decommit boundary collapses to EXACTLY the tight
//! boundary — zero waste) while preserving #205's real-page-safety guarantee
//! on 16/64 KiB-page systems (where the decommit boundary rounds up to the
//! same value #205 used to force unconditionally).
//!
//! ## What this test asserts
//!
//! The invariant that actually matters for decommit safety — now correctly
//! targeted at the RIGHT function:
//!
//! 1. `small_decommit_start`/`primordial_decommit_start` ARE multiples of the
//!    real runtime `aligned_vmem::page_size()` (the original point of #205's
//!    fix, now on the correct surface).
//! 2. The decommit boundary never starts BEFORE the tight payload boundary.
//! 3. On a 4 KiB-page machine (`page_size() == PAGE`), the decommit boundary
//!    equals the tight boundary EXACTLY — proving the payload recovery this
//!    task delivers. Gated on the RUNTIME page-size query (not a compile-time
//!    cfg), since the real page size cannot be known at compile time (the
//!    whole reason this split exists).

#![cfg(feature = "alloc-core")]

use sefer_alloc::SegmentLayout;

/// The conservative compile-time upper bound on real OS page sizes (see
/// `os::MAX_REALISTIC_PAGE_SIZE`). Hardcoded here (not imported) so the test
/// cross-checks the *value*, not the constant.
const MAX_REALISTIC_PAGE_SIZE: usize = 1 << 16;

#[test]
fn small_decommit_start_is_real_page_aligned() {
    let real_page = aligned_vmem::page_size();
    let start = SegmentLayout::small_decommit_start();
    let tight = SegmentLayout::SMALL_META_END;
    assert!(start > 0, "small_decommit_start must be non-zero");
    // The actual invariant: the decommit boundary is a multiple of the real OS
    // page — otherwise decommit/recommit silently reclaims the wrong byte range.
    assert_eq!(
        start % real_page,
        0,
        "small_decommit_start ({start}) must be a multiple of the real OS page size ({real_page}); \
         otherwise decommit/recommit silently reclaims the wrong byte range"
    );
    // The decommit boundary never starts before the tight payload boundary.
    assert!(
        start >= tight,
        "small_decommit_start ({start}) must be >= SMALL_META_END ({tight})"
    );
    // On a 4 KiB-page system the decommit boundary collapses to the tight
    // boundary EXACTLY — zero waste. This is the payload-recovery proof.
    if real_page == SegmentLayout::PAGE {
        assert_eq!(
            start, tight,
            "on a 4 KiB-page system, small_decommit_start ({start}) must equal SMALL_META_END ({tight}) \
             exactly (no payload waste)"
        );
    }
    // MAX_REALISTIC_PAGE_SIZE stays a valid superset bound (R8-6 keeps it
    // load-bearing via the debug_assert inside the decommit_start functions).
    assert!(
        real_page <= MAX_REALISTIC_PAGE_SIZE,
        "real OS page size ({real_page}) must be <= MAX_REALISTIC_PAGE_SIZE ({MAX_REALISTIC_PAGE_SIZE})"
    );
}

#[test]
fn primordial_decommit_start_is_real_page_aligned() {
    let real_page = aligned_vmem::page_size();
    let start = SegmentLayout::primordial_decommit_start();
    let tight = SegmentLayout::PRIMORDIAL_META_END;
    assert!(start > 0, "primordial_decommit_start must be non-zero");
    // The primordial decommit boundary is >= the small one (it stacks the
    // registry + hash + free-list on top) and must itself be real-page-aligned.
    assert!(
        start >= SegmentLayout::small_decommit_start(),
        "primordial_decommit_start ({start}) must be >= small_decommit_start ({})",
        SegmentLayout::small_decommit_start()
    );
    assert!(
        start >= tight,
        "primordial_decommit_start ({start}) must be >= PRIMORDIAL_META_END ({tight})"
    );
    assert_eq!(
        start % real_page,
        0,
        "primordial_decommit_start ({start}) must be a multiple of the real OS page size ({real_page})"
    );
    if real_page == SegmentLayout::PAGE {
        assert_eq!(
            start, tight,
            "on a 4 KiB-page system, primordial_decommit_start ({start}) must equal \
             PRIMORDIAL_META_END ({tight}) exactly (no payload waste)"
        );
    }
}

#[test]
fn both_decommit_boundaries_fit_within_segment_with_payload_room() {
    // Mirror of the in-crate `const _: () = assert!(... + PAGE <= SEGMENT)`
    // sanity checks — both decommit boundaries must leave at least one real
    // page of payload inside the 4 MiB segment.
    let real_page = aligned_vmem::page_size();
    let seg = SegmentLayout::SEGMENT;
    assert!(
        SegmentLayout::small_decommit_start() + real_page <= seg,
        "small decommit boundary + one real page must fit in SEGMENT"
    );
    assert!(
        SegmentLayout::primordial_decommit_start() + real_page <= seg,
        "primordial decommit boundary + one real page must fit in SEGMENT"
    );
}
