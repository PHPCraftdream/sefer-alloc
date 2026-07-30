//! R31-2 (task #465) — multi-threaded pool-cap THRESHOLD sweep, arm
//! **cap16** (`pool_segments=16, pool_byte_cap=64 MiB`).
//!
//! Sibling of `r31_2_pool_cap_threshold_ab_cap4.rs` — see that file's module
//! doc for the full rationale and workload description. The ONLY difference
//! between the four sibling binaries (`cap4`/`cap8`/`cap16`/`cap32`) is the
//! `(pool_segments, pool_byte_cap)` pair baked into the `static`; the
//! workload body (`include!`d from the same shared file) is byte-identical.
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r31_2_pool_cap_threshold_ab_cap4 --example r31_2_pool_cap_threshold_ab_cap8 --example r31_2_pool_cap_threshold_ab_cap16 --example r31_2_pool_cap_threshold_ab_cap32 --features "production alloc-stats"
//! node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap16
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

const POOL_SEGMENTS: usize = 16;
const POOL_BYTE_CAP: usize = 64 * 1024 * 1024;

const CONFIG: LargeCacheConfig = LargeCacheConfig::new().pool(
    SmallSegmentPoolConfig::new()
        .pool_segments(POOL_SEGMENTS)
        .pool_byte_cap(POOL_BYTE_CAP),
);

include!("_shared/r31_2_pool_cap_threshold_workload.rs");

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);

fn main() {
    run_arm("cap16", &GLOBAL, POOL_SEGMENTS as u64, POOL_BYTE_CAP as u64);
}
