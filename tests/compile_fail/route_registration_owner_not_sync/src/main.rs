//! R14-01 negative compile-fail oracle, compiled directly against the built
//! `sefer_alloc` rlib. `RouteRegistration` pins owner-only mutation with a
//! `PhantomData<Cell<()>>` marker, so it is `!Sync`: a shared reference to a
//! registration must not satisfy a `Sync` requirement. Fails with exactly one
//! E0277 while the marker in registration.rs stands.

fn require_sync<T: ?Sized + Sync>(_: &T) {}

fn check(route: &sefer_alloc::registry::segment_route::RouteRegistration<'_>) {
    require_sync(route);
}

fn main() {}
