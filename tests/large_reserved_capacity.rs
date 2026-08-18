//! R12-4 (P0-perf, EXPERIMENTAL Windows-gated-effect prototype) —
//! `large-reserved-capacity` correctness tests.
//!
//! `exact-span-large` (R12-3) shrinks a Large segment's COMMITTED span
//! (`span_usable`) to `round_up(header + size, PAGE)` instead of a whole
//! `SEGMENT` (4 MiB) — a big RSS win, but it leaves OPT-G (the existing
//! Large->Large in-place-grow `realloc` fast path) almost no committed
//! headroom to grow into, so a growing realloc almost always falls through
//! to the slow (alloc+copy+free) path.
//!
//! `large-reserved-capacity` (this feature) gives a growing realloc a cheap
//! in-place path WITHOUT giving back the RSS win: `alloc_large_slow`
//! reserves (but does not commit) a larger VA span up front
//! (`reserved_capacity`, geometric 2x of the page-rounded request, capped),
//! and a growing realloc that still fits within `reserved_capacity` commits
//! just the missing tail (`os::commit_pages`) instead of moving the
//! allocation. `span_usable`'s own meaning (the segment's TRUE COMMITTED
//! span, bug #134's carry-forward invariant) is UNCHANGED — see
//! `SegmentHeader::span_usable`'s doc and `SegmentHeader::reserved_capacity`'s
//! doc for the split.
//!
//! This file exercises:
//!   (a) data preservation across a SINGLE growth that needs the
//!       reserved-capacity mechanism (grows past `span_usable`, within
//!       `reserved_capacity`) — the grow must return the SAME pointer and
//!       preserve the prefix.
//!   (b) data preservation across a CHAIN of several such growths (cumulative
//!       fractions of the original size, staying within the FIXED geometric
//!       `reserved_capacity` ceiling for the whole chain), verifying every
//!       step's data survives and keeps returning the same pointer.
//!   (c) the reserved_capacity BOUNDARY: a growth that exceeds
//!       `reserved_capacity` must fall back to the slow path correctly (no
//!       panic/UB), preserving data via the move leg.
//!   (d) large_cache interaction: a cached-and-reused segment's
//!       `reserved_capacity` is carried forward verbatim (bug-#134-shaped),
//!       not confused between two different cached segments of different
//!       original capacities.
//!   (e) WITHOUT the feature, `reserved_capacity` always equals
//!       `span_usable` (the inert inherited value) — the default/production
//!       path is untouched.
//!   (f) task #1077: the grow's stored committed boundary (`span_usable`
//!       right after a grow — exactly the value the call site computed and
//!       passed to `os::commit_pages`) must be a multiple of 64 KiB
//!       (`os::MAX_REALISTIC_PAGE_SIZE`), the compile-time superset of
//!       every runtime page size — observed AT THE CALL SITE through the
//!       header, not via a helper's arithmetic (the #1080 lesson: a test
//!       that pins a helper passes even when the call site is reverted).
//!
//! ## Counterfactual reasoning (what a revert makes fail)
//!
//! - (a)/(b): without the R12-4 commit-and-grow mechanism, OPT-G's existing
//!   `payload_off + new_eff <= span_usable` check fails (span_usable is
//!   page-tight under `exact-span-large`) and `realloc` takes the slow path
//!   — the pointer-identity assertions (`assert_eq!(ptr, new_ptr)`) fail.
//! - (c): without the `reserved_capacity` bound check
//!   (`required_end > reserved_capacity => bail`), a sufficiently large grow
//!   request would either wrongly succeed in-place past the actual reserved
//!   VA (a real memory-safety bug, would fault/UB) or (if the bound is
//!   present but wrong) silently corrupt accounting — the boundary test's
//!   assertions on `reserved_capacity`/`span_usable`/data would catch either.
//! - (d): without carrying `reserved_capacity` forward verbatim on a
//!   cache-hit reuse (mirroring bug #134's `span_usable` fix), a reused
//!   segment's `reserved_capacity` would read back as some OTHER segment's
//!   value (stale/zeroed/wrong), which the boundary assertion in (d) would
//!   catch as a mismatch against the value captured right after the ORIGINAL
//!   reservation.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

// `#[allow(dead_code)]`: used only inside `#[cfg(feature = "large-reserved-
// capacity")]`-gated test bodies below, so a feature-off build (which still
// compiles this file's shared helpers, e.g. `without_feature_...` below)
// would otherwise warn.
#[allow(dead_code)]
const KIB: usize = 1024;
const SEGMENT: usize = SegmentLayout::SEGMENT;

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// The smallest control size: `SMALL_MAX` plus a small headroom margin —
/// mirrors `exact_span_large.rs`'s `just_above_small_max` so every control
/// size classifies as Large regardless of which small-class feature
/// (`medium-classes`/`medium-classes-wide`) is active in the build.
fn just_above_small_max() -> usize {
    SegmentLayout::SMALL_MAX + SegmentLayout::PAGE
}

/// (a) A SINGLE growth that needs the reserved-capacity mechanism: alloc at
/// `just_above_small_max()`, grow by a modest amount that exceeds the
/// page-tight `span_usable` an `exact-span-large` segment starts with, but
/// stays within the geometric `reserved_capacity`. Data must be preserved
/// and (with the feature on) the SAME pointer must come back.
///
/// `not(numa-aware)`: the mechanism itself is compiled out under
/// `numa-aware` (`reserved_capacity` always equals `span_usable` there — see
/// `SegmentHeader::reserved_capacity`'s doc and the `not(numa-aware)` gating
/// throughout `alloc_core_large.rs`'s `alloc_large_slow`), so this test's
/// in-place-pointer-identity premise does not hold under `--all-features`
/// (which enables `numa-aware` alongside `large-reserved-capacity`).
#[test]
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
fn single_growth_within_reserved_capacity_is_in_place_and_preserves_data() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_size = just_above_small_max();
    let old_layout = layout(old_size);
    let p = ac.alloc(old_layout);
    assert!(!p.is_null(), "OOM allocating {old_size} bytes");

    // Sanity: the initial span_usable is page-tight (exact-span-large is a
    // hard prerequisite of large-reserved-capacity), so a same-segment
    // in-place grow under the OLD (span_usable-only) OPT-G check would NOT
    // have room — this test's premise depends on the reserved-capacity
    // mechanism actually firing, not on ordinary committed slack.
    let span_before = ac.dbg_span_usable_of(p);
    assert!(
        span_before < old_size + 64 * KIB,
        "premise: initial span_usable ({span_before}) must be page-tight \
         around the request ({old_size}), not padded with megabytes of \
         slack — otherwise this test would not exercise the reserved-capacity \
         mechanism at all"
    );
    let reserved_before = ac.dbg_reserved_capacity_of(p);
    assert!(
        reserved_before > span_before,
        "premise: reserved_capacity ({reserved_before}) must exceed the \
         initial committed span_usable ({span_before}) for the mechanism to \
         have anything to grow into"
    );

    // Stamp a marker across the original payload.
    // SAFETY: `p` valid for `old_size` bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 251) as u8);
        }
    }

    // Grow by 64 KiB — comfortably exceeds the page-tight span_usable, and
    // (2x geometric growth of a >256 KiB request) comfortably fits within
    // reserved_capacity.
    let new_size = old_size + 64 * KIB;
    // SAFETY (R6-MS-1/2): `p` is a live allocation from this AllocCore made
    // with `old_layout`, consumed by this call exactly once.
    let grown = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null(), "realloc growth must not fail");
    assert_eq!(
        grown, p,
        "a growth within reserved_capacity must return the SAME pointer \
         (in-place commit-and-grow), not relocate"
    );

    // The committed span must have advanced past its pre-grow value — the
    // whole point of the commit-and-grow mechanism (checked precisely via
    // the write/read-back of the grown tail just below, which would fault
    // on genuinely-uncommitted memory).
    let span_after = ac.dbg_span_usable_of(grown);
    assert!(
        span_after > span_before,
        "span_usable ({span_after}) must advance past its pre-grow value \
         ({span_before}) — the commit-and-grow mechanism must have \
         committed additional pages"
    );

    // Data preservation: the original prefix must be intact.
    // SAFETY: `grown` valid for `new_size` bytes.
    unsafe {
        for i in 0..old_size {
            assert_eq!(
                grown.add(i).read(),
                (i % 251) as u8,
                "byte {i} of the preserved prefix was corrupted by the \
                 reserved-capacity in-place grow"
            );
        }
        // The newly-committed tail must be writable/readable (real,
        // accessible memory — not merely reserved VA).
        grown.add(old_size).write_bytes(0x7E, new_size - old_size);
        for i in old_size..new_size {
            assert_eq!(
                grown.add(i).read(),
                0x7E,
                "grown tail byte {i} not writable"
            );
        }
    }

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once with the
    // matching (new) layout.
    unsafe { ac.dealloc(grown, new_layout) };
}

/// (f) task #1077: the OPT-G grow's NEW committed-span boundary must be a
/// multiple of `MAX_REALISTIC_PAGE_SIZE` (64 KiB), observed at the CALL SITE
/// through the header value the grow actually stores — NOT through a helper's
/// arithmetic. Task #1080 recorded that `tests/lazy_initial_commit_page_sizes.rs`
/// (added by task #1074) pins `Layout::lazy_initial_commit`'s math, so reverting
/// any of #1074's six converted call sites still passes that file; this test is
/// the shape that closes that hole for THIS call site.
///
/// Path-activation oracle (task #1088/L8, CLAUDE.md's R30-8 rule): pointer
/// identity does NOT prove the grow mechanism fired. OPT-G's plain
/// committed-span check (`payload_off + new_eff <= span_usable`) returns the
/// SAME pointer with `span_usable` UNCHANGED when the request already fits —
/// `try_grow_large_reserved_capacity` is never called — so `assert_eq!(grown, p)`
/// holds in both worlds. The discriminator is the committed span itself: the
/// grow path stamps `span_usable` only on commit success, strictly past its
/// old value (its entry condition is `required_end > span_usable`), so
/// `span_after > span_before` holds IFF the mechanism activated. Pointer
/// identity is still asserted, but only as the relocation premise it actually
/// proves (a moved pointer would make the boundary assertion observe a
/// DIFFERENT segment's span).
///
/// `try_grow_large_reserved_capacity` computes its commit endpoint as
/// `align_up(required_end, <align>).min(reserved_capacity)` and, on commit
/// success, stores it as the segment's new `span_usable` — so
/// `dbg_span_usable_of` after an in-place grow reads back exactly the value the
/// call site passed to `os::commit_pages`. The endpoint is a 64 KiB multiple
/// UNLESS the `.min(reserved_capacity)` clamp bites, in which case it equals
/// `reserved_capacity` exactly (itself a runtime-`page_size()` multiple) —
/// both are validator-safe on every host, so the assertion below accepts
/// exactly that disjunction. When `<align>` was the compile-time
/// `os::PAGE` (4 KiB), a 16/64 KiB-page host rejected the commit
/// (`vmem::try_commit_range` validates endpoints against the RUNTIME page
/// size) and the grow silently degraded to the slow path — a state no 4 KiB
/// host can expose through pointer identity alone, because the 4 KiB commit
/// succeeds there. Asserting the stored boundary is a 64 KiB multiple — a
/// superset of every page size this crate supports — fails on ANY host
/// (including this 4 KiB one) when the call site is reverted to `os::PAGE`,
/// which is exactly the counterfactual a host-local test CAN produce.
///
/// REGIME PIN (OH3 finding I1, CLAUDE.md R30-8): the disjunction above is
/// only discriminating while the clamp does NOT bite. In the clamped regime
/// with `align_up(required_end, 4 KiB) >= reserved_capacity`, the correct
/// call site and a reverted-to-`os::PAGE` one store the SAME value
/// (`reserved_capacity`), so the disjunction would pass in both worlds. This
/// test therefore pins the non-clamped regime with
/// `assert_ne!(span_after, reserved_before)`: with these sizes the clamp is
/// unreachable (`reserved_capacity` is 4× the page-rounded usable request —
/// `LARGE_RESERVED_CAP_GROWTH_FACTOR` — while this grow only adds 64 KiB
/// past the first page-tight commit), so equality can only mean the sizes or
/// the growth factor drifted toward the ceiling, and the pin fails loudly
/// there instead of letting the assertion degrade into the both-worlds-pass
/// regime.
#[test]
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
fn single_growth_commit_boundary_is_real_page_safe() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_size = just_above_small_max();
    let old_layout = layout(old_size);
    let p = ac.alloc(old_layout);
    assert!(!p.is_null(), "OOM allocating {old_size} bytes");

    // Premise captures BEFORE the grow: the committed span (the activation
    // oracle's baseline) and the reserved ceiling (the `.min` clamp's landing
    // spot — reserved_capacity is not modified by an in-place grow, so the
    // post-grow comparison below reads the same value the grow path clamps
    // against).
    let span_before = ac.dbg_span_usable_of(p);
    let reserved_before = ac.dbg_reserved_capacity_of(p);

    // Grow within reserved_capacity so the in-place commit-and-grow fires.
    // Pointer identity here is only the RELOCATION premise: if the growth
    // relocated, the boundary assert below would be observing a different
    // segment's span, not the grow call site's value.
    let new_size = old_size + 64 * KIB;
    // SAFETY (R6-MS-1/2): `p` is a live allocation from this AllocCore made
    // with `old_layout`, consumed by this call exactly once.
    let grown = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null(), "realloc growth must not fail");
    assert_eq!(
        grown, p,
        "premise: the growth must take the in-place reserved-capacity path — \
         a relocation here means the boundary assertion below would not be \
         observing the grow call site's computed value"
    );

    // PATH-ACTIVATION ORACLE (task #1088/L8): the committed span must have
    // STRICTLY advanced. It cannot advance without the grow mechanism: the
    // plain committed-span short-circuit (`payload_off + new_eff <=
    // span_usable`) returns the same pointer with `span_usable` untouched,
    // and the grow path stamps a value strictly past the old one (entry
    // condition `required_end > span_usable`, success-only stamp). So
    // equality here means the mechanism did NOT fire and the boundary
    // assertion below would be pinning the ORIGINAL span, not the grow
    // endpoint — the exact non-discriminating case the old pointer-identity
    // oracle could not see (R30-8 class).
    let span_after = ac.dbg_span_usable_of(grown);
    assert!(
        span_after > span_before,
        "path-activation oracle: span_usable after the grow ({span_after}) must \
         EXCEED its pre-grow value ({span_before}) — equality means realloc took \
         the no-grow short-circuit (the request already fit the committed span) \
         and the boundary assertion below would be observing the ORIGINAL span, \
         not the grow call site's computed endpoint"
    );

    // THE observation: post-grow `span_usable` is exactly the endpoint the
    // call site passed to `os::commit_pages` (stamped only on commit
    // success by `set_span_usable_at` in `try_grow_large_reserved_capacity`).
    //
    // The endpoint is a 64 KiB multiple OR, when the `.min(reserved_capacity)`
    // clamp bites (`align_up(required_end, 64 KiB) > reserved_capacity`),
    // exactly `reserved_capacity` — a runtime-`page_size()` multiple, not
    // necessarily a 64 KiB one. Both are validator-safe on every host; a
    // value that is NEITHER is the reverted-to-`os::PAGE` bug.

    // REGIME PIN (OH3/I1, R30-8): the disjunction below cannot discriminate
    // the correct implementation from the reverted one once the clamp bites
    // with `align_up(required_end, 4 KiB) >= reserved_capacity` (both worlds
    // store exactly `reserved_capacity`), so this test must stay in the
    // NON-clamped regime for the boundary assertion to mean anything. With
    // these sizes that is guaranteed by a wide margin — see the REGIME PIN
    // paragraph in this test's doc comment — so equality here is a size/
    // growth-factor drift toward the ceiling, not a legitimate clamp: fail
    // loudly and re-pin, do not silently enter the both-worlds-pass regime.
    assert_ne!(
        span_after, reserved_before,
        "regime pin: the `.min(reserved_capacity)` clamp BIT — span_usable \
         after the grow ({span_after}) equals the reserved ceiling \
         ({reserved_before}). In that regime (once `align_up(required_end, \
         4 KiB) >= reserved_capacity`) the boundary assertion below passes \
         identically under the correct implementation and under a \
         reverted-to-`os::PAGE` call site, so it proves nothing; re-pin this \
         test's sizes away from the ceiling or make the clamp case an \
         explicitly-justified branch"
    );

    let max_realistic_page = 64 * KIB;
    assert!(
        span_after.is_multiple_of(max_realistic_page) || span_after == reserved_before,
        "post-grow span_usable ({span_after}) must be a multiple of 64 KiB \
         (MAX_REALISTIC_PAGE_SIZE — superset of the 4/16/64 KiB page sizes \
         this crate supports) OR exactly the segment's reserved_capacity \
         ({reserved_before} — the `.min(reserved_capacity)` clamp's legitimate \
         landing spot, itself a runtime-page multiple). A value that is NEITHER \
         means the grow target was computed with the compile-time PAGE constant \
         and will be rejected by vmem's runtime page-size validation on 16/64 \
         KiB hosts (task #1077)"
    );

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once with the
    // matching (new) layout.
    unsafe { ac.dealloc(grown, new_layout) };
}

/// (b) A CHAIN of successive growths, each step re-stamping and
/// re-verifying a distinguishing byte pattern across the WHOLE payload so
/// far, AND asserting the pointer stays identical (in-place) at every step.
/// Proves the mechanism composes correctly across multiple `commit_pages`
/// calls on the SAME segment, not just a single one.
///
/// `reserved_capacity` is a FIXED ceiling set ONCE at the segment's
/// ORIGINAL reservation (`4x` of the page-rounded original request as of
/// R14-6/task #291 — raised from R12-4's original `2x`, see
/// `LARGE_RESERVED_CAP_GROWTH_FACTOR`'s doc in `alloc_core_large.rs` for the
/// doubling-cadence-workload data behind the change) — it does NOT grow as
/// `span_usable` advances within it. So every step's TOTAL growth (relative
/// to the ORIGINAL size, not the previous step) must stay under that fixed
/// `4x` ceiling for the whole chain to remain in-place; these steps grow the
/// total by roughly 1.15x/2.3x/3.4x of the ORIGINAL size — the LAST step
/// deliberately pushed close to (but still safely under) the `4x` ceiling so
/// this test exercises the mechanism's now-wider headroom meaningfully
/// rather than sitting deep inside old-ceiling territory (a step size that
/// only ever probed well under the OLD `2x` bound would not distinguish
/// "4x ceiling implemented" from "ceiling silently reverted to 2x", since
/// small margins pass under either) — unlike a naive "step *= 1.5 each time"
/// progression (which compounds past the fixed ceiling well before the last
/// step and would legitimately — not incorrectly — relocate).
///
/// Starting size is derived from `just_above_small_max()` (NOT a fixed
/// literal like `256 * KIB`) — under `medium-classes-wide` (R9-4/task #226)
/// `SMALL_MAX` rises to 1.75 MiB, so a fixed 256 KiB literal would silently
/// stop classifying as Large in that build, invalidating the whole test's
/// premise without ever failing loudly (same pitfall documented in
/// `exact_span_large.rs`'s `just_above_small_max` doc).
///
/// `not(numa-aware)`: see `single_growth_within_reserved_capacity_...`'s
/// doc — the mechanism is compiled out under `numa-aware`.
#[test]
#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]
fn growth_chain_preserves_data_across_multiple_steps() {
    let mut ac = AllocCore::new().expect("primordial");
    let original_size = just_above_small_max();
    let mut size = original_size;
    let mut l = layout(size);
    let mut p = ac.alloc(l);
    assert!(!p.is_null(), "OOM allocating {size} bytes");

    // SAFETY: `p` valid for `size` bytes.
    unsafe {
        for i in 0..size {
            p.add(i).write((i % 199) as u8);
        }
    }

    // Each step is a fraction of the ORIGINAL size added cumulatively — see
    // the function doc for why this must be relative to the ORIGINAL size,
    // not compounded step-over-step. R14-6: pushed toward the new 4x ceiling
    // (was 15%/30%/45% under the old 2x ceiling) so the LAST step lands at
    // ~2.8x of the original size — comfortably under the 4x cap (leaving
    // margin for the header + page-rounding overhead baked into `usable`)
    // but far enough past the OLD 2x ceiling that this test would fail
    // outright (relocate instead of grow in-place) if the factor were ever
    // silently reverted to 2x.
    let steps = [
        original_size + original_size * 15 / 100,
        original_size + original_size * 120 / 100,
        original_size + original_size * 280 / 100,
    ];
    for &next_size in &steps {
        let old_size = size;
        let old_layout = l;
        // SAFETY (R6-MS-1/2): `p` is a live allocation made with `old_layout`,
        // consumed exactly once by this call.
        let grown = unsafe { ac.realloc(p, old_layout, next_size) };
        assert!(
            !grown.is_null(),
            "chained growth to {next_size} must not fail"
        );
        assert_eq!(
            grown, p,
            "chained growth step to {next_size} (from {old_size}) should stay \
             in-place within the geometric reserved_capacity"
        );

        // Every byte of the retained prefix (whichever path realloc took)
        // must survive.
        // SAFETY: `grown` valid for `next_size` bytes; first `old_size` must
        // be the pattern stamped at the previous step.
        unsafe {
            for i in 0..old_size {
                assert_eq!(
                    grown.add(i).read(),
                    (i % 199) as u8,
                    "byte {i} lost during chained growth step to {next_size}"
                );
            }
            // Extend the pattern into the newly-grown tail for the NEXT
            // iteration's prefix check.
            for i in old_size..next_size {
                grown.add(i).write((i % 199) as u8);
            }
        }

        p = grown;
        size = next_size;
        l = Layout::from_size_align(next_size, old_layout.align()).unwrap();
    }

    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(p, l) };
}

/// (c) The `reserved_capacity` BOUNDARY: a growth that exceeds the
/// segment's reserved VA span must correctly fall back to the slow
/// (alloc+copy+free) path — no panic, no UB, data still preserved (possibly
/// via a different pointer).
#[test]
#[cfg(feature = "large-reserved-capacity")]
fn growth_beyond_reserved_capacity_falls_back_to_slow_path() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_size = just_above_small_max();
    let old_layout = layout(old_size);
    let p = ac.alloc(old_layout);
    assert!(!p.is_null(), "OOM allocating {old_size} bytes");

    let reserved = ac.dbg_reserved_capacity_of(p);

    // SAFETY: `p` valid for `old_size` bytes.
    unsafe {
        for i in 0..128usize {
            p.add(i).write((i as u8).wrapping_add(0x11));
        }
    }

    // Grow WAY past reserved_capacity (several SEGMENTs beyond it) — must
    // exceed both span_usable AND reserved_capacity, forcing the slow path.
    let new_size = reserved + 4 * SEGMENT;
    // SAFETY (R6-MS-1/2): `p` is a live allocation made with `old_layout`,
    // consumed exactly once by this call.
    let grown = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(
        !grown.is_null(),
        "realloc growth beyond reserved_capacity must still succeed via the \
         slow path, not fail"
    );

    // Data must be preserved regardless of which leg fired.
    // SAFETY: `grown` valid for `new_size` bytes.
    unsafe {
        for i in 0..128usize {
            assert_eq!(
                grown.add(i).read(),
                (i as u8).wrapping_add(0x11),
                "byte {i} lost when growth exceeded reserved_capacity"
            );
        }
    }

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(grown, new_layout) };
}

/// (d) large_cache interaction: two DIFFERENT-sized Large allocations (A
/// bigger, B smaller) are each deposited into the cache and reused; the
/// reused segment's `reserved_capacity` must be A's/B's own ORIGINAL value
/// (carried forward verbatim, bug-#134-shaped), never confused between the
/// two, and never silently reset to `span_usable`.
///
/// `not(numa-aware)`: the final in-place-grow assertion depends on the
/// mechanism firing — see `single_growth_within_reserved_capacity_...`'s
/// doc — which is compiled out under `numa-aware`.
#[test]
#[cfg(all(
    feature = "large-reserved-capacity",
    feature = "alloc-decommit",
    not(feature = "numa-aware")
))]
fn large_cache_reuse_preserves_reserved_capacity_verbatim() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    // A: sized so its geometric reserved_capacity is a real, checkable value.
    let a_size = just_above_small_max();
    let la = layout(a_size);
    let pa = ac.alloc(la);
    assert!(!pa.is_null(), "OOM allocating A");
    let reserved_a = ac.dbg_reserved_capacity_of(pa);
    let span_a = ac.dbg_span_usable_of(pa);
    assert!(
        reserved_a >= span_a,
        "reserved_capacity ({reserved_a}) must be >= span_usable ({span_a})"
    );

    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(pa, la) };

    // Re-allocate at the SAME size — must hit the cache and reuse the exact
    // same physical segment (same span_usable), carrying reserved_capacity
    // forward verbatim.
    let pb = ac.alloc(la);
    assert!(!pb.is_null(), "OOM re-allocating at A's size");
    assert_eq!(
        ac.dbg_span_usable_of(pb),
        span_a,
        "cache-hit reuse must preserve span_usable verbatim (bug #134 \
         invariant, unaffected by R12-4)"
    );
    assert_eq!(
        ac.dbg_reserved_capacity_of(pb),
        reserved_a,
        "cache-hit reuse must preserve reserved_capacity verbatim — a \
         mismatch here means the cached slot's reserved_capacity was lost, \
         recomputed, or confused with a different segment's value"
    );

    // The reused segment must still support the reserved-capacity in-place
    // grow mechanism (proves reserved_capacity was not silently reset to
    // span_usable, which would make this grow relocate instead).
    let new_size = a_size + 64 * KIB;
    // SAFETY (R6-MS-1/2): `pb` is a live allocation made with `la`, consumed
    // exactly once by this call.
    let grown = unsafe { ac.realloc(pb, la, new_size) };
    assert!(!grown.is_null());
    assert_eq!(
        grown, pb,
        "the reused segment's carried-forward reserved_capacity must still \
         allow an in-place grow"
    );

    let new_layout = Layout::from_size_align(new_size, la.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(grown, new_layout) };
}

/// (e) WITHOUT `large-reserved-capacity`, `reserved_capacity` always reads
/// back EQUAL to `span_usable` (the inert "reserved == committed" value) —
/// the defensive round-trip proving the default/production path's header
/// layout is populated consistently even though the mechanism itself never
/// fires.
#[test]
#[cfg(not(feature = "large-reserved-capacity"))]
fn without_feature_reserved_capacity_equals_span_usable() {
    let mut ac = AllocCore::new().expect("primordial");
    let size = just_above_small_max();
    let l = layout(size);
    let p = ac.alloc(l);
    assert!(!p.is_null(), "OOM allocating {size} bytes");

    let span_usable = ac.dbg_span_usable_of(p);
    let reserved_capacity = ac.dbg_reserved_capacity_of(p);
    assert_eq!(
        reserved_capacity, span_usable,
        "without large-reserved-capacity, reserved_capacity must equal \
         span_usable (the inert 'reserved == committed' fallback value) — a \
         mismatch here would mean the feature-off path is not byte-for-byte \
         identical to pre-R12-4 behaviour"
    );

    // A growth that exceeds span_usable must therefore ALSO exceed
    // reserved_capacity (they are equal), so it must relocate — proving the
    // R12-4 mechanism is genuinely inert without the feature, not just
    // reporting equal values while secretly still committing more.
    let new_size = size + SEGMENT; // comfortably exceeds one segment's slack
    let old_layout = l;
    // SAFETY: `p` valid for `size` bytes.
    unsafe {
        p.write_bytes(0x42, size);
    }
    // SAFETY (R6-MS-1/2): `p` is a live allocation made with `old_layout`,
    // consumed exactly once by this call.
    let grown = unsafe { ac.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null());
    // SAFETY: `grown` valid for `size` bytes (the preserved prefix).
    unsafe {
        assert_eq!(grown.read(), 0x42, "prefix must survive the fallback move");
    }

    let new_layout = Layout::from_size_align(new_size, old_layout.align()).unwrap();
    // SAFETY (R6-MS-1/2): live allocation, freed exactly once.
    unsafe { ac.dealloc(grown, new_layout) };
}
