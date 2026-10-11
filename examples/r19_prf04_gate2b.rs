//! Pure production retention: no allocator wrapper.
use sefer_alloc::SeferAlloc;
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
#[path = "r19_prf04_gate2b_shared/harness.rs"]
mod harness;
fn clock() -> u64 {
    0
}
fn observation(_: u64) -> [u64; 5] {
    [0; 5]
}
fn main() {
    harness::run(&GLOBAL, false);
}
