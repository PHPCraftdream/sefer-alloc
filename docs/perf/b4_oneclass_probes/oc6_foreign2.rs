// oc6_foreign2.rs — foreign-free SELF-Ir probe with CACHED vs UNCACHED arms
// (D1′ round-2, P2#7).
//
// Same judge scheme as mt_foreign_free_probe: owner allocates N 256-B blocks,
// worker warms its own heap (TLS bound) OUTSIDE the measured window, then
// frees the owner's blocks through the #[inline(never)] w_free; the owner
// drains after join. Arms:
//   cached   — frees in allocation order: consecutive frees stay on the SAME
//              segment route → the worker's pin-cache hits (oracle: ~1 miss).
//   uncached — frees in a strided round-robin (stride coprime with N): every
//              consecutive free lands on a DIFFERENT segment route → the
//              pin-cache misses and publication runs publish_foreign_uncached
//              (oracle: misses ≈ frees). The bench-internals oracle run
//              prints the hit/miss deltas for THIS arm; the callgrind run is
//              a separate production build.
// Usage: zz_oc6_foreign2 <cached|uncached> <frees>
#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_possible_truncation)]

use core::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 {
    // SAFETY: GlobalAlloc contract; freed exactly once by the worker.
    unsafe { a.alloc(l) }
}

struct Handoff(Vec<(*mut u8, Layout)>);
unsafe impl Send for Handoff {}

#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) {
    // SAFETY: unique transfer of the current issued allocation via the
    // one-shot Handoff (same contract as mt_foreign_free_probe).
    unsafe { a.dealloc(p, l) }
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn oracle_snapshot() -> (u64, u64, u64) {
    let b4 = sefer_alloc::SeferAlloc::dbg_b4_collapse_counters();
    let pin = sefer_alloc::registry::segment_route::RoutePin::dbg_route_pin_cache_totals();
    (b4[3], pin.0, pin.1) // publish_ok, pin_hits, pin_misses
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn oracle_snapshot() -> (u64, u64, u64) {
    (0, 0, 0)
}

fn main() {
    let arm = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: zz_oc6_foreign2 <cached|uncached> [frees] [oracle]");
        std::process::exit(2);
    });
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let oracle = std::env::args().nth(3).as_deref() == Some("oracle");

    let s = sefer_alloc::SeferAlloc::new();
    let layout = Layout::from_size_align(256, 16).unwrap();
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(n);
    for _ in 0..n {
        let p = w_alloc(&s, layout);
        assert!(!p.is_null(), "OOM");
        slots.push((p, layout));
    }
    // Free order: allocation order (cached) vs interleaved round-robin over
    // the segment streams (uncached). Owner allocation is sequential, so
    // stream s = blocks [s*per, (s+1)*per) sit on their own segment route;
    // emitting one block per stream per round makes every consecutive free
    // land on a DIFFERENT route (guaranteed pin-cache miss for any small
    // cache), while allocation order stays on one route (hits).
    let setup_done = Arc::new(Barrier::new(2));
    let handoff = Arc::new(Mutex::new(Some(Handoff(slots))));
    let b2 = setup_done.clone();
    let h2 = handoff.clone();
    let arm2 = arm.clone();
    let worker = thread::spawn(move || {
        let w = sefer_alloc::SeferAlloc::new();
        let mut warm: Vec<(*mut u8, Layout)> = Vec::with_capacity(128);
        for _ in 0..128 {
            warm.push((w_alloc(&w, layout), layout));
        }
        for (p, l) in warm.drain(..) {
            // SAFETY: warm blocks are the worker's own issued allocations.
            unsafe { w.dealloc(p, l) };
        }
        b2.wait();
        let mut foreign = h2.lock().unwrap().take().unwrap().0;
        let pre = oracle_snapshot();
        if arm2 == "uncached" {
            const STREAMS: usize = 8;
            let per = foreign.len() / STREAMS;
            for i in 0..per {
                for s_i in 0..STREAMS {
                    let (p, l) = foreign[s_i * per + i];
                    w_free(&w, p, l);
                }
            }
            // Tail of the last stream (len % STREAMS), if any.
            for (p, l) in foreign[STREAMS * per..].iter().copied() {
                w_free(&w, p, l);
            }
        } else {
            for (p, l) in foreign.iter().copied() {
                w_free(&w, p, l);
            }
        }
        foreign.clear();
        let post = oracle_snapshot();
        if oracle {
            println!(
                "ORACLE arm={arm2} publish_ok={} pin_hits={} pin_misses={} (deltas +{}/{}/{})",
                post.0 - pre.0,
                post.1 - pre.1,
                post.2 - pre.2,
                post.0 - pre.0,
                post.1 - pre.1,
                post.2 - pre.2
            );
        }
    });
    setup_done.wait();
    worker.join().unwrap();
    s.trim_current_thread();
    println!("OC6_FOREIGN_DONE arm={arm} frees={n}");
}
