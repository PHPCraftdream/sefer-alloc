//! R20 PRF-04 claim-path oracle: runs the SAME bodies as the Ir-gate cells
//! (`benches/support/r20_prf04.rs`, included by path) between claim-source
//! counter snapshots and asserts the path each cell preregisters:
//! - `cold N`     -> every claim is an EMPTY-first-claim BUMP
//!   (`bump_delta == N`, zero reclaims, N new heaps);
//! - `recycled N` -> every measured claim reclaims the FREE slot through the
//!   reuse path (`reclaim_delta == N`, zero bumps, no high-water growth).
//!
//! Usage: `r20_prf04_claim_path_oracle cold N` | `... recycled N`.
//! Output: one JSON line on stdout; a violated assertion panics (nonzero exit).
//! Counters exist only under `bench-internals`, so this binary is a separate
//! build from the `production`-only iai cells; path selection does not depend
//! on them.
//!
//! The claim-source array has 4 entries on main (hint, bump, cap_scan,
//! recovery) and 5 on candidate B (`free_set` appended). The reader below is
//! length-agnostic: it indexes positions 0..=3 and takes the fifth via
//! `get(4)`, reporting `missing_fifth_explicit_zero` when absent.

use sefer_alloc::registry::{heaps_claimed_high_water, HeapRegistry};
use sefer_alloc::SeferAlloc;

#[allow(dead_code)] // the bench-only entry points (N/2N/4N, full recycled body) are unused here
#[path = "../benches/support/r20_prf04.rs"]
mod prf04;

#[global_allocator]
static ALLOC: SeferAlloc = SeferAlloc::new();

/// One snapshot of the claim-source counters + the registry high-water mark.
struct Snap {
    hint: u64,
    bump: u64,
    cap_scan: u64,
    recovery: u64,
    free_set: u64,
    claim_sources_len: usize,
    missing_fifth_explicit_zero: bool,
    high_water: u64,
}

fn read_snap() -> Snap {
    let arr = HeapRegistry::dbg_registry_claim_sources();
    let len = arr.len();
    Snap {
        hint: arr[0],
        bump: arr[1],
        cap_scan: arr[2],
        recovery: arr[3],
        free_set: arr.get(4).copied().unwrap_or(0),
        claim_sources_len: len,
        missing_fifth_explicit_zero: len < 5,
        high_water: u64::from(heaps_claimed_high_water()),
    }
}

fn json_snap(s: &Snap) -> String {
    format!(
        concat!(
            r#"{{"hint":{},"bump":{},"cap_scan":{},"recovery":{},"free_set":{},"#,
            r#""claim_sources_len":{},"missing_fifth_explicit_zero":{},"high_water":{}}}"#
        ),
        s.hint,
        s.bump,
        s.cap_scan,
        s.recovery,
        s.free_set,
        s.claim_sources_len,
        s.missing_fifth_explicit_zero,
        s.high_water
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, n) = match args.as_slice() {
        [m, n] if m == "cold" || m == "recycled" => (
            m.as_str(),
            n.parse::<usize>().unwrap_or_else(|_| panic!("bad N: {n}")),
        ),
        _ => panic!("usage: r20_prf04_claim_path_oracle cold N | recycled N"),
    };
    assert!(n > 0, "N must be > 0");

    // The main thread's own heap is already bound (global allocator) before
    // any snapshot, so it is outside every delta below.
    let (before, after) = if mode == "cold" {
        let b = read_snap();
        prf04::cold_first_claim(n);
        (b, read_snap())
    } else {
        prf04::recycled_warmup();
        // Snapshot AFTER the warmup: assertions cover the measured phase only.
        let b = read_snap();
        prf04::recycled_measured(n);
        (b, read_snap())
    };

    let bump_delta = after.bump - before.bump;
    let hint_delta = after.hint - before.hint;
    let cap_delta = after.cap_scan - before.cap_scan;
    let rec_delta = after.recovery - before.recovery;
    let free_delta = after.free_set - before.free_set;
    let reclaim_delta = hint_delta + cap_delta + rec_delta + free_delta;
    let new_heaps = after.high_water - before.high_water;

    let ok = if mode == "cold" {
        bump_delta == n as u64 && reclaim_delta == 0 && new_heaps == n as u64
    } else {
        reclaim_delta == n as u64 && bump_delta == 0 && new_heaps == 0
    };

    println!(
        concat!(
            r#"{{"mode":"{}","n":{},"warmup_k":{},"#,
            r#""claim_sources_before":{},"claim_sources_after":{},"#,
            r#""claim_sources_len":{},"missing_fifth_explicit_zero":{},"#,
            r#""bump_delta":{},"hint_delta":{},"reclaim_delta":{},"new_heaps":{},"#,
            r#""free_set_delta":{},"ok":{}}}"#
        ),
        mode,
        n,
        prf04::PRF04_WARMUP_K,
        json_snap(&before),
        json_snap(&after),
        after.claim_sources_len,
        after.missing_fifth_explicit_zero,
        bump_delta,
        hint_delta,
        reclaim_delta,
        new_heaps,
        free_delta,
        ok
    );
    assert!(ok, "PRF-04 oracle assertion FAILED for mode {mode} n={n}");
}
