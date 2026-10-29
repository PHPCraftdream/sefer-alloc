//! G1 rhythm probe (`docs/perf/g1_scan/*_PREREG.md`), measurement only.
//!
//! SC (discovery scans of a growing or large live heap):
//!   g1_rhythm_probe growth <size> <n> [mi]
//!   g1_rhythm_probe trickle <size> <n> <k> [control]   one foreign free every k×16 allocations
//!   g1_rhythm_probe bigheap <n_fill> <m>               fill with 64 B, measure m × 1 KiB
//! PC (route pin cache under a foreign free order):
//!   g1_rhythm_probe pincycle <m> <cyclic|seq|random>   2048 × 1 KiB blocks from each of m segments
//! RP (pause of the refill that reclaims a backlog of foreign frees):
//!   g1_rhythm_probe reclaim <n_fill> <n_free> [mi]     64 B; every later alloc timed on its own
//!
//! Prints one JSON line per run. Built with `bench-internals internals
//! alloc-stats` it carries the counter deltas of each phase (`"counters":
//! true`); built with `production` alone it carries times only, so timing
//! runs pay no diagnostic increments. `mi` runs the same body on mimalloc.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};
use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::sync::mpsc;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

static SEFER: SeferAlloc = SeferAlloc::new();
static MI: mimalloc::MiMalloc = mimalloc::MiMalloc;

const SEGMENT: usize = 4 << 20;
const REFILL: usize = 16;
const PC_PER_SEGMENT: usize = 2048;

const COUNTERS: bool = cfg!(all(
    feature = "bench-internals",
    feature = "internals",
    feature = "alloc-stats"
));

#[derive(Clone, Copy, Default)]
struct Counters {
    find_segment: u64,
    scans: u64,
    words: u64,
    foreign_ok: u64,
    lookups: u64,
    discovery: u64,
    deferred: u64,
    routed_miss: u64,
    slots: u64,
    pin: [u64; 4],
}

#[cfg(all(
    feature = "bench-internals",
    feature = "internals",
    feature = "alloc-stats"
))]
fn snapshot() -> Counters {
    use sefer_alloc::registry::segment_route::RoutePin;
    use sefer_alloc::AllocCore;
    let c = SeferAlloc::dbg_b4_collapse_counters();
    let u = SeferAlloc::dbg_b4_u31_scan_counters();
    let p = RoutePin::dbg_route_pin_cache_totals();
    Counters {
        find_segment: c[0],
        scans: c[1],
        words: c[2],
        foreign_ok: c[3],
        lookups: c[4],
        discovery: u[3],
        deferred: SeferAlloc::dbg_b4_u31_deferred_scans(),
        routed_miss: AllocCore::dbg_routed_miss_scans(),
        slots: AllocCore::dbg_full_scan_slots_examined(),
        pin: [p.0, p.1, p.2, p.3],
    }
}

#[cfg(not(all(
    feature = "bench-internals",
    feature = "internals",
    feature = "alloc-stats"
)))]
fn snapshot() -> Counters {
    Counters::default()
}

/// `"<prefix>name":delta,...` for one phase.
fn delta(prefix: &str, a: Counters, b: Counters) -> String {
    let pairs = [
        ("find_segment", b.find_segment - a.find_segment),
        ("scans", b.scans - a.scans),
        ("scan_words", b.words - a.words),
        ("foreign_publish_ok", b.foreign_ok - a.foreign_ok),
        ("directory_lookups", b.lookups - a.lookups),
        ("discovery_scans", b.discovery - a.discovery),
        ("deferred_scans", b.deferred - a.deferred),
        ("routed_miss_scans", b.routed_miss - a.routed_miss),
        ("full_scan_slots", b.slots - a.slots),
        ("pin_hits", b.pin[0] - a.pin[0]),
        ("pin_misses", b.pin[1] - a.pin[1]),
        ("pin_inserts", b.pin[2] - a.pin[2]),
        ("pin_evictions", b.pin[3] - a.pin[3]),
    ];
    pairs
        .iter()
        .map(|(k, v)| format!("\"{prefix}{k}\":{v}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn heap(mi: bool) -> &'static (dyn GlobalAlloc + Sync) {
    if mi {
        &MI as &(dyn GlobalAlloc + Sync)
    } else {
        &SEFER
    }
}

fn side(mi: bool) -> &'static str {
    if mi {
        "mimalloc"
    } else {
        "sefer"
    }
}

fn alloc(a: &(dyn GlobalAlloc + Sync), layout: Layout) -> usize {
    // SAFETY: non-zero size, power-of-two alignment; the block stays live
    // until process exit or until it is handed to a freer exactly once.
    let p = unsafe { a.alloc(layout) };
    assert!(!p.is_null());
    p.expose_provenance()
}

fn free(a: &(dyn GlobalAlloc + Sync), addr: usize, layout: Layout) {
    let p = core::ptr::with_exposed_provenance_mut::<u8>(addr);
    // SAFETY: `addr` is a live block of `layout` from `a`, handed over
    // exactly once; this is its only free.
    unsafe { a.dealloc(p, layout) };
}

fn segments(addrs: &[usize]) -> usize {
    addrs
        .iter()
        .map(|a| a & !(SEGMENT - 1))
        .collect::<HashSet<_>>()
        .len()
}

/// Frees `addrs` on a fresh thread, in order, and waits for it.
fn free_on_other_thread(
    a: &'static (dyn GlobalAlloc + Sync),
    addrs: Vec<usize>,
    layout: Layout,
) -> u128 {
    std::thread::spawn(move || {
        let t = Instant::now();
        for x in addrs {
            free(a, x, layout);
        }
        t.elapsed().as_nanos()
    })
    .join()
    .unwrap()
}

fn growth(size: usize, n: usize, mi: bool) {
    let a = heap(mi);
    let layout = Layout::from_size_align(size, 8).unwrap();
    let mut addrs = Vec::with_capacity(n);
    let before = snapshot();
    let t = Instant::now();
    for _ in 0..n {
        addrs.push(alloc(a, layout));
    }
    let ns = t.elapsed().as_nanos();
    let after = snapshot();
    black_box(&addrs);
    println!(
        "{{\"mode\":\"growth\",\"alloc\":\"{}\",\"counters\":{COUNTERS},\"size\":{size},\"n\":{n},\"segments\":{},{},\"phase_ns\":{ns}}}",
        side(mi),
        segments(&addrs),
        delta("", before, after)
    );
}

/// `control`: the same gifts are carved first but never handed over, so
/// the two arms start from the same carved range and magazine state.
fn trickle(size: usize, n: usize, k: usize, control: bool) {
    let a = heap(false);
    let layout = Layout::from_size_align(size, 8).unwrap();
    let gifts_total = n / (REFILL * k) + 1;
    let gifts: Vec<usize> = (0..gifts_total).map(|_| alloc(a, layout)).collect();
    let (to_freer, rx) = mpsc::channel::<usize>();
    let (ack_tx, ack) = mpsc::channel::<()>();
    let freer = std::thread::spawn(move || {
        for x in rx {
            free(heap(false), x, layout);
            ack_tx.send(()).unwrap();
        }
    });
    let mut gifted: HashMap<usize, usize> = HashMap::new();
    let mut latencies: Vec<usize> = Vec::new();
    let mut next_gift = 0usize;
    let mut addrs = Vec::with_capacity(n);
    let before = snapshot();
    let t = Instant::now();
    for i in 0..n {
        if !control && i % (REFILL * k) == 0 && next_gift < gifts.len() {
            to_freer.send(gifts[next_gift]).unwrap();
            ack.recv().unwrap();
            gifted.insert(gifts[next_gift], i);
            next_gift += 1;
        }
        let x = alloc(a, layout);
        if let Some(at) = gifted.remove(&x) {
            latencies.push((i - at) / REFILL);
        }
        addrs.push(x);
    }
    let ns = t.elapsed().as_nanos();
    let after = snapshot();
    drop(to_freer);
    freer.join().unwrap();
    black_box(&addrs);
    latencies.sort_unstable();
    let pct = |q: usize| {
        latencies
            .get(latencies.len().saturating_sub(1) * q / 100)
            .copied()
            .unwrap_or(0)
    };
    let mut all = addrs.clone();
    all.extend_from_slice(&gifts);
    println!(
        "{{\"mode\":\"trickle\",\"alloc\":\"sefer\",\"counters\":{COUNTERS},\"size\":{size},\"n\":{n},\"k\":{k},\"control\":{control},\"gifts_carved\":{gifts_total},\"gifts\":{next_gift},\"reissued\":{},\"reclaim_alloc_refills_p50\":{},\"reclaim_alloc_refills_max\":{},\"segments\":{},{},\"phase_ns\":{ns}}}",
        latencies.len(),
        pct(50),
        pct(100),
        segments(&all),
        delta("", before, after)
    );
}

fn bigheap(n_fill: usize, m: usize) {
    let a = heap(false);
    let fill = Layout::from_size_align(64, 8).unwrap();
    let probe = Layout::from_size_align(1024, 8).unwrap();
    let mut addrs: Vec<usize> = (0..n_fill).map(|_| alloc(a, fill)).collect();
    let fill_segments = segments(&addrs);
    let before = snapshot();
    let t = Instant::now();
    for _ in 0..m {
        addrs.push(alloc(a, probe));
    }
    let ns = t.elapsed().as_nanos();
    let after = snapshot();
    black_box(&addrs);
    println!(
        "{{\"mode\":\"bigheap\",\"alloc\":\"sefer\",\"counters\":{COUNTERS},\"n_fill\":{n_fill},\"m\":{m},\"fill_segments\":{fill_segments},\"segments\":{},{},\"phase_ns\":{ns}}}",
        segments(&addrs),
        delta("", before, after)
    );
}

/// xorshift64*, fixed seed: the same "random" order in every run.
fn shuffle(v: &mut [usize]) {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    for i in (1..v.len()).rev() {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        let j = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

/// Exactly `m` segments, `PC_PER_SEGMENT` blocks freed from each; every
/// other carved block stays live.
fn pincycle(m: usize, order: &str) {
    let a = heap(false);
    let layout = Layout::from_size_align(1024, 8).unwrap();
    let carved: Vec<usize> = (0..(m + 2) * 4096).map(|_| alloc(a, layout)).collect();
    let mut by_seg: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut index: HashMap<usize, usize> = HashMap::new();
    for &x in &carved {
        let s = x & !(SEGMENT - 1);
        let i = *index.entry(s).or_insert_with(|| {
            by_seg.push((s, Vec::new()));
            by_seg.len() - 1
        });
        by_seg[i].1.push(x);
    }
    by_seg.retain(|(_, v)| v.len() >= PC_PER_SEGMENT);
    assert!(
        by_seg.len() >= m,
        "only {} segments hold {PC_PER_SEGMENT} blocks",
        by_seg.len()
    );
    by_seg.truncate(m);
    for (_, v) in &mut by_seg {
        v.truncate(PC_PER_SEGMENT);
    }
    let mut frees: Vec<usize> = Vec::with_capacity(m * PC_PER_SEGMENT);
    match order {
        "seq" => by_seg.iter().for_each(|(_, v)| frees.extend_from_slice(v)),
        "cyclic" => {
            (0..PC_PER_SEGMENT).for_each(|k| by_seg.iter().for_each(|(_, v)| frees.push(v[k])))
        }
        "random" => {
            by_seg.iter().for_each(|(_, v)| frees.extend_from_slice(v));
            shuffle(&mut frees);
        }
        _ => panic!("order: cyclic | seq | random"),
    }
    let n = frees.len();
    let before = snapshot();
    let ns = free_on_other_thread(a, frees, layout);
    let after = snapshot();
    black_box(&carved);
    println!(
        "{{\"mode\":\"pincycle\",\"alloc\":\"sefer\",\"counters\":{COUNTERS},\"m\":{m},\"order\":\"{order}\",\"n\":{n},\"segments\":{},{},\"free_ns\":{ns}}}",
        by_seg.len(),
        delta("", before, after)
    );
}

fn reclaim(n_fill: usize, n_free: usize, mi: bool) {
    let a = heap(mi);
    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut live: Vec<usize> = (0..n_fill).map(|_| alloc(a, layout)).collect();
    let segs = segments(&live);
    // The backlog: every (n_fill / n_free)-th block, spread over all segments.
    let step = (n_fill / n_free).max(1);
    let mut backlog = Vec::with_capacity(n_free);
    let mut keep = Vec::with_capacity(n_fill + 2 * n_free);
    for (i, x) in live.drain(..).enumerate() {
        if i % step == 0 && backlog.len() < n_free {
            backlog.push(x);
        } else {
            keep.push(x);
        }
    }
    let freed = backlog.len();
    let b0 = snapshot();
    free_on_other_thread(a, backlog, layout);
    let b1 = snapshot();
    // Owner allocates again; every call is timed on its own.
    let calls = 2 * freed;
    let mut times: Vec<u64> = Vec::with_capacity(calls);
    for _ in 0..calls {
        let t = Instant::now();
        let x = alloc(a, layout);
        times.push(t.elapsed().as_nanos() as u64);
        keep.push(x);
    }
    let b2 = snapshot();
    black_box(&keep);
    let total: u64 = times.iter().sum();
    let mut sorted = times;
    sorted.sort_unstable();
    let q = |p: usize| sorted[(sorted.len() - 1) * p / 1000];
    println!(
        "{{\"mode\":\"reclaim\",\"alloc\":\"{}\",\"counters\":{COUNTERS},\"n_fill\":{n_fill},\"n_free\":{freed},\"calls\":{calls},\"segments\":{segs},{},{},\"call_ns_max\":{},\"call_ns_p999\":{},\"call_ns_p99\":{},\"call_ns_p50\":{},\"calls_ns_total\":{total}}}",
        side(mi),
        delta("backlog_", b0, b1),
        delta("", b1, b2),
        sorted[sorted.len() - 1],
        q(999),
        q(990),
        q(500),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num = |i: usize| -> usize { args[i].parse().expect("numeric argument") };
    let flag = |i: usize, f: &str| args.get(i).is_some_and(|s| s == f);
    match args.get(1).map(String::as_str) {
        Some("growth") => growth(num(2), num(3), flag(4, "mi")),
        Some("trickle") => trickle(num(2), num(3), num(4), flag(5, "control")),
        Some("bigheap") => bigheap(num(2), num(3)),
        Some("pincycle") => pincycle(num(2), &args[3]),
        Some("reclaim") => reclaim(num(2), num(3), flag(4, "mi")),
        _ => panic!("usage: see the module doc"),
    }
}
