//! PLAT-1 (task #205) — platform-portability belt-and-suspenders test for the
//! segment metadata / payload boundary offsets.
//!
//! `Layout::small_meta_end` / `Layout::primordial_meta_end` are the byte
//! offsets at which segment metadata ends and the payload region begins; they
//! are passed straight to `os::decommit_pages` / `os::recommit_pages` /
//! `os::commit_pages` as page-range boundaries. Those syscalls
//! (`madvise`/`VirtualFree`/`VirtualAlloc`) silently round a non-page-aligned
//! boundary to the nearest REAL OS page — so if the boundary were aligned only
//! to the 4 KiB compile-time `PAGE` constant, on a 16 KiB- or 64 KiB-page
//! machine it would land mid-real-page and the OS would reclaim/recommit the
//! wrong byte range, silently breaking the M6 invariant with no red signal.
//!
//! The fix aligns both offsets to `MAX_REALISTIC_PAGE_SIZE` (64 KiB) — a
//! compile-time superset of every real page size up to 64 KiB. This test
//! asserts the *actual* invariant that fix is supposed to guarantee: both
//! boundaries are multiples of the REAL, runtime-queried OS page size
//! (`aligned_vmem::page_size()`), AND of the 64 KiB compile-time bound the fix
//! uses (so the regression also trips on a 4 KiB-page dev machine, not only on
//! the 16 KiB-page macOS CI runner). Runs unconditionally under `alloc-core` —
//! cheap and correct everywhere.
//!
//! NON-VACUOUS: reverting `small_meta_end`/`primordial_meta_end` to align to
//! `PAGE` (4 KiB) makes `SMALL_META_END % 65536 != 0` on every machine (the
//! old values were 73728 / 106496), so at least the bound assertion fails
//! everywhere; on a 16 KiB / 64 KiB-page machine the real-page assertion fails
//! too.

#![cfg(feature = "alloc-core")]

use sefer_alloc::SegmentLayout;

/// The conservative compile-time upper bound on real OS page sizes that the
/// fix aligns both metadata boundaries to (see `os::MAX_REALISTIC_PAGE_SIZE`).
/// Hardcoded here (not imported) so the test cross-checks the *value*, not the
/// constant — if someone silently shrinks the constant below a real page size
/// this test still encodes the correct expectation.
const MAX_REALISTIC_PAGE_SIZE: usize = 1 << 16;

#[test]
fn small_meta_end_is_real_page_aligned() {
    let real_page = aligned_vmem::page_size();
    let end = SegmentLayout::SMALL_META_END;
    assert!(end > 0, "SMALL_META_END must be non-zero");
    // The actual invariant: the boundary is a multiple of the real OS page.
    assert_eq!(
        end % real_page,
        0,
        "SMALL_META_END ({end}) must be a multiple of the real OS page size ({real_page}); \
         otherwise decommit/recommit silently reclaims the wrong byte range"
    );
    // The compile-time bound the fix uses: catches the regression on a 4 KiB-page
    // machine too (where the real-page check above is trivially satisfied by the
    // old buggy 4 KiB alignment).
    assert_eq!(
        end % MAX_REALISTIC_PAGE_SIZE,
        0,
        "SMALL_META_END ({end}) must be a multiple of MAX_REALISTIC_PAGE_SIZE ({MAX_REALISTIC_PAGE_SIZE})"
    );
}

#[test]
fn primordial_meta_end_is_real_page_aligned() {
    let real_page = aligned_vmem::page_size();
    let end = SegmentLayout::PRIMORDIAL_META_END;
    assert!(end > 0, "PRIMORDIAL_META_END must be non-zero");
    // The primordial boundary is >= the small boundary (it stacks the registry +
    // hash + free-list on top) and must itself be real-page-aligned.
    assert!(
        end >= SegmentLayout::SMALL_META_END,
        "PRIMORDIAL_META_END ({end}) must be >= SMALL_META_END ({})",
        SegmentLayout::SMALL_META_END
    );
    assert_eq!(
        end % real_page,
        0,
        "PRIMORDIAL_META_END ({end}) must be a multiple of the real OS page size ({real_page})"
    );
    assert_eq!(
        end % MAX_REALISTIC_PAGE_SIZE,
        0,
        "PRIMORDIAL_META_END ({end}) must be a multiple of MAX_REALISTIC_PAGE_SIZE ({MAX_REALISTIC_PAGE_SIZE})"
    );
}

#[test]
fn both_boundaries_fit_within_segment_with_payload_room() {
    // Mirror of the in-crate `const _: () = assert!(... + PAGE <= SEGMENT)`
    // sanity checks — both metadata regions must leave at least one real page
    // of payload inside the 4 MiB segment.
    let real_page = aligned_vmem::page_size();
    let seg = SegmentLayout::SEGMENT;
    assert!(
        SegmentLayout::SMALL_META_END + real_page <= seg,
        "small metadata + one real page must fit in SEGMENT"
    );
    assert!(
        SegmentLayout::PRIMORDIAL_META_END + real_page <= seg,
        "primordial metadata + one real page must fit in SEGMENT"
    );
}
