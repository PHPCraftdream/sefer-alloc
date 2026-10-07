//! R14-01 negative compile-fail oracle, compiled directly against the built
//! `sefer_alloc` rlib. Same shape as `small_sidecar_shared_prepare_not_callable`
//! but exercising `SmallSidecar::issue`: unsealing ONLY `prepare` must still
//! fail this regression with exactly one E0624 naming the private `issue`.

fn negative(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    let sidecar = route.small_sidecar().unwrap();
    let _ = sidecar.issue(0, 0);
}

fn main() {}
