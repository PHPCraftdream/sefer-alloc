//! oc_mem_probe — deterministic "honest memory judge" for the B4 one-class
//! A/B comparison (design `.rush/art/b4/oneclass_design.md` §4.7 / §6.1 J4).
//!
//! REVISED after the mem4 review (P1#3 + P2#4):
//!   1. COLD MEASURES THE WORKLOAD HEAP. Previously `mode_cold()` ran the
//!      churn through a separate TLS `SeferAlloc` heap while main took
//!      `dbg_segment_state_reconciliation`/`dbg_pool_cap` from the RAW
//!      lease heap — the committed Δ=0 belonged to a different heap. Now
//!      the cold churn runs THROUGH the raw lease heap (`heap.alloc`/
//!      `heap.dealloc`) and every reconciliation/config read happens on
//!      that SAME heap. (Chosen over "read reconciliation from the TLS
//!      heap" because the TLS face exposes no per-heap reconciliation
//!      surface, and the raw lease is the surface mem4 must judge.)
//!      Consequence: `trim_current_thread` no longer applies (it is a TLS
//!      face API; the raw lease heap does not bind TLS, and this probe
//!      never binds TLS).
//!   2. COLD CONFIG: pool DISABLED (`pool_segments(0)`). With the pool
//!      enabled, emptied segments are retained pooled and their routes
//!      stay registered, so 64 churn iterations legitimately produce only
//!      a couple of route registrations — the register/release oracle
//!      would be unassertable. Disabling the pool (a documented `Some(0)`
//!      config, both shoulders identical) makes every iteration an actual
//!      segment register→release cycle.
//!   3. COLD ORACLE = successful route registration AND unlink counters. Each
//!      cycle records registration and unlink deltas; sidecar System free-call
//!      counts are never treated as route releases.
//!   4. RESOLVED BYTE-CAP HOOK. New read-only doc-hidden
//!      `bench-internals` accessor `dbg_pool_byte_cap_effective()`
//!      (`pool_cap × SEGMENT`, delegated HeapCore → AllocCore). HONESTY:
//!      the configured byte-ceiling VALUE is not retained at runtime —
//!      resolution (`AllocCore::new_with_config`) folds
//!      `min(pool_segments, pool_byte_cap / SEGMENT)` into the stored
//!      `pool_cap` and keeps only segments. The hook therefore returns
//!      the RESOLVED, operative byte bound derived from stored runtime
//!      state — never the requested figure — and the probe asserts it
//!      equals `dbg_pool_cap() × SEGMENT`.
//!   5. DEADPINS DENOMINATOR FIXED. The sidecar delta across the sweep
//!      window is (freed dead sidecars − the newly inserted live pin's
//!      sidecar): mem3 divided it by 4 while the measured delta was
//!      3 sidecars. Now the probe snapshots sidecar bytes AFTER
//!      registering the sweep fixture but BEFORE its pin insert (the
//!      sweep runs inside the insert), reports the actual number of
//!      pins the sweep freed from the `PIN_CACHE_SWEEPS` counter delta
//!      (which counts PER-PIN reclaims; 4 here), asserts it, and divides
//!      the mid→after delta by THAT counter value.
//!   6. `route_registrations` = probe-side count of actual `register()`
//!      calls during the scenario (6 fixtures + 1 sweep fixture = 7),
//!      NOT a live census. The census is still printed, labeled as a
//!      live census.
//!   7. THE FINAL MEM SNAPSHOT IS TAKEN BEFORE ANY FIXTURE DROP (deadpins
//!      keeps its fixtures alive at the measurement point), so the MEM
//!      line and the deadpins checkpoint describe ONE consistent state;
//!      checkpoint labels in the raw output make the phases explicit.
//!
//! Legacy fixes (D1′ round, retained):
//!   - sidecar/pin baselines BEFORE bootstrap/claim, symmetric shoulders;
//!   - `retained_pins` derived from the pin-cache exit-path counters;
//!   - descriptor_entries reported as a COUNT, never invented bytes.
//!
//! Compiles against the API surface that is IDENTICAL in base `bbc3c7b4`
//! PLUS the mem4 measurement hooks (two read-only accessors + one
//! diagnostic counter; applied as an identical patch to the A shoulder —
//! measurement-only, no behaviour change; the patch sha256 is printed in
//! each log's identity line):
//!   `RouteDirectory::{new, register, lookup, global, live_route_census_for_test,
//!   dbg_registrations_total}`, `RoutePin::{dbg_route_pin_cache_totals,
//!   dbg_cache_insert_for_test, linked}`,
//!   `SmallSidecar::system_totals_for_test`,
//!   `HeapRegistry::dbg_claim_lease_with_config`,
//!   `HeapCore::{dbg_pool_cap, dbg_pool_byte_cap_effective,
//!   dbg_segment_state_reconciliation}`, `aligned_vmem::reserve_aligned`.
//!
//! Modes (argv[1]): full | sparse | cold | deadpins
//!
//! Output: exactly one machine-readable line
//! `MEM committed_bytes=.. system_sidecar_bytes=.. retained_pins=..
//!  descriptor_entries=.. rss_kib=.. config_conflicts_total=..
//!  requested_pool=.. resolved_pool=.. resolved_pool_byte_cap=..`
//! plus, for `deadpins`, `route_registrations=..`, `sweep_freed_pins=..`,
//! `dead_route_bytes=..` and `dead_route_bytes_per_freed_pin=..`.
//!
//! Run (see oc_mem_run4.sh):
//!   cargo run --release --example zz_oc_mem_probe --features \
//!     "production internals bench-internals" -- full

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]
#![allow(clippy::cast_possible_truncation)]

use std::alloc::Layout;

use sefer_alloc::{
    registry::{
        bootstrap, config_conflicts_total,
        segment_route::{RouteDirectory, RouteKind, RoutePin, SmallSidecar},
        HeapCore, HeapRegistry,
    },
    LargeCacheConfig, SmallSegmentPoolConfig,
};

/// Requested pool config for full/sparse/deadpins (single arm — fresh
/// process per shoulder per R26-1 isolation rule, so the first claim is
/// unconditionally this config).
const POOL_SEGMENTS: usize = 8;
/// Requested byte ceiling. NOT the resolved value: the runtime folds
/// `min(pool_segments, byte_cap / SEGMENT)` into `pool_cap` and does not
/// retain the byte figure; the resolved bound comes from the
/// `dbg_pool_byte_cap_effective` hook (see module header §4).
const POOL_BYTE_CAP_REQUESTED: usize = 32 * 1024 * 1024;

/// Segment span (stable design constant; `os::SEGMENT` is crate-private).
const SEGMENT: usize = 4 * 1024 * 1024;

/// `full`: cap on 16-B block count (segment holds ~245 248; NULL ends the
/// loop when the pool cap / segment space is exhausted).
const FULL_MAX_BLOCKS: usize = 1 << 20;

/// `sparse`: DECLARED plan — the scenario must force at least this many
/// fresh small-route registrations (denominator of the oracle).
const SPARSE_PLAN_ROUTES: usize = 4;
/// 4-KiB blocks the sparse scenario allocates BEFORE the oracle snapshot.
/// 6144 blocks = 24 MiB: above the primordial payload (~1 KiB-blocks fit,
/// measured) plus three fresh 4-MiB segments, so ≥4 fresh routes register.
const SPARSE_ALLOC_BLOCKS: usize = 6144;
/// Sparse oracle lower bound: ≥95% of the plan (`ceil(0.95 × 4) = 4`).
const SPARSE_MIN_REGISTRATIONS: usize = 4;
/// Blocks kept live after the bulk free (sparse semantics: modest footprint).
const SPARSE_KEEP_LIVE: usize = 8;

/// `cold`: iterations of alloc-everything-then-free-everything. Each
/// iteration is one claimed register/release churn cycle (denominator of
/// the cold oracle). The per-iteration cap is deliberately ABOVE the
/// heap's whole committed capacity so every iteration actually exhausts
/// the primordial remainder AND forces fresh segment reserves (a fixed
/// 2.4 MiB burst fits inside the primordial segment and produces ZERO
/// registrations — measured). With the pool disabled every segment freed
/// at iteration end is released and re-registered next iteration.
const COLD_ITERS: usize = 64;
/// Blocks of 4 KiB per cold iteration (upper bound; alloc stops at NULL).
const COLD_BLOCKS_PER_ITER: usize = 8192;
/// Cold oracle lower bound: ≥95% of the 64 claimed cycles must have been
/// real route registrations (`ceil(0.95 × 64) = 61`).
const COLD_MIN_REGISTRATIONS: u64 = 61;

/// `deadpins`: fixture routes registered on dedicated reservations.
const DEADPIN_ROUTES: usize = 6;
/// Pin-cache capacity is 4 (`PIN_CACHE_SLOTS` in `route_pin_cache.rs`); the
/// first CACHE_ROUTES pins are inserted into the cache, the rest stay as
/// plain handles. 4 inserted + 2 uncached = 6 registrations (+1 sweep
/// fixture = 7 total probe-side registrations).
const PIN_CACHE_SLOTS: usize = 4;

fn config_for(mode: &str) -> LargeCacheConfig {
    let pool = if mode == "cold" {
        // Cold: pool DISABLED — see module header §2.
        SmallSegmentPoolConfig::new().pool_segments(0)
    } else {
        SmallSegmentPoolConfig::new()
            .pool_segments(POOL_SEGMENTS)
            .pool_byte_cap(POOL_BYTE_CAP_REQUESTED)
    };
    LargeCacheConfig::new().pool(pool)
}

/// Resolved pool cap this mode's config MUST produce (probe-side mirror of
/// `min(pool_segments, pool_byte_cap / SEGMENT)`; cold disables the pool).
fn expected_pool_cap(mode: &str) -> usize {
    if mode == "cold" {
        0
    } else {
        POOL_SEGMENTS.min(POOL_BYTE_CAP_REQUESTED / SEGMENT)
    }
}

/// RSS in KiB from `/proc/self/status` (`VmRSS:`), 0 when unavailable.
fn rss_kib() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            return kb;
        }
    }
    0
}

/// Live entries currently held in THIS thread's route pin cache.
///
/// Every exit path from a cache slot is counted: `PIN_CACHE_EVICTIONS`
/// (replace/evict), `PIN_CACHE_UNLINK_FALLBACKS` (on-hit dead release),
/// `PIN_CACHE_SWEEPS` (cold-path sweep reclaim), `PIN_CACHE_THREAD_RELEASES`
/// (thread-exit drop). The remaining exit (incarnation mismatch) is defense
/// in depth that "never fires today" (immutable field) and would otherwise
/// make this an under-count by at most one slot; it is documented, not
/// hidden. Single-threaded probe → `thread_releases` stays 0.
fn retained_pins(baseline: (u64, u64, u64, u64, u64, u64, u64)) -> i64 {
    let (h, _mi, ins, ev, uf, tr, sw) = RoutePin::dbg_route_pin_cache_totals();
    let (_h0, _mi0, ins0, ev0, uf0, tr0, sw0) = baseline;
    let d = |x: u64, x0: u64| -> i64 { x as i64 - x0 as i64 };
    // `_ = h` — hits never change occupancy; kept in the destructure only to
    // keep the tuple shape explicit.
    let _ = h;
    d(ins, ins0) - d(ev, ev0) - d(uf, uf0) - d(tr, tr0) - d(sw, sw0)
}

fn mem_line(
    committed: u64,
    sidecar_bytes: usize,
    pins: i64,
    rss: u64,
    conflicts: u64,
    resolved_segments: usize,
    resolved_byte_cap: usize,
    extra: &str,
) {
    println!(
        "MEM committed_bytes={committed} system_sidecar_bytes={sidecar_bytes} \
         retained_pins={pins} descriptor_entries={pins} rss_kib={rss} \
         config_conflicts_total={conflicts} \
         requested_pool=segments={POOL_SEGMENTS},byte_cap={POOL_BYTE_CAP_REQUESTED} \
         resolved_pool=segments={resolved_segments} \
         resolved_pool_byte_cap={resolved_byte_cap}(hook=dbg_pool_byte_cap_effective){extra}"
    );
}

fn sidecar_delta(base: (usize, usize, usize, usize, usize)) -> (usize, usize, usize, usize) {
    // (alloc_bytes, free_bytes, alloc_calls, free_calls) net of baseline.
    let (a, _z, f, ac, fc) = SmallSidecar::system_totals_for_test();
    let (_a0, _z0, f0, ac0, fc0) = base;
    (
        a - _a0,
        f - f0,
        ac.saturating_sub(ac0),
        fc.saturating_sub(fc0),
    )
}

fn mode_full(heap: &mut HeapCore) {
    let layout = Layout::from_size_align(16, 16).unwrap();
    let mut live: Vec<*mut u8> = Vec::with_capacity(FULL_MAX_BLOCKS);
    for _ in 0..FULL_MAX_BLOCKS {
        let p = heap.alloc(layout);
        if p.is_null() {
            break;
        }
        live.push(p);
    }
    eprintln!("full: live 16-B blocks = {}", live.len());
    std::hint::black_box(&live);
    // Blocks stay live at the measurement point (that IS the scenario).
}

fn mode_sparse(heap: &mut HeapCore) {
    // ACTIVATION ORACLE (D1′ round-2 P2#8, rebuilt after the mem4 review):
    // the earlier plan (a few small classes) fit entirely inside the
    // primordial segment → 0/0 VACUOUS. The scenario now allocates 4-KiB
    // blocks in a volume that EXCEEDS the primordial payload (~1 KiB-blocks
    // fit; measured), so the allocator MUST register fresh small routes:
    //   numerator   = registrations observed by the directory counter
    //                 (`dbg_registrations_total`) across the window;
    //   denominator = the DECLARED plan: SPARSE_PLAN_ROUTES fresh routes
    //                 (6144 4-KiB blocks = 24 MiB ≫ primordial + 3 segments,
    //                 so ≥4 fresh segments/routes are forced);
    //   oracle      = registrations_delta ≥ ceil(0.95 × plan).
    // SPARSE SEMANTICS: after the registration is forced, all but a few
    // blocks are freed (pool retains the routes) — the live footprint stays
    // modest while the registration fact is real.
    let directory = RouteDirectory::global();
    let regs_before = directory.dbg_registrations_total();
    let layout = Layout::from_size_align(4096, 16).unwrap();
    let mut live: Vec<*mut u8> = Vec::with_capacity(SPARSE_ALLOC_BLOCKS);
    for _ in 0..SPARSE_ALLOC_BLOCKS {
        let p = heap.alloc(layout);
        if p.is_null() {
            break;
        }
        live.push(p);
    }
    eprintln!(
        "sparse: allocated {} 4-KiB blocks (plan {SPARSE_ALLOC_BLOCKS}; exceeds the \
         primordial payload — fresh small routes forced)",
        live.len()
    );
    let regs_mid = directory.dbg_registrations_total();
    let reg_delta = regs_mid - regs_before;
    assert!(
        reg_delta as usize >= SPARSE_MIN_REGISTRATIONS,
        "sparse oracle: {reg_delta} registrations < required {SPARSE_MIN_REGISTRATIONS} \
         (95% of the {SPARSE_PLAN_ROUTES}-route plan) — plan did not activate"
    );
    // Sparse semantics: keep only a handful of blocks live; the (pool-enabled)
    // routes stay registered after the bulk free.
    let keep = live.split_off(live.len() - SPARSE_KEEP_LIVE.min(live.len()));
    for &p in &live {
        // SAFETY (allocator contract): `p` came from `heap.alloc` above with
        // the same `layout`, freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    let (small_after, prim_after, _large_after) = directory.live_route_census_for_test();
    let regs_after = directory.dbg_registrations_total();
    eprintln!(
        "sparse: oracle registrations_delta={reg_delta} (mid) / {} (post-free) >= \
         {SPARSE_MIN_REGISTRATIONS} required (plan {SPARSE_PLAN_ROUTES} routes); \
         kept live blocks={} (sparse semantics); census after free: small={small_after} \
         primordial={prim_after} (pool retains freed routes)",
        regs_after - regs_before,
        keep.len()
    );
    std::hint::black_box(&keep);
}

/// `cold` runs the churn THROUGH the raw lease heap (module header §1):
/// every registration, release and reconciliation read below belongs to
/// the SAME heap main measures.
fn mode_cold(heap: &mut HeapCore, base: (usize, usize, usize, usize, usize)) {
    let directory = RouteDirectory::global();
    let layout = Layout::from_size_align(4096, 16).unwrap();
    let regs_before = directory.dbg_registrations_total();
    let (live_before, prim_before, large_before) = directory.live_route_census_for_test();
    eprintln!(
        "cold: checkpoint pre-loop registrations_before={regs_before} \
         census small={live_before} primordial={prim_before} large={large_before}"
    );
    let unlinks_before = directory.dbg_unlinks_total();
    let mut cycle_registrations = 0u64;
    let mut census_removed = 0u64;
    let mut last_unlinks = unlinks_before;
    for iter in 0..COLD_ITERS {
        let (small_before, _, _) = directory.live_route_census_for_test();
        let regs_cycle_before = directory.dbg_registrations_total();
        let mut live: Vec<*mut u8> = Vec::with_capacity(COLD_BLOCKS_PER_ITER);
        for _ in 0..COLD_BLOCKS_PER_ITER {
            let p = heap.alloc(layout);
            if p.is_null() {
                break;
            }
            live.push(p);
        }
        let n = live.len();
        if iter == 0 || iter + 1 == COLD_ITERS {
            eprintln!("cold: iteration {iter} live 4-KiB blocks = {n} (NULL-terminated)");
        }
        let regs_after_alloc = directory.dbg_registrations_total();
        cycle_registrations += regs_after_alloc - regs_cycle_before;
        let (small_after_alloc, _, _) = directory.live_route_census_for_test();
        for &p in &live {
            // SAFETY (allocator contract): `p` came from `heap.alloc` above
            // with the same `layout`, freed exactly once.
            unsafe { heap.dealloc(p, layout) };
        }
        std::hint::black_box(live.len());
        let (small_after_free, _, _) = directory.live_route_census_for_test();
        let unlinks_after_cycle = directory.dbg_unlinks_total();
        let cycle_unlink_delta = unlinks_after_cycle - last_unlinks;
        last_unlinks = unlinks_after_cycle;
        census_removed += (small_after_alloc.saturating_sub(small_after_free)) as u64;
        eprintln!("cold: cycle {iter} registrations={} census {small_before}->{small_after_alloc}->{small_after_free} unlink_delta={cycle_unlink_delta}", regs_after_alloc-regs_cycle_before);
        std::hint::black_box(iter);
    }
    let regs_after = directory.dbg_registrations_total();
    let reg_delta = regs_after - regs_before;
    let unlink_delta = directory.dbg_unlinks_total() - unlinks_before;
    let (live_after, prim_after, large_after) = directory.live_route_census_for_test();

    eprintln!(
        "cold: checkpoint post-loop registrations_after={regs_after} \
         registrations_delta={reg_delta} (claimed cycles={COLD_ITERS}, \
         required >={COLD_MIN_REGISTRATIONS})"
    );
    eprintln!(
        "cold: release facts census small={live_after} primordial={prim_after} large={large_after} (pre-loop small={live_before}); registrations={cycle_registrations}, unlink_routes={unlink_delta}, census_removed={census_removed} over {COLD_ITERS} cycles"
    );
    assert!(
        reg_delta >= COLD_MIN_REGISTRATIONS,
        "cold oracle: {reg_delta} route registrations over {COLD_ITERS} claimed \
         register/release cycles < required {COLD_MIN_REGISTRATIONS} (95%)"
    );
    assert!(
        reg_delta > 0 && unlink_delta * 100 >= reg_delta * 95,
        "cold release oracle: successful route unlinks {unlink_delta}/{reg_delta} registrations <95%"
    );
    assert_eq!(census_removed, unlink_delta, "per-cycle census reductions must match successful unlink counter");
    assert!(cycle_registrations >= COLD_MIN_REGISTRATIONS, "cold claimed cycles did not activate enough route registrations");
}

/// Fixture route on a dedicated aligned reservation; dropping `FxFixture`
/// drops the `RouteRegistration` (production unlink: `linked = false` under
/// the shard lock) and releases the reservation.
struct FxFixture {
    _reservation: aligned_vmem::Reservation,
    #[allow(dead_code)] // kept for documentation symmetry; unlink happens in drop
    registration: sefer_alloc::registry::segment_route::RouteRegistration<'static>,
}

/// Probe-side REGISTRATION counter: counts actual `directory.register()`
/// calls (module header §6) — the honest `route_registrations`, not a
/// census.
struct RegCounter(usize);

fn make_fixture(
    directory: &'static RouteDirectory,
    owner: usize,
    regs: &mut RegCounter,
) -> FxFixture {
    // SEGMENT-aligned, SEGMENT-sized real reservation (aligned_vmem exists in
    // BOTH shoulders; verified `git show bbc3c7b4:crates/aligned-vmem`).
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    regs.0 += 1;
    // SAFETY (transmute-free shape): the registration borrows the
    // process-global directory, which outlives the probe; erasing the
    // lifetime is sound because the directory is `'static` and the
    // registration is dropped inside this probe before process exit.
    let registration = unsafe {
        std::mem::transmute::<
            sefer_alloc::registry::segment_route::RouteRegistration<'_>,
            sefer_alloc::registry::segment_route::RouteRegistration<'static>,
        >(directory.register(root, SEGMENT, root, owner, RouteKind::Small).unwrap())
    };
    FxFixture {
        _reservation: reservation,
        registration,
    }
}

/// `deadpins` (module headers §5–§7): fill the per-thread pin cache with
/// pins of routes, release (unlink) the routes while their pins stay
/// cached, measure the retention, then trigger the production sweep and
/// show it reclaimed. NOTHING is dropped before the caller's MEM snapshot:
/// the remaining fixtures/pins stay alive so the MEM line describes the
/// exact state the deadpins checkpoints reported.
fn mode_deadpins(
    base: (usize, usize, usize, usize, usize),
    pin_base: (u64, u64, u64, u64, u64, u64, u64),
) -> String {
    // Dedicated directory instance (same shape as tests/b4_r1_pin_cache.rs
    // zombie fixture); the pin cache itself is process-global TLS and is
    // shared, which is what we measure.
    let directory: &'static RouteDirectory = Box::leak(Box::new(RouteDirectory::new()));
    let mut regs = RegCounter(0);

    // Phase 1: register DEADPIN_ROUTES Small fixtures, insert the first
    // PIN_CACHE_SLOTS pins through the PRODUCTION insert path, keep the rest
    // as plain handles.
    let mut fixtures: Vec<FxFixture> = Vec::new();
    let mut held_pins: Vec<RoutePin> = Vec::new();
    for i in 0..DEADPIN_ROUTES {
        fixtures.push(make_fixture(directory, 0x0C_FE0 + i, &mut regs));
        let pin = directory.lookup(fixtures[i]._reservation.as_ptr()).unwrap();
        assert!(pin.linked(), "fixture route {i} must be linked after register");
        if i < PIN_CACHE_SLOTS {
            // Production insert path (same code the foreign-free hit path
            // uses); occupies one of the 4 slots.
            pin.dbg_cache_insert_for_test();
        } else {
            held_pins.push(pin);
        }
    }
    eprintln!(
        "deadpins: checkpoint phase-1 {DEADPIN_ROUTES} fixture routes registered \
         (probe register() calls so far={}), {PIN_CACHE_SLOTS} pins inserted via \
         production insert path",
        regs.0
    );

    // Phase 2: unlink (release) the first PIN_CACHE_SLOTS routes while their
    // pins stay cached. Production unlink = drop of the RouteRegistration.
    for f in fixtures.drain(..PIN_CACHE_SLOTS) {
        drop(f);
    }
    // Verify the unlink took: the held (uncached) pins belong to still-live
    // routes (never dropped), so they must still observe linked == true.
    for pin in &held_pins {
        assert!(pin.linked(), "uncached fixture route must still be linked");
    }

    // Phase 3: measure retention of the dead cached pins (before sweep).
    let totals_dead = RoutePin::dbg_route_pin_cache_totals();
    let retained_dead = retained_pins(pin_base);
    let sidecar_dead = {
        let (a, f, _ac, _fc) = sidecar_delta(base);
        a.saturating_sub(f)
    };
    // The sweep has NOT run yet: nothing dropped the dead pins (no insert,
    // no hit on their bases, no thread exit) → all 4 must still be retained.
    assert_eq!(
        retained_dead, PIN_CACHE_SLOTS as i64,
        "expected {PIN_CACHE_SLOTS} dead pins still retained before sweep, got {retained_dead}"
    );
    let (small_live, prim_live, large_live) = directory.live_route_census_for_test();
    eprintln!(
        "deadpins: checkpoint phase-3 (pre-sweep) retained={retained_dead} \
         sidecar_bytes={sidecar_dead} LIVE-CENSUS small={small_live} \
         primordial={prim_live} large={large_live} (census is NOT \
         route_registrations)"
    );
    assert_eq!(
        small_live, DEADPIN_ROUTES - PIN_CACHE_SLOTS,
        "dead routes must be unlinked from the directory (census {small_live})"
    );

    // Phase 4: trigger the production sweep — the NEXT insert sweeps dead
    // slots first (`cache_insert` → `sweep_dead_slots`).
    // Register the sweep fixture FIRST and snapshot sidecar bytes BEFORE the
    // insert (the sweep runs inside the insert), so the mid→after delta is
    // EXACTLY the dead sidecars freed — no new-sidecar credit mixed in.
    fixtures.push(make_fixture(directory, 0x0C_FE0 + 99, &mut regs));
    let sidecar_mid = {
        let (a, f, _ac, _fc) = sidecar_delta(base);
        a.saturating_sub(f)
    };
    let sweep_pin = directory
        .lookup(fixtures.last().unwrap()._reservation.as_ptr())
        .unwrap();
    sweep_pin.dbg_cache_insert_for_test();
    let totals_swept = RoutePin::dbg_route_pin_cache_totals();
    let retained_after = retained_pins(pin_base);
    // DENOMINATOR (module header §5): the sweep counter counts PER-PIN
    // reclaims, so its delta IS the actual number of pins the sweep freed.
    let freed_pins = totals_swept.6 - totals_dead.6;
    eprintln!(
        "deadpins: checkpoint phase-4 (post-sweep) retained={retained_after} \
         sweep_freed_pins(counter PIN_CACHE_SWEEPS delta)={freed_pins} \
         sidecar_bytes_mid(before sweep insert)={sidecar_mid}"
    );
    assert_eq!(
        retained_after, 1,
        "sweep must reclaim the {PIN_CACHE_SLOTS} dead pins, leaving only the inserted one"
    );
    assert_eq!(
        freed_pins, PIN_CACHE_SLOTS as u64,
        "sweep counter delta must equal the number of dead cached pins"
    );
    let sidecar_after = {
        let (a, f, _ac, _fc) = sidecar_delta(base);
        a.saturating_sub(f)
    };
    let dead_bytes = sidecar_mid.saturating_sub(sidecar_after);
    // Per-freed-pin cost over the ACTUAL freed-pin counter (4), not the old
    // (freed − newly-inserted) conflation.
    assert_eq!(
        dead_bytes % freed_pins as usize,
        0,
        "dead sidecar bytes {dead_bytes} must divide evenly by freed pins {freed_pins}"
    );
    let per_freed = dead_bytes / freed_pins as usize;
    eprintln!(
        "deadpins: checkpoint phase-4 sidecar mid={sidecar_mid} after={sidecar_after} \
         dead_route_bytes={dead_bytes} dead_route_bytes_per_freed_pin={per_freed}"
    );
    // Keep ALL fixtures/pins alive to the MEM snapshot (module header §7).
    std::hint::black_box((&fixtures, &held_pins));
    eprintln!(
        "deadpins: checkpoint final fixtures kept ALIVE to the MEM snapshot; \
         route_registrations={} (probe register() call count, not a census)",
        regs.0
    );
    format!(
        " route_registrations={}(probe register() calls: 6 fixtures + 1 sweep) \
         retained_pins_after_sweep={retained_after} sweep_freed_pins={freed_pins} \
         dead_route_bytes={dead_bytes} dead_route_bytes_per_freed_pin={per_freed}",
        regs.0
    )
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: zz_oc_mem_probe <full|sparse|cold|deadpins>");
        std::process::exit(2);
    });

    // FIX (review 1): baseline System-sidecar totals BEFORE bootstrap/claim —
    // symmetric across shoulders (primordial bootstrap sidecars counted in
    // the delta on BOTH arms).
    let base = SmallSidecar::system_totals_for_test();
    let pin_base = RoutePin::dbg_route_pin_cache_totals();

    let conflicts_before = config_conflicts_total();
    let _ = bootstrap::ensure();

    let mut lease = HeapRegistry::dbg_claim_lease_with_config(config_for(&mode))
        .unwrap_or_else(|| panic!("dbg_claim_lease_with_config returned None"));
    let heap: &mut HeapCore = lease.core();

    // R26-4 config evidence: the FIRST (and only) claim must have resolved to
    // exactly the expected pool segments for this mode (cold disables the
    // pool). Resolved byte bound from the mem4 hook on the SAME heap.
    let resolved = heap.dbg_pool_cap();
    let expected = expected_pool_cap(&mode);
    assert_eq!(
        resolved, expected,
        "resolved pool cap {resolved} != expected {expected} for mode {mode}"
    );
    let resolved_byte_cap = heap.dbg_pool_byte_cap_effective();
    assert_eq!(
        resolved_byte_cap,
        resolved * SEGMENT,
        "resolved byte bound {resolved_byte_cap} != pool_cap({resolved}) × SEGMENT"
    );

    let mut extra = String::new();
    match mode.as_str() {
        "full" => mode_full(heap),
        "sparse" => mode_sparse(heap),
        "cold" => mode_cold(heap, base),
        "deadpins" => extra = mode_deadpins(base, pin_base),
        other => {
            eprintln!("unknown mode {other:?} (want full|sparse|cold|deadpins)");
            std::process::exit(2);
        }
    }

    // Quiescent checkpoint (single thread, T=1 — totals are exact here),
    // taken on the WORKLOAD heap and BEFORE any fixture drop (deadpins keeps
    // its fixtures alive — module header §7).
    let (alloc_bytes, free_bytes, _alloc_calls, _free_calls) = sidecar_delta(base);
    let sidecar_bytes = alloc_bytes.saturating_sub(free_bytes);
    let retained = retained_pins(pin_base);
    let rec = heap.dbg_segment_state_reconciliation();
    let conflicts_delta = config_conflicts_total() - conflicts_before;

    // Hard contract (J4/R26-4): zero config conflicts in a fresh process.
    assert_eq!(
        conflicts_delta, 0,
        "CONFIG_CONFLICTS delta {conflicts_delta} != 0 — arm invalid"
    );

    mem_line(
        rec.total.committed_bytes as u64,
        sidecar_bytes,
        retained,
        rss_kib(),
        conflicts_delta,
        resolved,
        resolved_byte_cap,
        &extra,
    );
}
