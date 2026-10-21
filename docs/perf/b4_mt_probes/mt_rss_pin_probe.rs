// mt_rss_pin_probe.rs — R26-4 subprocess RSS-retention probe (X-pin review
// blocker 2). ONE PROCESS per arm (the wrapper script builds a separate
// snapshot per arm: A = base 2730d02a, B = current tree); this probe prints
// its own measured numbers on stdout.
//
// Scenario: an owner thread allocates one 256 B block per slot across
// N_BASES = 16 distinct 4 MiB segment bases (16 * 16384 = 262144 blocks,
// 64 MiB issued). A worker thread first warms its OWN heap (own alloc/free
// outside the window) so it is bound to its heap, then frees the owner's
// blocks interleaved ACROSS bases (16 distinct bases > MAX_LIVE_PINS, so the
// route pin cache saturates, evicts and retains up to the cap). join, then
// the process sleeps 2 s (idle) and prints:
//   VmRSS from /proc/self/status,
//   System requested bytes via SmallSidecar::system_totals_for_test
//     (internals build; measurement-only — Ir is never taken from this build),
//   pin-cache counters via RoutePin::dbg_route_pin_cache_totals.
// The wrapper computes delta RSS = B - A across the two arms; threshold
// delta <= +1 MiB.
use core::alloc::{GlobalAlloc, Layout};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 {
    unsafe { a.alloc(l) }
}

#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) {
    unsafe { a.dealloc(p, l) }
}

const CLASS: usize = 256;
const PER_SEG: usize = 16384; // 4 MiB / 256 B
const N_BASES: usize = 16;
const N: usize = PER_SEG * N_BASES;

/// Send wrapper: one-time unique transfer of currently-issued allocations
/// from owner to worker (same contract as the foreign-free probe's Handoff).
struct Handoff(Vec<(*mut u8, Layout)>);
unsafe impl Send for Handoff {}

fn rss_kib() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap();
    for line in s.lines() {
        if let Some(v) = line.strip_prefix("VmRSS:") {
            let kib = v.trim().trim_end_matches(" kB").trim().parse::<u64>();
            return kib.unwrap_or(0);
        }
    }
    0
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn pin_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
    sefer_alloc::registry::segment_route::RoutePin::dbg_route_pin_cache_totals()
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn pin_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
    (0, 0, 0, 0, 0, 0, 0)
}

#[cfg(feature = "internals")]
fn sys_bytes() -> usize {
    // (requested_alloc, requested_zeroed, requested_free, alloc_calls, free_calls)
    let (a, z, f, _, _) = sefer_alloc::registry::segment_route::SmallSidecar::system_totals_for_test();
    a + z - f
}
#[cfg(not(feature = "internals"))]
fn sys_bytes() -> usize {
    0
}

fn main() {
    let s = sefer_alloc::SeferAlloc::new();
    let l = Layout::from_size_align(CLASS, 16).unwrap();
    // Owner: allocate N blocks segment-by-segment so each 4 MiB segment base
    // fills before the next (16 distinct bases).
    let mut blocks: Vec<(*mut u8, Layout)> = Vec::with_capacity(N);
    for _off in 0..PER_SEG {
        for _seg in 0..N_BASES {
            blocks.push((w_alloc(&s, l), l));
        }
    }
    let handoff = Arc::new(Mutex::new(Some(Handoff(Vec::new()))));
    *handoff.lock().unwrap() = Some(Handoff(blocks));
    // Owner thread: hands the blocks over, then PARKS FOREVER — it must stay
    // alive holding its heap (its segments are the foreign-free target) and
    // must NOT trim/exit (a thread-exit trim would release the retention
    // under test before the idle measurement; the exit-path trim also does
    // not complete at this scale on either arm — measured, see the raw log).
    let h_owner = handoff.clone();
    let owner = thread::spawn(move || {
        // The blocks themselves are taken by the worker; the owner just stays
        // alive holding its heap and segments.
        loop {
            thread::park();
        }
    });
    let h_worker = handoff.clone();
    let worker = thread::spawn(move || {
        let w = sefer_alloc::SeferAlloc::new();
        // Own-heap warmup (bind this thread to its own heap).
        let mut warm: Vec<(*mut u8, Layout)> = Vec::with_capacity(128);
        for _ in 0..128 {
            warm.push((w_alloc(&w, l), l));
        }
        for (p, ll) in warm.drain(..) {
            w_free(&w, p, ll);
        }
        let foreign = h_worker.lock().unwrap().take().unwrap().0;
        // Interleave across bases: saturate the pin cache (16 bases > cap).
        for off in 0..PER_SEG {
            for seg in 0..N_BASES {
                let (p, ll) = foreign[seg * PER_SEG + off];
                w_free(&w, p, ll);
            }
        }
        // The worker stays ALIVE holding its heap and goes IDLE for 2 s —
        // exactly the R26-4 idle-retention scenario (no trim, no exit).
        thread::sleep(std::time::Duration::from_secs(2));
        println!(
            "PINS hits={} misses={} inserts={} evictions={} unlink_fb={} thread_releases={} sweeps={}",
            pin_totals().0,
            pin_totals().1,
            pin_totals().2,
            pin_totals().3,
            pin_totals().4,
            pin_totals().5,
            pin_totals().6
        );
        println!("RSS_KIB {}", rss_kib());
        println!(
            "SYS_REQUESTED_BYTES {} (alloc+zeroed-free, requested not RSS)",
            sys_bytes()
        );
        // Measurement taken: leave without running thread-exit trims (they
        // are not part of the idle-retention scenario).
        std::process::exit(0);
    });
    owner.join().ok(); // returns only via process::exit above
}
