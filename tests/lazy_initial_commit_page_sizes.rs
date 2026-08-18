//! Task #1074 — the macOS ARM64 (16 KiB-page) release blocker, made
//! host-independent.
//!
//! ## The bug this test pins
//!
//! `AllocCore`'s constructor returned `None` on every 16 KiB-page host
//! (CI run 32083383999, job `test macos (production)`, SHA 84055d3): the
//! lazy-reservation call sites computed
//! `initial_commit = meta_end + LAZY_FIRST_CHUNK`, which is a multiple of
//! the COMPILE-TIME `PAGE` (4 KiB) only — `meta_end` is a `const fn`
//! (`Layout::small_meta_end` / `primordial_meta_end`) and cannot call the
//! runtime `aligned_vmem::page_size()` query. Since commit `dd6d027`
//! (task #1037), `aligned_vmem`'s `validate_initial_commit` correctly
//! requires a multiple of the RUNTIME page size and rejects the request —
//! `reserve_aligned_lazy` returns `None` and `AllocCore::new()` fails
//! wholesale.
//!
//! Measured values at the time of the bug (non-hardened / `production`):
//! `SMALL_META_END = 73728`, `PRIMORDIAL_META_END = 192512`;
//! `initial_commit` = 335872 / 454656 — multiples of 4 KiB, but
//! 335872 % 16384 = 8192 and 454656 % 16384 = 12288. Hardened:
//! 598016 % 16384 = 8192 and 716800 % 16384 = 12288.
//!
//! ## Why a table test, not a host test
//!
//! The development host that authored the fix has 4 KiB pages: running the
//! REAL reservation path there can never reproduce a 16 KiB rejection. The
//! computation is therefore extracted into the PURE function
//! `Layout::lazy_initial_commit(meta_end, page_size)` (page size is a
//! parameter), exposed for tests as
//! [`SegmentLayout::small_lazy_initial_commit`] /
//! [`SegmentLayout::primordial_lazy_initial_commit`], and driven here
//! across every page size this crate documents as supported (4 KiB x86-64,
//! 16 KiB Apple Silicon, 64 KiB some aarch64/Linux configs — the same set
//! `os::MAX_REALISTIC_PAGE_SIZE` bounds). A regression back to the
//! unrounded sum fails the `is_multiple_of` assertion below on ANY host,
//! including a 4 KiB one.
//!
//! ## Frontier consistency
//!
//! The retain-decommit reset path (`alloc_core_small_pool.rs`, B3/R8-6)
//! resets a recycled lazy segment's `committed_payload_end` frontier to
//! `small_decommit_start() + LAZY_FIRST_CHUNK`. Because `LAZY_FIRST_CHUNK`
//! (256 KiB) is a multiple of every supported page size,
//! `align_up(meta_end + LFC, ps) == align_up(meta_end, ps) + LFC`, so the
//! fresh-reservation initial commit and the decommit-reset frontier must
//! stay IDENTICAL — asserted here against the real runtime page size.

#![cfg(all(
    feature = "alloc-core",
    any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    )
))]

use sefer_alloc::SegmentLayout;

/// Every page size this crate documents as supported (`os::MAX_REALISTIC_PAGE_SIZE`
/// = 64 KiB is the compile-time superset bound). Hardcoded here (not
/// imported) so the test cross-checks the SET, not the constant.
const SUPPORTED_PAGE_SIZES: [usize; 3] = [4 * 1024, 16 * 1024, 64 * 1024];

/// Mirror of `alloc_core_small::LAZY_FIRST_CHUNK` (256 KiB, `pub(crate)`).
/// Only used for the bound check `initial_commit >= meta_end + LFC` — the
/// value-under-test itself comes from the production function.
const LAZY_FIRST_CHUNK: usize = 256 * 1024;

#[test]
fn lazy_initial_commit_is_a_multiple_of_every_supported_page_size() {
    for &ps in &SUPPORTED_PAGE_SIZES {
        let small_ic = SegmentLayout::small_lazy_initial_commit(ps);
        assert_eq!(
            small_ic % ps,
            0,
            "small-segment lazy initial_commit ({small_ic}) must be a multiple of the page \
             size ({ps}); aligned_vmem::validate_initial_commit (commit dd6d027, task #1037) \
             rejects it otherwise and AllocCore::new() returns None on that host — the \
             task #1074 macOS ARM64 release blocker"
        );
        assert!(
            small_ic >= SegmentLayout::SMALL_META_END + LAZY_FIRST_CHUNK,
            "small-segment lazy initial_commit ({small_ic}) must cover the whole metadata \
             region ({}) plus LAZY_FIRST_CHUNK ({LAZY_FIRST_CHUNK})",
            SegmentLayout::SMALL_META_END
        );
        assert!(
            small_ic <= SegmentLayout::SEGMENT,
            "small-segment lazy initial_commit ({small_ic}) must fit within one SEGMENT ({})",
            SegmentLayout::SEGMENT
        );

        let prim_ic = SegmentLayout::primordial_lazy_initial_commit(ps);
        assert_eq!(
            prim_ic % ps,
            0,
            "primordial lazy initial_commit ({prim_ic}) must be a multiple of the page size \
             ({ps}); aligned_vmem::validate_initial_commit rejects it otherwise and \
             AllocCore::new() returns None on that host — the task #1074 macOS ARM64 \
             release blocker"
        );
        assert!(
            prim_ic >= SegmentLayout::PRIMORDIAL_META_END + LAZY_FIRST_CHUNK,
            "primordial lazy initial_commit ({prim_ic}) must cover the whole metadata region \
             ({}) plus LAZY_FIRST_CHUNK ({LAZY_FIRST_CHUNK})",
            SegmentLayout::PRIMORDIAL_META_END
        );
        assert!(
            prim_ic <= SegmentLayout::SEGMENT,
            "primordial lazy initial_commit ({prim_ic}) must fit within one SEGMENT ({})",
            SegmentLayout::SEGMENT
        );
    }
}

#[test]
fn lazy_initial_commit_matches_decommit_start_frontier_at_runtime_page_size() {
    // `align_up(meta_end + LFC, ps) == align_up(meta_end, ps) + LFC` for a
    // ps-multiple LFC — so the fresh-reservation initial commit and the B3
    // retain-decommit frontier reset must agree on the REAL runtime page
    // size, on every host this test runs on.
    let real_page = aligned_vmem::page_size();
    assert_eq!(
        SegmentLayout::small_lazy_initial_commit(real_page),
        SegmentLayout::small_decommit_start() + LAZY_FIRST_CHUNK,
        "fresh small-segment lazy initial_commit must equal the B3 decommit-reset frontier"
    );
    assert_eq!(
        SegmentLayout::primordial_lazy_initial_commit(real_page),
        SegmentLayout::primordial_decommit_start() + LAZY_FIRST_CHUNK,
        "fresh primordial lazy initial_commit must equal the decommit-safe boundary + LFC"
    );
}
