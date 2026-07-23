//! R14-3 (task #288) process-level A/B/B/A judge binary, **treatment arm**
//! (`class-aware-dirty` ON).
//!
//! Companion to `paired_ab_class_aware_dirty_off.rs` — see that file's module
//! doc for the full rationale, the fixed-work protocol, and why both axes
//! (`elapsed_ns` full round, `window_ns` sub-window) are emitted. The ONLY
//! difference between the two binaries is this one is built WITH
//! `class-aware-dirty`; both `include!` the identical workload source
//! (`examples/_shared/paired_ab_class_aware_dirty_workload.rs`).
//!
//! **Build:** `cargo build --release --example paired_ab_class_aware_dirty_on --features "production alloc-stats class-aware-dirty"`

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats"
))]

// Shared fixed-work round body — identical include to
// `paired_ab_class_aware_dirty_off.rs`; see that file / the shared workload's
// own module doc.
include!("_shared/paired_ab_class_aware_dirty_workload.rs");

fn main() {
    let _ = bootstrap::ensure();

    let (full_round_ns, window_ns, owner_allocs) = run_fixed_work_round();

    let segments_reserved_total = AllocCore::dbg_segments_reserved_total();

    proc_probe::emit("arm", "class_aware_dirty_on");
    proc_probe::emit_ns("elapsed_ns", full_round_ns);
    proc_probe::emit_ns("window_ns", window_ns);
    proc_probe::emit_u64("owner_allocs", owner_allocs as u64);
    proc_probe::emit_u64("segments_reserved_total", segments_reserved_total);
}

#[cfg(not(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats"
)))]
fn main() {
    eprintln!(
        "paired_ab_class_aware_dirty_on: requires --features \
         \"alloc-global alloc-xthread alloc-segment-directory alloc-stats class-aware-dirty\" \
         (e.g. \"production alloc-stats class-aware-dirty\")"
    );
    std::process::exit(1);
}
