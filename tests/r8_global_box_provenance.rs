//! Actual Box Drop calls through installed SeferAlloc, including narrow
//! typed provenance. The custom Miri target invokes this same support body.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

#[path = "support/r8_global_box_witness.rs"]
mod witness;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

#[test]
fn installed_box_drop_narrow_transfer_and_reissue() {
    assert_eq!(witness::narrow_two_rounds(&GLOBAL), 2);
}

#[cfg(miri)]
#[test]
fn installed_box_drop_retires_before_terminal_producer_resumes() {
    assert_eq!(witness::paused_terminal_owner_retirement(&GLOBAL), 1);
}
