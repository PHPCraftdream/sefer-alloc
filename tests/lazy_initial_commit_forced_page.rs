//! Task #1080 — the task #1074 / #1077 compile-time-`PAGE`-constant bug
//! class, pinned at the REAL reservation call sites under a FORCED 64 KiB
//! runtime page.
//!
//! ## The bug class
//!
//! Tasks #1074 and #1077 fixed the same class of bug five escapes apart: a
//! vmem-facing call site computed a byte count aligned to the COMPILE-TIME
//! `PAGE` constant (4 KiB) only, while `aligned_vmem` validates those
//! counts against the RUNTIME `page_size()` query (commit dd6d027, task
//! #1037). On 16/64 KiB-page hosts the vmem layer rejects the request; for
//! the lazy reservations this meant `reserve_aligned_lazy` returned `None`
//! and `AllocCore::new()` failed wholesale — the macOS ARM64 (16 KiB-page)
//! release blocker of CI run 32083383999.
//!
//! ## Why the previous test misses a call-site revert
//!
//! `tests/lazy_initial_commit_page_sizes.rs` (added by task #1074) pins the
//! PURE helpers `SegmentLayout::small_lazy_initial_commit(ps)` /
//! `primordial_lazy_initial_commit(ps)` — the helper's arithmetic, not the
//! call sites. Reverting any production call site back to the raw
//! `meta_end + LAZY_FIRST_CHUNK` sum STILL passes that file, because on a
//! 4 KiB-page host the rounded value and the raw sum coincide (the raw sum
//! is always a 4 KiB multiple — exactly why the bug was invisible on the
//! 4 KiB development host that authored the fix).
//!
//! ## How this test closes the hole
//!
//! Force `aligned_vmem::page_size()` to 64 KiB through the
//! `--cfg aligned_vmem_page_size_override` test seam, where the rounded and
//! raw values do NOT coincide, and drive the REAL call sites:
//!
//! (a) CONSTRUCT `AllocCore` — a reverted RESERVE call site computes an
//!     `initial_commit` the vmem layer rejects under the forced page, so
//!     `AllocCore::new()` returns `None` (primordial site:
//!     `src/alloc_core/bootstrap.rs`; small-segment sites:
//!     `src/alloc_core/alloc_core_small.rs` under
//!     `small-segment-lazy-commit`).
//! (b) READ BACK the stamped `committed_payload_end` frontier via the
//!     `internals` diagnostic `dbg_committed_payload_end_for` — a reverted
//!     STAMP call site (`bootstrap.rs` / `alloc_core_small.rs`) stores the
//!     unrounded sum, which is not a 64 KiB multiple.
//! (c) Under `small-segment-lazy-commit`, exhaust the primordial and check
//!     the fresh small segment's frontier the same way, then write and read
//!     back a byte to prove the committed prefix is genuinely writable.
//!
//! ## How the runtime half is reached: a build-time cfg flag, not a feature
//!
//! The runtime half additionally requires the build-time cfg flag, invoked
//! as `RUSTFLAGS="--cfg aligned_vmem_page_size_override" cargo test
//! --features "production internals" --test lazy_initial_commit_forced_page`.
//! The flag (not a cargo feature) is deliberate: a Cargo feature would be
//! reachable transitively from any downstream crate's feature resolution —
//! the exact feature-unification hazard task #962 /
//! docs/CORRECTNESS_OPEN_ITEMS.md item 42 closed for `mock` — while a cfg
//! flag is passed only explicitly per-build, so only the invocations that
//! ask for it can compile the override in (wired in `scripts/check-all.mjs`
//! and the ci.yml `test-windows` row).
//!
//! ## Process-global page-size containment
//!
//! The override is process-global, yet cannot disturb any other test:
//! (1) cargo runs each integration test FILE as its own PROCESS, so no other
//! test binary can observe the override; (2) within THIS binary the
//! forced-page test is the only `page_size()` consumer (the pure test above
//! never queries it); and (3) the `RestorePageSize` Drop guard restores the
//! real page even when the test panics mid-observation.
//!
//! ## Evidence discipline (R26-4 style)
//!
//! The override mutates process-global state, so the test FIRST asserts the
//! configuration it asked for actually took effect
//! (`aligned_vmem::page_size() == 64 * 1024`) before trusting any
//! observation made under it, and restores the real page size via a `Drop`
//! guard so even a panicking test cannot leak the forced page into sibling
//! tests.
//!
//! ## Scope and soundness
//!
//! The runtime part runs only on real Windows (not miri, not `numa-aware`)
//! — the one platform where the lazy reservation is genuinely two-phase
//! (reserve + partial commit); everywhere else `reserve_aligned_lazy`
//! commits the whole segment up front and the frontier is stamped
//! `SEGMENT`. Grow-on-carve stays sound under the forced page: the initial
//! frontier is 64 KiB-ROUNDED (a `page_size()` multiple by construction),
//! and `GROW_CHUNK` (256 KiB, equal to `LAZY_FIRST_CHUNK`) is a multiple of
//! every page size this crate supports, so every grow endpoint the carve
//! path computes stays page-aligned too.
//!
//! The pure test below (no lazy-feature cfg — it compiles under `hardened
//! medium-classes internals`, which has no lazy features) guards the
//! counterfactual's own premise: the RAW sums must stay non-multiples of
//! every larger supported page size for every known layout, so the
//! forced-64 KiB runtime observation keeps its discriminating power.

// File-level gate (verify-alloc-core-dbg-internals-exhaustive, H2): this
// file calls `dbg_committed_payload_end_for`, an `internals`-gated method,
// so the FILE's own cfg must carry `feature = "internals"` — a no-`internals`
// build then skips the whole file instead of referencing a gated method.
// The pure table test below needs no internals itself, but every row this
// file targets (the `... internals` test rows) passes it anyway.
#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::SegmentLayout;

/// Mirror of `alloc_core_small::LAZY_FIRST_CHUNK` (256 KiB, `pub(crate)`) —
/// same convention as `tests/lazy_initial_commit_page_sizes.rs`: used only
/// to form the raw sums and lower bounds here, never as the value under
/// test (that always comes from the production surface).
const LAZY_FIRST_CHUNK: usize = 256 * 1024;

/// The raw `meta_end + LAZY_FIRST_CHUNK` sums are always 4 KiB multiples —
/// exactly why the task #1074 bug is invisible on a 4 KiB host — and must
/// REMAIN non-multiples of every larger supported page size, so forcing a
/// 64 KiB page in the runtime test below keeps discriminating a reverted
/// call site from a fixed one.
#[test]
fn unrounded_lazy_sums_remain_discriminable_at_every_supported_page_size() {
    // The two compiled-in SegmentLayout constants PLUS the four
    // known-layout literals (plain and hardened layouts). The literals pin
    // the layout that is NOT compiled in under the current feature set, so
    // the hardened row is covered even when the hardened features are off
    // (and the plain row stays covered under `hardened ...` builds).
    let meta_ends = [
        SegmentLayout::SMALL_META_END,
        SegmentLayout::PRIMORDIAL_META_END,
        73728,  // plain SMALL_META_END (measured, task #1074)
        192512, // plain PRIMORDIAL_META_END
        335872, // hardened SMALL_META_END
        454656, // hardened PRIMORDIAL_META_END
    ];
    for &meta_end in &meta_ends {
        let raw_sum = meta_end + LAZY_FIRST_CHUNK;
        for &ps in &[4 * 1024, 16 * 1024, 64 * 1024] {
            if ps == 4 * 1024 {
                assert!(
                    raw_sum.is_multiple_of(ps),
                    "raw lazy sum {raw_sum} (meta_end {meta_end} + LAZY_FIRST_CHUNK \
                     {LAZY_FIRST_CHUNK}) must stay a multiple of the compile-time PAGE \
                     (4 KiB): that coincidence is exactly why a reverted task #1074 call \
                     site still passes on a 4 KiB host — if this fails, the segment \
                     layout drifted and the runtime counterfactual below must be \
                     re-derived"
                );
            } else {
                assert!(
                    !raw_sum.is_multiple_of(ps),
                    "raw lazy sum {raw_sum} (meta_end {meta_end} + LAZY_FIRST_CHUNK \
                     {LAZY_FIRST_CHUNK}) IS a multiple of {ps}: the forced-64 KiB \
                     counterfactual has lost discriminating power for this layout — \
                     re-derive the forced page choice for the runtime test in this file"
                );
            }
        }
    }
}

/// Runtime half: drives the REAL reserve/stamp call sites under a forced
/// 64 KiB `page_size()`. Compiled only when the build passes the
/// `aligned_vmem_page_size_override` cfg flag (see the file-level docs —
/// `RUSTFLAGS="--cfg aligned_vmem_page_size_override" cargo test ...`).
///
/// ONE test on purpose: the page-size override is PROCESS-GLOBAL — parallel
/// sibling tests in this binary would race it (one restoring the real page
/// while another is mid-observation), so every forced-page check lives in
/// this single test.
#[cfg(all(
    feature = "internals",
    aligned_vmem_page_size_override,
    any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ),
    not(feature = "numa-aware"),
    windows,
    not(miri)
))]
mod forced_page {
    use std::alloc::Layout;

    use sefer_alloc::{AllocCore, SegmentLayout};

    // Belt-and-braces for any future sibling test added to this binary that
    // also reads `page_size()`; cross-FILE isolation is structural (separate
    // processes), which is why this mutex is file-local and not shared.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Restores the real OS page size even if the test panics: `None`
    /// re-arms the query-on-next-call sentinel in the page-size cache.
    struct RestorePageSize;

    impl Drop for RestorePageSize {
        fn drop(&mut self) {
            aligned_vmem::page_size_override::set_page_size_override(None);
        }
    }

    #[test]
    fn lazy_initial_commit_call_sites_survive_a_forced_64k_page() {
        // Mirrors the mock suite's SERIAL discipline
        // (crates/aligned-vmem/tests/smoke.rs): poisoned-lock-tolerant, so a
        // panicking prior holder does not poison this run.
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());

        // Guard FIRST: everything after this line runs under the override,
        // and a panic unwinds through this Drop before sibling tests run.
        let _restore = RestorePageSize;

        // Force the runtime page-size query to 64 KiB ... Since task #1085
        // the seam also rejects a value smaller than the host's REAL page;
        // for this 64 KiB-only test that rejection is the RIGHT failure,
        // not something to skip past — there is no second arm to fall back
        // to, so a host that cannot run the forced-64 KiB oracle must fail
        // loudly rather than pass vacuously (every real Windows
        // configuration — 4 KiB x64, 64 KiB ARM64 — accepts 64 KiB, so the
        // assert is unreachable there; it trips only on hypothetical
        // larger-page hosts).
        assert!(
            aligned_vmem::page_size_override::set_page_size_override(Some(64 * 1024)),
            "64 KiB is a power of two >= PAGE and >= every real Windows page \
             size; the override seam must accept it (a rejection here means \
             the host's real page exceeds 64 KiB — see task #1085's \
             below-real-page rejection rule — and this host cannot run the \
             forced-page oracle)"
        );
        // ... and PROVE the configuration took effect before trusting any
        // observation made under it (R26-4 evidence discipline: assert the
        // config, then observe).
        assert_eq!(
            aligned_vmem::page_size(),
            64 * 1024,
            "config evidence: the page-size override must be active before the \
             call-site observations below mean anything"
        );

        // (a) the RESERVE call site: the primordial segment is the first
        // allocation target, so merely constructing AllocCore drives
        // bootstrap.rs's lazy reservation with the forced 64 KiB page.
        let mut a = AllocCore::new().expect(
            "the PRIMORDIAL lazy-reserve call site (src/alloc_core/bootstrap.rs) computed an \
             initial_commit that the vmem layer rejected under the forced 64 KiB page — \
             the task #1074 raw-sum regression",
        );
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(
            !p.is_null(),
            "first 16-byte allocation from the primordial must succeed"
        );

        // (b) the STAMP call site: read back the committed frontier the
        // segment header actually stores.
        let frontier = a.dbg_committed_payload_end_for(p).unwrap();
        assert_eq!(
            frontier,
            SegmentLayout::primordial_lazy_initial_commit(64 * 1024),
            "the STAMP call site in src/alloc_core/bootstrap.rs must store the page-rounded \
             value ({}), not the raw meta_end + LAZY_FIRST_CHUNK sum ({}) — the task #1074 \
             raw-sum regression",
            SegmentLayout::primordial_lazy_initial_commit(64 * 1024),
            SegmentLayout::PRIMORDIAL_META_END + super::LAZY_FIRST_CHUNK
        );
        assert!(
            frontier.is_multiple_of(64 * 1024),
            "primordial committed_payload_end ({frontier}) must be a multiple of the forced \
             64 KiB page: an unrounded raw-sum stamp (src/alloc_core/bootstrap.rs) breaks \
             every later page-size-validated vmem operation against this frontier"
        );
        assert!(
            frontier >= SegmentLayout::PRIMORDIAL_META_END + super::LAZY_FIRST_CHUNK,
            "primordial committed_payload_end ({frontier}) must still cover the whole \
             metadata region ({}) plus LAZY_FIRST_CHUNK ({})",
            SegmentLayout::PRIMORDIAL_META_END,
            super::LAZY_FIRST_CHUNK
        );

        // (c) the SMALL-segment reserve/stamp call sites: only lazy under
        // `small-segment-lazy-commit` (without it the second segment is
        // reserved eagerly and stamped SEGMENT).
        #[cfg(feature = "small-segment-lazy-commit")]
        {
            // Exhaust the primordial until the SEGMENT-aligned base changes
            // (technique precedent: tests/lazy_commit_frontier.rs,
            // `fresh_small_segment_frontier_is_correct`).
            let seg = SegmentLayout::SEGMENT;
            let prim_base = (p as usize) & !(seg - 1);
            let mut second_ptr = core::ptr::null_mut();
            for _ in 0..500_000 {
                let q = a.alloc(Layout::from_size_align(16, 8).unwrap());
                assert!(
                    !q.is_null(),
                    "primordial-exhaustion allocation must succeed"
                );
                if (q as usize) & !(seg - 1) != prim_base {
                    second_ptr = q;
                    break;
                }
            }
            assert!(
                !second_ptr.is_null(),
                "failed to trigger a second (small) segment reservation within 500_000 \
                 16-byte allocations"
            );
            let frontier2 = a.dbg_committed_payload_end_for(second_ptr).unwrap();
            assert_eq!(
                frontier2,
                SegmentLayout::small_lazy_initial_commit(64 * 1024),
                "the SMALL-segment reserve/stamp call sites (src/alloc_core/alloc_core_small.rs) \
                 must store the page-rounded value ({}), not the raw meta_end + \
                 LAZY_FIRST_CHUNK sum ({}) — the task #1074 raw-sum regression",
                SegmentLayout::small_lazy_initial_commit(64 * 1024),
                SegmentLayout::SMALL_META_END + super::LAZY_FIRST_CHUNK
            );
            assert!(
                frontier2.is_multiple_of(64 * 1024),
                "small-segment committed_payload_end ({frontier2}) must be a multiple of \
                 the forced 64 KiB page: the stamp call site in \
                 src/alloc_core/alloc_core_small.rs stored an unrounded raw sum"
            );
            // Prove the committed prefix is genuinely WRITABLE under the
            // forced page — the reserve really committed `frontier2` bytes,
            // not just stamped a number.
            // SAFETY: `second_ptr` is a live 16-byte allocation returned by
            // `a.alloc` above and still owned by `a`.
            unsafe {
                second_ptr.write(0x5C);
                assert_eq!(
                    second_ptr.read(),
                    0x5C,
                    "the fresh small segment's committed prefix must be writable under \
                     the forced 64 KiB page (src/alloc_core/alloc_core_small.rs reserve \
                     call site)"
                );
            }
        }

        // Drop the allocator BEFORE the guard restores the real page size
        // (the release path does not consult page_size(), but keeping the
        // forced page active for the whole observation — teardown included —
        // is the internally consistent order).
        drop(a);
    }
}
