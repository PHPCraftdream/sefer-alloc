//! L1 path-activation receipt. Candidate observers exist only in the cfg build.
use sefer_alloc::registry::{config_conflicts_total, heaps_claimed_high_water, HeapRegistry};
#[path = "../benches/support/r20_prf04_heap_reuse.rs"]
mod hr;
#[cfg(prf04_heap_reuse_oracle)]
fn candidate_observers() -> (Option<u64>, Option<u64>) {
    (
        Some(u64::from(HeapRegistry::dbg_free_slot_hint_popcount())),
        Some(HeapRegistry::dbg_registry_free_set_claims()),
    )
}
#[cfg(not(prf04_heap_reuse_oracle))]
fn candidate_observers() -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[derive(Clone, Copy)]
struct Snapshot {
    sources: [u64; 4],
    high: u64,
    conflicts: u64,
    spill: Option<u64>,
    take: Option<u64>,
}

impl Snapshot {
    fn take() -> Self {
        let (spill, take) = candidate_observers();
        Self {
            sources: HeapRegistry::dbg_registry_claim_sources(),
            high: u64::from(heaps_claimed_high_water()),
            conflicts: config_conflicts_total(),
            spill,
            take,
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        3,
        "usage: <cold|recycled|wave|wave_release|start> <N> <A|B>"
    );
    let family = args[0].as_str();
    let n = args[1].parse::<usize>().expect("N");
    let arm = args[2].as_str();
    assert!(n > 0 && ["A", "B"].contains(&arm));
    match family {
        "cold" | "start" => {
            let before = Snapshot::take();
            hr::cold(n);
            emit(family, n, arm, before);
        }
        "recycled" => {
            hr::recycled_setup();
            let before = Snapshot::take();
            hr::recycled(n);
            emit(family, n, arm, before);
        }
        "wave" => {
            hr::wave_setup();
            let before = Snapshot::take();
            hr::wave_claim(n);
            emit(family, n, arm, before);
        }
        "wave_release" => {
            let mut held = hr::wave_release_setup();
            let before = Snapshot::take();
            hr::wave_release_measured(&mut held, n);
            emit(family, n, arm, before);
            std::mem::forget(held);
        }
        _ => panic!("unknown family"),
    }
}

fn emit(family: &str, n: usize, arm: &str, before: Snapshot) {
    let after_sources = HeapRegistry::dbg_registry_claim_sources();
    let (spill, take) = candidate_observers();
    let observer = spill.is_some() && take.is_some();
    let conflicts_delta = config_conflicts_total() - before.conflicts;
    assert_eq!(conflicts_delta, 0);
    let delta = |i: usize| after_sources[i] - before.sources[i];
    let spill_delta = spill.zip(before.spill).map(|(after, prior)| {
        i64::from(u32::try_from(after).expect("bounded free-slot popcount"))
            - i64::from(u32::try_from(prior).expect("bounded free-slot popcount"))
    });
    let take_delta = take.zip(before.take).map(|(after, prior)| after - prior);
    println!(
        concat!(
            "{{\"arm\":\"{}\",\"family\":\"{}\",\"n\":{},",
            "\"hint\":{},\"bump\":{},\"capScan\":{},\"recovery\":{},",
            "\"highWaterDelta\":{},\"heap_reuse_spill_bits_delta\":{},",
            "\"heap_reuse_take_hits\":{},\"observer\":{},",
            "\"configConflictsDelta\":{},\"actualTargetCalls\":1,",
            "\"targetCallEvidence\":\"single direct named workload invocation; ",
            "dynamic execution verified by profile\"}}"
        ),
        arm,
        family,
        n,
        delta(0),
        delta(1),
        delta(2),
        delta(3),
        u64::from(heaps_claimed_high_water()) - before.high,
        spill_delta.map_or_else(|| "null".to_owned(), |v| v.to_string()),
        take_delta.map_or_else(|| "null".to_owned(), |v| v.to_string()),
        observer,
        conflicts_delta
    );
    if arm == "B" {
        assert!(observer, "candidate oracle hooks unavailable");
    }
}
