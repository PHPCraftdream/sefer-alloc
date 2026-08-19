//! Task #1173 (finding L1) — a poisoned page-size query must NOT panic in a
//! debug build; `decommit`/`Reservation::decommit` are documented, both in
//! `page_size()`'s own rustdoc and in the README's "If the one-time OS query
//! fails" section, as becoming unconditional no-ops under that state — with
//! no build-profile qualifier, unlike the range-contract tripwire, which IS
//! documented as debug-only.
//!
//! ## The defect this pins
//!
//! Before this task, `crate::api::decommit::decommit`'s poison branch
//! (`ps == PAGE_SIZE_QUERY_FAILED`) called `debug_assert!(false, ...)`
//! unconditionally — so ANY call to the free `decommit` function or the
//! `Reservation::decommit` method (which forwards to it) panicked in a debug
//! build whenever the one-time OS page-size query had failed, regardless of
//! how well-formed the caller's own range was. That contradicts the design
//! decision recorded in the commit that introduced the poison mechanism
//! (task #1145/#1139, `4cba9c1`): "Rejected: panicking (the README's 'never
//! panics' list stays at three)". The free function's own rustdoc has no
//! `# Panics` section at all, and `Reservation::decommit`'s `# Panics`
//! section only ever documented the range-contract tripwire — neither doc
//! ever promised (or even mentioned) a poison-state panic. This test proves
//! the code now matches that documented/designed no-op contract.
//!
//! ## Oracle
//!
//! Uses the same `page_size_query_override` seam as
//! `tests/page_size_query_failure.rs` to simulate a failed query without
//! needing real OS failure (unreachable on real hardware — the page size
//! comes from process-startup data on every supported platform).
//!
//! COUNTERFACTUAL (verified during development, recorded here): restoring
//! the pre-fix `debug_assert!(false, ...)` in the poison branch of
//! `crate::api::decommit::decommit` makes this test's debug-build run PANIC
//! at the very first `decommit`/`Reservation::decommit` call under poison
//! (`method_decommit_is_a_silent_no_op_under_poison`); with the fix, the
//! same debug build passes. `cargo test` on this workspace always builds
//! test binaries WITHOUT `--release`, so the debug-only tripwire path is
//! exactly the one this default test run exercises — see the raw output
//! captured in this task's report for both directions, and item #1073's
//! stale-artifact warning (`touch` + rebuild before trusting either run).
//!
//! ## Containment
//!
//! The overrides are process-global; per the established forced-page-file
//! discipline (tests/page_size_override.rs, tests/page_size_query_failure.rs)
//! this binary contains exactly ONE test so no sibling can race the override
//! window, and a `Drop` guard disarms the query override AND clears the
//! cache even on panic.

#![cfg(aligned_vmem_page_size_override)]

use aligned_vmem::page_size_override::set_page_size_override;
use aligned_vmem::page_size_query_override::set_page_size_query_override;
use aligned_vmem::{decommit, page_size, reserve_aligned, MIN_PAGE};

/// Disarms the raw-query override and re-arms the query-on-next-call cache
/// sentinel, restoring the real OS page size even if the test panics.
struct RestoreRealQuery;

impl Drop for RestoreRealQuery {
    fn drop(&mut self) {
        set_page_size_query_override(None);
        set_page_size_override(None);
    }
}

#[test]
fn method_decommit_is_a_silent_no_op_under_poison() {
    // ONE test on purpose: the overrides are process-global (module docs).
    let _restore = RestoreRealQuery;

    // Baseline: on this host the real query works.
    let real = page_size();

    // Live reservation, with data the poisoned no-op must not touch — 64
    // KiB so the decommit range below stays in-span on every supported
    // host page size.
    let mut r = reserve_aligned(64 * 1024, MIN_PAGE)
        .expect("reserve_aligned must not depend on page_size()");
    // SAFETY: `r.as_ptr()` is the base of a live, exclusively-owned span of
    // at least MIN_PAGE bytes.
    unsafe { r.as_ptr().write(0xCD) };

    // Simulate a failed query (raw answer 0 — the shape a `sysconf` error
    // return maps to), then clear the cache so the next call re-queries.
    assert!(
        set_page_size_query_override(Some(0)),
        "an invalid raw answer simulates query failure and must be accepted"
    );
    set_page_size_override(None);
    assert_eq!(
        page_size(),
        aligned_vmem::MIN_PAGE,
        "sanity: page_size() must be poisoned (degraded to the floor) before \
         this test's real assertions run"
    );

    // 1. `Reservation::decommit` — a WELL-FORMED, in-span, page-aligned
    //    range — must be a silent no-op under poison, not a panic. This is
    //    the exact call shape a debug-build consumer would make in ordinary
    //    use; before the fix it panicked regardless of this well-formedness.
    r.decommit(0, MIN_PAGE);

    // 2. The free `decommit` function, called directly, must likewise not
    //    panic under poison for a well-formed range.
    // SAFETY: `r.as_ptr()` is the base of the same live reservation; the
    // range is within its usable span and page-aligned.
    unsafe { decommit(r.as_ptr(), 0, MIN_PAGE) };

    // 3. The poisoned no-op must not have touched the data — no OS call was
    //    made, so the byte written above must still read back unchanged.
    // SAFETY: same span as the write above; reading back one byte.
    assert_eq!(
        unsafe { r.as_ptr().read() },
        0xCD,
        "a poisoned-state decommit call must be a true no-op: the data must \
         be untouched, exactly like the documented degraded-state contract \
         for every other page-granular state operation"
    );

    // Restoration: disarm the query override, clear the cache, and the real
    // page size — and real behavior — come back.
    assert!(set_page_size_query_override(None));
    assert!(set_page_size_override(None));
    assert_eq!(page_size(), real);
}
