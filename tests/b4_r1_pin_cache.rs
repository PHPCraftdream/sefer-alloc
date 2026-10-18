//! Chunk 2 of b4-r1: deterministic tests for the per-thread route pin cache.
//! The cache is process-global TLS state and the route registry is a
//! process-global structure, so every test runs under a `SerialGuard` and
//! asserts only counter deltas. Each counterfactual assertion names the
//! mutant that would turn it red.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin};
use sefer_alloc::SeferAlloc;

const SEGMENT: usize = 4 * 1024 * 1024;

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

type Totals = (u64, u64, u64, u64, u64, u64);

fn totals() -> Totals {
    RoutePin::dbg_route_pin_cache_totals()
}

/// The six counters are six separate Relaxed loads, not a snapshot: a read
/// taken while allocator-service threads churn can interleave and skew the
/// inserts-minus-releases balance. Wait for two identical consecutive reads
/// (quiescence) before any balance assertion.
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

#[test]
fn cached_pin_publishes_and_counts() {
    let _guard = SerialGuard::acquire();
    let before = totals();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let registration = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    // Make the granule at offset 0 LIVE so the sidecar accepts publication.
    assert!(registration.issue_small(0, 0));

    // Publish before any cached pin exists: a pure miss. Process-global
    // counters also see sporadic allocator-service traffic, so only lower
    // bounds are asserted for pure counts.
    assert!(
        !RoutePin::dbg_publish_cached_for_test(root.addr()),
        "no cached pin yet, publish must miss"
    );
    let after_miss = totals();
    assert!(after_miss.1 - before.1 >= 1, "at least the probe miss");

    // SAFETY: `lookup` reads only the address; no dereference happens here.
    let pin = directory.lookup(root).unwrap();
    pin.dbg_cache_insert_for_test();
    assert!(totals().2 - before.2 >= 1, "at least the seam insert");

    // MUTANT cache-disabled: HITS stay 0 and this assert fails.
    assert!(
        RoutePin::dbg_publish_cached_for_test(root.addr()),
        "cached pin must publish and count a hit"
    );
    let after = totals();
    assert!(after.0 - before.0 >= 1, "at least one hit");
    assert_eq!(after.4 - before.4, 0, "linked route: no unlink fallback");

    // SAFETY: `lookup` is address-only; the fresh pin observes the sidecar.
    let fresh = directory.lookup(root).unwrap();
    assert!(
        fresh.pending_for_test(root),
        "publication landed in the live route's sidecar"
    );
    drop(fresh);
    drop(registration);
}

#[test]
fn unlinked_cached_pin_is_released_immediately_and_never_publishes() {
    let _guard = SerialGuard::acquire();
    let before = totals();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let registration = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();

    // SAFETY: `lookup` reads only the address.
    let pin_probe = directory.lookup(root).unwrap();
    // SAFETY: address-only, as above.
    let cached = directory.lookup(root).unwrap();
    cached.dbg_cache_insert_for_test();

    // Production unlink: `RouteDirectory::remove` clears `linked` under the
    // shard lock and drops the owner count; the cached pin keeps refs >= 1
    // so the sidecar stays alive for the pending RMW.
    drop(registration);

    let hits_before = RoutePin::dbg_route_pin_cache_totals().0;
    // MUTANT skip-linked-check: this assert fails — the zombie sidecar would
    // gain the bit (and HITS would increment).
    // MUTANT publish-on-false / no-gate: same as above.
    assert!(
        !RoutePin::dbg_publish_cached_for_test(root.addr()),
        "unlinked cached pin must fall back, never publish"
    );
    let after = totals();
    assert_eq!(after.4 - before.4, 1, "one unlink fallback");
    assert!(after.1 - before.1 >= 1, "the fallback counts a miss");
    // Tiny window: a hit here can only come from the dead cached pin.
    assert_eq!(
        RoutePin::dbg_route_pin_cache_totals().0 - hits_before,
        0,
        "no hit for a dead pin"
    );

    // MUTANT keep-dead-pin: this refs assert fails (2 instead of 1).
    assert_eq!(
        RouteDirectory::dbg_route_pin_refs(&pin_probe),
        1,
        "cached dead pin was released immediately; only the probe remains"
    );
    assert!(
        !pin_probe.pending_for_test(root),
        "nothing may be published into the unlinked route's sidecar"
    );
    drop(pin_probe);
}

#[test]
fn re_registered_route_is_served_after_cache_fallback() {
    let _guard = SerialGuard::acquire();
    let before = totals();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let first = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    let first_incarnation = first.incarnation();

    // SAFETY: `lookup` reads only the address.
    let pin_probe = directory.lookup(root).unwrap();
    // SAFETY: address-only, as above.
    let cached = directory.lookup(root).unwrap();
    cached.dbg_cache_insert_for_test();
    drop(first);

    // The fallback empties the dead slot and falls through to a fresh lookup.
    assert!(
        !RoutePin::dbg_publish_cached_for_test(root.addr()),
        "zombie pin must not publish"
    );

    // The old route is unlinked; re-registration gets a fresh incarnation.
    let second = directory
        .register(root, SEGMENT, root, 18, RouteKind::Small)
        .unwrap();
    assert!(second.incarnation() > first_incarnation);
    // Make the granule at offset 0 LIVE on the new route so the sidecar
    // accepts the cached publication.
    assert!(second.issue_small(0, 0));

    // Production fallback's insert step: the dead slot was already emptied,
    // so no eviction is charged for replacing it.
    // SAFETY: `lookup` reads only the address.
    let fresh = directory.lookup(root).unwrap();
    fresh.dbg_cache_insert_for_test();
    assert!(totals().2 - before.2 >= 2, "two seam inserts");

    // MUTANT skip-linked-check / publish-on-false: the zombie slot (or the
    // new slot pointing at the old sidecar) would serve this and the old
    // probe would observe the pending bit.
    assert!(
        RoutePin::dbg_publish_cached_for_test(root.addr()),
        "new route's cached pin must publish"
    );
    assert!(
        !pin_probe.pending_for_test(root),
        "publication went to the NEW sidecar, not the zombie"
    );
    // SAFETY: address-only lookup on the re-registered route.
    let check = directory.lookup(root).unwrap();
    assert!(
        check.pending_for_test(root),
        "publication landed in the new route's sidecar"
    );
    drop(check);
    drop(pin_probe);
    drop(second);
}

#[test]
fn thread_exit_releases_cached_pins() {
    let _guard = SerialGuard::acquire();
    let (h0, m0, i0, e0, _u0, r0) = quiescent_totals();
    thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let directory = RouteDirectory::new();
                let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
                let root = reservation.as_ptr();
                let registration = directory
                    .register(root, SEGMENT, root, 9, RouteKind::Small)
                    .unwrap();
                // SAFETY: `lookup` reads only the address.
                let pin = directory.lookup(root).unwrap();
                pin.dbg_cache_insert_for_test();
                drop(registration);
                // Thread exit runs the TLS Drop: PIN_CACHE_THREAD_RELEASES.
            });
        }
    });
    let (h1, m1, i1, e1, _u1, r1) = quiescent_totals();
    assert!(i1 - i0 >= 8, "at least one insert per thread");
    assert!(r1 - r0 >= 8, "one thread release per thread's cached pin");
    // Running balance: lifetime `inserts - evictions - thread_releases`
    // equals (pins currently cached) + (uncounted unlink-fallback/span-miss
    // removals), so it can never go negative — negative would mean a pin
    // left a slot without any counted release (a leaked/re-bucketed pin).
    // The window delta itself may shrink because pins inserted before this
    // test (by already-joined tests' threads) can release during it, so only
    // the bounded form is asserted here.
    assert!(i1 >= e1 + r1, "no pin leaves a slot uncounted");
    // (i1 - e1 - r1) >= (i0 - e0 - r0) - 8 rearranged subtraction-free:
    // i1 + e0 + r0 + 8 >= i0 + e1 + r1 (no u64 underflow possible).
    assert!(
        i1 + e0 + r0 + 8 >= i0 + e1 + r1,
        "in-window shrink bounded by concurrent pre-window releases"
    );
    let _ = (h0, h1, m0, m1);
}

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

#[test]
fn many_threads_many_segments_foreign_frees_balance() {
    let _guard = SerialGuard::acquire();
    let before = quiescent_totals();
    let foreign_before = AllocCore::dbg_foreign_or_unroutable_frees();

    let small_layout = Layout::from_size_align(256, 8).unwrap();
    let mid_layout = Layout::from_size_align(4096, 8).unwrap();
    // 4096 x 256 B (1 MiB, one small segment) plus 4096 x 4 KiB (16 MiB,
    // spanning several segments; 4 KiB may classify Large — that is fine,
    // Large routes simply exercise the uncached path).
    let mut small_ptrs = Vec::with_capacity(4096);
    let mut mid_ptrs = Vec::with_capacity(4096);
    for _ in 0..4096 {
        // SAFETY: valid nonzero Layout; null is checked immediately.
        let p = unsafe { GLOBAL.alloc(small_layout) };
        assert!(!p.is_null(), "small allocation failed");
        // Address-only transfer: provenance is exposed here and re-exposed
        // in the worker (the same pattern as `stats_foreign_or_unroutable_frees`).
        small_ptrs.push(p.expose_provenance());
        // SAFETY: valid nonzero Layout; null is checked immediately.
        let p = unsafe { GLOBAL.alloc(mid_layout) };
        assert!(!p.is_null(), "mid allocation failed");
        mid_ptrs.push(p.expose_provenance());
    }

    let workers = 4;
    let small_chunks: Vec<_> = small_ptrs
        .chunks(4096 / workers)
        .map(<[usize]>::to_vec)
        .collect();
    let mid_chunks: Vec<_> = mid_ptrs
        .chunks(4096 / workers)
        .map(<[usize]>::to_vec)
        .collect();
    let mut handles = Vec::new();
    for (small, mid) in small_chunks.into_iter().zip(mid_chunks) {
        handles.push(thread::spawn(move || {
            for addr in small {
                // SAFETY: the unique live allocation and its exact original
                // Layout were transferred to this worker; provenance was
                // exposed at allocation and is re-exposed here; freed once.
                unsafe { GLOBAL.dealloc(std::ptr::with_exposed_provenance_mut(addr), small_layout) };
            }
            for addr in mid {
                // SAFETY: as above, exact original Layout, freed once.
                unsafe { GLOBAL.dealloc(std::ptr::with_exposed_provenance_mut(addr), mid_layout) };
            }
            // Worker exit: TLS dtor releases its cached pins.
        }));
    }
    for handle in handles {
        handle.join().expect("worker panicked");
    }

    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees() - foreign_before,
        0,
        "no free lost, no free dropped"
    );
    let after = quiescent_totals();
    assert!(after.0 > before.0, "same-base repeats must hit the cache");
    assert!(after.1 > before.1, "first touches must miss");
    // Balance against the pre-test baseline: the balance only ever grows
    // (uncounted unlink-fallback releases), so a negative delta means a pin
    // left the cache uncounted — a leaked/re-bucketed pin.
    let b0 = (before.2 - before.3 - before.5, after.2 - after.3 - after.5);
    // Lifetime balance can never go negative (see `thread_exit_releases_cached_pins`);
    // the window delta may shrink from pre-window pins releasing in-window.
    assert!(after.2 >= after.3 + after.5, "no pin leaves a slot uncounted");
    assert!(
        ((after.2 - after.3 - after.5) as i64)
            >= ((before.2 - before.3 - before.5) as i64) - 64,
        "in-window shrink bounded by concurrent pre-window releases: {:?}",
        b0
    );
    // The owner keeps nothing: both vectors were moved into the workers via
    // `zip` and are dropped inside each worker after the frees, so the
    // pointers were never double-freed.
}
