//! Shipping-layer R20 PRF-04 activation oracle. Arguments: <l2_cold|l2_recycled|l2_wave_release> N.
use sefer_alloc::{
    registry::{config_conflicts_total, heaps_claimed_high_water, HeapRegistry},
    SeferAlloc,
};
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
#[allow(dead_code)]
#[path = "../benches/support/r20_prf04_heap_reuse.rs"]
mod hr;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (family, n) = match a.as_slice() {
        [f, n] if ["l2_cold", "l2_recycled", "l2_wave_release"].contains(&f.as_str()) => {
            (f.as_str(), n.parse::<usize>().expect("N"))
        }
        _ => panic!("usage: <l2_cold|l2_recycled|l2_wave_release> N"),
    };
    let arm = std::env::var("R20_PRF04HR_ARM").unwrap_or_else(|_| panic!("R20_PRF04HR_ARM unset"));
    let requested = std::env::var("R20_PRF04HR_REQUESTED").unwrap_or_else(|_| {
        if arm == "A" {
            "FreshFirst".into()
        } else {
            "ReuseFirst".into()
        }
    });
    let snap = || {
        let x = HeapRegistry::dbg_registry_claim_sources();
        (x.to_vec(), u64::from(heaps_claimed_high_water()))
    };
    let conflicts = config_conflicts_total();
    if family == "l2_recycled" {
        hr::l2_recycled_warmup();
    }
    hr::reset_target_activation();
    if family == "l2_wave_release" {
        let mut h = hr::l2_wave_holders(hr::HR_WAVE_SLOTS);
        let before = snap();
        let pop_before = free_popcount();
        hr::l2_wave_release_measured(&mut h, n);
        let after = snap();
        let pop_after = free_popcount();
        emit(
            family, n, &arm, &requested, before, after, conflicts, &h, pop_before, pop_after,
        );
        std::mem::forget(h);
        return;
    }
    let before = snap();
    if family == "l2_cold" {
        hr::l2_child_cold(n);
    } else {
        hr::l2_recycled(n);
    }
    let after = snap();
    emit(
        family,
        n,
        &arm,
        &requested,
        before,
        after,
        conflicts,
        &[],
        free_popcount(),
        free_popcount(),
    );
}
#[cfg(prf04_heap_reuse_observer)]
fn free_popcount() -> u64 {
    HeapRegistry::dbg_free_slot_hint_popcount()
}
#[cfg(not(prf04_heap_reuse_observer))]
fn free_popcount() -> u64 {
    0
}
fn emit(
    f: &str,
    n: usize,
    arm: &str,
    requested: &str,
    b: (Vec<u64>, u64),
    a: (Vec<u64>, u64),
    conflicts: u64,
    _held: &[hr::L2Holder],
    pop_before: u64,
    pop_after: u64,
) {
    let delta = |i: usize| a.0.get(i).copied().unwrap_or(0) - b.0.get(i).copied().unwrap_or(0);
    let (target_claim_delta, target_calls) = hr::target_activation();
    let hint = delta(0);
    let bump = delta(1);
    let scan = delta(2);
    let recovery = delta(3);
    let free = delta(4);
    let reclaim = hint + scan + recovery + free;
    let new_heaps = a.1 - b.1;
    let expected_targets = if f == "l2_wave_release" {
        hr::HR_WAVE_SLOTS as u64
    } else {
        n as u64
    };
    let target_ok = target_claim_delta == expected_targets && target_calls == expected_targets;
    let pop = pop_after;
    let pop_avail = cfg!(prf04_heap_reuse_observer);
    let ok = target_ok
        && (f != "l2_cold" || (bump == n as u64 && new_heaps == n as u64))
        && (f != "l2_recycled" || (hint == n as u64 && bump == 0 && new_heaps == 0))
        && (f != "l2_wave_release"
            || (hint == 0
                && bump == 0
                && reclaim == 0
                && free == 0
                && new_heaps == 0
                && pop == if arm == "S" { n as u64 - 1 } else { 0 }));
    println!(
        r#"{{"arm":"{}","family":"{}","n":{},"size":"{}","bumpDelta":{},"hintDelta":{},"reclaimDelta":{},"newHeaps":{},"freeSetDelta":{},"claimSourcesLen":{},"missingFreeSetExplicitZero":{},"popcount":{},"popcountAvailable":{},"requestedKind":"{}","configConflictsDelta":{},"shippingLayer":true,"targetClaimDelta":{},"firstAllocActivated":{},"boundEvidence":"unavailable; actual GlobalAlloc claim delta proves first bind","ok":{}}}"#,
        arm,
        f,
        n,
        match n {
            16 => "n",
            32 => "2n",
            64 => "4n",
            _ => "n",
        },
        bump,
        hint,
        reclaim,
        new_heaps,
        free,
        a.0.len(),
        a.0.len() < 5,
        pop,
        pop_avail,
        requested,
        config_conflicts_total() - conflicts,
        target_claim_delta,
        target_calls == expected_targets,
        ok
    );
    assert!(ok, "L2 activation/counter oracle failed");
}
