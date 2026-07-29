//! R29-4 (task #435) — segment-state reconciliation gate.
//!
//! MEASUREMENT-ONLY. Extends R27-3's retention finding (`docs/perf/
//! R27_3_POOL_RETENTION_GATE.md` §2) by attributing the ~+4 MiB/heap
//! "committed-non-pooled" residual (the half of the post-teardown RSS delta
//! that survived pool drain and was NEVER reconciled to a tracked counter) to
//! a SPECIFIC named segment state — or honestly reporting that it does not
//! fully reconcile.
//!
//! ## How it works
//!
//! Uses the new `HeapCore::dbg_segment_state_reconciliation()` hook (a
//! `bench-internals`-gated safe `pub fn` that iterates every registered
//! segment-table slot, classifies each into exactly one state, and totals
//! committed/reserved bytes per state) at two measurement points per arm:
//! post-teardown and post-drain. Runs the SAME batch-120 pressure shape as
//! R27-3 (same `hc_run_batch`, same config builder, same self-verification)
//! but single-threaded (one heap per child process — the per-heap state
//! accounting is exact and deterministic, so the multi-thread RSS-scaling
//! apparatus of R27-3 is unnecessary for THIS question).
//!
//! ## Subprocess isolation
//!
//! One child process per `(pool_segments, pool_byte_cap)` arm (2 children
//! total). A fresh process has a fresh `HeapRegistry`, so the first-claim-wins
//! registry-slot reuse bug that invalidated R25-5's RSS axis is eliminated by
//! construction (same rationale as R27-3 / R26-1). Each child hard-asserts
//! `verified_cap == pool_segments` and `config_conflicts_delta == 0` before
//! its number is trusted.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r29_4_segment_state_reconciliation_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::process::Command;

use sefer_alloc::{
    alloc_core::SegmentStateReconciliation,
    registry::{config_conflicts_total, HeapCore, HeapRegistry},
    LargeCacheConfig, SmallSegmentPoolConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters — the REAL candidate configs (matching R27-3).
// ---------------------------------------------------------------------------

const CONFIGS: &[(usize, usize)] = &[(4, 16 * 1024 * 1024), (8, 32 * 1024 * 1024)];

const SIZE: usize = 1024;
const CHURN_WORKING_SET: usize = 256;
const OPS: usize = 1024;

/// Pressure-producing batch size (same as R27-3).
const PRESSURE_BATCH_SIZE: usize = 120;

/// Number of pressure batches to run (R27-3 used a 1.5 s time window; we use
/// a fixed count for determinism — the per-state accounting is exact, so we
/// need enough batches for the pool to settle, not a specific wall-clock).
const PRESSURE_BATCHES: usize = 20;

// ---------------------------------------------------------------------------
// Churn primitives — byte-for-byte copies of R27-3's (via R25-5/R26-1).
// ---------------------------------------------------------------------------

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    #[inline]
    fn next_usize(&mut self) -> usize {
        self.next_u64() as usize
    }
}

fn hc_churn_prefill(heap: &mut HeapCore, layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        let p = heap.alloc(layout);
        live.push(p);
    }
    live
}

fn hc_churn_step(heap: &mut HeapCore, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `heap` with `layout` above.
            unsafe { heap.dealloc(old, layout) };
        }
        live[idx] = heap.alloc(layout);
    }
    std::hint::black_box(&live);
}

fn hc_churn_teardown(heap: &mut HeapCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `heap` with `layout` above.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

fn hc_run_batch(heap: &mut HeapCore, layout: Layout, batch_size: usize) {
    let mut inputs: Vec<Vec<*mut u8>> = (0..batch_size)
        .map(|_| hc_churn_prefill(heap, layout, CHURN_WORKING_SET))
        .collect();
    for live in &mut inputs {
        hc_churn_step(heap, layout, live, OPS);
        hc_churn_teardown(heap, layout, live);
    }
}

// ---------------------------------------------------------------------------
// Config builder (same as R27-3).
// ---------------------------------------------------------------------------

fn config_for(pool_segments: usize, pool_byte_cap: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(pool_segments)
            .pool_byte_cap(pool_byte_cap),
    )
}

// ---------------------------------------------------------------------------
// Child/arm mode.
// ---------------------------------------------------------------------------

fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

fn emit_state(prefix: &str, rec: &SegmentStateReconciliation) {
    let states = [
        ("primordial", &rec.primordial),
        ("small_pooled", &rec.small_pooled),
        ("small_active", &rec.small_active),
        ("small_empty_orphan", &rec.small_empty_orphan),
        (
            "small_decommitted_retained",
            &rec.small_decommitted_retained,
        ),
        ("large_active", &rec.large_active),
        ("large_cached", &rec.large_cached),
    ];
    for (name, acct) in &states {
        println!("RESULT {prefix}_state_{name}_count={}", acct.count);
        println!(
            "RESULT {prefix}_state_{name}_committed_kib={}",
            acct.committed_bytes / 1024
        );
        println!(
            "RESULT {prefix}_state_{name}_reserved_kib={}",
            acct.reserved_bytes / 1024
        );
    }
    println!("RESULT {prefix}_total_count={}", rec.total.count);
    println!(
        "RESULT {prefix}_total_committed_kib={}",
        rec.total.committed_bytes / 1024
    );
    println!(
        "RESULT {prefix}_total_reserved_kib={}",
        rec.total.reserved_bytes / 1024
    );
    println!("RESULT {prefix}_unknown_count={}", rec.unknown_count);
}

fn run_child() {
    let pool_segments = parse_env_usize("R29_4_POOL_SEGMENTS");
    let pool_byte_cap = parse_env_usize("R29_4_POOL_BYTE_CAP");
    let pool_byte_cap_mib = (pool_byte_cap / (1024 * 1024)) as u64;

    let conflicts_before = config_conflicts_total();

    // Claim the heap.
    let heap_ptr = HeapRegistry::claim_with_config(config_for(pool_segments, pool_byte_cap));
    assert!(
        !heap_ptr.is_null(),
        "HeapRegistry::claim_with_config returned null"
    );
    // SAFETY: `heap_ptr` was just returned by `claim_with_config`.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    // SELF-VERIFICATION #1: resolved cap equals the requested one.
    let resolved = heap.dbg_pool_cap();
    assert_eq!(
        resolved, pool_segments,
        "resolved cap ({resolved}) != requested ({pool_segments})"
    );

    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    // Warm-up.
    {
        let mut live = hc_churn_prefill(heap, layout, CHURN_WORKING_SET);
        hc_churn_step(heap, layout, &mut live, OPS);
        hc_churn_teardown(heap, layout, &live);
    }

    // Pressure phase: batched churn.
    for _ in 0..PRESSURE_BATCHES {
        hc_run_batch(heap, layout, PRESSURE_BATCH_SIZE);
    }

    // Post-teardown snapshot.
    let rec_post = heap.dbg_segment_state_reconciliation();
    let pooled_post = heap.dbg_pooled_count();
    let table_count_post = heap.segment_bases().count();
    let decommit_post = sefer_alloc::AllocCore::dbg_decommit_count();

    // Verify identity: sum of per-state counts == table count.
    let sum_counts = rec_post.primordial.count
        + rec_post.small_pooled.count
        + rec_post.small_active.count
        + rec_post.small_empty_orphan.count
        + rec_post.small_decommitted_retained.count
        + rec_post.large_active.count
        + rec_post.large_cached.count
        + rec_post.unknown_count;
    assert_eq!(
        sum_counts, table_count_post,
        "RECONCILIATION IDENTITY FAILED post-teardown: sum of per-state counts ({sum_counts}) \
         != table.count() ({table_count_post}) — a segment was not classified"
    );

    // Drain the pool.
    let drained = heap.dbg_drain_small_pool();

    // Post-drain snapshot.
    let rec_drain = heap.dbg_segment_state_reconciliation();
    let table_count_drain = heap.segment_bases().count();

    // Verify identity post-drain too.
    let sum_counts_drain = rec_drain.primordial.count
        + rec_drain.small_pooled.count
        + rec_drain.small_active.count
        + rec_drain.small_empty_orphan.count
        + rec_drain.small_decommitted_retained.count
        + rec_drain.large_active.count
        + rec_drain.large_cached.count
        + rec_drain.unknown_count;
    assert_eq!(
        sum_counts_drain, table_count_drain,
        "RECONCILIATION IDENTITY FAILED post-drain: sum of per-state counts ({sum_counts_drain}) \
         != table.count() ({table_count_drain})"
    );

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // Emit scalar fields.
    proc_probe::emit_u64("pool_segments", pool_segments as u64);
    proc_probe::emit_u64("pool_byte_cap_mib", pool_byte_cap_mib);
    proc_probe::emit_u64("verified_cap", pool_segments as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("pooled_post", pooled_post as u64);
    proc_probe::emit_u64("table_count_post", table_count_post as u64);
    proc_probe::emit_u64("decommit_total_post", decommit_post);
    proc_probe::emit_u64("drained", drained as u64);
    proc_probe::emit_u64("table_count_drain", table_count_drain as u64);

    // Emit per-state breakdowns.
    emit_state("post", &rec_post);
    emit_state("drain", &rec_drain);

    println!(
        "OK cap={pool_segments} byte_cap={pool_byte_cap_mib}MiB verified_cap={pool_segments} \
         cfg_conflicts_delta={conflicts_delta} pooled_post={pooled_post} \
         table_post={table_count_post} drained={drained} table_drain={table_count_drain} \
         post_empty_orphan={} drain_empty_orphan={}",
        rec_post.small_empty_orphan.count, rec_drain.small_empty_orphan.count,
    );

    // SAFETY: `heap_ptr` was returned by `claim_with_config` above.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}

// ---------------------------------------------------------------------------
// Orchestrator mode.
// ---------------------------------------------------------------------------

fn run_orchestrator() {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("cannot get current_exe: {e}"));

    println!("=== R29-4 segment-state reconciliation gate ===");
    println!();

    for &(pool_segments, pool_byte_cap) in CONFIGS {
        let label = format!("cap{}_{}MiB", pool_segments, pool_byte_cap / (1024 * 1024));
        println!("--- arm: {label} ---");

        let output = Command::new(&exe)
            .env("R29_4_CHILD", "1")
            .env("R29_4_POOL_SEGMENTS", pool_segments.to_string())
            .env("R29_4_POOL_BYTE_CAP", pool_byte_cap.to_string())
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn child for {label}: {e}"));

        if !output.status.success() {
            eprintln!("CHILD STDERR for {label}:");
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            panic!("child for {label} failed: {}", output.status);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{stdout}");
        println!();
    }

    // Summary: print the key comparison.
    println!("=== SUMMARY: post-teardown per-state segment counts ===");
    println!("(see RESULT lines above for full per-state breakdown)");
}

fn main() {
    if std::env::var("R29_4_CHILD").is_ok() {
        run_child();
    } else {
        run_orchestrator();
    }
}
