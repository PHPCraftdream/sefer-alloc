//! Ph4b (task #2092, chunk B) — P2 negative control: WITHOUT a maintenance
//! worker (`start_maintenance()` is never called), late sidecar publications
//! into a recycled (FREE) slot must stay PENDING and unconsumed forever —
//! per the G3+ owner decision, an ownerless FREE heap is drained only by a
//! future claimant's re-claim trim or the maintenance worker, never by magic.
//!
//! Oracles:
//! (a) `maintenance_running() == false` throughout;
//! (b) `segments_released_total` is constant across repeated `stats()` reads,
//!     including across owner-side activity (alloc/free + `trim_current_thread`)
//!     on a DIFFERENT (main) thread — no drain, no retirement;
//! (c) under `internals`, every cross-thread freed address keeps its route
//!     with `pending_for_test() == true` (the publication was made and is
//!     still pending), and `foreign_or_unroutable_frees` did not move
//!     (the frees routed, they were not dropped);
//! (d) `dbg_drain`-style consumption is structurally excluded: pending bits
//!     survive the main thread's explicit trim, proving no worker consumed them.
//!
//! Run in an isolated child process (r8_autonomous pattern) so the real
//! `#[global_allocator]` never serves another test's worker.
#![cfg(all(feature = "production", feature = "internals"))]

use std::alloc::{GlobalAlloc, Layout};
use std::process::Command;
use std::sync::mpsc;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::SeferAlloc;
use sefer_alloc::SegmentLayout;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const CHILD: &str = "SEFER_R11_PH4B_NO_WORKER_CHILD";
const TEST: &str = "no_worker_pending_stays";
const MARKER: &str = "NO_WORKER_PENDING_STAYS_OK";
const BLOCK: usize = 2048;

fn lookup(address: usize) -> Option<sefer_alloc::registry::segment_route::RoutePin> {
    RouteDirectory::global().lookup(core::ptr::without_provenance_mut::<u8>(address))
}

fn scenario() {
    assert!(
        !SeferAlloc::maintenance_running(),
        "the harness must never start the worker for this negative control"
    );
    let layout = Layout::from_size_align(BLOCK, 16).expect("valid layout");

    // Bind the MAIN thread's own slot first so no allocation below can claim
    // the producer's recycled slot behind our back.
    let _warm = unsafe { GLOBAL.alloc(layout) };
    assert!(!_warm.is_null(), "warm-up alloc returned null");

    // Thread A: allocate ordinary Small blocks (same size class as
    // r8_autonomous's releasable segments), hand them to the main thread,
    // and exit — recycling its registry slot while the blocks stay live.
    let count = 2 * SegmentLayout::SEGMENT / BLOCK;
    let (tx, rx) = mpsc::channel::<usize>();
    let producer = std::thread::spawn(move || {
        for _ in 0..count {
            // SAFETY: valid non-zero layout.
            let pointer = unsafe { GLOBAL.alloc(layout) };
            assert!(!pointer.is_null(), "producer alloc returned null");
            if tx.send(pointer as usize).is_err() {
                break;
            }
        }
    });
    let addresses: Vec<usize> = rx.iter().collect();
    producer.join().expect("producer exited normally");
    assert_eq!(addresses.len(), count, "producer handed over every block");

    let stats_before = GLOBAL.stats();

    // Thread B: a dealloc-only consumer frees HALF the ordinary Small blocks
    // cross-thread. Blocks carved from the permanent primordial carrier are
    // skipped (they belong to the always-live fallback, not a releasable slot).
    // Its first allocator call is a dealloc, so it binds no slot (R6-OPT-P0-1)
    // and cannot re-claim the producer's slot; the frees go to the sidecar
    // ingress of the now-FREE slot as pending terminal publications.
    let freed: Vec<usize> = addresses
        .iter()
        .copied()
        .filter(|&address| lookup(address).is_some_and(|pin| pin.kind() == RouteKind::Small))
        .take(count / 2)
        .collect();
    assert!(!freed.is_empty(), "no ordinary Small blocks were allocated");
    let freed_clone = freed.clone();
    std::thread::spawn(move || {
        for address in freed_clone {
            // SAFETY: each address was allocated by the producer with this
            // exact layout and handed over by value; freed exactly once here.
            unsafe { GLOBAL.dealloc(core::ptr::without_provenance_mut::<u8>(address), layout) };
        }
    })
    .join()
    .expect("dealloc-only consumer exited normally");

    assert!(!SeferAlloc::maintenance_running());

    // (c) every cross-thread free routed (none dropped) and published.
    let stats_after_frees = GLOBAL.stats();
    assert_eq!(
        stats_after_frees.foreign_or_unroutable_frees, stats_before.foreign_or_unroutable_frees,
        "cross-thread frees must route to the sidecar, not be dropped"
    );
    for &address in &freed {
        let pin = lookup(address).expect("freed block keeps its route");
        assert_eq!(pin.kind(), RouteKind::Small);
        assert!(
            pin.pending_for_test(core::ptr::without_provenance_mut::<u8>(address)),
            "cross-thread free of {address:#x} must be a pending publication"
        );
    }

    // (b) repeated reads: the release counter is frozen without a worker.
    assert_eq!(
        GLOBAL.stats().segments_released_total,
        stats_before.segments_released_total,
        "no worker: pending publications must not retire segments"
    );

    // Owner-side activity on the MAIN thread (its own slot): alloc/free churn
    // plus an explicit trim must not touch the producer's FREE-slot segments.
    for _ in 0..64 {
        // SAFETY: valid non-zero layout; freed with the same layout.
        let pointer = unsafe { GLOBAL.alloc(layout) };
        assert!(!pointer.is_null());
        unsafe { GLOBAL.dealloc(pointer, layout) };
    }
    GLOBAL.trim_current_thread();

    let stats_final = GLOBAL.stats();
    assert!(!SeferAlloc::maintenance_running());
    assert_eq!(
        stats_final.segments_released_total, stats_before.segments_released_total,
        "without a worker nothing may consume the pending publications, \
         so segments_released_total must stay frozen"
    );
    assert_eq!(
        stats_final.foreign_or_unroutable_frees, stats_before.foreign_or_unroutable_frees,
        "main-thread churn must not turn anything unroutable"
    );
    // (c)+(d) the pending publications are still there — nobody drained them.
    for &address in &freed {
        let pin = lookup(address).unwrap_or_else(|| {
            panic!("pending publication for {address:#x} was consumed without a worker")
        });
        assert!(
            pin.pending_for_test(core::ptr::without_provenance_mut::<u8>(address)),
            "publication for {address:#x} must remain pending without a worker"
        );
    }
    println!("{MARKER}");
}

#[test]
fn no_worker_pending_stays() {
    if std::env::var_os(CHILD).is_some() {
        scenario();
        return;
    }
    let exe = std::env::current_exe().expect("test binary");
    let out = Command::new(exe)
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .expect("scenario subprocess");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "scenario failed: {:?}\n{stdout}\n{stderr}",
        out.status
    );
    assert!(
        stdout.contains(MARKER),
        "scenario missed its behavior oracle: {stdout}\n{stderr}"
    );
}
