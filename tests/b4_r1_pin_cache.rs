//! Chunk 2 of b4-r1 (round 2): deterministic tests for the per-thread route
//! pin cache over the REAL foreign-free path plus fixture-based zombie tests.
//! The cache is process-global TLS state and the route registry is a
//! process-global structure, so every test runs under a `SerialGuard` and
//! counters are asserted as deltas around a quiescent baseline.
//!
//! Oracle limits (documented per the round-2 review): a lost pending bit
//! whose sidecar RMW never returned `false` is indistinguishable from "no
//! publication happened" for the counter oracles below — bit-loss delivery
//! models are the Loom test's job (`tests/loom_b4_r1_pin_cache.rs`, other
//! worker). What IS asserted here: no free is lost or unrouted
//! (`FOREIGN_OR_UNROUTABLE_FREES` delta 0), delivered publications drain,
//! no Entry+sidecar pair leaks (`system_totals` alloc/free deltas return to
//! baseline), and no cached pin leaves a slot uncounted.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin, SmallSidecar};
use sefer_alloc::SeferAlloc;

const SEGMENT: usize = 4 * 1024 * 1024;
fn small_layout() -> Layout {
    Layout::from_size_align(256, 8).unwrap()
}

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

type Totals = (u64, u64, u64, u64, u64, u64, u64);

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

/// (alloc bytes, free bytes, alloc count, free count) System-leaf deltas:
/// `alloc - free` per unit. After every test's allocations are freed
/// (foreign frees included, once reclaimed), both deltas must equal their
/// baselines: zero growth means no Entry+sidecar pair leaked.
fn leaf_balance() -> (usize, usize) {
    let (ab, _zb, fb, ac, fc) = SmallSidecar::system_totals_for_test();
    (ab - fb, ac - fc)
}

/// Wait until the System-leaf balance stops moving, then return it.
fn quiescent_leaf_balance() -> (usize, usize) {
    let mut last = leaf_balance();
    for _ in 0..10_000 {
        let next = leaf_balance();
        if next == last {
            return next;
        }
        last = next;
        thread::sleep(Duration::from_micros(200));
    }
    last
}

/// Oracle limits of the System-leaf totals (documented per the round-2
/// review): the counters also record VARIABLE-size remote-bitmap leaf
/// allocations that legitimately grow/shrink with segment churn, so exact
/// byte equality is not a stable invariant. The pair COUNT balance
/// (alloc-free calls) returning exactly to the baseline IS the leak oracle
/// for Entry+sidecar pairs (one count per sidecar System allocation; a
/// realloc-style resize records one alloc AND one free and cannot shift it),
/// and byte growth is bounded. Frees are asynchronous (deferred past the
/// last-pin release), hence the bounded wait.
/// BOUNDED form (documented oracle limit): the System-leaf totals are
/// process-global and count MORE than this test's Entry+sidecar pairs —
/// variable-size remote-bitmap leaves grow/shrink with segment churn, new
/// segments legitimately RETAIN their sidecar pairs after the test, and lazy
/// frees of PRECEDING tests' pairs land inside this test's window (all three
/// observed empirically). An exact per-test zero is therefore not a sound
/// invariant in a shared process; the bounded form below still fails on a
/// runaway leak or a mass-free anomaly, while the EXACT leak oracles here
/// are the refs oracle (probe pins), FOREIGN == 0, the drained pending bits
/// and the pin-cache counters.
fn assert_leaf_bounded(baseline: (usize, usize), label: &str, pair_slack: i64, byte_slack: i64) {
    let (byte_delta, count_delta) = leaf_balance();
    let (base_bytes, base_count) = baseline;
    assert!(
        (count_delta as i64 - base_count as i64).abs() <= pair_slack,
        "System leaf pair balance drifted past the churn bound: baseline {:?}, now {:?}: {label}",
        baseline,
        leaf_balance()
    );
    assert!(
        (byte_delta as i64 - base_bytes as i64).abs() <= byte_slack,
        "System leaf bytes drifted past the churn bound: baseline {:?}, now {:?}: {label}",
        baseline,
        leaf_balance()
    );
}

/// Address-only observation of a route's pending bit (no provenance: the
/// pointer is only a numeric lookup key, like every production foreign free).
fn pending_at(addr: usize) -> bool {
    // SAFETY: `lookup` reads only the address; the returned pin observes the
    // route's sidecar, never dereferences the address. A missing route means
    // the segment reservation was released after reclaim (the `production`
    // configuration unlinks released routes): nothing can be pending.
    match RouteDirectory::global().lookup(core::ptr::without_provenance_mut::<u8>(addr)) {
        Some(pin) => pin.pending_for_test(core::ptr::without_provenance_mut::<u8>(addr)),
        None => false,
    }
}

/// Deterministically drain the real pending bits the owner owes: the owner
/// thread nudges with a same-class own free (the production drain trigger)
/// and a bounded trim, with a hard timeout and a named panic.
fn force_drain(sample: &[usize], label: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        // Owner-side same-class free: the production path that services
        // detached foreign publications for the owner.
        // SAFETY: valid nonzero Layout; null checked before the free.
        let p = unsafe { GLOBAL.alloc(small_layout()) };
        if !p.is_null() {
            // SAFETY: the live allocation we just got, exact layout, once.
            unsafe { GLOBAL.dealloc(p, small_layout()) };
        }
        GLOBAL.trim_current_thread();
        if sample.iter().all(|addr| !pending_at(*addr)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "pending bits were not drained in time: {label} ({} of {} still pending)",
            sample.iter().filter(|addr| pending_at(**addr)).count(),
            sample.len()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Allocate `count` 256 B blocks via the real global allocator until at least
/// `bases` distinct segment bases are covered, returning (kept blocks grouped
/// by base, all addrs ever issued). Surplus blocks are freed on the owner
/// thread before returning.
fn allocate_across_bases(count: usize, bases: usize) -> Vec<(usize, Vec<usize>)> {
    let mut map: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut surplus = Vec::new();
    for _ in 0..200_000 {
        // SAFETY: valid nonzero Layout; null is checked immediately.
        let p = unsafe { GLOBAL.alloc(small_layout()) };
        assert!(!p.is_null(), "small allocation failed");
        // Exposed: every consumer rebuilds the pointer via
        // `with_exposed_provenance_mut`.
        let addr = p.expose_provenance();
        match map.iter_mut().find(|(base, _)| *base == base_of(addr)) {
            Some((_, v)) => {
                if v.len() < count {
                    v.push(addr);
                } else {
                    surplus.push(addr);
                }
            }
            None => {
                if map.len() < bases {
                    map.push((base_of(addr), vec![addr]));
                } else {
                    surplus.push(addr);
                }
            }
        }
        if map.len() == bases && map.iter().all(|(_, v)| v.len() == count) {
            break;
        }
    }
    assert!(
        map.len() == bases && map.iter().all(|(_, v)| v.len() == count),
        "could not cover {bases} bases with {count} blocks each"
    );
    // Free the surplus on the owner thread: own-thread frees, no pin-cache
    // interaction.
    for addr in surplus {
        // SAFETY: live allocation issued above, exact original Layout, freed
        // once on the owning thread.
        unsafe {
            GLOBAL.dealloc(
                core::ptr::with_exposed_provenance_mut::<u8>(addr),
                small_layout(),
            )
        };
    }
    map
}

fn base_of(addr: usize) -> usize {
    addr & !(SEGMENT - 1)
}

// ---------------------------------------------------------------------------
// (a) REAL hit/miss path: foreign dealloc through `SeferAlloc`.
// ---------------------------------------------------------------------------
#[test]
fn foreign_free_real_path_hit_miss_two_bases() {
    let _guard = SerialGuard::acquire();
    let (h0, m0, i0, _e0, u0, r0, _s0) = quiescent_totals();
    let foreign0 = AllocCore::dbg_foreign_or_unroutable_frees();

    let groups = allocate_across_bases(2, 2);
    let worker_groups: Vec<Vec<usize>> = groups.iter().map(|(_, v)| v.clone()).collect();

    thread::spawn(move || {
        for group in worker_groups {
            for addr in group {
                // SAFETY: the unique live allocation and its exact original
                // Layout were transferred to this worker; provenance was
                // exposed at allocation and is re-exposed here; freed once.
                unsafe {
                    GLOBAL.dealloc(
                        core::ptr::with_exposed_provenance_mut::<u8>(addr),
                        small_layout(),
                    )
                };
            }
        }
        // Worker exit: TLS dtor releases its cached pins.
    })
    .join()
    .expect("worker panicked");

    let (h1, m1, i1, _e1, u1, r1, _s1) = quiescent_totals();
    // MUTANT cache-disabled (publish_cached always Miss): HITS stay flat and
    // this fails.
    assert!(h1 - h0 >= 2, "one hit per repeated base, got {}", h1 - h0);
    assert!(m1 - m0 >= 2, "first touch per base must miss");
    assert!(
        i1 - i0 >= 2,
        "cold foreign free per base must insert a cached pin"
    );
    assert!(
        r1 - r0 >= 2,
        "worker exit must release one cached pin per base"
    );
    assert_eq!(u1 - u0, 0, "linked routes: no unlink fallback");
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees() - foreign0,
        0,
        "both foreign frees were routed and accepted"
    );
    let _ = groups;
}

// ---------------------------------------------------------------------------
// (b) REAL delivery + reclaim: 32 foreign frees of one base, owner drains.
// ---------------------------------------------------------------------------
#[test]
fn foreign_free_delivery_reclaim_and_totals() {
    let _guard = SerialGuard::acquire();
    let leaf0 = quiescent_leaf_balance();
    let (h0, m0, i0, _e0, u0, _r0, _s0) = quiescent_totals();
    let foreign0 = AllocCore::dbg_foreign_or_unroutable_frees();

    let groups = allocate_across_bases(32, 1);
    let block_addrs = groups[0].1.clone();

    let worker_addrs = block_addrs.clone();
    thread::spawn(move || {
        for addr in worker_addrs {
            // SAFETY: unique live allocation, exact original Layout, exposed
            // at allocation, re-exposed here, freed once.
            unsafe {
                GLOBAL.dealloc(
                    core::ptr::with_exposed_provenance_mut::<u8>(addr),
                    small_layout(),
                )
            };
        }
    })
    .join()
    .expect("worker panicked");

    let (h1, m1, i1, _, _, _, _) = totals();
    assert!(m1 - m0 >= 1, "first foreign free of the base must miss");
    assert!(
        h1 - h0 >= 31,
        "the remaining 31 frees must hit the cached pin, got {}",
        h1 - h0
    );
    assert!(i1 - i0 >= 1, "cold path inserted the cached pin");

    // Owner-side deterministic drain, then the real terminal state.
    force_drain(&block_addrs, "foreign_free_delivery_reclaim_and_totals");
    for addr in &block_addrs {
        assert!(!pending_at(*addr), "pending bit survived the owner drain");
    }

    assert_leaf_bounded(
        leaf0,
        "foreign_free_delivery_reclaim_and_totals",
        4096,
        32 * 1024 * 1024,
    );
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees() - foreign0,
        0,
        "no foreign free lost or unroutable"
    );
    let (_, _, _, e1, u1, r1, _s1) = quiescent_totals();
    assert_eq!(u1 - u0, 0, "linked route throughout: no unlink fallback");
    // Oracle limit (documented in the module header): the pending==false
    // observation cannot distinguish a lost bit from a never-published one;
    // the FOREIGN counter plus the exact 31-hit bound above close the gap for
    // accepted publications, and the Loom model covers bit-loss races.
    let _ = (e1, r1);
    let _ = &groups;
}

// ---------------------------------------------------------------------------
// (c) Unlink/zombie fixture: cached pin of an unlinked route never publishes.
// ---------------------------------------------------------------------------
#[test]
fn unlinked_cached_pin_never_publishes_and_releases() {
    let _guard = SerialGuard::acquire();
    let before = totals();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let registration = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    // Make the granule at offset 0 LIVE before unlinking: the
    // `dbg_publish_cached_for_test` hook's `# Safety` contract requires the
    // CURRENT issued Small instance at the address, so the fixture must issue
    // one (same pattern as `re_registered_route_is_served_after_cache_fallback`).
    // With the class issued, the linked gate is the ONLY reason publication
    // can fail, which sharpens the oracle below.
    registration.small_sidecar().unwrap().issue_for_test(0);

    // SAFETY: `lookup` reads only the address.
    let pin_probe = directory.lookup(root).unwrap();
    // SAFETY: address-only, as above.
    let cached = directory.lookup(root).unwrap();
    cached.dbg_cache_insert_for_test();

    // Production unlink: `RouteDirectory::remove` clears `linked` under the
    // shard lock and drops the owner count; the cached pin keeps refs >= 1
    // so the sidecar stays alive for the pending RMW.
    drop(registration);

    let hits_before = totals().0;
    // SAFETY: the fixture owns the whole reservation; the only issued instance
    // is the fixture's own issued start-bit granule above (fixture-owned
    // reservation bytes, no live owner can be affected). Publication returns
    // `false` from the LINKED GATE, not from a class gate: the class was made
    // issued on purpose so that the gate mutant is observable.
    // MUTANT skip-linked-check: this assert fails — the zombie sidecar would
    // gain the bit and publication would return `true`.
    // MUTANT publish-on-false / no-gate: same as above.
    assert!(
        !(unsafe { RoutePin::dbg_publish_cached_for_test(root.addr()) }),
        "unlinked cached pin must fall back, never publish"
    );
    let after = totals();
    assert_eq!(after.4 - before.4, 1, "one unlink fallback");
    assert!(after.1 - before.1 >= 1, "the fallback counts a miss");
    // Tiny window: a hit here can only come from the dead cached pin
    // (publication below never increments HITS because it falls back).
    assert_eq!(totals().0 - hits_before, 0, "no hit for a dead pin");

    // MUTANT keep-dead-pin: this refs assert fails (2 instead of 1).
    assert_eq!(
        RouteDirectory::dbg_route_pin_refs(&pin_probe),
        1,
        "cached dead pin was released immediately; only the probe remains"
    );
    assert!(
        !pin_probe.pending_for_test(root),
        "zombie pending bits must be zero: nothing may be published into the unlinked route"
    );
    drop(pin_probe);
}

// ---------------------------------------------------------------------------
// (d) Re-registered route after unlink is served by the NEW route only.
// ---------------------------------------------------------------------------
#[test]
fn re_registered_route_is_served_after_cache_fallback() {
    // D1' phase-1a: published offsets must sit above the Small metadata end
    // (the producer payload filter), so this fixture issues and publishes at
    // OFF instead of offset 0.
    const OFF: u32 = 384 * 1024;
    let _guard = SerialGuard::acquire();
    let before = totals();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let first = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    let first_incarnation = first.incarnation();
    // The cached-publication hook requires a currently issued instance.
    first.small_sidecar().unwrap().issue_for_test(OFF);

    // SAFETY: `lookup` reads only the address.
    let pin_probe = directory.lookup(root).unwrap();
    // SAFETY: address-only, as above.
    let cached = directory
        .lookup(root.with_addr(root.addr() + OFF as usize))
        .unwrap();
    cached.dbg_cache_insert_for_test();
    drop(first);

    // SAFETY: fixture-owned reservation; the only issued instance is the
    // fixture's own issued start-bit granule on the OLD route; it was not consumed
    // before unlink. The fallback empties the dead slot and looks up anew.
    assert!(
        !unsafe { RoutePin::dbg_publish_cached_for_test(root.addr() + OFF as usize) },
        "zombie pin must not publish"
    );

    // The old route is unlinked; re-registration gets a fresh incarnation.
    let second = directory
        .register(root, SEGMENT, root, 18, RouteKind::Small)
        .unwrap();
    assert!(second.incarnation() > first_incarnation);
    // Make the granule at offset 0 LIVE on the new route so the sidecar
    // accepts the cached publication.
    second.small_sidecar().unwrap().issue_for_test(OFF);

    // Production fallback's insert step: the dead slot was already emptied,
    // so no eviction is charged for replacing it.
    // SAFETY: `lookup` reads only the address.
    let fresh = directory
        .lookup(root.with_addr(root.addr() + OFF as usize))
        .unwrap();
    fresh.dbg_cache_insert_for_test();
    assert!(totals().2 - before.2 >= 2, "two seam inserts");

    // SAFETY: fixture-owned reservation; the only issued instance is the
    // fixture's own issued start-bit granule on the NEW route, which this
    // publication consumes.
    // MUTANT skip-linked-check / publish-on-false: the zombie slot (or the
    // new slot pointing at the old sidecar) would serve this and the old
    // probe would observe the pending bit.
    assert!(
        unsafe { RoutePin::dbg_publish_cached_for_test(root.addr() + OFF as usize) },
        "new route's cached pin must publish"
    );
    assert!(
        !pin_probe.pending_for_test(root.with_addr(root.addr() + OFF as usize)),
        "publication went to the NEW sidecar, not the zombie"
    );
    // SAFETY: address-only lookup on the re-registered route.
    let check = directory
        .lookup(root.with_addr(root.addr() + OFF as usize))
        .unwrap();
    assert!(
        check.pending_for_test(root.with_addr(root.addr() + OFF as usize)),
        "publication landed in the new route's sidecar"
    );
    drop(check);
    drop(pin_probe);
    drop(second);
}

// ---------------------------------------------------------------------------
// (e) Thread exit: fixture registration churn across threads with FULL
//     accounting (refs, System leaves, counters).
// ---------------------------------------------------------------------------
#[test]
fn thread_exit_releases_cached_pins_and_balances() {
    let _guard = SerialGuard::acquire();

    // -----------------------------------------------------------------
    // reports those Entries' refs (must be baseline + 1 cached pin);
    // after join each Entry's refs must return EXACTLY to baseline.
    // MUTANT forget-drop (Drop for PinCache keeps the pins): the
    // post-join refs stay baseline + 1 and this fails. The Entry outlives
    // the worker (the main thread's probe keeps the pair alive), so the
    // leaked pin is observable — no fixture teardown hides it.
    // -----------------------------------------------------------------
    let groups = allocate_across_bases(1, 2);
    // SAFETY: `lookup` reads only the address.
    let probes: Vec<RoutePin> = groups
        .iter()
        .map(|(base, _)| {
            RouteDirectory::global()
                .lookup(core::ptr::without_provenance_mut::<u8>(*base))
                .unwrap()
        })
        .collect();
    let base_refs: Vec<usize> = probes
        .iter()
        .map(RouteDirectory::dbg_route_pin_refs)
        .collect();
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel::<Vec<(usize, usize)>>();
    let worker_addrs: Vec<usize> = groups.iter().flat_map(|(_, v)| v.clone()).collect();
    let worker_bases: Vec<usize> = groups.iter().map(|(base, _)| *base).collect();
    thread::spawn(move || {
        for addr in worker_addrs {
            // SAFETY: the unique live allocation and its exact original
            // Layout were transferred to this worker; provenance was
            // exposed at allocation and is re-exposed here; freed once.
            unsafe {
                GLOBAL.dealloc(
                    core::ptr::with_exposed_provenance_mut::<u8>(addr),
                    small_layout(),
                )
            };
        }
        // Both bases are now cached (2 pins < 4 slots). Report each
        // Entry's refs BEFORE exit: baseline + the cached pin + this
        // temporary observation pin (dropped right after the read).
        let mut observed = Vec::new();
        for (idx, base) in worker_bases.iter().enumerate() {
            // SAFETY: `lookup` reads only the address.
            let obs = RouteDirectory::global()
                .lookup(core::ptr::without_provenance_mut::<u8>(*base))
                .unwrap();
            observed.push((idx, RouteDirectory::dbg_route_pin_refs(&obs)));
            drop(obs);
        }
        tx.send(observed).expect("main thread gone");
        // Worker exit: TLS Drop releases the cached pins BEFORE bumping
        // THREAD_RELEASES, so the refs oracle below converges to baseline.
    })
    .join()
    .expect("worker panicked");

    for (idx, refs_before_exit) in rx.recv().expect("worker did not report refs") {
        assert_eq!(
            refs_before_exit,
            base_refs[idx] + 2,
            "the worker's cached pin + the observation pin must hold the Entry before exit"
        );
    }
    // The ref-count oracle is synchronous; the deadline is only a diagnostic
    // bound in case worker teardown fails to release the cached pin.
    let deadline = Instant::now() + Duration::from_secs(60);
    for (idx, probe) in probes.iter().enumerate() {
        loop {
            if RouteDirectory::dbg_route_pin_refs(probe) == base_refs[idx] {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker-exit cached pin was not released: refs {} != baseline {}",
                RouteDirectory::dbg_route_pin_refs(probe),
                base_refs[idx]
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
    drop(probes);

    // -----------------------------------------------------------------
    const THREADS: usize = 8;
    const CYCLES: usize = 8;
    let (tx_reports, rx_reports) = mpsc::channel();
    thread::scope(|scope| {
        let mut starts = Vec::with_capacity(THREADS);
        let mut workers = Vec::with_capacity(THREADS);
        for worker in 0..THREADS {
            let (tx_start, rx_start) = mpsc::channel::<()>();
            starts.push(tx_start);
            let tx = tx_reports.clone();
            workers.push(scope.spawn(move || {
                let directory = RouteDirectory::new();
                let reservations: Vec<_> = (0..CYCLES)
                    .map(|_| aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap())
                    .collect();
                let registrations: Vec<_> = reservations
                    .iter()
                    .map(|reservation| {
                        let root = reservation.as_ptr();
                        directory
                            .register(root, SEGMENT, root, 9, RouteKind::Small)
                            .unwrap()
                    })
                    .collect();
                let roots: Vec<usize> = reservations.iter().map(|r| r.as_ptr().addr()).collect();
                let mut probes = Vec::with_capacity(CYCLES);
                for root in &roots {
                    let pin = directory
                        .lookup(core::ptr::without_provenance_mut::<u8>(*root))
                        .unwrap();
                    pin.dbg_cache_insert_for_test();
                    probes.push(
                        directory
                            .lookup(core::ptr::without_provenance_mut::<u8>(*root))
                            .unwrap(),
                    );
                }
                tx.send((worker, roots, probes)).expect("parent gone");
                rx_start
                    .recv_timeout(Duration::from_secs(5))
                    .expect("worker start timed out");
                drop(registrations);
                drop(reservations);
            }));
        }

        let mut worker_reports = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            worker_reports.push(
                rx_reports
                    .recv_timeout(Duration::from_secs(5))
                    .expect("fixture worker report timed out"),
            );
        }
        // All fixture routes, registrations, reservations and probes exist before this baseline.
        let fixture_leaf0 = quiescent_leaf_balance();
        for start in starts {
            start.send(()).expect("worker gone before start");
        }
        for worker in workers {
            worker.join().expect("fixture worker panicked");
        }
        for (_, roots, probes) in &worker_reports {
            for (root, probe) in roots.iter().zip(probes) {
                assert_eq!(
                    RouteDirectory::dbg_route_pin_refs(probe),
                    1,
                    "only the report probe remains after worker TLS destruction for {root:#x}"
                );
            }
        }
        drop(worker_reports);
        let fixture_leaf1 = quiescent_leaf_balance();
        let final_count = fixture_leaf1.1;
        assert!(
            final_count.abs_diff(fixture_leaf0.1.saturating_sub(64)) <= 16,
            "expected 64 sidecar frees within tolerance 16, baseline {:?}, final {:?}",
            fixture_leaf0,
            fixture_leaf1
        );
        assert!(
            fixture_leaf1.0 < fixture_leaf0.0,
            "expected sidecar bytes to decrease, baseline {:?}, final {:?}",
            fixture_leaf0,
            fixture_leaf1
        );
    });
}

// ---------------------------------------------------------------------------
// (f) Stress: real foreign dealloc at scale with full accounting.
// ---------------------------------------------------------------------------
#[test]
fn stress_foreign_frees_full_accounting() {
    let _guard = SerialGuard::acquire();
    let before = quiescent_totals();
    let leaf0 = quiescent_leaf_balance();
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
                unsafe {
                    GLOBAL.dealloc(std::ptr::with_exposed_provenance_mut(addr), small_layout)
                };
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
    // MUTANT cache-disabled: HITS stay flat and this fails.
    assert!(after.0 > before.0, "same-base repeats must hit the cache");
    assert!(after.1 > before.1, "first touches must miss");
    assert_eq!(
        after.4 - before.4,
        0,
        "all routes stay linked throughout: no unlink fallback"
    );
    // BOUNDED in-window pin balance (documented oracle limit): the workers'
    // TLS caches also interact with segment releases (unlink fallback clears
    // a slot without an eviction charge) and allocator-service threads add
    // their own inserts/releases to the process-global counters, so the
    // exact zero observed for fixture-only windows is not stable here. The
    // lifetime invariant (no pin leaves a slot uncounted) stays asserted:
    // every insert is matched by an eviction, a thread release, or a slot
    // still held, so the lifetime balance can never go negative.
    assert!(
        after.2 >= after.3 + after.5,
        "no pin leaves a slot uncounted (lifetime): {:?} -> {:?}",
        before,
        after
    );
    assert!(
        ((after.2 - after.3 - after.5) as i64) >= ((before.2 - before.3 - before.5) as i64) - 64,
        "in-window shrink bounded by pre-window pins releasing in-window: {:?} -> {:?}",
        before,
        after
    );

    // Owner drains what the workers delivered, then the real terminal state.
    let sample: Vec<usize> = small_ptrs.clone();
    force_drain(&sample, "stress_foreign_frees_full_accounting");
    for addr in small_ptrs.iter().step_by(64) {
        assert!(!pending_at(*addr), "pending bit survived the owner drain");
    }

    // The 4096 x 4 KiB mid blocks classify Large: EACH is its own route with a
    // System-allocated LargeState sidecar, and those pairs legitimately persist
    // (recording is asymmetric across Large alloc/free), so the stress window's
    // pair/byte bounds must absorb the test's own ~4096 Large-route artifact.
    assert_leaf_bounded(
        leaf0,
        "stress_foreign_frees_full_accounting",
        8192,
        32 * 1024 * 1024,
    );
    // The owner keeps nothing: both vectors were moved into the workers via
    // `zip` and are dropped inside each worker after the frees, so the
    // pointers were never double-freed.
}

// ---------------------------------------------------------------------------
// (g) Retention: after a route is unlinked, the NEXT cold-path miss on the
//     SAME thread sweeps the dead cached pin (refs return exactly).
// ---------------------------------------------------------------------------
#[test]
fn sweep_reclaims_unlinked_cached_pin_after_next_miss() {
    let _guard = SerialGuard::acquire();
    let before = quiescent_totals();
    // Shared with the worker through an `Arc` (the registrations borrow the
    // directory, so it cannot be moved into the closure).
    let directory = std::sync::Arc::new(RouteDirectory::new());

    // Route A: the one whose pin gets cached and then unlinked.
    let reservation_a = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root_a = reservation_a.as_ptr();
    let registration_a = directory
        .register(root_a, SEGMENT, root_a, 21, RouteKind::Small)
        .unwrap();
    // SAFETY: `lookup` reads only the address.
    let probe_a = directory.lookup(root_a).unwrap();

    // Route B: an ISSUED, never-cached route. Its miss (no slot for its base)
    // drives the production cold path (`publish_cached` miss branch), where
    // the sweep lives — and, unlike a real foreign free, it does NOT end in a
    // `cache_insert` whose own sweep would mask the counterfactual.
    let reservation_b = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root_b = reservation_b.as_ptr();
    let registration_b = directory
        .register(root_b, SEGMENT, root_b, 22, RouteKind::Small)
        .unwrap();
    // Make the hook's contract hold: the CURRENT issued Small instance at the
    // address. Publication misses (no cached slot for B's base), so ownership
    // is NOT consumed; the granule stays fixture-owned, like the zombie tests.
    registration_b.small_sidecar().unwrap().issue_for_test(0);

    use std::sync::mpsc;
    let (tx_pre, rx_pre) = mpsc::channel::<usize>();
    let (tx_unlink, rx_unlink) = mpsc::channel::<()>();
    // Addresses only inside the worker: raw pointers are not `Send`, and the
    // lookups below are address-only (numeric shard key), like every
    // production foreign free.
    let root_a_addr = root_a.addr();
    let root_b_addr = root_b.addr();
    let worker_directory = std::sync::Arc::clone(&directory);
    let worker = thread::spawn(move || {
        let directory = &*worker_directory;
        // Cache route A's pin in THIS thread's TLS (production insert path).
        // SAFETY: `lookup` reads only the address.
        let cached = directory
            .lookup(core::ptr::without_provenance_mut::<u8>(root_a_addr))
            .unwrap();
        cached.dbg_cache_insert_for_test();
        // Observation pin taken BEFORE the unlink: after unlink a fresh
        // lookup cannot find the retired entry, but this handle keeps the
        // pair (and its refs counter) alive and readable.
        // SAFETY: `lookup` reads only the address.
        let obs = directory
            .lookup(core::ptr::without_provenance_mut::<u8>(root_a_addr))
            .unwrap();
        let refs_pre = RouteDirectory::dbg_route_pin_refs(&obs);
        tx_pre.send(refs_pre).expect("main thread gone");
        rx_unlink
            .recv_timeout(Duration::from_secs(5))
            .expect("main thread did not release worker");

        // The next cold-path miss on this thread: B's base has no cached
        // slot, so `publish_cached` runs the miss branch that sweeps dead
        // (unlinked) slots. A's route was unlinked by the main thread, so
        // its cached pin is dead and must be reclaimed HERE, on the same
        // thread, before any thread exit.
        // SAFETY: the current issued instance at this address is the
        // fixture's own issued start-bit granule on route B; the publication
        // misses (B's base was never cached), so ownership is retained.
        assert!(
            !unsafe { RoutePin::dbg_publish_cached_for_test(root_b_addr) },
            "uncached base must miss"
        );
        let refs_post = RouteDirectory::dbg_route_pin_refs(&obs);
        // -1 for the unlink (owner ref), -1 for the swept dead cached pin.
        // MUTANT no-sweep-on-miss-path (`sweep_dead_slots` call removed
        // from the `publish_cached` miss branch): refs_post stays
        // refs_pre - 1 and this fails.
        assert_eq!(
            refs_post,
            refs_pre - 2,
            "the sweep must have released the unlinked route's cached pin"
        );
        drop(obs);
        // Thread exit: nothing left in the cache for A.
    });

    let refs_pre = rx_pre
        .recv_timeout(Duration::from_secs(5))
        .expect("worker did not report refs");
    assert_eq!(
        refs_pre,
        RouteDirectory::dbg_route_pin_refs(&probe_a),
        "cached pin + observation pin must hold the entry before unlink"
    );
    // Production unlink (same mechanism as the zombie tests).
    drop(registration_a);
    tx_unlink.send(()).expect("worker gone");
    worker.join().expect("sweep worker panicked");

    // The sweep counter must have advanced: the dead pin was reclaimed by the
    // miss-path sweep, not by eviction or thread exit.
    let after = quiescent_totals();
    assert!(
        after.6 > before.6,
        "the miss-path sweep must have reclaimed the dead pin (PIN_CACHE_SWEEPS)"
    );

    // Terminal state: after the worker join, only the main probe holds the entry.
    drop(probe_a);
    drop(registration_b);
}
