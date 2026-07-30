//! R31-2 (task #465) — multi-threaded pool-cap THRESHOLD sweep, arm
//! **cap4** (`pool_segments=4, pool_byte_cap=16 MiB` — the current
//! `production` default, i.e. `SeferAlloc::new()`'s resolved config).
//!
//! ## Why this file exists
//!
//! R30-7 (task #456) found `decommit_calls_total = 40` in BOTH its `default`
//! (cap 4) and `Profile::Throughput` (cap 8) arms of an 8-thread/4-size-mix
//! server-shaped workload — a mechanism delta of ZERO. That does not prove
//! pooling has no value at this concurrency; cap 8 may simply not be large
//! enough to change the mechanism for THIS workload shape. This sweep adds
//! cap 16 and cap 32 to search for the actual threshold. This file is the
//! cap-4 BASELINE arm every other arm is compared against.
//!
//! `SeferAlloc::with_config` bakes its config at COMPILE time (`static`
//! initialiser) — the same structural reason R27-4's `cap4`/`cap8` and
//! R30-7's `default`/`throughput` needed separate binaries applies here:
//! four binaries, one per swept cap, sharing one `include!`d workload body
//! (`examples/_shared/r31_2_pool_cap_threshold_workload.rs`).
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r31_2_pool_cap_threshold_ab_cap4 --example r31_2_pool_cap_threshold_ab_cap8 --example r31_2_pool_cap_threshold_ab_cap16 --example r31_2_pool_cap_threshold_ab_cap32 --features "production alloc-stats"
//! node scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json --arms cap4,cap8
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

const POOL_SEGMENTS: usize = 4;
const POOL_BYTE_CAP: usize = 16 * 1024 * 1024;

const CONFIG: LargeCacheConfig = LargeCacheConfig::new().pool(
    SmallSegmentPoolConfig::new()
        .pool_segments(POOL_SEGMENTS)
        .pool_byte_cap(POOL_BYTE_CAP),
);

include!("_shared/r31_2_pool_cap_threshold_workload.rs");

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);

fn main() {
    run_arm("cap4", &GLOBAL, POOL_SEGMENTS as u64, POOL_BYTE_CAP as u64);
}
