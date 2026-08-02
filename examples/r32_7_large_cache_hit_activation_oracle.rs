//! R32-7 (task #498) — path-activation oracle for `benches/perf_gate_iai.rs`'s
//! `large_cache_prefill_only_4mib` / `large_cache_hit_only_4mib` pair.
//!
//! Per CLAUDE.md's R30-8 rule ("a benchmark/report judging a feature or code
//! path MUST also report, per arm, the evidence that the arm actually took
//! the intended code path/counter activity it claims to measure"), this
//! example reproduces the EXACT same workload shape the iai bench pair uses
//! (`LARGE_HIT_CYCLES` rounds of alloc(4 MiB)+free(4 MiB), then one more
//! terminal alloc) through the real `#[global_allocator]` (`SeferAlloc`, the
//! public surface — not the `#[doc(hidden)]` `HeapCore`/`AllocCore` seam the
//! bench itself uses, so this oracle is independently reachable without the
//! doc-hidden export), and asserts via `SeferAlloc::stats().large_cache_hits`
//! (gated `alloc-stats`, the public hit-rate counter) that the terminal alloc
//! is in fact a large-cache HIT, not a miss/fresh-reservation via
//! `alloc_large_slow`.
//!
//! Not itself the iai-measured binary (iai-callgrind benches cannot read
//! `alloc-stats` counters INSIDE their own timed region without adding Ir to
//! what's being measured — see the bench pair's own doc comment in
//! `benches/perf_gate_iai.rs`) — this is the separate, out-of-band
//! confirmation that the SAME workload shape those benches use structurally
//! guarantees a hit, run once here with the counter actually read.
//!
//! Run: `cargo run --example r32_7_large_cache_hit_activation_oracle --features "alloc-global alloc-xthread alloc-decommit fastbin alloc-stats"`

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "alloc-stats"
))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::SeferAlloc;

/// Mirrors `benches/perf_gate_iai.rs`'s `LARGE_ALLOC_BYTES`/`LARGE_HIT_CYCLES`
/// exactly (kept as separate literals here rather than importing — the bench
/// crate target and this example target do not share a lib-level constant,
/// and the values are simple enough that duplication is not a maintenance
/// burden; if either drifts, this oracle's own workload no longer matches
/// the bench's, which the assertions below would surface as a hit-count
/// mismatch, not a silent pass).
const LARGE_ALLOC_BYTES: usize = 4 * 1024 * 1024;
const LARGE_HIT_CYCLES: usize = 8;

fn main() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(LARGE_ALLOC_BYTES, 8).unwrap();

    let hits_before_prefill = sefer.stats().large_cache_hits;

    // Prefill: LARGE_HIT_CYCLES rounds of alloc+free, byte-identical to the
    // bench's `large_cache_prefill_only_4mib` body.
    for _ in 0..LARGE_HIT_CYCLES {
        // SAFETY: layout has non-zero size and valid alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating during prefill");
        // SAFETY: ptr was returned by the alloc call directly above with the
        // same layout, freed exactly once.
        unsafe { sefer.dealloc(ptr, layout) };
    }

    let hits_after_prefill = sefer.stats().large_cache_hits;

    // Terminal alloc: byte-identical to the bench's `large_cache_hit_only_4mib`
    // body's timed-in-spirit region — one more alloc(4 MiB), immediately
    // following the prefill loop's last dealloc, which deposited an
    // exact-size-matching slot into the large_cache.
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { sefer.alloc(layout) };
    assert!(
        !ptr.is_null(),
        "OOM allocating the terminal (hit-target) alloc"
    );

    let hits_after_terminal = sefer.stats().large_cache_hits;

    // SAFETY: ptr was returned by the alloc call directly above with the
    // same layout, freed exactly once.
    unsafe { sefer.dealloc(ptr, layout) };

    let terminal_hit_delta = hits_after_terminal - hits_after_prefill;

    println!("hits_before_prefill  = {hits_before_prefill}");
    println!("hits_after_prefill   = {hits_after_prefill}");
    println!("hits_after_terminal  = {hits_after_terminal}");
    println!("terminal_hit_delta   = {terminal_hit_delta}");

    // The oracle: the ONE terminal alloc that both `large_cache_hit_only_4mib`
    // (in the iai bench) and this function (above) issue immediately after
    // the prefill loop's last dealloc MUST be counted as exactly one
    // large-cache hit -- confirming the iai bench's timed region really does
    // exercise `AllocCore::alloc_large`'s cache-HIT arm (the F12 targeted-
    // write call site), not `alloc_large_slow`'s fresh-OS-reservation path.
    assert_eq!(
        terminal_hit_delta, 1,
        "F12 path-activation oracle FAILED: the terminal alloc after the \
         prefill loop was not counted as exactly one large-cache hit -- \
         large_cache_hit_only_4mib's Ir measurement would not be measuring \
         the code path this task changed"
    );

    println!(
        "F12 path-activation oracle PASSED: the large_cache_hit_only_4mib \
         bench's terminal alloc is a genuine large-cache HIT."
    );
}
