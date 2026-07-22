//! R12-4 THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Measures the realloc-growth-chain behaviour the R12-4 feature
//! (`large-reserved-capacity`) targets: a Large allocation growing in
//! several steps (256 KiB -> 4 MiB) via `AllocCore::realloc`. Reports, per
//! chain:
//!   - move legs: how many of the growth steps RELOCATED (returned a
//!     different pointer) vs stayed IN PLACE.
//!   - copied bytes: total bytes copied by the move legs (the `old_size` of
//!     each relocating step — this is exactly what `Node::copy_nonoverlapping`
//!     moves in the real `AllocCore::realloc` slow path).
//!   - realloc latency: total wall-clock across the whole chain (best of a
//!     few repeats).
//!   - peak commit/RSS: `proc-memstat::snapshot()` immediately after the
//!     chain completes (same-instant RSS + commit-charge probe, matching
//!     `r12_3_exact_span_measure.rs`'s own methodology).
//!
//! Run once per feature combination (each is a separate process so the
//! comparison is honest — no committed-page carryover between arms):
//!   cargo run --release --example r12_4_reserved_capacity_measure --features production
//!   cargo run --release --example r12_4_reserved_capacity_measure --features "production,exact-span-large"
//!   cargo run --release --example r12_4_reserved_capacity_measure --features "production,exact-span-large,large-reserved-capacity"
//!
//! (The middle arm is R12-3 ALONE — the "problem" this task's own analysis
//! predicts: exact-span-large's tight `span_usable` starves OPT-G. The third
//! arm is R12-3 + R12-4 together — the proposed fix.)

use core::alloc::Layout;
use proc_memstat::snapshot;
use sefer_alloc::AllocCore;
use std::time::Instant;

const KIB: usize = 1024;
const MIB: usize = 1024 * 1024;

/// One realloc-growth chain: 256 KiB -> 512 KiB -> 1 MiB -> 2 MiB -> 4 MiB.
/// Each step's `old_size` is exactly the bytes a relocating move leg would
/// copy.
const CHAIN: [usize; 5] = [256 * KIB, 512 * KIB, MIB, 2 * MIB, 4 * MIB];

struct ChainResult {
    move_legs: u32,
    in_place_legs: u32,
    copied_bytes: u64,
    wall_ns: u64,
}

fn run_chain_once() -> ChainResult {
    let mut ac = AllocCore::new().expect("primordial");
    let mut size = CHAIN[0];
    let layout0 = Layout::from_size_align(size, 8).unwrap();
    let mut ptr = ac.alloc(layout0);
    assert!(!ptr.is_null(), "OOM allocating initial {size} bytes");

    let mut move_legs = 0u32;
    let mut in_place_legs = 0u32;
    let mut copied_bytes = 0u64;

    let start = Instant::now();
    for &next_size in &CHAIN[1..] {
        let old_size = size;
        let old_layout = Layout::from_size_align(old_size, 8).unwrap();
        // SAFETY: `ptr` is a live allocation from this AllocCore made with
        // `old_layout`, consumed exactly once by this call.
        let grown = unsafe { ac.realloc(ptr, old_layout, next_size) };
        assert!(!grown.is_null(), "realloc growth to {next_size} failed");
        if grown == ptr {
            in_place_legs += 1;
        } else {
            move_legs += 1;
            copied_bytes += old_size as u64;
        }
        ptr = grown;
        size = next_size;
    }
    let wall_ns = start.elapsed().as_nanos() as u64;

    let final_layout = Layout::from_size_align(size, 8).unwrap();
    // SAFETY: `ptr` is a live allocation from this AllocCore made with
    // `final_layout`, freed exactly once here.
    unsafe { ac.dealloc(ptr, final_layout) };

    ChainResult {
        move_legs,
        in_place_legs,
        copied_bytes,
        wall_ns,
    }
}

fn main() {
    let exact_span = cfg!(feature = "exact-span-large");
    let reserved_cap = cfg!(feature = "large-reserved-capacity");
    println!("=== R12-4 reserved-capacity realloc-growth-chain measurement ===");
    println!(
        "feature exact-span-large:        {}",
        if exact_span { "ON" } else { "OFF" }
    );
    println!(
        "feature large-reserved-capacity: {}",
        if reserved_cap { "ON" } else { "OFF" }
    );
    println!(
        "chain: {:?} KiB",
        CHAIN.iter().map(|b| b / KIB).collect::<Vec<_>>()
    );
    println!();

    // Correctness/behavior sample: run once and report the per-chain move-leg
    // breakdown (deterministic given the feature set — not a distribution).
    let sample = run_chain_once();
    println!(
        "move legs (relocated): {}  |  in-place legs: {}  |  copied bytes total: {}",
        sample.move_legs, sample.in_place_legs, sample.copied_bytes
    );

    // Latency: best of 20 repeats (fresh AllocCore each time — cold-start
    // consistent measurement, matching r12_3's per-size isolation).
    let best_ns = (0..20).map(|_| run_chain_once().wall_ns).min().unwrap();
    println!("realloc chain wall-clock (best of 20): {best_ns} ns");

    // Peak commit/RSS: measure a FRESH chain's before/after delta (isolated
    // AllocCore, isolated process — no cross-arm interference).
    let before = snapshot();
    let mut ac = AllocCore::new().expect("primordial");
    let mut size = CHAIN[0];
    let layout0 = Layout::from_size_align(size, 8).unwrap();
    let mut ptr = ac.alloc(layout0);
    assert!(!ptr.is_null());
    for &next_size in &CHAIN[1..] {
        let old_layout = Layout::from_size_align(size, 8).unwrap();
        let grown = unsafe { ac.realloc(ptr, old_layout, next_size) };
        assert!(!grown.is_null());
        ptr = grown;
        size = next_size;
    }
    // Touch the final payload so every committed page is actually faulted in
    // (RSS only counts faulted pages, matching r12_3's own touch discipline).
    unsafe {
        ptr.write_bytes(0xAB, size);
    }
    let after = snapshot();
    let rss_delta = after.rss.saturating_sub(before.rss);
    let commit_delta = after.commit.saturating_sub(before.commit);
    println!(
        "peak RSS delta: {} KiB  |  peak commit-charge delta: {} KiB",
        rss_delta / 1024,
        commit_delta / 1024
    );
    // SAFETY: `ptr` is a live allocation from this AllocCore, freed exactly
    // once here.
    unsafe { ac.dealloc(ptr, Layout::from_size_align(size, 8).unwrap()) };
    drop(ac);
}
