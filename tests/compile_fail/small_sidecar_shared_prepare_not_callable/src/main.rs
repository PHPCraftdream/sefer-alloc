//! R14-01 negative compile-fail oracle, compiled directly against the built
//! `sefer_alloc` rlib. Every statement is valid except the final call to
//! `SmallSidecar::clear_pending_start_bit` through a SHARED `&SmallSidecar`
//! obtained from `RouteRegistration::small_sidecar()` — the owner-only
//! carve-clear seam is `pub(crate)` and must fail here with exactly one
//! E0624 (D1' phase-1b: the former `prepare` fixture target was removed
//! with the issue transaction). The function body is typechecked even when
//! never called, so the fixture never constructs a registration at runtime.

fn negative(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    let sidecar = route.small_sidecar().unwrap();
    sidecar.clear_pending_start_bit(0);
}

fn main() {}
