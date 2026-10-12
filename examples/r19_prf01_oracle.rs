#[cfg(r19_sidecar_summary_oracle)]
#[allow(dead_code)]
#[path = "../benches/support/r19_prf01.rs"]
mod workload;
#[cfg(r19_sidecar_summary_oracle)]
use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
#[cfg(r19_sidecar_summary_oracle)]
use sefer_alloc::SeferAlloc;
#[cfg(r19_sidecar_summary_oracle)]
use serde_json::json;
#[cfg(r19_sidecar_summary_oracle)]
use std::alloc::{GlobalAlloc, Layout};
#[cfg(r19_sidecar_summary_oracle)]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(r19_sidecar_summary_oracle)]
use workload::R19Prf01;

#[cfg(not(r19_sidecar_summary_oracle))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("r19_prf01_oracle requires cfg(r19_sidecar_summary_oracle)".into())
}

#[cfg(r19_sidecar_summary_oracle)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(r18_sidecar_scan_bench) || cfg!(r19_prf01_producer_control) {
        return Err("incompatible oracle cfg".into());
    }
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 || args[1].trim().is_empty() {
        return Err("usage: r19_prf01_oracle MODE RUN_ID".into());
    }
    let mode = args[0].as_str();
    if ![
        "multiseg_cold_256k",
        "seg_cycle_decommit_256k",
        "growth16_n",
        "growth16_2n",
        "growth16_4n",
        "publication",
        "producer",
        "probe-lease",
        "probe-global",
    ]
    .contains(&mode)
    {
        return Err("unknown oracle mode".into());
    }
    if cfg!(feature = "alloc-stats")
        || cfg!(feature = "large-cache-extended")
        || cfg!(feature = "exact-span-large")
        || cfg!(feature = "large-reserved-capacity")
        || cfg!(feature = "hardened")
        || cfg!(feature = "numa-aware")
    {
        return Err("alloc-stats is not the frozen oracle feature set".into());
    }
    let run_id = args[1].clone();
    let mode = args[0].clone();
    let output = std::thread::spawn(move || {
        let conflicts_before = config_conflicts_total();
        let allocator = SeferAlloc::new();
        let mut lease = if mode == "probe-lease" {
            let _ = bootstrap::ensure();
            Some(HeapRegistry::dbg_claim_lease().expect("lease claim failed"))
        } else {
            None
        };
        let (slots, budget) = if let Some(lease) = lease.as_mut() {
            let heap = lease.core();
            (heap.dbg_large_cache_total_slots(), heap.dbg_large_cache_budget())
        } else if mode == "probe-global" {
            let layout = Layout::from_size_align(16, 8).unwrap();
            // SAFETY: valid layout; unique probe block remains live until this
            // thread reads its current owned heap and deallocates it exactly once.
            let ptr = unsafe { allocator.alloc(layout) };
            assert!(!ptr.is_null());
            let config = (allocator.dbg_current_large_cache_total_slots(), allocator.dbg_current_large_cache_budget());
            // SAFETY: live probe block from this allocator with its original layout.
            unsafe { allocator.dealloc(ptr, layout) };
            config
        } else {
            (8, None)
        };
        if mode.starts_with("probe-") {
            assert_eq!(slots, 8, "config probe must own a heap");
            assert_eq!(budget, None);
        }
        assert!(!SeferAlloc::maintenance_running());
        let mut remaining = None;
        let mut producer_consumed = None;
        let (work, bytes) = match mode.as_str() {
            "multiseg_cold_256k" => (68, 258_752),
            "seg_cycle_decommit_256k" => (204, 258_752),
            "growth16_n" => (32_768, 16),
            "growth16_2n" => (65_536, 16),
            "growth16_4n" => (131_072, 16),
            "publication" => (68, 258_752),
            "producer" => (4096, 16),
            "probe-lease" | "probe-global" => (1, 16),
            _ => unreachable!("validated mode"),
        };
        let prepared = match mode.as_str() {
            "publication" => Some(R19Prf01::publication_setup()),
            "producer" => Some(R19Prf01::producer_setup()),
            _ => None,
        };
        let before = HeapCore::dbg_prf01_sidecar_oracle();
        match mode.as_str() {
            "multiseg_cold_256k" => R19Prf01::multiseg(2),
            "seg_cycle_decommit_256k" => R19Prf01::multiseg(6),
            "growth16_n" | "growth16_2n" | "growth16_4n" => R19Prf01::growth16(work),
            "publication" => prepared.unwrap().publication_window(),
            "producer" => prepared.unwrap().producer_window(),
            "probe-lease" | "probe-global" => (),
            _ => unreachable!("validated mode"),
        }
        let after = HeapCore::dbg_prf01_sidecar_oracle();
        let counts: [u64; 8] = core::array::from_fn(|i| after[i].checked_sub(before[i]).expect("oracle counter regressed"));
        if mode == "publication" || mode == "producer" {
            let before = allocator.dbg_sidecar_ingress_stats();
            allocator.trim_current_thread();
            let after = allocator.dbg_sidecar_ingress_stats();
            assert_eq!(after.0.checked_sub(before.0), Some(1));
            let consumed = after.1.checked_sub(before.1).expect("trim counter regressed");
            if mode == "publication" { remaining = Some(consumed); } else { producer_consumed = Some(consumed); assert_eq!(consumed, 4096); }
        }
        let (slots, budget) = if mode == "probe-lease" {
            (slots, budget)
        } else {
            (allocator.dbg_current_large_cache_total_slots(), allocator.dbg_current_large_cache_budget())
        };
        assert_eq!(slots, 8, "workload must retain an owned heap");
        assert_eq!(budget, None);
        let conflicts = config_conflicts_total().checked_sub(conflicts_before).expect("conflict counter regressed");
        assert_eq!(conflicts, 0);
        let names = ["pending_words_visited", "pending_words_exchanged", "nonempty_cuts", "discovery_records_reclaimed", "routed_scans", "summary_words_visited", "summary_rmw", "summary_load"];
        let mut oracle = serde_json::Map::new();
        for (name, count) in names.into_iter().zip(counts) { oracle.insert(name.to_owned(), json!(count)); }
        oracle.insert("strict_trim_remaining".into(), json!(remaining));
        oracle.insert("unobservedReasons".into(), if remaining.is_some() {json!({})} else {json!({"strict_trim_remaining":"only publication observes strict-trim remainder"})});
        // Declarations bound checkpoint coverage, never observed incarnation counts.
        let expected_points: Vec<(&str, &str, usize, usize)> = match mode.as_str() {
            "multiseg_cold_256k" | "seg_cycle_decommit_256k" => (0..work / 34)
                .map(|round| (mode.as_str(), "live-batch-before-free", round, 34)).collect(),
            "growth16_n" | "growth16_2n" | "growth16_4n" =>
                vec![("growth16", "final-live-batch-before-free", 0, work)],
            "publication" => vec![
                ("publication", "prepared-live-batch-before-handoff", 0, 34),
                ("publication", "discovery-live-batch-before-free", 0, 34),
                ("publication", "discovery-live-batch-before-free", 1, 34),
            ],
            "producer" => vec![("producer", "selected-live-batch-before-handoff", 0, work)],
            "probe-lease" | "probe-global" => Vec::new(),
            _ => unreachable!("validated mode"),
        };
        let geometry = R19Prf01::observed_geometry();
        let checkpoints = geometry["checkpoints"].as_array().expect("checkpoints missing");
        assert_eq!(checkpoints.len(), expected_points.len());
        for (checkpoint, &(name, phase, round, blocks)) in checkpoints.iter().zip(&expected_points) {
            assert_eq!(checkpoint["name"], json!(name));
            assert_eq!(checkpoint["phase"], json!(phase));
            assert_eq!(checkpoint["point"], json!(phase));
            assert_eq!(checkpoint["round"], json!(round));
            assert_eq!(checkpoint["roundIndexBase"], json!(0));
            assert_eq!(checkpoint["blockCount"], json!(blocks));
            assert_eq!(checkpoint["blockBytes"], json!(bytes));
            assert_eq!(checkpoint["method"], json!(R19Prf01::segment_count_method()));
        }
        let declared_points: Vec<_> = expected_points.iter().map(|&(name, phase, round, blocks)|
            json!({"name":name,"phase":phase,"point":phase,"round":round,"roundIndexBase":0,"blockCount":blocks,"blockBytes":bytes})
        ).collect();
        let declared_bounds = json!({
            "workCount":work,"blockBytes":bytes,"alignment":8,
            "checkpointCount":expected_points.len(),"checkpoints":declared_points,
            "producerCandidateLimit":if mode == "producer" {Some(8192)} else {None},
            "producerCandidatesIncluded":false,
        });
        let (segments, segment_method) = if mode.starts_with("probe-") {
            assert_eq!(geometry["segmentCount"], json!(0));
            let (small, primordial, _) = sefer_alloc::registry::segment_route::RouteDirectory::global().live_route_census_for_test();
            (small.checked_add(primordial).expect("census overflow"),
                "process-wide live Small+Primordial directory census for configuration probe only; not paired workload geometry")
        } else {
            (usize::try_from(geometry["segmentCount"].as_u64().expect("union cardinality missing")).expect("union cardinality does not fit usize"),
                R19Prf01::segment_count_method())
        };
        assert!(segments > 0);
        let requested = json!({"maintenance":false,"workCount":work,"segmentCount":segments,"blockBytes":bytes,"cachePolicy":"default","cacheSlots":8,"cacheBudget":{"policy":"unbounded"}});
        let mut resolved = requested.clone();
        resolved["maintenance"] = json!(SeferAlloc::maintenance_running());
        resolved["cacheSlots"] = json!(slots);
        resolved["cacheBudget"] = budget.map_or_else(|| json!({"policy":"unbounded"}), |bytes| json!({"policy":"bounded","bytes":bytes}));
        let features: Vec<_> = [("production", cfg!(feature="production")), ("bench-internals", cfg!(feature="bench-internals")), ("internals", cfg!(feature="internals")), ("alloc-stats", cfg!(feature="alloc-stats"))].into_iter().filter_map(|(name,on)| on.then_some(name)).collect();
        json!({"version":1,"mode":mode,"startedAtUnixMs":started,"features":features,"cfg":["r19_sidecar_summary_oracle"],"claimPath":if mode == "probe-lease" {"lease"} else {"global"},"smallSidecarSizeBytes":HeapCore::dbg_prf01_small_sidecar_size(),"oracle":oracle,"producerStrictTrimConsumed":producer_consumed,"checkpoints":checkpoints,"segmentIncarnations":geometry["incarnations"],"segmentCountMethod":segment_method,"declaredWorkloadBounds":declared_bounds,"config":{"requested":requested,"resolved":resolved,"configConflictsDelta":conflicts,"processBoundary":"isolated-process","runId":run_id,"pid":std::process::id()}})
    }).join().map_err(|_| "oracle worker panicked")?;
    println!("{output}");
    Ok(())
}
