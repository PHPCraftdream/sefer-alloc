// PRF03 A2: one source for exact-production native and diagnostic twin.
// serde_json is locked at 1.0.150; no hashing dependency: runner hashes the
// UTF-8 bytes of requestedConfigDebug, without quotes or a trailing newline.
use sefer_alloc::{LargeCacheConfig, SeferAlloc};
use serde_json::{json, Value};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::hint::black_box;
use std::sync::{Arc, Barrier};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
const BYTES: usize = 4 * 1024 * 1024;

fn cycle(layout: Layout) {
    // SAFETY: layout is valid and nonzero; allocation uses the installed allocator.
    let ptr = unsafe { alloc(black_box(layout)) };
    if ptr.is_null() {
        handle_alloc_error(layout);
    }
    black_box(ptr);
    // SAFETY: this worker uniquely owns the live allocation, valid for at least
    // one byte. Initialize before reading; deallocate exactly once with its layout.
    unsafe {
        ptr.write_volatile(0x5a);
        black_box(ptr.read_volatile());
        dealloc(black_box(ptr), layout);
    }
}

fn cache() -> Value {
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    {
        let slots = GLOBAL.dbg_current_large_cache_total_slots();
        let budget = GLOBAL.dbg_current_large_cache_budget();
        json!({"cacheSlots": slots, "cacheBudget": budget.map_or_else(|| json!({"policy":"unbounded"}), |bytes| json!({"policy":"bounded", "bytes":bytes}))})
    }
    #[cfg(not(all(feature = "bench-internals", feature = "internals")))]
    {
        json!({"cacheSlots":null, "cacheBudget":null})
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: r19_prf03_native_mt N TOTAL_WORK RUN_ID; N=1|4|8".into());
    }
    let n: usize = args[0].parse()?;
    let work: usize = args[1].parse()?;
    if ![1, 4, 8].contains(&n) || work == 0 || !work.is_multiple_of(8) || args[2].trim().is_empty()
    {
        return Err("invalid N, TOTAL_WORK or RUN_ID".into());
    }
    let twin = cfg!(all(feature = "bench-internals", feature = "internals"));
    if cfg!(feature = "bench-internals") != cfg!(feature = "internals")
        || cfg!(feature = "alloc-stats")
        || cfg!(feature = "large-cache-extended")
        || cfg!(feature = "exact-span-large")
        || cfg!(feature = "large-reserved-capacity")
        || cfg!(feature = "hardened")
        || cfg!(feature = "numa-aware")
    {
        return Err("unsupported feature set".into());
    }
    let requested_debug = format!("{:?}", LargeCacheConfig::DEFAULT);
    let layout = Layout::from_size_align(BYTES, 8)?;
    let ready = Arc::new(Barrier::new(n + 1));
    let start = Arc::new(Barrier::new(n + 1));
    let done = Arc::new(Barrier::new(n + 1));
    let exit = Arc::new(Barrier::new(n + 1));
    let per_worker = work / n;
    let mut handles = Vec::with_capacity(n);
    // Main-thread scaffolding is initialized before the high-water baseline.
    let before = GLOBAL.stats();
    let fallback_before = GLOBAL.fallback_config_conflicts();
    assert!(!SeferAlloc::maintenance_running());
    for index in 0..n {
        let (ready, start, done, exit) = (
            Arc::clone(&ready),
            Arc::clone(&start),
            Arc::clone(&done),
            Arc::clone(&exit),
        );
        handles.push(std::thread::spawn(move || {
            let mut samples = Vec::with_capacity(per_worker);
            cycle(layout);
            let resolved = cache();
            let stats_start = GLOBAL.stats();
            let fallback_start = GLOBAL.fallback_config_conflicts();
            let maintenance_start = SeferAlloc::maintenance_running();
            ready.wait();
            start.wait();
            for _ in 0..per_worker {
                let t = Instant::now();
                cycle(layout);
                samples.push(u64::try_from(t.elapsed().as_nanos()).expect("latency exceeds u64"));
            }
            let stats_end = GLOBAL.stats();
            let fallback_end = GLOBAL.fallback_config_conflicts();
            let maintenance_end = SeferAlloc::maintenance_running();
            done.wait();
            // Keep TLS heaps alive until the parent takes the done snapshot.
            exit.wait();
            (samples, json!({"worker":index, "workCount":per_worker, "cacheSlots":resolved["cacheSlots"], "cacheBudget":resolved["cacheBudget"], "maintenance":maintenance_start, "maintenanceAfter":maintenance_end, "configConflictsDelta":stats_end.config_conflicts.checked_sub(stats_start.config_conflicts).unwrap(), "fallbackConfigConflictsDelta":fallback_end.checked_sub(fallback_start).unwrap()}))
        }));
    }
    ready.wait();
    let ready_stats = GLOBAL.stats();
    let t = Instant::now();
    start.wait();
    done.wait();
    let elapsed_ns = u64::try_from(t.elapsed().as_nanos())?;
    let done_stats = GLOBAL.stats();
    let fallback_done = GLOBAL.fallback_config_conflicts();
    let maintenance_after = SeferAlloc::maintenance_running();
    exit.wait();
    let mut samples = Vec::with_capacity(work);
    let mut workers = Vec::with_capacity(n);
    for handle in handles {
        let (worker_samples, metadata) = handle.join().map_err(|_| "worker panicked")?;
        assert_eq!(worker_samples.len(), per_worker);
        assert_eq!(metadata["maintenance"], false);
        assert_eq!(metadata["maintenanceAfter"], false);
        assert_eq!(metadata["configConflictsDelta"], 0);
        assert_eq!(metadata["fallbackConfigConflictsDelta"], 0);
        if twin {
            assert_eq!(
                metadata["cacheSlots"], 8,
                "zero slots means fallback, not owned"
            );
            assert_eq!(metadata["cacheBudget"], json!({"policy":"unbounded"}));
            if let Some(first) = workers.first() {
                let first: &Value = first;
                assert_eq!(metadata["cacheSlots"], first["cacheSlots"]);
                assert_eq!(metadata["cacheBudget"], first["cacheBudget"]);
            }
        }
        samples.extend(worker_samples);
        workers.push(metadata);
    }
    assert_eq!(samples.len(), work);
    let claims = ready_stats
        .heaps_claimed_high_water
        .checked_sub(before.heaps_claimed_high_water)
        .ok_or("claim counter regressed")?;
    let reserved = done_stats
        .segments_reserved_total
        .checked_sub(ready_stats.segments_reserved_total)
        .ok_or("reserve counter regressed")?;
    let released = done_stats
        .segments_released_total
        .checked_sub(ready_stats.segments_released_total)
        .ok_or("release counter regressed")?;
    let conflicts = done_stats
        .config_conflicts
        .checked_sub(before.config_conflicts)
        .ok_or("conflict counter regressed")?;
    let fallback = fallback_done
        .checked_sub(fallback_before)
        .ok_or("fallback counter regressed")?;
    assert_eq!(claims, u64::try_from(n)?);
    assert_eq!((reserved, released, conflicts, fallback), (0, 0, 0, 0));
    assert!(!maintenance_after);
    samples.sort_unstable();
    let rank = work
        .checked_mul(99)
        .ok_or("p99 rank overflow")?
        .div_ceil(100);
    let resolved_cache = if twin {
        json!({"cacheSlots":workers[0]["cacheSlots"],"cacheBudget":workers[0]["cacheBudget"]})
    } else {
        json!({"cacheSlots":null,"cacheBudget":null})
    };
    let features: Vec<_> = [
        ("production", cfg!(feature = "production")),
        ("bench-internals", cfg!(feature = "bench-internals")),
        ("internals", cfg!(feature = "internals")),
        ("alloc-stats", cfg!(feature = "alloc-stats")),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    println!(
        "{}",
        json!({
            "version":1, "role":if twin {"twin"} else {"native"}, "verdict":"VALID", "features":features, "n":n,
            "startedAtUnixMs":started_at, "requestedConfigDebug":requested_debug,
            "nsPerOp":elapsed_ns as f64 / work as f64, "p99Ns":samples[rank-1], "latencySampleCount":samples.len(),
            "metricUnit":"nanoseconds", "operation":"one alloc(4MiB,8)/free pair", "warmupCycles":n,
            "p99Method":"nearest-rank ceil(0.99 * process samples)",
            "throughputWindow":"start barrier through done barrier; excludes spawn/warmup/join/sort; includes latency instrumentation",
            "config":{"requested":{"cacheBudget":{"policy":"unbounded"},"cacheSlots":8,"maintenance":false,"workCount":work},
                "resolved":{"cacheBudget":resolved_cache["cacheBudget"],"cacheSlots":resolved_cache["cacheSlots"],"maintenance":maintenance_after,"workCount":samples.len()},
                "configConflictsDelta":conflicts,"fallbackConfigConflictsDelta":fallback,"processBoundary":"isolated-process","runId":args[2],"pid":std::process::id()},
            "workers":workers,"heapsClaimedHighWaterDelta":claims,"readyDoneReservedDelta":reserved,"readyDoneReleasedDelta":released,
            "stats":{"large_cache_hits":null,"tcache_hits":null,"unavailableReason":"alloc-stats compiled out"},
            "resolvedCacheUnavailableReason":if twin {Value::Null} else {json!("exact production has no owned cache observer; runner must merge the immediately preceding twin with provenance under A2")}
        })
    );
    Ok(())
}
