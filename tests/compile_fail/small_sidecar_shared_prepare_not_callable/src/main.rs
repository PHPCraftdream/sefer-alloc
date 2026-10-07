//! R14-01 negative compile-fail oracle, compiled directly against the built
//! `sefer_alloc` rlib. Every statement is valid except the final call to
//! `SmallSidecar::prepare` through a SHARED `&SmallSidecar` obtained from
//! `RouteRegistration::small_sidecar()` — after the R14-01 capability move,
//! `prepare` is `pub(crate)` owner-only and must fail here with exactly one
//! E0624. The function body is typechecked even when never called, so the
//! fixture never constructs a registration at runtime.

fn negative(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    let sidecar = route.small_sidecar().unwrap();
    let _ = sidecar.prepare(0, 0);
}

fn main() {}
