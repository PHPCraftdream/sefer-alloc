//! R32-0 (task #490) — wall-clock A/B companion probe, **baseline arm**
//! (`virgin-zero-skip` OFF). See
//! `examples/_shared/r32_0_virgin_zero_skip_cost_probe_workload.rs`'s module
//! doc for the full design (real `HeapCore` production layer, plain-`alloc`
//! recycled/steady-state-hit worst case at 4 KiB).
//!
//! **Build:** `cargo build --release --example r32_0_cost_probe_alloc_recycled_off --features "production alloc-stats"`

include!("_shared/r32_0_virgin_zero_skip_cost_probe_workload.rs");

fn main() {
    run("alloc_recycled_off");
}
