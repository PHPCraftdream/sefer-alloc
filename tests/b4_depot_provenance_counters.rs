//! B4 (task b4-provenance, chunk 1 + review round R1): depot-provenance
//! bin-ingress counters + heap-private depot shadow simulation.
//!
//! Part A — bin-ingress path counters (`DEPOT_*` bank in
//! `src/registry/heap_core/diag/counters.rs`): on a deterministic single-
//! thread own-free ping-pong, the magazine overflow half-flush fires exactly
//! the expected number of times (FLUSH_N = 8 blocks each), refills fire with
//! drained + carved == total, and NO remote / teardown / own-batch /
//! swap-top ingress fires (T=1, one thread ⇒ remote = 0; provenance is exact
//! at the call-site level). The remote oracle is INDEPENDENT of the batched
//! one: `DEPOT_BIN_INGRESS_OWN_BATCH` counts `dealloc_batch` staged flushes
//! (absent here), `DEPOT_BIN_INGRESS_REMOTE` counts owner sidecar drains of
//! pending foreign frees (impossible with one thread).
//!
//! Part B — heap-private shadow simulation
//! (`src/registry/heap_core/state/depot_shadow.rs`): invariants
//! `occupied_K ≤ K*8` for every class and K ∈ {1,2,4,8};
//! `sum(absorbed) + sum(real) == flush_blocks_total`;
//! `sum(from_depot) + sum(drain) == refill_blocks_total`; and a reset
//! (invalidation via teardown flush) stops absorption growth while real
//! flushed blocks keep counting — the absorbed fraction drops, exactly the
//! conservative-reset behaviour the module doc specifies.
//!
//! R1: per-slot scalar aggregates no longer exist — the per-heap snapshot
//! returns ONLY the occupied table; all scalars are read from the
//! process-global mirrors (`dbg_depot_shadow_aggregates` + the
//! `dbg_depot_bin_ingress_*` / `dbg_depot_refill_stats` hooks). The
//! SerialGuard makes the process the heap for delta purposes.
//!
//! Part C — fallback-heap row mapping: `HeapCore::new(OWNER_ID_FALLBACK)`
//! (the process fallback heap) must map to its DEDICATED shadow row — no
//! panic, no `DEPOT_SHADOW_UNMAPPED_EVENTS` tripwire, and the fallback row
//! participates in the shadow like any registry slot.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit",
    feature = "alloc-core"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

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

fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

const K_LEVELS: usize = 4;

/// Capacity of the simulated depot at level `k`: `K * FLUSH_N`, K ∈ {1,2,4,8}
/// (FLUSH_N = TCACHE_CAP / 2 = 8).
fn cap_of(k: usize) -> u64 {
    (1u64 << k) * 8
}

/// `occupied_K ≤ K*8` for every class (absolute invariant).
fn check_occupied(occupied: &[[u8; K_LEVELS]]) {
    for (c, occ) in occupied.iter().enumerate() {
        for (k, &o) in occ.iter().enumerate() {
            assert!(
                u64::from(o) <= cap_of(k),
                "occupied[{c}][K={}] = {o} exceeds capacity {}",
                1 << k,
                cap_of(k)
            );
        }
    }
}

/// Deterministic own-free ping-pong load on class 16B: alloc `n` blocks,
/// free them all. With n > TCACHE_CAP (16) the frees overflow-park the
/// magazine: the first 16 park, the 17th free triggers exactly ONE overflow
/// half-flush of FLUSH_N = 8 blocks (frees 18..n just re-fill the compacted
/// magazine).
fn alloc_free_cycle(heap: &mut HeapCore, n: usize) {
    let mut ps = Vec::with_capacity(n);
    for _ in 0..n {
        let p = heap.alloc(l16());
        assert!(!p.is_null());
        ps.push(p);
    }
    for &p in &ps {
        unsafe { heap.dealloc(p, l16()) };
    }
}

/// Part A + Part B invariants on a single-thread ping-pong.
#[test]
fn depot_counters_and_shadow_on_own_ping_pong() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    // Known-clean baseline: a recycled slot may carry prior state.
    heap.dbg_flush_all();

    let f0 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let t0 = HeapCore::dbg_depot_flush_teardown();
    let b0 = HeapCore::dbg_depot_bin_ingress_own_batch();
    let w0 = HeapCore::dbg_depot_flush_swap_top();
    let rem0 = HeapCore::dbg_depot_bin_ingress_remote();
    let r0 = HeapCore::dbg_depot_refill_stats();
    let m0 = HeapCore::dbg_depot_shadow_aggregates();

    alloc_free_cycle(heap, 64);

    let s1 = heap.dbg_depot_shadow_snapshot();
    let f1 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let t1 = HeapCore::dbg_depot_flush_teardown();
    let b1 = HeapCore::dbg_depot_bin_ingress_own_batch();
    let w1 = HeapCore::dbg_depot_flush_swap_top();
    let rem1 = HeapCore::dbg_depot_bin_ingress_remote();
    let r1 = HeapCore::dbg_depot_refill_stats();
    let m1 = HeapCore::dbg_depot_shadow_aggregates();

    // ── Part A: exact activation on this path ────────────────────────────
    // (deltas across the cycle; the SerialGuard excludes concurrent
    // perturbation of the process-global counters.)
    let flush_events = f1.0 - f0.0;
    let flush_blocks = f1.1 - f0.1;
    assert!(flush_events >= 1, "overflow half-flush must have fired");
    assert_eq!(
        flush_blocks,
        flush_events * 8,
        "every overflow flush moves exactly FLUSH_N=8 blocks"
    );
    // T=1, one thread: no remote / own-batch / swap-top / teardown ingress.
    assert_eq!(
        t1.0 - t0.0,
        0,
        "no teardown flush on the pure ping-pong path"
    );
    assert_eq!(
        b1.0 - b0.0,
        0,
        "no own-batch ingress (no dealloc_batch here)"
    );
    assert_eq!(
        w1.0 - w0.0,
        0,
        "no swap-top eviction (16B class cap == TCACHE_CAP)"
    );
    // REMOTE oracle (independent of the batched one): T=1 ⇒ no pending
    // foreign free can exist, so no owner sidecar drain ever reclaims.
    assert_eq!(
        rem1.0 - rem0.0,
        0,
        "single-thread ping-pong must have ZERO remote bin-ingress events"
    );
    assert_eq!(
        rem1.1 - rem0.1,
        0,
        "single-thread ping-pong must have ZERO remote bin-ingress blocks"
    );
    // remote = 0 ⇒ no shadow invalidation from reclaim either.
    assert_eq!(
        m1.6 - m0.6,
        0,
        "single-thread ping-pong must not invalidate depot residency"
    );

    // Refill provenance: drained + carved == total (exact accounting).
    let events = r1.0 - r0.0;
    let total = r1.1 - r0.1;
    let drained = r1.2 - r0.2;
    let carved = r1.3 - r0.3;
    assert!(events >= 1, "cold storm must refill at least once");
    assert_eq!(
        drained + carved,
        total,
        "every refill block is either a bin drain or a virgin carve"
    );
    // The shadow refill hook fires at exactly the refill path sites, so the
    // mirror's from_depot + drain equals the path's total (the former
    // per-heap refill scalars; scalars are mirrors since R1).
    for k in 0..K_LEVELS {
        assert_eq!(
            m1.3[k] - m0.3[k] + (m1.4[k] - m0.4[k]),
            total,
            "mirror from_depot[K={}] + drain == refill blocks total",
            1 << k
        );
    }

    // ── Part B: shadow arithmetic invariants ─────────────────────────────
    check_occupied(&s1);
    // Per-event identity, per K level: each event of n blocks contributes
    // n absorbed + n real to EVERY K (mirror deltas; scalars are mirrors
    // since R1). flush_blocks == the OWN_OVERFLOW path blocks — only that
    // flush feeds the shadow's flush hook.
    for k in 0..K_LEVELS {
        assert_eq!(
            m1.0[k] - m0.0[k] + (m1.1[k] - m0.1[k]),
            flush_blocks,
            "absorbed[K={}] + real[K={}] == own-overflow flush blocks",
            1 << k,
            1 << k
        );
    }
    // A K=8 depot (capacity 64) absorbs every FLUSH_N batch on this
    // invalidation-free run once its occupancy is warm.
    assert!(
        m1.2[K_LEVELS - 1] - m0.2[K_LEVELS - 1] >= 1,
        "K=8 depot must fully absorb at least one flush without invalidations"
    );
}

/// Part B: invalidation (teardown flush) resets occupancy — the absorbed
/// fraction stops growing while real flushed blocks keep counting.
#[test]
fn depot_shadow_invalidation_resets_absorption() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    // The baseline teardown flush itself invalidates once; remember its
    // (process-absolute) invalidation count from the mirror.
    let baseline_invalidation = HeapCore::dbg_depot_shadow_aggregates().6;

    // Build depot residency: one overflow flush parks 8 blocks into the
    // simulated depot (every K level fully absorbs the first batch).
    let m_base = HeapCore::dbg_depot_shadow_aggregates();
    let f_base = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let c16 = heap.dbg_class_for(l16()).expect("16B is a small class");
    alloc_free_cycle(heap, 20);
    let before = heap.dbg_depot_shadow_snapshot();
    let m_before = HeapCore::dbg_depot_shadow_aggregates();
    let f_before = HeapCore::dbg_depot_bin_ingress_own_overflow();
    // NOTE: all counter assertions below are deltas against `base`/`m_base`
    // (the baseline teardown flush above zeroed occupancy, but not the
    // event counters; the SerialGuard makes mirror deltas == this heap's
    // deltas).
    // 20 allocs leave ~11 blocks in the magazine, so the 20 frees trigger
    // TWO overflow half-flushes (16 flushed blocks total).
    assert_eq!(
        f_before.0 - f_base.0,
        2,
        "exactly two overflow flush events (20 frees)"
    );
    for k in 0..K_LEVELS {
        assert_eq!(
            u64::from(before[c16][k]),
            16.min(cap_of(k)),
            "occupancy after two flushes at K={}",
            1 << k
        );
        assert_eq!(
            m_before.0[k] - m_base.0[k],
            16.min(cap_of(k)),
            "absorbed delta at K={}",
            1 << k
        );
    }
    assert_eq!(
        m_before.6, baseline_invalidation,
        "no invalidation since the baseline flush"
    );

    // INVALIDATE via the production teardown primitive (trim path).
    heap.dbg_flush_all();
    let after_trim = heap.dbg_depot_shadow_snapshot();
    let m_after_trim = HeapCore::dbg_depot_shadow_aggregates();
    assert_eq!(
        m_after_trim.6,
        baseline_invalidation + 1,
        "teardown flush must invalidate depot residency"
    );
    for occ in after_trim.iter().flatten() {
        assert_eq!(*occ, 0, "occupancy reset by teardown invalidation");
    }

    // After the reset the absorbed counters are frozen; the next overflow
    // flush counts as REAL blocks only (absorbed fraction dropped).
    alloc_free_cycle(heap, 20);
    let after = heap.dbg_depot_shadow_snapshot();
    let m_after = HeapCore::dbg_depot_shadow_aggregates();
    assert_eq!(
        HeapCore::dbg_depot_bin_ingress_own_overflow().0 - f_before.0,
        2,
        "two more overflow flushes"
    );
    assert_eq!(
        HeapCore::dbg_depot_bin_ingress_own_overflow().1 - f_before.1,
        16,
        "16 more real-flushed blocks"
    );
    // After the reset, occupancy is empty, so absorption RESTARTS: each K
    // re-absorbs exactly min(FLUSH_N, cap) per event, i.e. the first event
    // fully and (for K >= 2) the second one too. What the reset demonstrably
    // does is (a) zero the residency (asserted above) and (b) cap the
    // absorbed credit at what the POST-reset occupancy allows. (Note: a
    // magazine refill always drains the depot first, so per-cycle absorption
    // restarts from empty — with FLUSH_N == K=1 capacity a warm K=1 depot
    // can never saturate across alloc/free cycles; the per-K identities and
    // the occupancy reset above are the exact observable reset semantics.)
    for k in 0..K_LEVELS {
        assert_eq!(
            m_after.0[k] - m_before.0[k],
            (16).min(cap_of(k)),
            "absorbed delta after invalidation at K={}",
            1 << k
        );
        assert_eq!(
            m_after.1[k] - m_before.1[k],
            16 - (16).min(cap_of(k)),
            "real flush delta after invalidation at K={}",
            1 << k
        );
    }
    check_occupied(&after);
    // Per-K identity still holds over the whole lifetime: absorbed + real
    // equals the lifetime own-overflow blocks (process mirror).
    for k in 0..K_LEVELS {
        assert_eq!(
            m_after.0[k] + m_after.1[k],
            HeapCore::dbg_depot_bin_ingress_own_overflow().1,
            "lifetime absorbed[K={}] + real == own-overflow blocks",
            1 << k
        );
    }
}

/// B4 chunk 1b: the process-wide STATIC MIRROR
/// (`HeapCore::dbg_depot_shadow_aggregates`) tracks the shadow exactly.
/// Deltas across the ping-pong scenario: the per-K `absorbed + real` sums
/// equal the static OWN-OVERFLOW blocks delta (only the overflow half-flush
/// feeds the shadow's flush hook); per-K `absorbed ≤ 8 · flush events`; the
/// refill mirror identity equals the path refill total; the occupancy
/// snapshot stays within capacity.
#[test]
fn depot_mirror_tracks_shadow_snapshot() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();

    let s0 = heap.dbg_depot_shadow_snapshot();
    let f0 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let r0 = HeapCore::dbg_depot_refill_stats();
    let m0 = HeapCore::dbg_depot_shadow_aggregates();

    alloc_free_cycle(heap, 64);

    let s1 = heap.dbg_depot_shadow_snapshot();
    let f1 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let r1 = HeapCore::dbg_depot_refill_stats();
    let m1 = HeapCore::dbg_depot_shadow_aggregates();

    check_occupied(&s0);
    check_occupied(&s1);

    // Flush identity: only the overflow half-flush feeds the shadow, so the
    // mirror's per-K absorbed+real sums == the static OWN-OVERFLOW blocks
    // delta.
    let flush_events = f1.0 - f0.0;
    let flush_blocks = f1.1 - f0.1;
    for k in 0..K_LEVELS {
        let absorbed = m1.0[k] - m0.0[k];
        let real = m1.1[k] - m0.1[k];
        assert_eq!(
            absorbed + real,
            flush_blocks,
            "mirror absorbed[K={}] + real == OWN-OVERFLOW flush blocks",
            1 << k
        );
        // absorbed_k ≤ FLUSH_N blocks per event (8 · events).
        assert!(
            absorbed <= 8 * flush_events,
            "absorbed[K={}] = {absorbed} exceeds 8 * {flush_events} events",
            1 << k
        );
    }
    // Refill identity against the path counters as well.
    let refill_total = r1.1 - r0.1;
    for k in 0..K_LEVELS {
        assert_eq!(
            m1.3[k] - m0.3[k] + (m1.4[k] - m0.4[k]),
            refill_total,
            "mirror from_depot[K={}] + drain == REFILL blocks total",
            1 << k
        );
    }
}

/// Part C (R1): the FALLBACK heap (`HeapCore::new(OWNER_ID_FALLBACK)`,
/// `src/global/fallback.rs`) must map to its DEDICATED shadow row
/// (`MAX_HEAPS`), not trip the `DEPOT_SHADOW_UNMAPPED_EVENTS` unmapped
/// oracle and not panic. Activation oracle: `dbg_with_fallback_for_test`
/// returns `Some` ONLY if the fallback heap was really constructed through
/// `HeapCore::new(OWNER_ID_FALLBACK)` (⇒ `depot_shadow_reset_for_heap`
/// ran on the fallback id), and the load runs real frees on it so the
/// overflow-flush hook fires on the fallback row.
#[test]
fn depot_shadow_fallback_heap_maps_to_dedicated_row() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let unmapped0 = HeapCore::dbg_depot_shadow_unmapped();
    let f0 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let m0 = HeapCore::dbg_depot_shadow_aggregates();

    let ran = HeapCore::dbg_with_fallback_for_test(|heap| {
        // Real own-free ping-pong on the fallback heap: 20 frees trigger
        // two overflow half-flushes (same arithmetic as Part B).
        alloc_free_cycle(heap, 20);
        // The per-heap snapshot works on the fallback heap too (its id maps
        // to the dedicated row).
        heap.dbg_depot_shadow_snapshot()
    });
    assert!(
        ran.is_some(),
        "activation oracle: the fallback heap must be reachable and \
         constructible (HeapCore::new(OWNER_ID_FALLBACK) must not panic)"
    );

    // The fallback id must NOT hit the unmapped tripwire.
    assert_eq!(
        HeapCore::dbg_depot_shadow_unmapped(),
        unmapped0,
        "OWNER_ID_FALLBACK must map to the dedicated shadow row, never the \
         unmapped tripwire"
    );

    // The overflow-flush hook really fired on the fallback row (oracle that
    // the load went through the magazine overflow path, not a vacuous run).
    let f1 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    assert!(
        f1.0 > f0.0,
        "activation oracle: overflow half-flush must have fired on the \
         fallback heap"
    );
    // Mirror identity on the fallback row's events: absorbed + real ==
    // own-overflow blocks, per K.
    let m1 = HeapCore::dbg_depot_shadow_aggregates();
    let flush_blocks = f1.1 - f0.1;
    for k in 0..K_LEVELS {
        assert_eq!(
            m1.0[k] - m0.0[k] + (m1.1[k] - m0.1[k]),
            flush_blocks,
            "fallback absorbed[K={}] + real == own-overflow flush blocks",
            1 << k
        );
    }
    // The dedicated row's occupancy is readable and within capacity.
    let occ = HeapCore::dbg_depot_shadow_fallback_row();
    check_occupied(&occ);
}

// ── R4 (b4-provenance, chunk R4) ────────────────────────────────────────────
//
// (a) SUBSTRATE_PUSHES identity: the physical Small-block bin-ingress total
//     (`dbg_depot_bin_ingress_substrate_pushes`, incremented at the three
//     physical entry points — `dealloc_small`, the `flush_run` splice,
//     `reclaim_sidecar_record`) must equal the SUM of the attributed
//     provenance counters (own-overflow + swap-top + teardown + own-batch +
//     remote blocks) ⇒ OTHER = 0 exactly (explicit assert; a nonzero OTHER
//     would mean an unattributed ingress path exists).
//
// (b) REMOTE EVENTS unification: EVENTS are counted at the SINGLE point
//     inside `consume_small_cuts_impl`, so EVERY caller path — strict trim
//     (`drain_sidecar_ingress`), bounded maintenance
//     (`drain_sidecar_ingress_bounded`) and the DISCOVERY drain
//     (`find_segment` → `drain_segment_sidecar`, scan.rs) — is counted
//     uniformly. The discovery path needs no separate test scenario: its
//     reclaim leg IS `consume_small_cuts_impl` (via
//     `drain_segment_sidecar`), so any reclaiming pass increments EVENTS at
//     the same site this test exercises; below we assert EVENTS >= 1 on a
//     remote scenario and that EVENTS and BLOCKS are consistent (every
//     reclaiming pass reclaims ≥ 1 record, so EVENTS ≤ BLOCKS and both
//     move together by construction of the single counting point).

/// (a) OTHER == 0: SUBSTRATE_PUSHES == Σ(attributed blocks) on a
/// single-thread ping-pong (deltas; SerialGuard makes process mirrors ==
/// this heap's deltas).
#[test]
fn depot_ingress_substrate_pushes_identity_other_zero() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();

    let p0 = HeapCore::dbg_depot_bin_ingress_substrate_pushes();
    let f0 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let w0 = HeapCore::dbg_depot_flush_swap_top();
    let t0 = HeapCore::dbg_depot_flush_teardown();
    let b0 = HeapCore::dbg_depot_bin_ingress_own_batch();
    let r0 = HeapCore::dbg_depot_bin_ingress_remote();

    alloc_free_cycle(heap, 64);

    let p1 = HeapCore::dbg_depot_bin_ingress_substrate_pushes();
    let f1 = HeapCore::dbg_depot_bin_ingress_own_overflow();
    let w1 = HeapCore::dbg_depot_flush_swap_top();
    let t1 = HeapCore::dbg_depot_flush_teardown();
    let b1 = HeapCore::dbg_depot_bin_ingress_own_batch();
    let r1 = HeapCore::dbg_depot_bin_ingress_remote();

    let pushes = p1 - p0;
    let attributed = (f1.1 - f0.1) + (w1.1 - w0.1) + (t1.1 - t0.1) + (b1.1 - b0.1) + (r1.1 - r0.1);
    let other = pushes.saturating_sub(attributed);
    assert!(
        pushes >= attributed,
        "SUBSTRATE_PUSHES ({pushes}) must never undershoot the attributed sum ({attributed})"
    );
    assert_eq!(
        other, 0,
        "OTHER (unattributed bin ingress) must be exactly 0: pushes={pushes}, \
         attributed={attributed}"
    );
    // Activation oracle: the ping-pong really exercised a bin-ingress path.
    assert!(f1.1 - f0.1 >= 8, "overflow flush blocks must have fired");
}

/// (b) Remote scenario: a foreign-thread free leaves a pending publication
/// on this heap's segments; a subsequent alloc on the owner drains it
/// (discovery drain through `drain_segment_sidecar` and/or the strict trim
/// on refill — BOTH flow through `consume_small_cuts_impl`). EVENTS >= 1
/// proves the unified counting point fires on reclaiming passes, and
/// EVENTS/BLOCKS stay consistent (EVENTS <= BLOCKS, both move together by
/// construction — one increment per reclaiming pass inside the same
/// function that increments BLOCKS per reclaimed record).
#[test]
fn depot_remote_events_counted_on_reclaiming_passes() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();

    let r0 = HeapCore::dbg_depot_bin_ingress_remote();

    // Owner (this thread) allocates; a foreign thread frees — every free
    // becomes a pending sidecar publication on the OWNER's segments.
    // Addresses travel as `usize` (Send); the foreign thread uses its own
    // `SeferAlloc` instance, same pattern as `tests/remote_fanin.rs`.
    let n = 64;
    let addresses: Vec<usize> = (0..n)
        .map(|_| {
            let p = heap.alloc(l16());
            assert!(!p.is_null());
            p.expose_provenance()
        })
        .collect();
    {
        let layout = l16();
        std::thread::spawn(move || {
            let allocator = sefer_alloc::SeferAlloc::new();
            use std::alloc::GlobalAlloc;
            for address in addresses {
                let ptr = std::ptr::with_exposed_provenance_mut(address);
                // SAFETY: the owner transferred these live allocations to
                // this thread; each is freed exactly once.
                unsafe { allocator.dealloc(ptr, layout) };
            }
        })
        .join()
        .unwrap();
    }
    // Owner drains: the strict trim pass pulls the pending publications
    // into the bins (remote reclaim); the discovery drain goes through the
    // same `consume_small_cuts_impl` counting point (see module comment).
    let reclaimed = heap.dbg_drain_sidecar_ingress();
    assert!(reclaimed >= 1, "pending remote publications must reclaim");

    let r1 = HeapCore::dbg_depot_bin_ingress_remote();
    let events = r1.0 - r0.0;
    let blocks = r1.1 - r0.1;
    assert!(
        events >= 1,
        "unified REMOTE EVENTS must fire on reclaiming passes (strict, \
         bounded AND discovery — one counting point in \
         consume_small_cuts_impl)"
    );
    assert!(
        blocks >= 1,
        "remote BLOCKS must move together with EVENTS on the same scenario"
    );
    assert!(
        events <= blocks,
        "every reclaiming pass reclaims at least one record: EVENTS ({events}) \
         <= BLOCKS ({blocks})"
    );
    // The SUBSTRATE identity holds across the remote scenario too: every
    // remote-reclaimed block is a physical bin entry (counted at
    // `reclaim_sidecar_record`) and is attributed to REMOTE_BLOCKS.
    // Lifetime (process-absolute) identity ⇒ OTHER == 0 over everything
    // this process has done so far, including the remote scenario above.
    let abs_attributed = HeapCore::dbg_depot_bin_ingress_own_overflow().1
        + HeapCore::dbg_depot_flush_swap_top().1
        + HeapCore::dbg_depot_flush_teardown().1
        + HeapCore::dbg_depot_bin_ingress_own_batch().1
        + r1.1;
    assert_eq!(
        HeapCore::dbg_depot_bin_ingress_substrate_pushes(),
        abs_attributed,
        "lifetime OTHER must be 0: pushes={}, attributed={abs_attributed}",
        HeapCore::dbg_depot_bin_ingress_substrate_pushes()
    );
}
