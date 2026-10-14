#[cfg(r19_sidecar_summary_oracle)]
#[path = "../benches/support/r20_prf01_controls.rs"]
mod controls;
#[cfg(r19_sidecar_summary_oracle)]
#[allow(dead_code)]
#[path = "../benches/support/r19_prf01.rs"]
mod workload;

#[cfg(r19_sidecar_summary_oracle)]
use sefer_alloc::{
    global::tls_heap::{current_for_alloc, current_for_trim},
    registry::{config_conflicts_total, HeapCore},
    SeferAlloc,
};
#[cfg(r19_sidecar_summary_oracle)]
use serde_json::{json, Map, Value};
#[cfg(r19_sidecar_summary_oracle)]
use std::{
    alloc::{GlobalAlloc, Layout},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(r19_sidecar_summary_oracle))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("r19_sidecar_summary_oracle requires cfg(r19_sidecar_summary_oracle)".into())
}

#[cfg(r19_sidecar_summary_oracle)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3
        || args[0] != "--c3-json"
        || args[1].trim().is_empty()
        || args[2].trim().is_empty()
    {
        return Err("usage: r20_prf01_c3_oracle --c3-json <cell> <processId>".into());
    }
    let cell = args[1].clone();
    let run_id = args[2].clone();
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let output = std::thread::spawn(move || -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if cfg!(feature = "alloc-stats")
            || cfg!(feature = "large-cache-extended")
            || cfg!(feature = "exact-span-large")
            || cfg!(feature = "large-reserved-capacity")
            || cfg!(feature = "hardened")
            || cfg!(feature = "numa-aware")
        {
            return Err("incompatible oracle feature set".into());
        }
        if cfg!(r18_sidecar_scan_bench)
            || cfg!(r19_prf01_producer_control)
            || cfg!(r20_prf01_c3_producer_control)
        {
            return Err("incompatible oracle cfg".into());
        }
        let conflicts_before = config_conflicts_total();
        let allocator = SeferAlloc::new();
        // Resolve/bind the owner heap without adding an allocation to any workload.
        // SAFETY: the `Own` result is this thread's live heap; no reference is held here.
        let _owner = match current_for_alloc() {
            sefer_alloc::global::tls_heap::CurrentHeap::Own(core) => core,
            sefer_alloc::global::tls_heap::CurrentHeap::Fallback => return Err("owner heap unavailable".into()),
        };
        let supported = cfg!(r20_prf01_c3_oracle);
        let prepared = match cell.as_str() {
            "r19_prf01_publication" => Some(workload::R19Prf01::publication_setup()),
            "r19_prf01_joined_producer_same_word" => None,
            _ => None,
        };
        let same_prepared = if cell == "r19_prf01_joined_producer_same_word" {
            Some(same_word_setup(&allocator)?)
        } else {
            None
        };
        let before = HeapCore::dbg_prf01_sidecar_oracle();
        let summary_before = c3_snapshot(supported);
        let mut geometry = json!({"segmentCount":null,"unique_pending_words":null,"method":"not-applicable"});
        let mut strict = None;
        let mut strict_diag = Value::Null;
        let mut quiescent = None;
        let mut applicable = true;
        match cell.as_str() {
            "multiseg_cold_256k" => workload::R19Prf01::multiseg(2),
            "seg_cycle_decommit_256k" => workload::R19Prf01::multiseg(6),
            "growth16_n" => workload::R19Prf01::growth16(32_768),
            "growth16_2n" => workload::R19Prf01::growth16(65_536),
            "growth16_4n" => workload::R19Prf01::growth16(131_072),
            "r19_prf01_publication" => {
                prepared.expect("publication prepared before baseline").publication_window();
            }
            "r19_prf01_joined_producer_same_word" => {
                let (addresses, prepared_geometry) = same_prepared.as_ref().expect("same-word batch prepared before baseline");
                let layout = Layout::from_size_align(16, 8)?;
                let measured_before = HeapCore::dbg_prf01_sidecar_oracle();
                let c3_before = c3_snapshot(supported);
                std::thread::scope(|scope| scope.spawn(|| {
                    for &address in addresses {
                        // SAFETY: each retained address is a unique live allocation with this exact layout.
                        unsafe { allocator.dealloc(core::ptr::with_exposed_provenance_mut(address), layout) };
                    }
                }).join().expect("joined producer panicked"));
                let after = HeapCore::dbg_prf01_sidecar_oracle();
                let c3_after = c3_snapshot(supported);
                geometry = prepared_geometry.clone();
                let ingress_before = allocator.dbg_sidecar_ingress_stats();
                allocator.trim_current_thread();
                let ingress_after = allocator.dbg_sidecar_ingress_stats();
                assert_eq!(ingress_after.1.checked_sub(ingress_before.1).expect("trim ingress counter regressed"), 4096);
                return Ok(make_output(Output { cell:&cell, run_id:&run_id, started, before:&measured_before, after:&after, c3:c3_delta(c3_before,c3_after), quiescent:c3_after, strict:None, strict_diag:Value::Null, supported, applicable, geometry, conflicts:config_conflicts_total().checked_sub(conflicts_before).expect("conflicts regressed"), slots:allocator.dbg_current_large_cache_total_slots(), budget:allocator.dbg_current_large_cache_budget() }));
            }
            "r20_prf01_c3_producer_distinct_words" | "r20_prf01_c3_remote_large_free" => {
                use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
                let large = cell.ends_with("large_free");
                let layout = Layout::from_size_align(if large { sefer_alloc::SegmentLayout::SMALL_MAX + 1 } else { 1024 }, 8)?;
                let count = if large { 1 } else { 4096 };
                let mut addresses = Vec::with_capacity(count);
                let mut words = std::collections::BTreeSet::new();
                let mut incarnations = std::collections::BTreeSet::new();
                for _ in 0..count {
                    // SAFETY: allocator is live and layout is valid; this pointer is retained until joined deallocation.
                    let ptr = unsafe { allocator.alloc(layout) };
                    assert!(!ptr.is_null());
                    let address = ptr.expose_provenance();
                    let pin = RouteDirectory::global().lookup(ptr).expect("live route absent");
                    if large {
                        assert_eq!(pin.kind(), RouteKind::Large);
                        assert!(pin.contains_payload(ptr, layout.size()));
                    } else {
                        assert!(matches!(pin.kind(), RouteKind::Small | RouteKind::Primordial));
                        assert!(pin.contains_payload(ptr, layout.size()));
                        // SAFETY: pointer is a live allocation from this allocator; its canonical reservation is read-only while it is retained.
                        let (_, _, root) = unsafe { allocator.dbg_current_reservation_for_test(ptr) }.expect("canonical reservation missing");
                        let offset = address.checked_sub(root).expect("address before root");
                        assert_eq!(offset % 1024, 0);
                        words.insert((pin.incarnation(), offset / 1024));
                    }
                    incarnations.insert(pin.incarnation());
                    addresses.push(address);
                }
                let measured_before = HeapCore::dbg_prf01_sidecar_oracle();
                let c3_before = c3_snapshot(supported);
                std::thread::scope(|scope| scope.spawn(|| {
                    for &address in &addresses {
                        let ptr = core::ptr::with_exposed_provenance_mut(address);
                        // SAFETY: each pointer is a unique live allocation with this exact layout; producer joins before inspection.
                        unsafe { allocator.dealloc(ptr, layout) };
                    }
                }).join().expect("joined producer panicked"));
                let after = HeapCore::dbg_prf01_sidecar_oracle();
                let c3_after = c3_snapshot(supported);
                geometry = json!({"segmentCount":incarnations.len(),"unique_pending_words":if large {json!(0)} else {json!(words.len())},"method":"pinned live RouteDirectory incarnation set; Small/Primordial allocations grouped by canonical-root pending-word index"});
                if !large { assert_eq!(words.len(), 4096); }
                let trim_before = allocator.dbg_sidecar_ingress_stats();
                allocator.trim_current_thread();
                let trim_after = allocator.dbg_sidecar_ingress_stats();
                let _consumed = trim_after.1.checked_sub(trim_before.1).expect("trim ingress counter regressed");
                return Ok(make_output(Output { cell:&cell, run_id:&run_id, started, before:&measured_before, after:&after, c3:c3_delta(c3_before,c3_after), quiescent:c3_after, strict:None, strict_diag:Value::Null, supported, applicable, geometry, conflicts:config_conflicts_total().checked_sub(conflicts_before).expect("conflicts regressed"), slots:allocator.dbg_current_large_cache_total_slots(), budget:allocator.dbg_current_large_cache_budget() }));
            }
            "r20_prf01_strict_trim_payload" | "r20_prf01_bounded_maintenance" => {
                let layout = Layout::from_size_align(32, 8)?;
                let mut addresses = Vec::with_capacity(20);
                let mut incarnations = std::collections::BTreeSet::new();
                for _ in 0..20 {
                    // SAFETY: allocator is live, layout is valid, and each allocation is retained until its joined deallocation.
                    let ptr=unsafe{allocator.alloc(layout)};
                    assert!(!ptr.is_null());
                    let pin=sefer_alloc::registry::segment_route::RouteDirectory::global().lookup(ptr).expect("live route absent");
                    incarnations.insert(pin.incarnation());
                    addresses.push(ptr.expose_provenance());
                }
                geometry=json!({"segmentCount":incarnations.len(),"unique_pending_words":null,"method":"pinned live RouteDirectory incarnation set before joined frees"});
                // SAFETY: each address is a distinct live 32-byte allocation, deallocated once before the producer thread joins.
                // SAFETY: each address is a distinct live 32-byte allocation from allocator, deallocated once before joining.
                std::thread::spawn(move || { let producer=SeferAlloc::new(); for address in addresses { unsafe { producer.dealloc(core::ptr::with_exposed_provenance_mut(address),layout) }; } }).join().map_err(|_|"trim producer panicked")?;
                let before_trim = allocator.dbg_sidecar_ingress_stats();
                let strict_before = allocator.dbg_prf01_strict_words_visited();
                quiescent = c3_snapshot(supported);
                let maintenance = cell.ends_with("bounded_maintenance");
                if maintenance {
                    let (n, units) = allocator.dbg_background_maintenance_current_thread(1);
                    assert!(units <= 1 && n <= 20);
                } else {
                    allocator.trim_current_thread();
                }
                let strict_after = allocator.dbg_prf01_strict_words_visited();
                let actual = strict_after.0.checked_sub(strict_before.0).expect("strict words counter regressed");
                let records = strict_after.1.checked_sub(strict_before.1).expect("strict records counter regressed");
                let end = strict_before.2;
                let start = strict_before.3;
                if maintenance {
                    assert_eq!(actual, 0);
                } else {
                    assert_eq!(actual, end.checked_sub(start).expect("strict range regressed"));
                    assert_eq!(records, 20);
                }
                strict_diag = json!({"wordsVisited":actual,"recordsConsumed":records,"endWords":end,"startWords":start});
                let after_trim = allocator.dbg_sidecar_ingress_stats();
                if !maintenance {
                    strict = Some(20u64.checked_sub(after_trim.1.checked_sub(before_trim.1).expect("trim counter regressed")).expect("trim consumed exceeds expected payload"));
                }
            }
            "r20_prf01_start_one_alloc_free_16b" => controls::start_one_alloc_free_16b(),
            "r20_prf01_small_churn_16b_n" => controls::small_churn_16b(64), "r20_prf01_small_churn_16b_2n" => controls::small_churn_16b(128), "r20_prf01_small_churn_16b_4n" => controls::small_churn_16b(256),
            "r20_prf01_churn_256b_n" => controls::churn_256b(64), "r20_prf01_churn_256b_2n" => controls::churn_256b(128), "r20_prf01_churn_256b_4n" => controls::churn_256b(256),
            "r20_prf01_churn_write_256b_n" => controls::churn_write_256b(64), "r20_prf01_churn_write_256b_2n" => controls::churn_write_256b(128), "r20_prf01_churn_write_256b_4n" => controls::churn_write_256b(256),
            "r20_prf01_aligned_churn_640b_a128_n" => controls::aligned_churn_640b_a128(64), "r20_prf01_aligned_churn_640b_a128_2n" => controls::aligned_churn_640b_a128(128), "r20_prf01_aligned_churn_640b_a128_4n" => controls::aligned_churn_640b_a128(256),
            _ if cell.starts_with("mimalloc_") => applicable=false,
            _ => return Err(format!("unknown C3 cell: {cell}").into()),
        }
        let after=HeapCore::dbg_prf01_sidecar_oracle();
        let summary_after=c3_snapshot(supported);
        if cell == "r19_prf01_publication" || cell == "r19_prf01_joined_producer_same_word" {
            let ingress_before=allocator.dbg_sidecar_ingress_stats();
            allocator.trim_current_thread();
            let ingress_after=allocator.dbg_sidecar_ingress_stats();
            let consumed=ingress_after.1.checked_sub(ingress_before.1).expect("trim ingress counter regressed");
            if cell == "r19_prf01_publication" { strict=Some(consumed); } else { assert_eq!(consumed,4096); }
        }
        if ["multiseg_cold_256k","seg_cycle_decommit_256k","growth16_n","growth16_2n","growth16_4n","r19_prf01_publication"].contains(&cell.as_str()) {
            geometry=workload::R19Prf01::observed_geometry();
            geometry["unique_pending_words"]=Value::Null;
        }
        Ok(make_output(Output { cell:&cell, run_id:&run_id, started, before:&before, after:&after, c3:c3_delta(summary_before,summary_after), quiescent:quiescent.or(summary_after), strict, strict_diag, supported, applicable, geometry, conflicts:config_conflicts_total().checked_sub(conflicts_before).expect("conflicts regressed"), slots:allocator.dbg_current_large_cache_total_slots(), budget:allocator.dbg_current_large_cache_budget() }))
    }).join().map_err(|_|"oracle worker panicked")??;
    println!("{output}");
    Ok(())
}

#[cfg(r19_sidecar_summary_oracle)]
fn same_word_setup(
    allocator: &SeferAlloc,
) -> Result<(Vec<usize>, Value), Box<dyn std::error::Error + Send + Sync>> {
    use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
    let layout = Layout::from_size_align(16, 8)?;
    let mut groups = std::collections::BTreeMap::<u64, Vec<usize>>::new();
    for _ in 0..8192 {
        // SAFETY: allocator is live and layout is valid; allocation is tracked until selected/excluded.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        let address = ptr.expose_provenance();
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("issued allocation route missing");
        assert!(matches!(
            pin.kind(),
            RouteKind::Small | RouteKind::Primordial
        ));
        assert!(pin.contains_payload(ptr, 16));
        groups.entry(pin.incarnation()).or_default().push(address);
    }
    let selected = groups
        .values()
        .find(|addresses| addresses.len() >= 4096)
        .expect("bounded 8192 candidates did not provide 4096 allocations on ONE actual route");
    let selected: std::collections::BTreeSet<_> = selected.iter().take(4096).copied().collect();
    let mut retained = Vec::with_capacity(4096);
    for addresses in groups.values() {
        for &address in addresses {
            if selected.contains(&address) {
                retained.push(address);
            } else {
                // SAFETY: unique live excluded candidate with its exact original layout; retired once.
                unsafe {
                    allocator.dealloc(core::ptr::with_exposed_provenance_mut(address), layout)
                };
            }
        }
    }
    assert_eq!(retained.len(), 4096);
    let mut words = std::collections::BTreeSet::new();
    let mut incarnations = std::collections::BTreeSet::new();
    for &address in &retained {
        let ptr = core::ptr::with_exposed_provenance_mut(address);
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("retained live route missing");
        assert!(matches!(
            pin.kind(),
            RouteKind::Small | RouteKind::Primordial
        ));
        incarnations.insert(pin.incarnation());
        // SAFETY: retained live allocation and canonical reservation are stable for this read-only diagnostic.
        let (_, _, root) = unsafe { allocator.dbg_current_reservation_for_test(ptr) }
            .expect("canonical reservation missing");
        let offset = address.checked_sub(root).expect("address before root");
        assert_eq!(offset % 16, 0);
        words.insert((pin.incarnation(), offset / 1024));
    }
    assert_eq!(incarnations.len(), 1);
    assert!(
        words.len() > 0 && words.len() < 4096,
        "same-word geometry must include repeated pending words"
    );
    Ok((
        retained,
        json!({"segmentCount":incarnations.len(),"unique_pending_words":words.len(),"method":"pinned live RouteDirectory incarnation and canonical-root pending-word index"}),
    ))
}

#[cfg(r19_sidecar_summary_oracle)]
struct Output<'a> {
    cell: &'a str,
    run_id: &'a str,
    started: u128,
    before: &'a [u64; 8],
    after: &'a [u64; 8],
    c3: Option<[u64; 4]>,
    quiescent: Option<[u64; 4]>,
    strict: Option<u64>,
    strict_diag: Value,
    supported: bool,
    applicable: bool,
    geometry: Value,
    conflicts: u64,
    slots: usize,
    budget: Option<usize>,
}

#[cfg(r19_sidecar_summary_oracle)]
fn c3_delta(before: Option<[u64; 4]>, after: Option<[u64; 4]>) -> Option<[u64; 4]> {
    let (Some(a), Some(b)) = (before, after) else {
        return None;
    };
    Some([
        b[0].checked_sub(a[0])?,
        b[1].checked_sub(a[1])?,
        b[2].checked_sub(a[2])?,
        b[3],
    ])
}
#[cfg(r19_sidecar_summary_oracle)]
fn c3_snapshot(supported: bool) -> Option<[u64; 4]> {
    #[cfg(r20_prf01_c3_oracle)]
    {
        if !supported {
            return None;
        }
        // The pointer denotes this thread's live owner. The read-only borrow is
        // deliberately short and callers join child producers before snapshots.
        // SAFETY: current_for_trim returns this thread's live heap; no conflicting borrow is held.
        let core = unsafe { current_for_trim()? };
        // SAFETY: current_for_trim returns a valid pointer to this thread's live heap; no conflicting borrow is held while dereferencing.
        let core = unsafe { &*core };
        let raw = core.dbg_prf01_c3_oracle();
        assert_eq!(raw[6], HeapCore::dbg_prf01_small_sidecar_size() as u64);
        let missing = core.dbg_prf01_c3_quiescent_summary_missing_bits();
        assert_eq!(raw[5], missing);
        Some([raw[1], raw[2], raw[3], missing])
    }
    #[cfg(not(r20_prf01_c3_oracle))]
    {
        let _ = supported;
        None
    }
}
#[cfg(r19_sidecar_summary_oracle)]
fn make_output(x: Output<'_>) -> Value {
    let delta: [u64; 8] = core::array::from_fn(|i| {
        x.after[i]
            .checked_sub(x.before[i])
            .expect("oracle counter regressed")
    });
    let names = [
        "pending_words_visited",
        "pending_words_exchanged",
        "nonempty_cuts",
        "discovery_records_reclaimed",
        "routed_scans",
        "summary_words_visited",
        "summary_hint_rmw",
        "summary_hint_loads",
    ];
    let mut oracle = Map::new();
    for (n, v) in names.into_iter().zip(delta) {
        oracle.insert(n.into(), json!(v));
    }
    oracle.insert(
        "unique_pending_words".into(),
        x.geometry["unique_pending_words"].clone(),
    );
    oracle.insert(
        "strict_trim_remaining".into(),
        x.strict.map_or(Value::Null, |v| json!(v)),
    );
    oracle.insert(
        "quiescent_summary_missing_bits".into(),
        x.quiescent.map_or(Value::Null, |v| json!(v[3])),
    );
    oracle.insert(
        "sizeof_small_sidecar".into(),
        json!(HeapCore::dbg_prf01_small_sidecar_size()),
    );
    let mut reasons = Map::new();
    if !x.supported {
        for k in [
            "summary_words_visited",
            "summary_hint_loads",
            "summary_hint_rmw",
            "quiescent_summary_missing_bits",
        ] {
            oracle.insert(k.into(), Value::Null);
            reasons.insert(k.into(), json!("cfg(r20_prf01_c3_oracle) unavailable"));
        }
    } else if let Some(v) = x.c3 {
        oracle.insert("summary_words_visited".into(), json!(v[0]));
        oracle.insert("summary_hint_loads".into(), json!(v[1]));
        oracle.insert("summary_hint_rmw".into(), json!(v[2]));
    } else {
        for k in [
            "summary_words_visited",
            "summary_hint_loads",
            "summary_hint_rmw",
            "quiescent_summary_missing_bits",
        ] {
            oracle.insert(k.into(), Value::Null);
            reasons.insert(
                k.into(),
                json!("candidate diagnostic unavailable; no numeric observation"),
            );
        }
    }
    if x.strict.is_none() {
        reasons.insert(
            "strict_trim_remaining".into(),
            json!("not observed for this cell"),
        );
    }
    if !x.applicable {
        for key in ["requested", "resolved", "configConflictsDelta"] {
            reasons.insert(
                key.into(),
                json!("not applicable: external allocator is not invoked by SeferAlloc oracle"),
            );
        }
    }
    if x.geometry["unique_pending_words"].is_null() {
        reasons.insert(
            "unique_pending_words".into(),
            json!("not observed for this cell"),
        );
    }
    let features: Vec<_> = [
        ("production", cfg!(feature = "production")),
        ("bench-internals", cfg!(feature = "bench-internals")),
        ("internals", cfg!(feature = "internals")),
    ]
    .into_iter()
    .filter_map(|(n, on)| on.then_some(n))
    .collect();
    let mut cfg = vec!["r19_sidecar_summary_oracle"];
    if x.supported {
        cfg.push("r20_prf01_c3_oracle");
    }
    let (block_bytes, work_count): (Value, Value) = match x.cell {
        "multiseg_cold_256k" => (json!(258_752), json!(68)),
        "seg_cycle_decommit_256k" => (json!(258_752), json!(204)),
        "growth16_n" => (json!(16), json!(32_768)),
        "growth16_2n" => (json!(16), json!(65_536)),
        "growth16_4n" => (json!(16), json!(131_072)),
        "r19_prf01_publication" => (json!(258_752), json!(68)),
        "r19_prf01_joined_producer_same_word" => (json!(16), json!(4096)),
        "r20_prf01_c3_producer_distinct_words" => (json!(1024), json!(4096)),
        "r20_prf01_c3_remote_large_free" => {
            (json!(sefer_alloc::SegmentLayout::SMALL_MAX + 1), json!(1))
        }
        "r20_prf01_strict_trim_payload" | "r20_prf01_bounded_maintenance" => (json!(32), json!(20)),
        "r20_prf01_start_one_alloc_free_16b" => (json!(16), json!(1)),
        cell => {
            let bytes = if cell.contains("640b") {
                640
            } else if cell.contains("256b") {
                256
            } else {
                16
            };
            let ops = if cell.ends_with("_4n") {
                256
            } else if cell.ends_with("_2n") {
                128
            } else {
                64
            };
            (json!(bytes), json!(ops))
        }
    };
    let requested = json!({"maintenance":false,"blockBytes":block_bytes,"workCount":work_count,"segmentCount":x.geometry["segmentCount"],"unique_pending_words":x.geometry["unique_pending_words"],"cachePolicy":"default","cacheSlots":8,"cacheBudget":{"policy":"unbounded"}});
    let mut resolved = requested.clone();
    resolved["maintenance"] = json!(SeferAlloc::maintenance_running());
    resolved["cacheSlots"] = json!(x.slots);
    resolved["cacheBudget"] = x.budget.map_or_else(
        || json!({"policy":"unbounded"}),
        |b| json!({"policy":"bounded","bytes":b}),
    );
    let config = if !x.applicable {
        json!({"requested":Value::Null,"resolved":Value::Null,"configConflictsDelta":Value::Null,"notApplicableReason":"external allocator not invoked by SeferAlloc oracle","processBoundary":"isolated-process","runId":x.run_id,"pid":std::process::id()})
    } else {
        json!({"requested":requested,"resolved":resolved,"configConflictsDelta":x.conflicts,"processBoundary":"isolated-process","runId":x.run_id,"pid":std::process::id()})
    };
    oracle.insert("unobservedReasons".into(), Value::Object(reasons));
    json!({"cell":x.cell,"processId":x.run_id,"startedAtUnixMs":x.started,"features":features,"cfg":cfg,"config":config,"geometry":x.geometry,"oracle":oracle,"strictTrimDiagnostics":x.strict_diag})
}
