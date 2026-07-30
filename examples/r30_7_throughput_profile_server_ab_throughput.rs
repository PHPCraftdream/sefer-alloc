//! R30-7 (task #456), Deliverable 4 — application-shaped cross-thread A/B
//! arm: **`throughput`** (the small pool `(8, 32 MiB)` + large-cache
//! headroom `64 MiB` combination — R31-9/task #473 split what was originally
//! the single bundled `Profile::Throughput` enum variant into two composed
//! axes; this arm reproduces the SAME resolved config via
//! `Profile::new().small_pool(SmallPoolPolicy::Throughput).large_cache(LargeCachePolicy::Trimmed64MiB)`).
//!
//! Sibling of `r30_7_throughput_profile_server_ab_default.rs` — see that
//! file's module doc for the full rationale and workload description. The
//! ONLY difference between the two binaries is which `SeferAlloc` config is
//! installed; the workload body (`include!`d from the same shared file) is
//! byte-identical.
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r30_7_throughput_profile_server_ab_default --example r30_7_throughput_profile_server_ab_throughput --features "production alloc-stats"
//! node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,throughput
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use sefer_alloc::{LargeCachePolicy, Profile, SmallPoolPolicy};

include!("_shared/r30_7_server_shaped_workload.rs");

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_profile(
    Profile::new()
        .small_pool(SmallPoolPolicy::Throughput)
        .large_cache(LargeCachePolicy::Trimmed64MiB),
);

fn main() {
    run_arm("throughput", &GLOBAL);
}
