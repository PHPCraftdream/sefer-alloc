// PRF03 A2 primary/paired oracle and lease/global config probes.
// Locked serde_json 1.0.150. All observations are outside benchmark bodies.
#[path = "../benches/support/r19_prf03.rs"]
mod workload;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
use sefer_alloc::{LargeCacheConfig, SeferAlloc};
use serde_json::{json, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::{SystemTime, UNIX_EPOCH};

fn occupied(heap: &HeapCore) -> usize {
    heap.dbg_large_cache_slot_sizes().iter().flatten().count()
}
fn census() -> usize {
    RouteDirectory::global().live_route_census_for_test().2
}
fn counters() -> (u64, u64) {
    (
        AllocCore::dbg_segments_reserved_total(),
        AllocCore::dbg_segments_released_total(),
    )
}
fn delta(before: (u64, u64)) -> (u64, u64) {
    let now = counters();
    (
        now.0.checked_sub(before.0).unwrap(),
        now.1.checked_sub(before.1).unwrap(),
    )
}
fn hits(heap: &HeapCore) -> Option<u64> {
    #[cfg(feature = "alloc-stats")]
    {
        Some(heap.dbg_large_cache_hits())
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        let _ = heap;
        None
    }
}
fn scans() -> Option<u64> {
    #[cfg(feature = "alloc-stats")]
    {
        Some(AllocCore::dbg_routed_miss_scans())
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        None
    }
}
fn diff(now: Option<u64>, before: Option<u64>) -> Option<u64> {
    now.zip(before).map(|(n, b)| n.checked_sub(b).unwrap())
}
fn config(
    slots: usize,
    budget: Option<usize>,
    work: Option<usize>,
    conflicts: u64,
    run_id: &str,
) -> Value {
    assert_eq!(slots, 8);
    assert_eq!(budget, None);
    assert_eq!(conflicts, 0);
    assert!(!SeferAlloc::maintenance_running());
    let requested = json!({"cacheBudget":{"policy":"unbounded"},"cacheSlots":8,"maintenance":false,"workCount":work});
    let resolved = json!({"cacheBudget":budget.map_or_else(|| json!({"policy":"unbounded"}), |bytes| json!({"policy":"bounded","bytes":bytes})),"cacheSlots":slots,"maintenance":SeferAlloc::maintenance_running(),"workCount":work});
    assert_eq!(requested, resolved);
    json!({"requested":requested,"resolved":resolved,"configConflictsDelta":conflicts,"processBoundary":"isolated-process","runId":run_id,"pid":std::process::id()})
}

// Owns System memory, never an allocator reservation. Pointer is not Send;
// Drop uses its original layout. No references into the span are formed.
struct IncarnationProbe {
    ptr: *mut u8,
    layout: Layout,
}
impl IncarnationProbe {
    fn new() -> Self {
        let layout = Layout::from_size_align(workload::BYTES, workload::BYTES).unwrap();
        // SAFETY: valid nonzero SEGMENT-sized/aligned layout, allocated explicitly
        // via System; retained by this owner until all registrations are dropped.
        let ptr = unsafe { System.alloc(layout) };
        assert!(!ptr.is_null(), "System probe allocation failed");
        Self { ptr, layout }
    }
    fn issue(&self) -> u64 {
        let registration = RouteDirectory::global()
            .register_for_test(
                self.ptr,
                self.layout.size(),
                self.ptr,
                usize::MAX,
                RouteKind::Large,
            )
            .expect("probe registration failed");
        let incarnation = registration.incarnation();
        drop(registration); // Must be absent before every census and tail cycle.
        incarnation
    }
}
impl Drop for IncarnationProbe {
    fn drop(&mut self) {
        // SAFETY: this owner allocated ptr via System with layout; registrations
        // and pins have already been dropped, and deallocation occurs once.
        unsafe { System.dealloc(self.ptr, self.layout) };
    }
}
fn identity(heap: &HeapCore, ptr: *mut u8) -> (usize, usize, u64, String) {
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("live Large route missing");
    assert_eq!(pin.kind(), RouteKind::Large);
    (
        heap.dbg_segment_base_of_ptr(ptr).addr(),
        pin.owner(),
        pin.incarnation(),
        format!("{:?}", pin.kind()),
    )
}
fn free(heap: &mut HeapCore, ptr: *mut u8, layout: Layout) {
    // SAFETY: all callers pass their uniquely owned live allocation from heap
    // with exactly its allocation layout, once; identity pins are already dropped.
    unsafe { heap.dealloc(ptr, layout) };
}

fn main() {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(args.len()==3 || (args.len()==4 && args[3]=="--negative-control"),"usage: r19_prf03_paired_oracle R1|R2|R3|probe-lease|probe-global A|B RUN_ID [--negative-control]");
    let regime = args[0].as_str();
    let cached = match args[1].as_str() {
        "A" => 0usize,
        "B" => 1usize,
        _ => panic!("expected A or B"),
    };
    assert!(!args[2].trim().is_empty());
    if cfg!(feature = "large-cache-extended")
        || cfg!(feature = "exact-span-large")
        || cfg!(feature = "large-reserved-capacity")
        || cfg!(feature = "hardened")
        || cfg!(feature = "numa-aware")
    {
        panic!("unsupported feature set");
    }
    let features: Vec<_> = [
        ("production", cfg!(feature = "production")),
        ("bench-internals", cfg!(feature = "bench-internals")),
        ("internals", cfg!(feature = "internals")),
        ("alloc-stats", cfg!(feature = "alloc-stats")),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    let negative = args.len() == 4;
    let conflicts = config_conflicts_total();
    let mut output = json!({"version":1,"arm":args[1],"regime":regime,"features":features,"startedAtUnixMs":started,"requestedConfigDebug":format!("{:?}",LargeCacheConfig::DEFAULT),"negativeControl":negative,"pid":std::process::id(),"runId":args[2],"processBoundary":"isolated-process"});
    if regime == "probe-global" {
        assert!(!negative);
        let global = SeferAlloc::new();
        let layout = Layout::from_size_align(workload::BYTES, 8).unwrap();
        // SAFETY: layout is valid and nonzero; direct GlobalAlloc uses this
        // SeferAlloc instance. The result is deallocated once below.
        let ptr = unsafe { global.alloc(layout) };
        assert!(!ptr.is_null());
        let slots = global.dbg_current_large_cache_total_slots();
        let budget = global.dbg_current_large_cache_budget();
        assert_ne!(slots, 0, "global probe must own a heap");
        // SAFETY: ptr is the live allocation from this instance and exact layout.
        unsafe { global.dealloc(ptr, layout) };
        output["config"] = config(
            slots,
            budget,
            None,
            config_conflicts_total().checked_sub(conflicts).unwrap(),
            &args[2],
        );
        output["claimPath"] = json!("global");
        println!("{output}");
        return;
    }
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().unwrap();
    let heap = lease.core();
    let slots = heap.dbg_large_cache_total_slots();
    let budget = heap.dbg_large_cache_budget();
    assert_eq!(slots, 8);
    assert_eq!(budget, None);
    output["claimPath"] = json!("lease");
    if regime == "probe-lease" {
        assert!(!negative);
        output["config"] = config(
            slots,
            budget,
            None,
            config_conflicts_total().checked_sub(conflicts).unwrap(),
            &args[2],
        );
        println!("{output}");
        return;
    }
    let baseline = census();
    let scans_before = scans();
    let mut last_prefix = None;
    let mut previous_payload = None;
    let mut changes = 0usize;
    let mut observe = |heap: &HeapCore, ptr: *mut u8| {
        let root = heap.dbg_segment_base_of_ptr(ptr).addr();
        let offset = ptr.addr().checked_sub(root).unwrap();
        if let Some((old_root, old_offset)) = previous_payload {
            assert_eq!(root, old_root, "R3 reservation changed");
            assert_ne!(offset, old_offset, "R3 payload did not change");
            changes += 1;
        }
        previous_payload = Some((root, offset));
    };
    match regime {
        "R1" => {
            let layout = Layout::from_size_align(workload::BYTES, 8).unwrap();
            for _ in 0..8 {
                let ptr = heap.alloc(layout);
                black_box(ptr);
                assert!(!ptr.is_null());
                last_prefix = Some((ptr.addr(), identity(heap, ptr)));
                free(heap, ptr, layout);
            }
        }
        "R2" => workload::eviction_round(heap),
        "R3" => workload::payload_prefill(heap, &mut observe),
        _ => panic!("unknown regime"),
    }
    let prefill_scans = scans();
    let hits_before = hits(heap);
    let slots_before = occupied(heap);
    let before = counters();
    let mut alloc_delta = None;
    let mut slots_alloc = None;
    let mut census_alloc = None;
    let mut incarnation_delta = None;
    let mut route_consistency = Value::Null;
    match regime {
        "R1" => {
            let probe = IncarnationProbe::new();
            let bracket_before = probe.issue();
            let layout = Layout::from_size_align(
                if negative {
                    3 * workload::BYTES
                } else {
                    workload::BYTES
                },
                8,
            )
            .unwrap();
            let ptr = heap.alloc(layout);
            black_box(ptr);
            assert!(!ptr.is_null());
            let tail = identity(heap, ptr);
            alloc_delta = Some(delta(before));
            slots_alloc = Some(occupied(heap));
            census_alloc = Some(census().checked_sub(baseline).unwrap());
            let (prefix_ptr, prefix) = last_prefix.unwrap();
            if !negative {
                assert_eq!(ptr.addr(), prefix_ptr);
                assert_eq!(tail.0, prefix.0);
                assert_eq!(tail.1, prefix.1);
                assert_eq!(tail.3, prefix.3);
                assert_eq!(
                    tail.2,
                    if cached == 0 {
                        bracket_before.checked_add(1).unwrap()
                    } else {
                        prefix.2
                    }
                );
                assert_eq!(slots_before, 1);
                assert_eq!(slots_alloc, Some(0));
                assert_eq!(alloc_delta, Some((0, 0)));
                assert_eq!(census_alloc, Some(1));
            } else {
                assert!(slots_alloc.unwrap() >= slots_before);
                assert_eq!(alloc_delta.unwrap().0, 1);
            }
            free(heap, ptr, layout);
            let bracket_after = probe.issue();
            incarnation_delta = Some(
                bracket_after
                    .checked_sub(bracket_before)
                    .unwrap()
                    .checked_sub(1)
                    .unwrap(),
            );
            route_consistency = json!({"samePointer":ptr.addr()==prefix_ptr,"sameRoot":tail.0==prefix.0,"sameOwner":tail.1==prefix.1,"sameKind":tail.3==prefix.3,"tailIncarnation":tail.2,"prefixIncarnation":prefix.2,"bracketBefore":bracket_before,"bracketAfter":bracket_after});
            if !negative {
                assert_eq!(incarnation_delta, Some((1 - cached) as u64));
            }
        }
        "R2" => {
            if !negative {
                workload::eviction_round(heap);
            }
        }
        "R3" => {
            let layout =
                Layout::from_size_align(workload::BYTES, if negative { 65536 } else { 8 }).unwrap();
            let ptr = heap.alloc(layout);
            black_box(ptr);
            assert!(!ptr.is_null());
            observe(heap, ptr);
            alloc_delta = Some(delta(before));
            slots_alloc = Some(occupied(heap));
            free(heap, ptr, layout);
        }
        _ => unreachable!(),
    }
    let whole_delta = delta(before);
    let slots_after = occupied(heap);
    let cached_census = census().checked_sub(baseline).unwrap();
    let tail_hits = diff(hits(heap), hits_before);
    let tail_scans = diff(scans(), prefill_scans);
    let whole_scans = diff(scans(), scans_before);
    let work_count = if regime == "R2" {
        2 * workload::OBJECTS
    } else {
        9
    };
    match regime {
        "R1" if !negative => {
            assert_eq!(whole_delta, (0, 0));
            assert_eq!(slots_after, 1);
            assert_eq!(cached_census, cached);
            if let Some(h) = tail_hits {
                assert_eq!(h, 1);
            }
        }
        "R1" => {
            if let Some(h) = tail_hits {
                assert_eq!(h, 0);
            }
        }
        "R2" => {
            assert_eq!((slots_before, slots_after), (slots, slots));
            assert_eq!(
                whole_delta,
                (
                    (workload::OBJECTS - slots) as u64,
                    (workload::OBJECTS - slots) as u64
                )
            );
            if let Some(h) = tail_hits {
                assert_eq!(h, slots as u64);
            }
        }
        "R3" => {
            assert_eq!(alloc_delta, Some((0, 0)));
            assert_eq!(whole_delta, (0, 0));
            assert_eq!(slots_alloc, Some(slots_before - 1));
            assert_eq!(slots_after, slots_before);
            if let Some(h) = tail_hits {
                assert_eq!(h, 1);
            }
            assert_eq!(
                changes, 8,
                "R3 negative control must reject unchanged payload"
            );
        }
        _ => unreachable!(),
    }
    output["config"] = config(
        slots,
        budget,
        Some(work_count),
        config_conflicts_total().checked_sub(conflicts).unwrap(),
        &args[2],
    );
    output["oracle"] = json!({"tailHitDelta":tail_hits,"incarnationDelta":incarnation_delta,"cachedCensus":cached_census});
    output["structural"] = json!({"slotsBefore":slots_before,"slotsAfterAlloc":slots_alloc,"slotsAfterFree":slots_after,"allocReservedDelta":alloc_delta.map(|d|d.0),"allocReleasedDelta":alloc_delta.map(|d|d.1),"cycleReservedDelta":whole_delta.0,"cycleReleasedDelta":whole_delta.1,"censusAfterAlloc":census_alloc,"cachedCensus":cached_census,"routeConsistency":route_consistency,"payloadChanges":changes});
    output["routedScans"] = json!(whole_scans);
    output["routedMissScans"] = json!(whole_scans);
    output["routedScanSplit"] =
        json!({"prefill":diff(prefill_scans,scans_before),"tail":tail_scans});
    output["countersUnavailableReason"] = if cfg!(feature = "alloc-stats") {
        Value::Null
    } else {
        json!("alloc-stats compiled out; zero is not evidence")
    };
    output["description"] = json!(if regime == "R2" {
        "eviction-dominated 33/41"
    } else {
        regime
    });
    println!("{output}");
}
