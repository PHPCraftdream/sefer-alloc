//! Shared fixed workload; serde_json pinned by Cargo.lock to 1.0.150.
use sefer_alloc::SeferAlloc;
use std::sync::{Arc, Barrier};
use std::time::Instant;
fn workload() {
    let mut blocks = Vec::with_capacity(640);
    for size in [64, 256, 1024, 2048, 4096] {
        for _ in 0..128 {
            let mut block = vec![0u8; size];
            for byte in block.iter_mut().step_by(4096) {
                *byte = 0xab;
            }
            blocks.push(block);
        }
    }
    let mut large = vec![0u8; 2 * 1024 * 1024];
    for byte in large.iter_mut().step_by(4096) {
        *byte = 0xab;
    }
    std::hint::black_box((&blocks, &large));
    drop((blocks, large));
}

pub fn run(global: &SeferAlloc, observer: bool) {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 5, "mode arm K W required");
    let mode = args[1].as_str();
    let arm = args[2].as_str();
    let k: usize = args[3].parse().expect("K integer");
    let w: usize = args[4].parse().expect("W integer");
    assert!(arm == "A" || arm == "B");
    match mode {
        "wave" => {
            assert!(([2, 8, 32].contains(&k) && [16, 128].contains(&w)) || (k == 2 && w == 2))
        }
        "live" => assert!([4, 64].contains(&k) && w == 1),
        "churn" => assert!(k == 1 && [20, 2000].contains(&w)),
        _ => panic!("unknown mode"),
    }
    assert!(!SeferAlloc::maintenance_running());
    let main_live = vec![1u8; 4096];
    std::hint::black_box(&main_live);
    let before = global.stats();
    assert_eq!(before.heaps_claimed_high_water, 1);
    assert_eq!(before.config_conflicts, 0);
    #[cfg(feature = "bench-internals")]
    let sources_before = sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
    #[cfg(feature = "bench-internals")]
    let config_before = sefer_alloc::registry::HeapRegistry::dbg_registry_default_config_results();
    #[cfg(feature = "alloc-stats")]
    let scans_before = sefer_alloc::alloc_core::AllocCore::dbg_routed_miss_scans();
    let memory_before = proc_probe::try_snapshot().expect("memory snapshot");
    let mut peak_rss = memory_before.rss;
    let mut peak_commit = memory_before.commit_charge;
    let start = Instant::now();
    let mut observed = Vec::with_capacity(k * w);
    for _ in 0..w {
        let barrier = Arc::new(Barrier::new(k + 1));
        let entry = Arc::new(Barrier::new(k + 1));
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let release_ns = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut handles = Vec::with_capacity(k);
        for _ in 0..k {
            let barrier = Arc::clone(&barrier);
            let entry = Arc::clone(&entry);
            let live = mode == "live";
            let released = Arc::clone(&released);
            let release_ns = Arc::clone(&release_ns);
            handles.push(std::thread::spawn(move || {
                // Bootstrap events are observed by the wrapper, never retimed.
                if live {
                    entry.wait();
                    while !released.load(std::sync::atomic::Ordering::Acquire) {
                        std::hint::spin_loop();
                    }
                }
                workload();
                // All claims/work complete before any TLS exit publication.
                barrier.wait();
                barrier.wait();
                crate::observation(release_ns.load(std::sync::atomic::Ordering::Acquire))
            }));
        }
        if mode == "live" {
            entry.wait();
            release_ns.store(crate::clock(), std::sync::atomic::Ordering::Release);
            released.store(true, std::sync::atomic::Ordering::Release);
        }
        barrier.wait();
        let active = proc_probe::try_snapshot().expect("barrier snapshot");
        peak_rss = peak_rss.max(active.rss);
        peak_commit = match (peak_commit, active.commit_charge) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
        barrier.wait();
        for handle in handles {
            observed.push(handle.join().expect("worker panic"));
        }
        assert!(!SeferAlloc::maintenance_running());
    }
    let wall_ns = u64::try_from(start.elapsed().as_nanos()).expect("duration fits");
    let after = global.stats();
    let idle = proc_probe::try_snapshot().expect("idle snapshot");
    let delta = after.heaps_claimed_high_water - before.heaps_claimed_high_water;
    let expected = u64::try_from(1 + w * (k - 1)).expect("count fits");
    match mode {
        "wave" if arm == "A" => assert_eq!(delta, expected),
        "wave" => assert!(delta <= u64::try_from(k + 1).expect("K fits")),
        "live" => assert_eq!(delta, u64::try_from(k).expect("K fits")),
        "churn" => assert!(delta <= 2),
        _ => unreachable!(),
    }
    assert_eq!(after.config_conflicts, 0);
    #[cfg(feature = "bench-internals")]
    let (free_set, sources, source_length, fifth_missing_zero, config_results) = {
        let end = sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
        // ABI A4/B5: appended fifth counter, never move the original four.
        let old: &[u64] = &sources_before;
        let end: &[u64] = &end;
        assert!(end.len() == 4 || end.len() == 5);
        assert_eq!(old.len(), end.len());
        let delta: Vec<u64> = end.iter().zip(old).map(|(a, b)| a - b).collect();
        let free_set = delta.get(4).copied().unwrap_or(0);
        assert_eq!(
            delta.iter().sum::<u64>(),
            u64::try_from(k * w).expect("claims fit")
        );
        if mode == "wave" && arm == "B" {
            assert_eq!(end.len(), 5);
            assert!(free_set + delta[0] >= u64::try_from((w - 1) * k - 1).expect("reuse fits"));
            assert!(delta[1] <= u64::try_from(k + 1).expect("K fits"));
        }
        if mode == "live" {
            assert_eq!(free_set, 0);
        }
        let config = sefer_alloc::registry::HeapRegistry::dbg_registry_default_config_results();
        let config = [config[0] - config_before[0], config[1] - config_before[1]];
        assert_eq!(config, [u64::try_from(k * w).expect("claims fit"), 0]);
        (
            Some(delta.get(4).copied().unwrap_or(0)),
            Some(delta),
            Some(end.len()),
            Some(end.len() == 4),
            Some(config),
        )
    };
    #[cfg(not(feature = "bench-internals"))]
    let sources: Option<Vec<u64>> = None;
    #[cfg(not(feature = "bench-internals"))]
    let source_length: Option<usize> = None;
    #[cfg(not(feature = "bench-internals"))]
    let fifth_missing_zero: Option<bool> = None;
    #[cfg(not(feature = "bench-internals"))]
    let free_set: Option<u64> = None;
    #[cfg(not(feature = "bench-internals"))]
    let config_results: Option<[u64; 2]> = None;
    #[cfg(feature = "alloc-stats")]
    let routed_scans =
        Some(sefer_alloc::alloc_core::AllocCore::dbg_routed_miss_scans() - scans_before);
    #[cfg(not(feature = "alloc-stats"))]
    let routed_scans: Option<u64> = None;
    if observer {
        assert_eq!(observed.len(), k * w);
        assert!(
            observed.iter().all(|r| r[0] == 1),
            "exactly one first allocating call per worker"
        );
        #[cfg(feature = "bench-internals")]
        assert_eq!(
            sources
                .as_ref()
                .expect("diagnostic sources")
                .iter()
                .sum::<u64>(),
            observed.iter().map(|r| r[0]).sum::<u64>()
        );
    }
    println!(
        "{}",
        serde_json::json!({
            "schema": 2, "mode": mode, "arm": arm, "k": k, "w": w,
            "pid": std::process::id(), "diagnostic": cfg!(feature = "bench-internals"),
            "observer_status": if observer { "OBSERVED" } else { "NONE" },
            "observer": observer,
            "first_call_metric": if observer { Some("first allocating call, includes TLS bind/claim") } else { None },
            "pre_first_alloc_deallocs": if observer { Some(observed.iter().map(|r| r[3]).collect::<Vec<u64>>()) } else { None },
            "first_call_samples": if observer { Some(&observed) } else { None },
            "live_valid": !observer || mode != "live" || observed.iter().all(|r| r[1] > r[4]), "maintenance_running": SeferAlloc::maintenance_running(),
            "requested_config": "LargeCacheConfig::new",
            "config_conflicts_before": before.config_conflicts, "config_conflicts_after": after.config_conflicts,
            "count_before": before.heaps_claimed_high_water, "count_after": after.heaps_claimed_high_water,
            "count_delta": delta, "segments_reserved_delta": after.segments_reserved_total - before.segments_reserved_total,
            "rss_before": memory_before.rss, "rss_idle": idle.rss, "rss_peak_sampled": peak_rss.max(idle.rss),
            "commit_before": memory_before.commit_charge, "commit_idle": idle.commit_charge,
            "commit_peak_sampled": match (peak_commit, idle.commit_charge) { (Some(a), Some(b)) => Some(a.max(b)), _ => None },
            "claim_sources": sources, "claim_sources_len": source_length,
            "free_set": free_set,
            "missing_fifth_explicit_zero": fifth_missing_zero, "resolved_config_results": config_results,
            "routed_scans": routed_scans, "wall_ns": (!cfg!(feature = "bench-internals")).then_some(wall_ns)
        })
    );
}
