//! R25-3 (task #397) — gate 5 non-Ir cross-check for the FLUSH_N sweep's
//! gate 3 (oscillating live-set) finding.
//!
//! `docs/perf/R25_3_FLUSH_N_SWEEP_GATE.md` measured `oscillating_live_set_16b`
//! (an iai-callgrind Ir judge) at 25,719 Ir (FLUSH_N=8, current) vs. 62,183 Ir
//! (FLUSH_N=16) — a >2x regression when the magazine-overflow flush empties
//! the magazine completely instead of half-flushing. The task brief requires
//! a non-Ir metric (refill count / tcache hit rate) for this gate, not Ir
//! alone, in case an Ir-only measurement missed a refill-thrash regression
//! that shows up as MORE refill calls rather than more per-op Ir. This probe
//! reuses the EXISTING `alloc-stats` counter `SeferAlloc::stats().tcache_hits`
//! (no new counter invented, per the task brief's "reuse rather than
//! inventing" instruction) to derive the magazine MISS (= refill) count for
//! the identical oscillating workload the iai arm exercises natively — no
//! WSL/valgrind required, since this is process-native counter reading, not
//! an instruction-count judge.
//!
//! `tcache_hits` requires the `alloc-stats` feature (not part of `production`
//! — see `AllocStats::tcache_hits`'s doc). Every alloc that is NOT a magazine
//! hit is either a genuine OOM (does not happen in this bounded workload) or
//! took the `refill_magazine_slow` cold path — so
//! `misses = total_allocs - tcache_hits` is exactly the refill-event count
//! for this single-threaded, single-class scenario.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r25_3_flush_n_oscillating_probe \
//!     --features "production alloc-stats"
//! ```
//!
//! Run once per `FLUSH_N` value (hand-edit `src/registry/tcache.rs`'s
//! `FLUSH_N` const between runs, matching the iai sweep's own protocol) to
//! reproduce the sweep's refill-count axis.

#![cfg(all(feature = "alloc-global", feature = "fastbin", feature = "alloc-stats"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Same shape as the `oscillating_live_set_16b` iai arm
/// (`benches/perf_gate_iai.rs`): grow the live set 8 -> 24 (crossing
/// `TCACHE_CAP`=16 from below via allocs), then shrink 24 -> 8 (crossing 16
/// from above via frees), repeated `OSC_ROUNDS` times.
const OSC_ROUNDS: usize = 20;

fn main() {
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut live: [*mut u8; 24] = [std::ptr::null_mut(); 24];
    let mut live_n: usize = 0;

    // Untimed warm-up: bring the live set to the low end (8), matching the
    // iai arm's own untimed warm-up.
    for slot in live.iter_mut().take(8) {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        *slot = unsafe { GLOBAL.alloc(layout) };
        live_n += 1;
    }

    let hits_before = GLOBAL.stats().tcache_hits;
    let mut total_allocs: u64 = 0;

    for _ in 0..OSC_ROUNDS {
        while live_n < 24 {
            // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
            live[live_n] = unsafe { GLOBAL.alloc(layout) };
            live_n += 1;
            total_allocs += 1;
        }
        while live_n > 8 {
            live_n -= 1;
            let ptr = live[live_n];
            if !ptr.is_null() {
                // SAFETY: ptr was returned by the alloc loop above with the
                // same layout, freed exactly once.
                unsafe { GLOBAL.dealloc(ptr, layout) };
            }
        }
    }

    let hits_after = GLOBAL.stats().tcache_hits;
    let hits = hits_after - hits_before;
    let misses = total_allocs.saturating_sub(hits);
    let hit_rate = if total_allocs > 0 {
        (hits as f64) / (total_allocs as f64) * 100.0
    } else {
        0.0
    };

    println!("=== R25-3 oscillating live-set probe ===");
    println!(
        "(FLUSH_N is a private pub(crate) const in src/registry/tcache.rs -- \
         record which value was compiled in when citing this run's output; \
         see docs/perf/R25_3_FLUSH_N_SWEEP_GATE_summary.csv for the per-value \
         results already collected)"
    );
    println!("OSC_ROUNDS: {OSC_ROUNDS}");
    println!("total timed allocs: {total_allocs}");
    println!("tcache hits: {hits}");
    println!("tcache misses (== refill_magazine_slow calls): {misses}");
    println!("hit rate: {hit_rate:.2}%");
}
