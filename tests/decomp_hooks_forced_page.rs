//! Task #1081 (F6) — the R29-3 decomposition hooks' page-alignment
//! contract, pinned at the REAL `os::decommit_pages`/`os::recommit_pages`
//! boundary under FORCED 16/64 KiB runtime pages.
//!
//! ## The bug this test pins
//!
//! `AllocCore::dbg_decomp_decommit_payload` /
//! `AllocCore::dbg_decomp_recommit_payload`
//! (`src/alloc_core/alloc_core_small_pool.rs`) computed their payload start
//! from `SegLayout::small_meta_end()` — a `const fn` aligned only to the
//! compile-time `PAGE` (4 KiB; non-hardened value 73728).
//! `os::decommit_pages` documents "offsets MUST be page-aligned", and
//! `aligned_vmem::decommit`'s range-contract `debug_assert!` (restored by
//! task #1072) checks that against the RUNTIME `page_size()` — so on a
//! 16/64 KiB-page host the decommit hook panics (73728 % 16384 == 8192) and
//! the recommit twin returns `false`, leaving the example's
//! write-after-decommit loop writing into uncommitted pages under Windows
//! `MEM_DECOMMIT` semantics. Two commits of one wave interacted: task #1074
//! converted the two NEIGHBOURING hooks
//! (`dbg_decomp_win_reserve_only`/`dbg_decomp_win_commit_only`) to the
//! runtime query and did not touch this pair; task #1072 then made the
//! previously-silent violation loud. Neither commit considered the pair.
//! The fix routes both hooks through `SegLayout::small_decommit_start()`
//! (the R8-6/task-#219 runtime-page-safe boundary the production decommit
//! call sites in the same file already use).
//!
//! ## Why the forced-page shape (task #1080's seam)
//!
//! On a 4 KiB-page host `small_meta_end()` IS a `page_size()` multiple, so
//! running the buggy hook can never fail locally — exactly the #1074/#1077
//! bug class `tests/lazy_initial_commit_forced_page.rs` (task #1080) was
//! built for. This file reuses that seam: force `aligned_vmem::page_size()`
//! to 16 KiB and 64 KiB, where the tight value and the page-safe value do
//! NOT coincide, and drive the REAL hooks on a REAL reserved segment. A
//! reverted hook (back to `small_meta_end()`) trips
//! `aligned_vmem::decommit`'s range-contract `debug_assert!` on ANY host in
//! a debug build — the counterfactual this file exists to keep true.
//!
//! It also pins the two accessor hooks the same wave left inconsistent
//! (task #1081 F10b): `dbg_decomp_page_size` now returns the RUNTIME page
//! (was the compile-time `os::PAGE`), and `dbg_decomp_payload_range` now
//! returns the DECOMMIT-SAFE range `[small_decommit_start(), SEGMENT)` (was
//! the tight `[small_meta_end(), SEGMENT)`) so the examples' first-touch and
//! re-fault loops step exactly the range the hooks decommit. On 4 KiB hosts
//! both changes are value-identical to before — no published measurement's
//! basis changes (R29-3/R32-13 were measured on 4 KiB-page hosts).
//!
//! ## Scope
//!
//! The pure half compiles and runs under every feature row that carries
//! `internals` + `alloc-decommit` + `bench-internals` (e.g. the
//! `production alloc-stats bench-internals internals` test row) with no
//! special flags. The forced half additionally requires the build-time
//! `--cfg aligned_vmem_page_size_override` flag (the task #962
//! cfg-flag-not-feature discipline; wired in `scripts/check-all.mjs` and
//! the ci.yml `test-windows` forced-page row) as
//! `RUSTFLAGS="--cfg aligned_vmem_page_size_override" cargo test
//! --features "production internals bench-internals" --test
//! decomp_hooks_forced_page`. Like the #1080 file, the forced half is ONE
//! test (the override is process-global), serialized on a file-local mutex
//! with a `Drop` guard restoring the real page size.

// File-level gate (verify-alloc-core-dbg-internals-exhaustive, check 2/2):
// this file calls `internals`-gated `dbg_decomp_*` methods, so its own cfg
// must carry `feature = "internals"` (plus the hooks' own
// `alloc-decommit` + `bench-internals` gates) — a build without them skips
// the whole file instead of failing to compile.
#![cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]

use sefer_alloc::{AllocCore, SegmentLayout};

/// Every page size this crate documents as supported
/// (`os::MAX_REALISTIC_PAGE_SIZE` = 64 KiB is the compile-time superset
/// bound) — same convention as `tests/lazy_initial_commit_page_sizes.rs`:
/// hardcoded so the test cross-checks the SET, not a constant.
const SUPPORTED_PAGE_SIZES: [usize; 3] = [4 * 1024, 16 * 1024, 64 * 1024];

/// The TIGHT `small_meta_end()` values for the two layouts (plain /
/// hardened), mirrored from `tests/lazy_initial_commit_forced_page.rs` so
/// the hardened literal stays covered even when the hardened features are
/// off (and vice versa).
const PLAIN_SMALL_META_END: usize = 73728;
const HARDENED_SMALL_META_END: usize = 335872;

/// Pure half (no cfg flag): pins the counterfactual's own premise — the
/// TIGHT `small_meta_end()` boundary must stay a 4 KiB multiple (which is
/// exactly why the F6 bug is invisible on a 4 KiB host) and a NON-multiple
/// of every larger supported page size (which is what makes the forced-page
/// half below discriminate a reverted hook from a fixed one).
#[test]
fn tight_meta_end_stays_discriminable_at_every_supported_page_size() {
    let meta_ends = [
        SegmentLayout::SMALL_META_END,
        PLAIN_SMALL_META_END,
        HARDENED_SMALL_META_END,
    ];
    for &meta_end in &meta_ends {
        assert!(
            meta_end.is_multiple_of(4 * 1024),
            "tight small_meta_end ({meta_end}) must stay a multiple of the compile-time \
             PAGE (4 KiB): that coincidence is exactly why the task #1081 F6 bug is \
             invisible on a 4 KiB host — if this fails, the segment layout drifted and \
             the forced-page counterfactual below must be re-derived"
        );
        for &ps in &SUPPORTED_PAGE_SIZES[1..] {
            assert!(
                !meta_end.is_multiple_of(ps),
                "tight small_meta_end ({meta_end}) IS a multiple of {ps}: the forced-page \
                 counterfactual has lost discriminating power for this layout — re-derive \
                 the forced page choices below"
            );
        }
    }
}

// Serializes THIS binary's two `page_size()` consumers (the pure accessor
// test below and the forced-page module). Containment rationale (same
// discipline as tests/lazy_initial_commit_forced_page.rs, whose binary has
// only ONE consumer by construction): `aligned_vmem::page_size()`'s cold
// path loads the cache sentinel, runs a slow OS query, then blindly stores
// the REAL page — a pure test running that store concurrently with the
// forced-page test's override would clobber it (observed once as
// `dbg_decomp_page_size() == 4096` under a live 16384 override). The mutex
// makes the cold query complete before (or after) every override window,
// never inside one. Uncontended — and uncontendedable in no-cfg builds,
// where the forced module does not exist.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Pure half: the two accessor hooks report the RUNTIME page size and the
/// decommit-safe range (F10b pins). Runs on any host; on a 4 KiB dev host
/// it also structurally pins the F6 fix's accessor half — the range hook and
/// `small_decommit_start()` are both runtime queries, and their agreement is
/// what the examples' first-touch loops rely on.
#[test]
fn page_size_and_range_hooks_report_the_runtime_page_safe_boundary() {
    // Holds the file-local SERIAL mutex for the whole test: this test's own
    // `aligned_vmem::page_size()` call is this binary's ONLY cold-path
    // page-size query outside the forced-page module, and it must not
    // interleave with a forced override (see SERIAL's comment above).
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());

    let real_page = aligned_vmem::page_size();
    assert_eq!(
        AllocCore::dbg_decomp_page_size(),
        real_page,
        "dbg_decomp_page_size must report the RUNTIME page size (task #1081 F10b; was \
         the compile-time os::PAGE) — consumers divide payload byte ranges by it"
    );
    let (start, end) = AllocCore::dbg_decomp_payload_range();
    assert_eq!(
        start,
        SegmentLayout::small_decommit_start(),
        "dbg_decomp_payload_range must report the DECOMMIT-SAFE boundary the \
         decommit/recommit hooks actually use, not the tight small_meta_end()"
    );
    assert_eq!(end, SegmentLayout::SEGMENT);
    assert!(
        start.is_multiple_of(real_page),
        "payload range start ({start}) must be a multiple of the runtime page ({real_page})"
    );
    assert!(start >= SegmentLayout::SMALL_META_END);
    assert!(start < SegmentLayout::SEGMENT);
}

/// Forced-page half (task #1080's `--cfg aligned_vmem_page_size_override`
/// seam): drives the REAL decommit/recommit hooks on a REAL reserved segment
/// under 16 KiB and 64 KiB runtime pages. ONE test on purpose — the override
/// is process-global (see the file-level docs).
#[cfg(all(
    aligned_vmem_page_size_override,
    not(feature = "numa-aware"),
    not(miri)
))]
mod forced_page {
    use sefer_alloc::{AllocCore, SegmentLayout};

    // Same discipline as tests/lazy_initial_commit_forced_page.rs: one
    // forced-page test per binary, serialized on the file-level
    // poison-tolerant mutex (shared with the pure accessor test above —
    // see SERIAL's comment).

    /// Restores the real OS page size even if the test panics mid-observation
    /// (`None` re-arms the query-on-next-call sentinel in the page-size
    /// cache).
    struct RestorePageSize;

    impl Drop for RestorePageSize {
        fn drop(&mut self) {
            aligned_vmem::page_size_override::set_page_size_override(None);
        }
    }

    #[test]
    fn decomp_hooks_decommit_recommit_page_aligned_under_forced_pages() {
        let _serial = super::SERIAL.lock().unwrap_or_else(|p| p.into_inner());

        // Both sizes where the tight const boundary and the runtime-safe
        // boundary diverge for every known layout (see the pure test above).
        for &forced in &[16 * 1024, 64 * 1024] {
            // Guard FIRST: everything after this line runs under the
            // override, and a panic unwinds through this Drop before any
            // sibling observation. Restores the real page at the end of EACH
            // iteration.
            let _restore = RestorePageSize;

            assert!(
                aligned_vmem::page_size_override::set_page_size_override(Some(forced)),
                "{forced} is a power of two >= PAGE; the override seam must accept it"
            );
            // R26-4 evidence discipline: assert the config took effect
            // before trusting any observation made under it.
            assert_eq!(
                aligned_vmem::page_size(),
                forced,
                "config evidence: the page-size override must be active before the \
                 hook observations below mean anything"
            );
            assert_eq!(
                AllocCore::dbg_decomp_page_size(),
                forced,
                "the page-size hook must observe the forced runtime page (F10b)"
            );

            // Range evidence under the forced page: the boundary the hooks
            // use is a forced-page multiple.
            let (start, end) = AllocCore::dbg_decomp_payload_range();
            assert_eq!(
                start,
                SegmentLayout::small_decommit_start(),
                "the range hook must report the runtime-page-safe boundary under the \
                 forced page"
            );
            assert!(
                start.is_multiple_of(forced),
                "payload start ({start}) must be a multiple of the forced page ({forced})"
            );
            assert_eq!(end, SegmentLayout::SEGMENT);

            // Drive the REAL hooks on a real reserved segment. Under plain
            // `production` (no `small-segment-lazy-commit`) the small segment
            // is reserved eagerly, so the hooks' documented `# Safety`
            // precondition ("payload fully committed") genuinely holds.
            let mut a = AllocCore::new().expect("AllocCore::new must survive the forced page");
            let handle = a
                .dbg_decomp_reserve_and_keep()
                .expect("reserve for the decomp-hook observation");
            let base = handle.dbg_base();

            // THE counterfactual: a hook reverted to `small_meta_end()`
            // passes 73728 (a 4 KiB-only multiple) to `os::decommit_pages`;
            // `aligned_vmem::decommit`'s range-contract `debug_assert!`
            // (task #1072) panics HERE on any host, debug profile.
            unsafe { AllocCore::dbg_decomp_decommit_payload(base) };
            assert!(
                unsafe { AllocCore::dbg_decomp_recommit_payload(base) },
                "recommit under the forced page must succeed (and itself be \
                 page-aligned — the twin hook shares the same boundary)"
            );

            // Prove the recommitted range is genuinely writable again — the
            // Windows write-after-decommit hazard this pair exists for.
            // SAFETY: `base` is a live reserved segment whose payload was
            // just recommitted by the hook above, and `start` is the
            // page-aligned boundary the hook recommitted from.
            unsafe {
                core::ptr::write_volatile(base.add(start), 1u8);
                assert_eq!(core::ptr::read_volatile(base.add(start)), 1u8);
            }

            unsafe { a.dbg_decomp_release(handle) };
            // Drop the allocator BEFORE the guard restores the real page
            // size (same ordering discipline as the #1080 forced-page test).
            drop(a);
        }
    }
}
