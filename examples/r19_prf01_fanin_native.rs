use sefer_alloc::SeferAlloc;
use serde_json::{json, Value};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::sync::{Arc, Barrier};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
const WORK: usize = 4096;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let args: Vec<_> = std::env::args().skip(1).collect();
    let smoke = args.len() == 3 && args[2] == "--smoke";
    if (args.len() != 2 && !smoke) || args[1].trim().is_empty() {
        return Err("usage: r19_prf01_fanin_native N RUN_ID".into());
    }
    let n: usize = args[0].parse()?;
    if ![1, 2, 4, 8].contains(&n) {
        return Err("N must be 1, 2, 4 or 8".into());
    }
    let work = if smoke { 64 } else { WORK };
    let twin = cfg!(all(feature = "bench-internals", feature = "internals"));
    if cfg!(feature = "bench-internals") != cfg!(feature = "internals")
        || cfg!(feature = "alloc-stats")
        || cfg!(feature = "large-cache-extended")
        || cfg!(feature = "exact-span-large")
        || cfg!(feature = "large-reserved-capacity")
        || cfg!(feature = "hardened")
        || cfg!(feature = "numa-aware")
        || cfg!(r19_sidecar_summary_oracle)
        || cfg!(r18_sidecar_scan_bench)
        || cfg!(r19_prf01_producer_control)
    {
        return Err("unsupported feature/cfg combination".into());
    }
    assert!(!SeferAlloc::maintenance_running());
    let before = GLOBAL.stats();
    let fallback = GLOBAL.fallback_config_conflicts();
    let layout = Layout::from_size_align(16, 8)?;
    let mut addresses = Vec::with_capacity(work);
    for _ in 0..work {
        // SAFETY: valid nonzero layout; parent owns every allocation until its
        // unique free obligation is transferred to one explicitly joined worker.
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        addresses.push(ptr.expose_provenance());
    }
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    let geometry = {
        use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
        let mut incarnation = None;
        let mut root = None;
        let mut owner = None;
        for &address in &addresses {
            let ptr = core::ptr::with_exposed_provenance_mut(address);
            let pin = RouteDirectory::global()
                .lookup(ptr)
                .expect("live route missing");
            assert!(matches!(
                pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(pin.contains_payload(ptr, 16));
            // SAFETY: exact live allocation start owned by this parent thread;
            // no free, conflicting owner borrow or retirement occurs in this loop.
            let (_, _, actual_root) = unsafe { GLOBAL.dbg_current_reservation_for_test(ptr) }
                .expect("owned canonical root missing");
            if let Some(previous) = incarnation {
                assert_eq!(pin.incarnation(), previous);
            }
            if let Some(previous) = root {
                assert_eq!(actual_root, previous);
            }
            if let Some(previous) = owner {
                assert_eq!(pin.owner(), previous);
            }
            incarnation = Some(pin.incarnation());
            root = Some(actual_root);
            owner = Some(pin.owner());
        }
        let slots = GLOBAL.dbg_current_large_cache_total_slots();
        assert_eq!(slots, 8);
        assert_eq!(GLOBAL.dbg_current_large_cache_budget(), None);
        json!({"routeIncarnation":incarnation,"canonicalRoot":root,"owner":owner,"segmentCount":1,"blockCount":addresses.len(),"cacheSlots":slots,"cacheBudget":{"policy":"unbounded"}})
    };
    #[cfg(not(all(feature = "bench-internals", feature = "internals")))]
    let geometry = Value::Null;
    let ready = Arc::new(Barrier::new(n + 1));
    let start = Arc::new(Barrier::new(n + 1));
    let done = Arc::new(Barrier::new(n + 1));
    let exit = Arc::new(Barrier::new(n + 1));
    let mut handles = Vec::with_capacity(n);
    for chunk in addresses.chunks(work / n) {
        let owned = chunk.to_vec();
        let (ready, start, done, exit) = (
            Arc::clone(&ready),
            Arc::clone(&start),
            Arc::clone(&done),
            Arc::clone(&exit),
        );
        handles.push(std::thread::spawn(move || {
            let mut samples = Vec::with_capacity(work / n);
            ready.wait();
            start.wait();
            for &address in &owned {
                let ptr = core::ptr::with_exposed_provenance_mut(address);
                let t = Instant::now();
                // SAFETY: disjoint chunks transfer each live installed-global
                // allocation exactly once, with exposed provenance and original layout.
                unsafe { dealloc(ptr, layout) };
                samples.push(u64::try_from(t.elapsed().as_nanos()).expect("latency exceeds u64"));
            }
            done.wait();
            exit.wait();
            drop(owned);
            samples
        }));
    }
    ready.wait();
    let t = Instant::now();
    start.wait();
    done.wait();
    let elapsed = u64::try_from(t.elapsed().as_nanos())?;
    let done_stats = GLOBAL.stats();
    let fallback_done = GLOBAL.fallback_config_conflicts();
    let maintenance_done = SeferAlloc::maintenance_running();
    exit.wait();
    let mut samples = Vec::with_capacity(work);
    for handle in handles {
        samples.extend(handle.join().map_err(|_| "producer panicked")?);
    }
    assert_eq!(samples.len(), work);
    let conflicts = done_stats
        .config_conflicts
        .checked_sub(before.config_conflicts)
        .ok_or("conflict counter regressed")?;
    let fallback_delta = fallback_done
        .checked_sub(fallback)
        .ok_or("fallback counter regressed")?;
    assert_eq!((conflicts, fallback_delta), (0, 0));
    assert!(!SeferAlloc::maintenance_running());
    assert!(!maintenance_done);
    let features: Vec<_> = [
        ("production", cfg!(feature = "production")),
        ("bench-internals", cfg!(feature = "bench-internals")),
        ("internals", cfg!(feature = "internals")),
        ("alloc-stats", cfg!(feature = "alloc-stats")),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    if smoke {
        println!(
            "{}",
            json!({"version":1,"scope":"micro-smoke only; no timing evidence","role":if twin {"twin"} else {"native"},"n":n,"features":features,"cfg":[],"workCount":work,"geometry":geometry,"runId":args[1],"pid":std::process::id(),"configConflictsDelta":conflicts,"fallbackConfigConflictsDelta":fallback_delta})
        );
        return Ok(());
    }
    samples.sort_unstable();
    let requested = json!({"maintenance":false,"workCount":WORK,"segmentCount":1,"blockBytes":16,"cachePolicy":"default","cacheSlots":8,"cacheBudget":{"policy":"unbounded"}});
    let mut resolved = requested.clone();
    if !twin {
        for key in ["segmentCount", "cachePolicy", "cacheSlots", "cacheBudget"] {
            resolved[key] = Value::Null;
        }
    }
    println!(
        "{}",
        json!({"version":1,"role":if twin {"twin"} else {"native"},"n":n,"startedAtUnixMs":started,"features":features,"cfg":[],"entry":"installed #[global_allocator] SeferAlloc","geometry":geometry,"resolvedUnavailableReason":if twin {Value::Null} else {json!("production has no route/config readback; requires same-source twin provenance")},"config":{"requested":requested,"resolved":resolved,"configConflictsDelta":conflicts,"fallbackConfigConflictsDelta":fallback_delta,"processBoundary":"isolated-process","runId":args[1],"pid":std::process::id()},"nsPerOp":elapsed as f64 / WORK as f64,"p50Ns":samples[WORK.div_ceil(2)-1],"p99Ns":samples[(WORK*99).div_ceil(100)-1],"latencySampleCount":samples.len(),"quantileMethod":"nearest-rank within process","throughputWindow":"start through done barriers; includes clocks and sample writes; excludes spawn and join","operation":"remote Small16 free"})
    );
    Ok(())
}
