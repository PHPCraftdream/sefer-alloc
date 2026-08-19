//! Fail-closed handling of a FAILED one-time OS page-size query.
//!
//! The hazard (audit item behind tasks #1139/#1145): `query_os_page_size`
//! used to collapse a failed `sysconf(_SC_PAGESIZE)` to `0`, which
//! `validate_page_size_impl` then folded to `PAGE` (4 KiB) — making a failed
//! query indistinguishable from a genuine 4 KiB answer, cached for the
//! process lifetime. On a 16/64 KiB-page host that believed-4-KiB page lets
//! decommit ranges pass validation that the OS then rounds UP to the real
//! page, discarding live data outside the requested range through a safe
//! API. The fix: a failed query POISONS the cache instead; `page_size()`
//! stays infallible (returns the `MIN_PAGE` floor) but every page-granular
//! state operation fails closed, reporting the OS-side no-code error
//! through the `try_*` forms and `try_page_size`.
//!
//! ## Oracle
//!
//! The failure is unreachable on real hardware (the page size comes from
//! process-startup data on every supported platform), so this test injects
//! it through `page_size_query_override::set_page_size_query_override` — the
//! raw-query seam that exists precisely because the CACHE seam
//! (`set_page_size_override`) structurally cannot express "the query
//! failed" (it validates every `Some` as a real page size).
//!
//! COUNTERFACTUAL (verified during development, recorded here): with the
//! pre-fix fold-to-`PAGE` behavior restored in `init_page_size_cache`, this
//! test FAILS at the first `try_page_size().unwrap_err()` (the pre-fix code
//! reports `Ok(4096)` for a failed query), and again at every fail-closed
//! assertion below (`try_decommit` returns `Ok` and really decommits).
//!
//! ## Containment
//!
//! The overrides are process-global; per the established forced-page-file
//! discipline (tests/page_size_override.rs), this binary contains exactly
//! ONE test so no sibling can race the override window, and a `Drop` guard
//! disarms the query override AND clears the cache even on panic.

#![cfg(aligned_vmem_page_size_override)]

use aligned_vmem::page_size_override::set_page_size_override;
use aligned_vmem::page_size_query_override::set_page_size_query_override;
use aligned_vmem::{page_size, reserve_aligned, try_page_size, MIN_PAGE};

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
fn failed_os_page_size_query_fails_closed() {
    // ONE test on purpose: the overrides are process-global (module docs).
    let _restore = RestoreRealQuery;

    // Baseline: on this host the real query works.
    let real = try_page_size().expect("real OS page-size query must succeed on a CI host");
    assert_eq!(page_size(), real);

    // Simulate a failed query (raw answer 0 — the shape a `sysconf` error
    // return maps to), then clear the cache so the next call re-queries.
    assert!(
        set_page_size_query_override(Some(0)),
        "an invalid raw answer simulates query failure and must be accepted"
    );
    set_page_size_override(None);

    // 1. The infallible accessor stays infallible and returns the
    //    conservative floor — NOT a value masquerading as a real answer.
    assert_eq!(
        page_size(),
        MIN_PAGE,
        "page_size() must degrade to the MIN_PAGE floor, not report a fake OS answer"
    );

    // 2. The fallible twin reports the failure — as an OS-side no-code
    //    failure, NOT a caller contract violation.
    let e = try_page_size().expect_err("try_page_size must report the failed query");
    assert!(
        !e.is_invalid_argument(),
        "a failed OS query is not the caller's fault"
    );
    assert_eq!(
        e.os_code(),
        None,
        "no per-call OS code exists for this state"
    );

    // 3. The reservation lifecycle is UNAFFECTED — reserve/use/release never
    //    consult the runtime page size. 64 KiB so that step 6's real-page
    //    decommit stays in-span on every supported host page size.
    let mut r = reserve_aligned(64 * 1024, MIN_PAGE)
        .expect("reserve_aligned must not depend on page_size()");
    // Live data the fail-closed decommit must not touch.
    // SAFETY: `r.as_ptr()` is the base of a live, exclusively-owned span of
    // at least MIN_PAGE bytes.
    unsafe { r.as_ptr().write(0xAB) };

    // 4. Every page-granular state operation fails CLOSED, with the no-code
    //    error on the try_ forms and `false` on the bool forms.
    let e = r
        .try_decommit(0, MIN_PAGE)
        .expect_err("try_decommit must fail closed under a poisoned page size");
    assert!(
        !e.is_invalid_argument(),
        "the poisoned page size must not be misreported as an argument error"
    );
    let e = r
        .try_recommit(0, MIN_PAGE)
        .expect_err("try_recommit must fail closed under a poisoned page size");
    assert!(!e.is_invalid_argument());
    assert!(
        !r.recommit(0, MIN_PAGE),
        "the bool form must refuse (the caller must not write on false, so \
         false is the fail-closed answer)"
    );
    #[cfg(feature = "lazy-commit")]
    {
        let e = aligned_vmem::try_reserve_aligned_lazy(MIN_PAGE, MIN_PAGE, MIN_PAGE)
            .expect_err("the lazy constructor must fail closed under a poisoned page size");
        assert!(!e.is_invalid_argument());
    }
    // SAFETY: same span as the write above; reading back one byte.
    assert_eq!(
        unsafe { r.as_ptr().read() },
        0xAB,
        "fail-closed state operations must leave the data untouched"
    );

    // 5. While poisoned, the CACHE seam refuses to arm (no real-page floor
    //    exists to validate against).
    assert!(
        !set_page_size_override(Some(64 * 1024)),
        "set_page_size_override must refuse to arm while the real query is failing"
    );

    // 6. Restoration: disarm the query override, clear the cache, and the
    //    real page size — and real behavior — come back.
    assert!(set_page_size_query_override(None));
    assert!(set_page_size_override(None));
    assert_eq!(page_size(), real);
    if real <= r.len() {
        r.try_decommit(0, real)
            .expect("decommit must work again once the real query is restored");
    }
}
