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
//! - E0463: prefix "can't find crate for `" and suffix
//!   " which `sefer_alloc` depends on".
//! - E0603: "module `registry` is private".
//!
//! Other failures are hard errors; only the exact count-matched uncoded abort
//! summary is allowed. Compatible candidates are not proven linked into the
//! current test build.

fn positive(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    route.prepare_small(0, 0);
    route.issue_small(0, 0);
    let _ = route.small_sidecar().unwrap().scan(0);
}

fn main() {}
