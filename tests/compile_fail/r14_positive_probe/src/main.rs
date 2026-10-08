//! Artifact-selection input for the R14-01 negative harness (NOT a
//! permanent assertion): a fixture that only exercises the PUBLIC owner API
//! of `RouteRegistration`. A candidate `libsefer_alloc-*.rlib` is COMPATIBLE
//! when this compiles cleanly with exit 0; it is INCOMPATIBLE only on exit 1
//! with only these restricted coded messages:
//! - E0432: "unresolved import `sefer_alloc::registry`".
//! - E0433: "cannot find `registry` in `sefer_alloc`" or
//!   "could not find `registry` in `sefer_alloc`".
//! - E0460: prefix "found possibly newer version of crate `" and suffix
//!   "` which `sefer_alloc` depends on".
//! - E0461: prefix "couldn't find crate `sefer_alloc` with expected target triple ".
//! - E0463: prefix "can't find crate for `" and suffix
//!   " which `sefer_alloc` depends on".
//! - E0603: "module `registry` is private".
//!
//! E0461 is rustc's foreign-target verdict, not a layout or marker inference.
//! A candidate counts as foreign only on exit 1 with nonempty coded errors
//! all matching that verdict. The harness skips the test only when candidates
//! are nonempty, every candidate is foreign, and none is compatible. Empty
//! candidate sets and all-feature-incompatible sets hard fail. With a usable
//! current native artifact the checks run, including on native arm64.
//! Other failures (including stale-source E0599/E0624 in reused target dirs)
//! are hard errors; only the exact count-matched uncoded abort summary is
//! allowed. Compatible candidates are not proven linked into the current test
//! build.

fn positive(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    route.prepare_small(0, 0);
    route.issue_small(0, 0);
    let _ = route.small_sidecar().unwrap().scan(0);
}

fn main() {}
