//! R14-01 negative compile-fail oracle, compiled directly against the built
//! `sefer_alloc` rlib. Same shape as `small_sidecar_shared_prepare_not_callable`
//! but exercising `SmallSidecar::issue`: unsealing ONLY the owner issue path
//! (`issue_for_test`) must still fail this regression with exactly one E0624
//! naming the private `issue` (D1' phase-1b: the issue is infallible — one
//! argument — but stays `pub(crate)`).

fn negative(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    let sidecar = route.small_sidecar().unwrap();
    sidecar.issue(0);
}

fn main() {}
