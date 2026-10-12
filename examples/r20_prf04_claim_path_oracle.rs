//! R20 PRF-04 claim-path oracle: runs the SAME bodies as the Ir-gate cells
//! (`benches/support/r20_prf04.rs`, included by path) between claim-source
//! counter snapshots and asserts the path each cell preregisters.
//!
//! Usage: `r20_prf04_claim_path_oracle <start|cold|recycled|wave> <N> <A|B>`.
//! `start` ignores N (always one claim). Output: one JSON line on stdout; a
//! violated assertion panics (nonzero exit).
//!
//! Expected deltas over the measured phase (N claims):
//! - start    -> 1 EMPTY-first-claim bump, 1 new heap;
//! - cold     -> N bumps, zero reclaims, N new heaps;
//! - recycled -> N reclaims through the hint, zero bumps, no new heap;
//! - wave on A (4 counters) -> 1 hint claim, N-1 bumps, N-1 new heaps (the FREE
//!   slots stay stranded: main bumps before it ever scans);
//! - wave on B (5 counters) -> 1 hint claim, N-1 free-set claims, zero bumps,
//!   zero new heaps.
//!
//! The counters exist only under `bench-internals`, so this binary is a
//! separate build from the `production internals` iai cells; path selection
//! does not depend on them. The system allocator stays global (no Sefer heap is
//! bound to the main thread), so the registry starts empty exactly as in the
//! iai process. The claim-source array has 4 entries on main (hint, bump,
//! cap_scan, recovery) and 5 on candidate B (`free_set` appended); the label
//! must agree with that length.

use sefer_alloc::registry::{heaps_claimed_high_water, HeapRegistry};

#[allow(dead_code)] // the bench-only entry points (N/2N/4N, full bodies) are unused here
#[path = "../benches/support/r20_prf04.rs"]
mod prf04;

/// One snapshot of the claim-source counters + the registry high-water mark.
struct Snap {
    hint: u64,
    bump: u64,
    cap_scan: u64,
    recovery: u64,
    free_set: u64,
    claim_sources_len: usize,
    high_water: u64,
}

fn read_snap() -> Snap {
    let arr = HeapRegistry::dbg_registry_claim_sources();
    Snap {
        hint: arr[0],
        bump: arr[1],
        cap_scan: arr[2],
        recovery: arr[3],
        free_set: arr.get(4).copied().unwrap_or(0),
        claim_sources_len: arr.len(),
        high_water: u64::from(heaps_claimed_high_water()),
    }
}

fn json_snap(s: &Snap) -> String {
    format!(
        concat!(
            r#"{{"hint":{},"bump":{},"cap_scan":{},"recovery":{},"free_set":{},"#,
            r#""claim_sources_len":{},"high_water":{}}}"#
        ),
        s.hint, s.bump, s.cap_scan, s.recovery, s.free_set, s.claim_sources_len, s.high_water
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, n, arm) = match args.as_slice() {
        [m, n, a]
            if ["start", "cold", "recycled", "wave"].contains(&m.as_str())
                && ["A", "B"].contains(&a.as_str()) =>
        {
            (
                m.as_str(),
                n.parse::<usize>().unwrap_or_else(|_| panic!("bad N: {n}")),
                a.as_str(),
            )
        }
        _ => panic!("usage: r20_prf04_claim_path_oracle <start|cold|recycled|wave> N <A|B>"),
    };
    assert!(n > 0, "N must be > 0");
    let n = if mode == "start" { 1 } else { n };
    let wave_slots = prf04::PRF04_WAVE_SLOTS;
    assert!(
        mode != "wave" || n <= wave_slots,
        "wave N must not exceed the FREE slots left behind"
    );

    let (before, after) = match mode {
        "start" | "cold" => {
            let b = read_snap();
            prf04::cold_first_claim(n);
            (b, read_snap())
        }
        "recycled" => {
            prf04::recycled_warmup();
            // Snapshot AFTER the warmup: assertions cover the measured phase only.
            let b = read_snap();
            prf04::recycled_measured(n);
            (b, read_snap())
        }
        _ => {
            prf04::wave_warmup();
            let b = read_snap();
            prf04::wave_measured(n);
            (b, read_snap())
        }
    };
    assert_eq!(
        arm == "B",
        after.claim_sources_len == 5,
        "arm label {arm} disagrees with claim_sources_len {}",
        after.claim_sources_len
    );

    let bump = after.bump - before.bump;
    let hint = after.hint - before.hint;
    let scan = after.cap_scan - before.cap_scan;
    let recovery = after.recovery - before.recovery;
    let free_set = after.free_set - before.free_set;
    let new_heaps = after.high_water - before.high_water;
    let reclaim = hint + scan + recovery + free_set;
    let n64 = n as u64;

    let ok = match (mode, arm) {
        ("start" | "cold", _) => bump == n64 && reclaim == 0 && new_heaps == n64,
        ("recycled", _) => reclaim == n64 && hint == n64 && bump == 0 && new_heaps == 0,
        ("wave", "A") => {
            hint == 1
                && bump == n64 - 1
                && scan == 0
                && recovery == 0
                && free_set == 0
                && new_heaps == n64 - 1
        }
        ("wave", _) => {
            hint == 1
                && free_set == n64 - 1
                && bump == 0
                && scan == 0
                && recovery == 0
                && new_heaps == 0
        }
        _ => unreachable!(),
    };

    println!(
        concat!(
            r#"{{"mode":"{}","n":{},"arm":"{}","warmup_k":{},"wave_slots":{},"#,
            r#""claim_sources_before":{},"claim_sources_after":{},"#,
            r#""claim_sources_len":{},"missing_fifth_explicit_zero":{},"#,
            r#""bump_delta":{},"hint_delta":{},"reclaim_delta":{},"new_heaps":{},"#,
            r#""free_set_delta":{},"ok":{}}}"#
        ),
        mode,
        n,
        arm,
        prf04::PRF04_WARMUP_K,
        wave_slots,
        json_snap(&before),
        json_snap(&after),
        after.claim_sources_len,
        after.claim_sources_len < 5,
        bump,
        hint,
        reclaim,
        new_heaps,
        free_set,
        ok
    );
    assert!(
        ok,
        "PRF-04 oracle assertion FAILED for mode {mode} n={n} arm={arm}"
    );
}
