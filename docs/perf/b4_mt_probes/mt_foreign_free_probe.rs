// mt_foreign_free_probe.rs — Phase-1 baseline judge for the REAL foreign-free
// path through `GlobalAlloc::dealloc` (b4-mt round, NO src/ changes).
//
// Judge scheme:
//   owner thread (SeferAlloc::new) allocates N blocks (2-3 small classes in
//   locality mode, one class spread over MANY segment bases in capacity mode),
//   hands the (ptr, Layout) pairs to a worker thread. The worker FIRST warms
//   its own heap with its own alloc/free OUTSIDE the measured window (so
//   current_for_dealloc resolves Own and dealloc_routing's owned_block_of /
//   hash_find miss is actually exercised — without the warmup a ForeignNoBind
//   thread would skip owned_block_of entirely). Then, inside the measured
//   window, the worker ONLY deallocates through the #[inline(never)] w_free.
//   Owner drains after join (trim_current_thread).
//   Modes:  foreign — window frees the OWNER's blocks (real foreign path)
//           own     — control: worker pre-allocates its OWN blocks before the
//                     window (same class geometry) and the window frees only
//                     those (own free path, no foreign publication).
//   Trace:  locality (classes 16/64/256, ≤4 segment bases, pin-cache hits)
//           capacity (single 256B class, >4 segment bases, pin-cache misses)
//   Oracle: with `oracle` as 4th arg the probe prints counter snapshots
//           (bench-internals build only; compiled out otherwise).
// Usage: mt_foreign_free_probe <foreign|own> <locality|capacity> <frees> [oracle]
use core::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 {
    unsafe { a.alloc(l) }
}
/// Send wrapper for the one-time handoff of current issued allocations from
/// the owner thread to the worker. SAFETY: the Vec is moved out of the shared
/// Mutex exactly once (`take()`), the owner never touches the pointers again
/// after handing them over, and the worker frees each pair exactly once — a
/// unique transfer of ownership, the same contract `HeapCore::publish_foreign`
/// itself relies on. No aliasing access exists on either side.
struct Handoff(Vec<(*mut u8, Layout)>);
unsafe impl Send for Handoff {}

#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) {
    unsafe { a.dealloc(p, l) }
}

const LOCALITY_CLASSES: [usize; 3] = [16, 64, 256];
const CAPACITY_CLASS: usize = 256; // 4 MiB / 256 B = 16384 blocks per segment

fn layout_for(trace: &str, i: usize) -> Layout {
    let size = if trace == "capacity" {
        CAPACITY_CLASS
    } else {
        LOCALITY_CLASSES[i % LOCALITY_CLASSES.len()]
    };
    Layout::from_size_align(size, 16).unwrap()
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn oracle_snapshot() -> (u64, u64, u64, u64) {
    let b4 = sefer_alloc::SeferAlloc::dbg_b4_collapse_counters();
    // dbg_b4_collapse_counters positional order: [3] = B4_FOREIGN_PUBLISH_OK,
    // [4] = B4_DIRECTORY_LOOKUPS.
    let pin = sefer_alloc::registry::segment_route::RoutePin::dbg_route_pin_cache_totals();
    // pin: (hits, misses, inserts, evictions, unlink_fallbacks, releases, sweeps)
    (b4[3], b4[4], pin.0, pin.1)
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn oracle_snapshot() -> (u64, u64, u64, u64) {
    (0, 0, 0, 0)
}

fn oracle_line(tag: &str, snap: (u64, u64, u64, u64), oracle: bool) {
    if oracle {
        println!(
            "ORACLE_{tag} publish_ok={} dir_lookups={} pin_hits={} pin_misses={}",
            snap.0, snap.1, snap.2, snap.3
        );
    }
}

fn run(mode: &str, trace: &str, k: usize, oracle: bool) {
    let s = sefer_alloc::SeferAlloc::new();
    // Setup (owner, pre-window): allocate k blocks the worker will free.
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(k);
    for i in 0..k {
        let l = layout_for(trace, i);
        slots.push((w_alloc(&s, l), l));
    }
    let setup_done = Arc::new(Barrier::new(2));
    let handoff = Arc::new(Mutex::new(Some(Handoff(Vec::new()))));
    *handoff.lock().unwrap() = Some(Handoff(slots));

    let b2 = setup_done.clone();
    let h2 = handoff.clone();
    let mode2 = mode.to_string();
    let trace2 = trace.to_string();
    let worker = thread::spawn(move || {
        let w = sefer_alloc::SeferAlloc::new();
        // Own-heap warmup, strictly OUTSIDE the measured window: bind this
        // thread to its own heap via own alloc/free (otherwise the thread
        // stays ForeignNoBind and owned_block_of/hash_find is never reached).
        let mut warm: Vec<(*mut u8, Layout)> = Vec::with_capacity(128);
        for i in 0..128 {
            let l = layout_for(&trace2, i);
            warm.push((w_alloc(&w, l), l));
        }
        for (p, l) in warm.drain(..) {
            w_free(&w, p, l);
        }
        b2.wait(); // warmup complete
        let foreign = h2.lock().unwrap().take().unwrap().0;
        let pre = oracle_snapshot();
        oracle_line("PRE", pre, oracle);
        // Own-control setup also pre-window: same class geometry, own heap.
        let own_control = if mode2 == "own" {
            let mut mine: Vec<(*mut u8, Layout)> = Vec::with_capacity(foreign.len());
            for (i, _) in foreign.iter().enumerate() {
                let l = layout_for(&trace2, i);
                mine.push((w_alloc(&w, l), l));
            }
            Some(mine)
        } else {
            None
        };
        // Capacity trace: interleave frees ACROSS segment bases (walk by
        // offset-within-segment first) so consecutive frees land on different
        // bases — with >4 bases this genuinely thrashes the 4-slot pin cache.
        let cap_order: Option<Vec<usize>> = if trace2 == "capacity" {
            const PER_SEG: usize = 16384; // 4 MiB / 256 B
            let n = foreign.len();
            let segs = n.div_ceil(PER_SEG);
            let mut order = Vec::with_capacity(n);
            for off in 0..PER_SEG {
                for seg in 0..segs {
                    let idx = seg * PER_SEG + off;
                    if idx < n {
                        order.push(idx);
                    }
                }
            }
            Some(order)
        } else {
            None
        };
        b2.wait(); // measured window = ONLY the frees below
        match own_control {
            Some(mine) => {
                for (p, l) in mine {
                    w_free(&w, p, l);
                }
            }
            None => match cap_order {
                Some(order) => {
                    for i in order {
                        let (p, l) = foreign[i];
                        w_free(&w, p, l);
                    }
                }
                None => {
                    for (p, l) in foreign {
                        w_free(&w, p, l);
                    }
                }
            },
        }
        let post = oracle_snapshot();
        oracle_line("POST", post, oracle);
    });
    setup_done.wait();
    setup_done.wait(); // release worker into its measured window
    worker.join().unwrap();
    // Owner drain after join.
    s.trim_current_thread();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map(String::as_str).unwrap_or("foreign");
    let trace = a.get(2).map(String::as_str).unwrap_or("locality");
    let k: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let oracle = a.get(4).map(String::as_str) == Some("oracle");
    run(mode, trace, k, oracle);
}
