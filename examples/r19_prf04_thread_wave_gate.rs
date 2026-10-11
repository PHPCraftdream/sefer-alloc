//! C1 baseline only: fresh subprocess, barrier-retained spawn/join waves.
use sefer_alloc::SeferAlloc;
use std::sync::{Arc, Barrier};
use std::time::Instant;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
const K: usize = 32;

fn main() {
    let w: usize = std::env::args()
        .nth(1)
        .expect("W required")
        .parse()
        .expect("W integer");
    assert!(w == 1 || w == 128);
    assert!(!SeferAlloc::maintenance_running());
    // Bind main before the baseline. Its heap remains LIVE throughout all waves.
    let main_live = vec![1u8; 4096];
    std::hint::black_box(&main_live);
    let mut latencies = Vec::with_capacity(w * K);
    let before = GLOBAL.stats();
    assert_eq!(
        before.heaps_claimed_high_water, 1,
        "unexpected pre-wave heaps"
    );
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
    for _ in 0..w {
        let barrier = Arc::new(Barrier::new(K + 1));
        let mut handles = Vec::with_capacity(K);
        for _ in 0..K {
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let start = Instant::now();
                let first = vec![0u8; 64];
                std::hint::black_box(&first);
                let first_ns = u64::try_from(start.elapsed().as_nanos()).expect("duration fits");
                // ~1 MiB of Small allocations across 64 B..4 KiB, plus 2 MiB Large.
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
                drop((first, blocks, large));
                // Every worker has allocated AND freed before anyone can exit.
                // No TLS destructor can publish an early reuse hint during claims.
                barrier.wait();
                barrier.wait();
                first_ns
            }));
        }
        barrier.wait();
        let active = proc_probe::try_snapshot().expect("active snapshot");
        peak_rss = peak_rss.max(active.rss);
        peak_commit = match (peak_commit, active.commit_charge) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
        barrier.wait();
        for handle in handles {
            latencies.push(handle.join().expect("worker panic"));
        }
        assert!(!SeferAlloc::maintenance_running());
    }
    let wall_ns = u64::try_from(start.elapsed().as_nanos()).expect("duration fits");
    let after = GLOBAL.stats();
    let idle = proc_probe::try_snapshot().expect("idle snapshot");
    let expected = 1 + w * (K - 1);
    assert_eq!(
        after.heaps_claimed_high_water - before.heaps_claimed_high_water,
        expected as u64
    );
    assert_eq!(after.config_conflicts, 0);
    #[cfg(feature = "bench-internals")]
    let sources = {
        let end = sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
        let delta = std::array::from_fn::<_, 4, _>(|i| end[i] - sources_before[i]);
        assert_eq!(delta, [(w - 1) as u64, expected as u64, 0, 0]);
        let config = sefer_alloc::registry::HeapRegistry::dbg_registry_default_config_results();
        assert_eq!(config[0] - config_before[0], (w * K) as u64);
        assert_eq!(config[1] - config_before[1], 0);
        delta
    };
    #[cfg(feature = "bench-internals")]
    let config_results = {
        let end = sefer_alloc::registry::HeapRegistry::dbg_registry_default_config_results();
        [end[0] - config_before[0], end[1] - config_before[1]]
    };
    #[cfg(not(feature = "bench-internals"))]
    let config_results: Option<[u64; 2]> = None;
    #[cfg(not(feature = "bench-internals"))]
    let sources: Option<[u64; 4]> = None;
    #[cfg(feature = "alloc-stats")]
    let routed_scans =
        Some(sefer_alloc::alloc_core::AllocCore::dbg_routed_miss_scans() - scans_before);
    #[cfg(not(feature = "alloc-stats"))]
    let routed_scans: Option<u64> = None;
    // Raw samples precede aggregation. Diagnostic timing is deliberately absent.
    println!(
        "{}",
        serde_json::json!({
            "schema": 1, "k": K, "w": w, "pid": std::process::id(),
            "diagnostic": cfg!(feature = "bench-internals"),
            "maintenance_running": SeferAlloc::maintenance_running(),
            "requested_config": "LargeCacheConfig::new", "resolved_default_matches": cfg!(feature = "bench-internals"),
            "config_conflicts_before": before.config_conflicts, "config_conflicts_after": after.config_conflicts,
            "count_before": before.heaps_claimed_high_water, "count_after": after.heaps_claimed_high_water,
            "count_delta": after.heaps_claimed_high_water - before.heaps_claimed_high_water,
            "segments_reserved_delta": after.segments_reserved_total - before.segments_reserved_total,
            "rss_before": memory_before.rss, "rss_idle": idle.rss, "rss_peak_sampled": peak_rss.max(idle.rss),
            "commit_before": memory_before.commit_charge, "commit_idle": idle.commit_charge,
            "commit_peak_sampled": peak_commit, "virtual_size_idle": idle.virtual_size,
            "claim_sources": sources, "resolved_config_results": config_results, "routed_scans": routed_scans,
            "wall_ns": (!cfg!(feature = "bench-internals")).then_some(wall_ns),
            "first_workload_alloc_ns": if cfg!(feature = "bench-internals") { None } else { Some(latencies) }
        })
    );
}
