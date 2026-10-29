//! R20 PRF-04 heap-reuse L1 path-activation oracle.
//!
//! Usage: `r20_prf04_heap_reuse_oracle <start|cold|recycled|wave|wave_release> <N>`.
//! `R20_PRF04HR_ARM` must be A or S and agree with the observed source count.
//! Emits one JSON line; failed path assertions panic.

use sefer_alloc::registry::{config_conflicts_total, heaps_claimed_high_water, HeapRegistry};

#[allow(dead_code)]
#[path = "../benches/support/r20_prf04_heap_reuse.rs"]
mod hr;

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

#[cfg(prf04_heap_reuse_observer)]
fn popcount_now() -> (u64, bool) {
    (HeapRegistry::dbg_free_slot_hint_popcount(), true)
}

#[cfg(not(prf04_heap_reuse_observer))]
fn popcount_now() -> (u64, bool) {
    (0, false)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (family, n) = match args.as_slice() {
        [family, n]
            if ["start", "cold", "recycled", "wave", "wave_release"].contains(&family.as_str()) =>
        {
            (
                family.as_str(),
                n.parse::<usize>().unwrap_or_else(|_| panic!("bad N: {n}")),
            )
        }
        _ => panic!("usage: <start|cold|recycled|wave|wave_release> N"),
    };
    let arm = std::env::var("R20_PRF04HR_ARM").unwrap_or_else(|_| panic!("R20_PRF04HR_ARM unset"));
    assert!(["A", "S"].contains(&arm.as_str()), "bad arm {arm}");
    assert!(n > 0, "N must be > 0");
    let n = if family == "start" { 1 } else { n };
    assert!(
        family != "wave" && family != "wave_release" || n <= hr::HR_WAVE_SLOTS,
        "wave/wave_release N must not exceed the held leases"
    );

    let conflicts_before = config_conflicts_total();
    let (before, after) = match (family, ()) {
        ("start" | "cold", _) => {
            let b = read_snap();
            hr::prf04::cold_first_claim(n);
            (b, read_snap())
        }
        ("recycled", _) => {
            hr::prf04::recycled_warmup();
            let b = read_snap();
            hr::prf04::recycled_measured(n);
            (b, read_snap())
        }
        ("wave", _) => {
            hr::prf04::wave_warmup();
            let b = read_snap();
            hr::prf04::wave_measured(n);
            (b, read_snap())
        }
        ("wave_release", _) => {
            let mut held = hr::wave_release_warmup();
            let b = read_snap();
            hr::wave_release_measured(&mut held, n);
            let a = read_snap();
            std::mem::forget(held);
            (b, a)
        }
        _ => unreachable!(),
    };
    let (popcount_after, popcount_available) = popcount_now();
    let config_conflicts_delta = config_conflicts_total() - conflicts_before;
    assert_eq!(
        arm == "S",
        after.claim_sources_len >= 5,
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
    let ok = match (family, arm.as_str()) {
        ("start" | "cold", _) => {
            bump == n64
                && reclaim == 0
                && free_set == 0
                && new_heaps == n64
                && (family != "cold" || popcount_after == 0)
        }
        ("recycled", _) => {
            reclaim == n64 && hint == n64 && bump == 0 && new_heaps == 0 && popcount_after == 0
        }
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
        ("wave_release", _) => {
            hint == 0
                && bump == 0
                && scan == 0
                && recovery == 0
                && free_set == 0
                && reclaim == 0
                && new_heaps == 0
                && popcount_after == if arm == "S" { n64 - 1 } else { 0 }
        }
        _ => unreachable!(),
    };
    let size = match n {
        hr::HR_2N => "2n",
        hr::HR_4N => "4n",
        _ => "n",
    };
    let requested_kind = std::env::var("R20_PRF04HR_REQUESTED").unwrap_or_else(|_| {
        if arm == "A" {
            "FreshFirst".into()
        } else {
            "ReuseFirst".into()
        }
    });
    println!(
        r#"{{"arm":"{}","family":"{}","n":{},"size":"{}","bumpDelta":{},"hintDelta":{},"reclaimDelta":{},"newHeaps":{},"freeSetDelta":{},"claimSourcesLen":{},"missingFreeSetExplicitZero":{},"popcount":{},"popcountAvailable":{},"requestedKind":"{}","configConflictsDelta":{},"waveSlots":{},"warmupK":{},"targetClaimDelta":0,"firstAllocActivated":false,"shippingLayer":false,"ok":{}}}"#,
        arm,
        family,
        n,
        size,
        bump,
        hint,
        reclaim,
        new_heaps,
        free_set,
        after.claim_sources_len,
        after.claim_sources_len < 5,
        popcount_after,
        popcount_available,
        requested_kind,
        config_conflicts_delta,
        hr::HR_WAVE_SLOTS,
        hr::prf04::PRF04_WARMUP_K,
        ok
    );
    assert!(
        ok,
        "PRF-04 heap-reuse oracle assertion FAILED for family {family} n={n} arm={arm}"
    );
}
