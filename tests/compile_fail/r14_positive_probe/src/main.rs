//! Artifact-selection input for the R14-01 negative harness (NOT a
//! permanent assertion): a fixture that only exercises the PUBLIC owner API
//! of `RouteRegistration`. A candidate `libsefer_alloc-*.rlib` is COMPATIBLE
//! when this compiles cleanly with exit 0; it is INCOMPATIBLE only on exit 1
//! with coded E0432/E0433/E0603 diagnostics specifically identifying a
//! missing/private `registry` module. Uncoded errors and any other failure
//! are hard harness errors.

fn positive(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    route.prepare_small(0, 0);
    route.issue_small(0, 0);
    let _ = route.small_sidecar().unwrap().scan(0);
}

fn main() {}
