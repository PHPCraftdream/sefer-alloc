// R6-T38 review fix — compile-fail bait for the crate-only tail of the
// crate-root public types: outside this crate, `AllocCore::segment_bases`
// and `RouteDirectory::register` must be UNNAMEABLE (E0624, "method is
// private"), never silently callable. Compiled ONLY by
// tests/regression_r6_t38_public_tail_private.rs as a child process, like
// the sibling fixtures under tests/compile_fail/ (own detached [workspace]
// table, never part of the root target set).

fn probe_segment_bases(core: &mut sefer_alloc::AllocCore) {
    let _ = core.segment_bases();
}

fn probe_register(directory: &sefer_alloc::registry::segment_route::RouteDirectory) {
    let _ = directory.register(
        core::ptr::null_mut(),
        0,
        core::ptr::null_mut(),
        0,
        sefer_alloc::registry::segment_route::RouteKind::Small,
    );
}

fn main() {}
