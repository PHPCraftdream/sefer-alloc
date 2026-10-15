// R6-T38 compile-fail bait: in a build without `internals`,
// `AllocCore::segment_bases` must be crate-private (E0624, "method is
// private"), never callable from outside the crate. `RouteDirectory::register`
// needs no probe here: without `internals` the whole `registry` module is
// crate-private. Compiled ONLY by
// tests/regression_r6_t38_public_tail_private.rs as a child process, like
// the sibling fixtures under tests/compile_fail/ (own detached [workspace]
// table, never part of the root target set).

fn probe_segment_bases(core: &sefer_alloc::AllocCore) {
    let _ = core.segment_bases().count();
}

fn main() {
    let _ = probe_segment_bases;
}
