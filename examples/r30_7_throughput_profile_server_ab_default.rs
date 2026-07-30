//! R30-7 (task #456), Deliverable 4 — application-shaped cross-thread A/B
//! arm: **`default`** (`production` small-pool default `(4, 16 MiB)` +
//! large-cache headroom default `256 MiB`, i.e. `SeferAlloc::new()`).
//!
//! ## Why this file exists
//!
//! The `(8, 32 MiB)` throughput recipe's ~22 % win (README, R27-4) rests on
//! ONE micro-workload: a SINGLE-THREADED, single-shot
//! prefill/churn/teardown-per-batch loop
//! (`churn_prefill`/`churn_step`/`churn_teardown`, 1024 B, batch size 120).
//! That shape never overlaps two threads' allocation traffic and never
//! keeps a working set alive continuously the way a real server does
//! between requests. This pair of binaries (`_default` / this file, and
//! `_throughput`, its sibling) exercises the SAME profile comparison in a
//! more application-shaped scenario: several threads, each simulating an
//! independent concurrent request handler, allocating+freeing a MIX of
//! small object sizes (not one fixed size) repeatedly in a continuous
//! server-like loop (not a single burst-then-teardown), for a fixed
//! duration — closer to "many concurrent connections handling variously-
//! sized requests" than to the original micro-benchmark's single-size,
//! single-shot shape.
//!
//! `SeferAlloc::with_config`/`with_profile` bake their config at COMPILE
//! time (`static` initialiser) — the same structural reason R27-4's
//! `cap4`/`cap8` and R30-6's four `latency_h*` arms needed separate
//! binaries applies here: this file is the `default` arm, its sibling
//! `r30_7_throughput_profile_server_ab_throughput.rs` is the
//! `(8, 32 MiB)` small-pool / `64 MiB` large-cache arm (originally the
//! bundled `Profile::Throughput` enum variant; R31-9/task #473 split it
//! into `SmallPoolPolicy::Throughput` + `LargeCachePolicy::Trimmed64MiB`
//! composed explicitly — same resolved config). Both run the IDENTICAL
//! workload function.
//!
//! ## Workload shape
//!
//! `THREADS` worker threads, each running `ROUNDS` rounds of:
//! 1. Allocate a "request" — a small `Vec<Box<[u8]>>`-style burst of
//!    `REQUEST_ALLOCS_PER_ROUND` objects, sizes drawn from a small
//!    realistic mix (`SIZE_CLASSES`: 64 B / 256 B / 1024 B / 4096 B —
//!    headers, small structs, medium buffers, page-sized buffers), each
//!    genuinely touched (first and last byte written) so the allocation is
//!    real, not elided.
//! 2. Free a FRACTION of the previous round's objects and replace them
//!    (working-set churn across size classes, not a single fixed size) —
//!    mirrors a connection pool where some objects outlive one round and
//!    others don't.
//! 3. At the end of every round, free everything from THAT round before
//!    starting the next — bounds peak memory per thread while still
//!    reproducing a continuous alloc/free cycle across the whole run (NOT
//!    single-shot: `ROUNDS` repetitions keep the segment-carve/decay cycle
//!    running for the full timed region, unlike the original 8-batch
//!    micro-benchmark's short timed window).
//!
//! Timed region wraps ALL threads' full run (`ROUNDS` rounds each),
//! joined before `elapsed_ns` is read — the metric is END-TO-END wall time
//! for the whole concurrent workload, the shape an application actually
//! cares about (not a per-thread average).
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

include!("_shared/r30_7_server_shaped_workload.rs");

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn main() {
    run_arm("default", &GLOBAL);
}
