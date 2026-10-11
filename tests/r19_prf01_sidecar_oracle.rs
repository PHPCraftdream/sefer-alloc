//! Actual GlobalAlloc discovery oracle; no timing or B measurements.
#![cfg(all(
    feature = "production",
    feature = "bench-internals",
    feature = "internals",
    r19_sidecar_summary_oracle
))]

#[path = "../benches/support/r19_prf01.rs"]
mod r19_prf01;
use r19_prf01::R19Prf01;
use sefer_alloc::{registry::HeapCore, SeferAlloc};
use std::process::Command;

const SCENARIO_ENV: &str = "SEFER_PRF01_SCENARIO";

fn child(test: &str, scenario: &str) {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", test, "--nocapture"])
        .env(SCENARIO_ENV, scenario)
        .output()
        .expect("spawn isolated oracle child");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    // Child assertions, including frozen count failures, are never swallowed.
    // Do not print stderr paths into evidence; status and assertion stdout suffice.
    assert!(
        output.status.success(),
        "isolated {scenario} oracle failed: {}",
        output.status
    );
}

fn scenario() -> Option<String> {
    std::env::var(SCENARIO_ENV).ok()
}

fn delta(before: [u64; 8], after: [u64; 8]) -> [u64; 8] {
    core::array::from_fn(|i| {
        after[i]
            .checked_sub(before[i])
            .expect("oracle counter decreased")
    })
}

fn print_counts(name: &str, counts: [u64; 8], remaining: Option<u64>) {
    println!("{{\"name\":\"{name}\",\"pending_words_visited\":{},\"pending_words_exchanged\":{},\"nonempty_cuts\":{},\"discovery_records_reclaimed\":{},\"routed_scans\":{},\"summary_words_visited\":{},\"summary_rmw\":{},\"summary_load\":{},\"strict_trim_remaining\":{}}}",
        counts[0],counts[1],counts[2],counts[3],counts[4],counts[5],counts[6],counts[7],remaining.map_or_else(||"null".to_owned(),|n|n.to_string()));
}

#[test]
fn growth_oracles() {
    let Some(selected) = scenario() else {
        child("growth_oracles", "multiseg_cold_256k");
        child("growth_oracles", "seg_cycle_decommit_256k");
        return;
    };
    assert!(["multiseg_cold_256k", "seg_cycle_decommit_256k"].contains(&selected.as_str()));
    println!(
        "{{\"small_sidecar_size_bytes\":{}}}",
        HeapCore::dbg_prf01_small_sidecar_size()
    );
    // Exactly one geometry per child process. A fresh thread alone does not
    // establish a fresh registry heap; System is the process global allocator.
    for (name, rounds, expected) in [
        ("multiseg_cold_256k", 2, 319_063u64),
        ("seg_cycle_decommit_256k", 6, 1_004_503u64),
    ] {
        if name != selected {
            continue;
        }
        std::thread::spawn(move || {
            let before = HeapCore::dbg_prf01_sidecar_oracle();
            R19Prf01::multiseg(rounds);
            let counts = delta(before, HeapCore::dbg_prf01_sidecar_oracle());
            print_counts(name, counts, None);
            assert!(
                counts[4] > 0,
                "actual routed discovery scan construction missing"
            );
            assert!(
                counts[0].abs_diff(expected) <= expected / 100,
                "frozen 1% tolerance"
            );
            assert_eq!(
                counts[0], counts[1],
                "A must exchange every visited pending word"
            );
            assert_eq!(counts[2], 0);
            assert_eq!(counts[3], 0);
            assert_eq!(&counts[5..], &[0, 0, 0]);
        })
        .join()
        .unwrap();
    }
}

#[test]
fn publication_oracle() {
    let Some(selected) = scenario() else {
        child("publication_oracle", "publication");
        return;
    };
    assert_eq!(selected, "publication");
    std::thread::spawn(|| {
        let owner = R19Prf01::publication_setup();
        let before = HeapCore::dbg_prf01_sidecar_oracle();
        owner.publication_window();
        let counts = delta(before, HeapCore::dbg_prf01_sidecar_oracle());
        assert!(counts[4] > 0);
        assert_eq!(
            counts[3], 34,
            "discovery must actually reclaim joined remote allocations"
        );
        assert!(counts[2] > 0 && counts[2] <= counts[1]);
        let allocator = SeferAlloc::new();
        let trim_before = allocator.dbg_sidecar_ingress_stats();
        allocator.trim_current_thread();
        let trim_after = allocator.dbg_sidecar_ingress_stats();
        assert_eq!(
            trim_after.0 - trim_before.0,
            1,
            "one actual strict trim pass"
        );
        let remaining = trim_after.1 - trim_before.1;
        print_counts("r19_prf01_publication", counts, Some(remaining));
        assert_eq!(
            remaining, 0,
            "no joined publication left for strict trim after discovery"
        );
        assert_eq!(&counts[5..], &[0, 0, 0]);
    })
    .join()
    .unwrap();
}

#[test]
fn producer_oracle() {
    let Some(selected) = scenario() else {
        child("producer_oracle", "producer");
        return;
    };
    assert_eq!(selected, "producer");
    std::thread::spawn(|| {
        let owner = R19Prf01::producer_setup();
        let before = HeapCore::dbg_prf01_sidecar_oracle();
        owner.producer_window();
        let counts = delta(before, HeapCore::dbg_prf01_sidecar_oracle());
        print_counts("r19_prf01_joined_producer_same_word", counts, None);
        assert_eq!(
            counts, [0; 8],
            "producer alone must not invoke owner discovery in A"
        );
        let allocator = SeferAlloc::new();
        let trim_before = allocator.dbg_sidecar_ingress_stats();
        allocator.trim_current_thread();
        let trim_after = allocator.dbg_sidecar_ingress_stats();
        assert_eq!(
            trim_after.1 - trim_before.1,
            4096,
            "real remote frees, not a vacuous zero API check"
        );
    })
    .join()
    .unwrap();
}
