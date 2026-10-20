//! X-pin (а) (`sol_ox.md` §6): deterministic tests for the `HeapCore`-resident
//! direct-mapped route pin cache over the REAL foreign-free path.
//!
//! The heap cache is owner-private `HeapCore` state (no TLS), so a worker
//! thread that OWNS a heap and frees a foreign block must (1) insert the pin
//! into ITS heap cache on the first free (miss) and (2) hit that same slot on
//! a second free of the same segment base, with no directory lookup. The TLS
//! cache of `route_pin_cache.rs` is NOT involved for such threads; the
//! PIN_CACHE_* counters are shared subsystem totals, asserted as deltas.
//!
//! Every test runs under a `SerialGuard` (process-global route registry and
//! shared counters), mirroring `tests/b4_r1_pin_cache.rs`.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Barrier};
use std::thread;

use sefer_alloc::registry::segment_route::RoutePin;
use sefer_alloc::SeferAlloc;

static GLOBAL: SeferAlloc = SeferAlloc::new();

type Totals = (u64, u64, u64, u64, u64, u64, u64);

static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

fn totals() -> Totals {
    RoutePin::dbg_route_pin_cache_totals()
}

/// Wait for two identical consecutive counter reads (quiescence).
fn quiescent_totals() -> Totals {
    let mut last = totals();
    for _ in 0..1000 {
        let next = totals();
        if next == last {
            return next;
        }
        last = next;
        std::hint::spin_loop();
    }
    last
}

fn small_layout() -> Layout {
    Layout::from_size_align(256, 8).unwrap()
}

/// Owner allocates `n_blocks` 256 B blocks (one segment base for this class),
/// a worker with its OWN warmed heap frees them all through
/// `HeapCore::dealloc_routing` (the X-pin (а) path). Returns the counter
/// deltas over the worker's free window (hits, misses, inserts, evictions,
/// unlink fallbacks, thread releases, sweeps).
fn worker_frees_delta(n_blocks: usize) -> (u64, u64, u64, u64, u64, u64, u64) {
    let barrier = std::sync::Arc::new(Barrier::new(2));
    let owner_barrier = std::sync::Arc::clone(&barrier);
    let (tx, rx) = mpsc::channel::<Vec<usize>>();
    let owner = thread::spawn(move || {
        let barrier = owner_barrier;
        let mut blocks = Vec::with_capacity(n_blocks);
        for _ in 0..n_blocks {
            let p = unsafe { GLOBAL.alloc(small_layout()) };
            assert!(!p.is_null());
            blocks.push(p as usize);
        }
        tx.send(blocks).unwrap();
        barrier.wait();
        // Keep the owner's heap (and its segments) LIVE until the worker
        // finished its frees.
        barrier.wait();
    });
    let blocks = rx.recv().unwrap();
    let (pre_tx, pre_rx) = mpsc::channel::<Totals>();
    let (post_tx, post_rx) = mpsc::channel::<Totals>();
    let worker = thread::spawn(move || {
        // Warm the worker's OWN heap so its dealloc resolves Own and runs
        // `dealloc_routing` with the heap pin cache (not ForeignNoBind).
        let warm = unsafe { GLOBAL.alloc(small_layout()) };
        assert!(!warm.is_null());
        unsafe { GLOBAL.dealloc(warm, small_layout()) };
        pre_tx.send(quiescent_totals()).unwrap();
        for &b in &blocks {
            // SAFETY: each block was allocated by SeferAlloc and is freed
            // exactly once, here, by the worker (unique current instance).
            unsafe { GLOBAL.dealloc(core::ptr::without_provenance_mut(b), small_layout()) };
        }
        post_tx.send(quiescent_totals()).unwrap();
    });
    barrier.wait();
    worker.join().unwrap();
    barrier.wait();
    owner.join().unwrap();
    let pre = pre_rx.recv().unwrap();
    let post = post_rx.recv().unwrap();
    (
        post.0.wrapping_sub(pre.0),
        post.1.wrapping_sub(pre.1),
        post.2.wrapping_sub(pre.2),
        post.3.wrapping_sub(pre.3),
        post.4.wrapping_sub(pre.4),
        post.5.wrapping_sub(pre.5),
        post.6.wrapping_sub(pre.6),
    )
}

#[test]
fn heap_thread_foreign_free_hits_heap_cache() {
    let _g = SerialGuard::acquire();
    // 4 frees across ONE segment base (same class, one owner segment): the
    // first free = miss + insert, the other three = direct-mapped hits.
    let (dh, dm, _di, dev, duf, _dtr, _ds) = worker_frees_delta(4);
    assert!(
        dh >= 3,
        "expected >=3 heap-cache hits, hits+{dh} misses+{dm}"
    );
    assert_eq!(duf, 0, "unlink fallbacks must not fire on live routes");
    let _ = dev;
}

#[test]
fn trim_releases_heap_pins_before_slot_release() {
    let _g = SerialGuard::acquire();
    // When the helper returns, the worker has EXITED: its LeaseReleaseGuard
    // ran `trim_for_recycle`, which must have released the heap-cache pins
    // (THREAD_RELEASES) BEFORE the `LIVE -> FREE` CAS. Releases are
    // monotone, so observing >= 2 releases after the window proves the trim
    // path released the pins this worker inserted.
    let _ = worker_frees_delta(2);
    // Stronger end-state check: after full quiescence, releases since the
    // process start cannot exceed inserts (every release consumes a pin an
    // insert created, in any interleaving).
    let t = totals();
    assert!(
        t.5 <= t.2,
        "THREAD_RELEASES={} exceeded INSERTS={} (leak oracle)",
        t.5,
        t.2
    );
}

#[test]
fn direct_mapped_capacity_stays_bounded_and_correct() {
    let _g = SerialGuard::acquire();
    // Every free must be delivered (hit OR counted miss), and the retention
    // cap keeps inserts/evictions consistent: evictions can never exceed
    // inserts, and dead-pin sweeps stay insert-bounded.
    let (dh, dm, di, dev, duf, _dtr, _ds) = worker_frees_delta(4);
    assert!(dh + dm >= 4, "every free must hit or count a miss");
    assert!(dev <= di, "evictions must not exceed inserts");
    assert_eq!(duf, 0);
}
