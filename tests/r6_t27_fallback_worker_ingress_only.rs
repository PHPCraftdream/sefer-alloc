//! R6-T27 regression: the maintenance worker must run only the ingress-only
//! fallback step while the shared fallback heap shows external lock activity,
//! and resume the full cold trim (tcache flush, pool drain, evict) only after
//! an idle window. On the pre-R6-T27 code the worker flushed the tcache on
//! every ~10 ms pass, so a magazine refilled by frees while the fallback heap
//! was "busy" was drained again within one pass — the ACTIVE-phase asserts
//! below go red against that behavior.
//!
//! ACTIVE (>=150 real fallback visits, one poke per pass): catches the old
//! unconditional per-pass trim AND a "trim after `IDLE_VISITS` successful
//! visits ignoring activity" mutant — the phase is forced past 100 counted
//! successful fallback acquisitions while external activity continues, so
//! only activity-aware accounting keeps the magazine alive.
//!
//! HELD (fallback lock held across >=150 acked worker passes, magazine
//! re-checked under the hold and 2 passes after release): a mutant whose idle
//! counter keeps growing across failed (`None`) visits reaches the trim
//! threshold during the hold and flushes the magazine on the first visit
//! after release; with correct accounting every failed visit resets the
//! streak, so a full fresh idle window is needed and the magazine survives.
//!
//! IDLE (>=120 quiet fallback visits, then one reading): a mutant that never
//! trims goes red here. This phase proves an eventual idle trim only; it does
//! not pin the lower bound of the idle window.

#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::time::{Duration, Instant};

use sefer_alloc::global::MaintenanceService;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::{AllocCore, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Small block classes: unused by the rest of this test binary, at least
/// 16 bytes so a usable `Layout` exists for each.
const CANDIDATES: [usize; 3] = [26, 27, 28];
/// Mirrors `FALLBACK_IDLE_VISITS` in `src/global/maintenance_service.rs`
/// (private there; duplicated here so no public API is added for tests).
const IDLE_VISITS: u64 = 100;
/// ACTIVE phase target: 1.5x the threshold in REAL fallback visits (not
/// passes), so a worker that "trims after `IDLE_VISITS` successful visits
/// regardless of activity" is forced past its trigger inside this phase.
const ACTIVE_VISITS: u64 = (3 * IDLE_VISITS) / 2;
/// HELD phase: hold the fallback lock across this many acked worker passes;
/// every pass's fallback visit fails (`None`) while the lock is held.
const HELD_PASSES: u64 = (3 * IDLE_VISITS) / 2;
/// IDLE phase target: threshold + slack in real fallback visits before the
/// single reading poke, so the idle streak must have fully re-accumulated.
const IDLE_TARGET_VISITS: u64 = IDLE_VISITS + 20;
/// Phase budgets (seconds). Sum <= ~30 s; a stalled worker fails fast.
const ACTIVE_DEADLINE: Duration = Duration::from_secs(12);
const HELD_DEADLINE: Duration = Duration::from_secs(8);
const IDLE_DEADLINE: Duration = Duration::from_secs(8);

/// Wait for exactly one more real worker pass, bounded by `deadline`.
/// Returns false on timeout; the caller asserts with a message.
fn wait_one_pass(deadline: Instant) -> bool {
    let before = MaintenanceService::passes_for_test();
    let remaining = deadline.saturating_duration_since(Instant::now());
    remaining > Duration::ZERO && MaintenanceService::wait_after_for_test(before, remaining)
}

#[test]
fn fallback_worker_ingress_only_while_active() {
    SeferAlloc::start_maintenance().expect("activation");

    // Pick a class with an empty magazine on the fallback heap.
    let picked = HeapCore::dbg_with_fallback_for_test(|heap| {
        CANDIDATES
            .iter()
            .copied()
            .find(|&c| heap.dbg_tcache_count(c) == 0 && AllocCore::dbg_block_size(c) >= 16)
    })
    .expect("fallback heap available");
    let class = picked.expect("no untouched candidate class with a >=16-byte block size");
    let bs = AllocCore::dbg_block_size(class);
    let layout = Layout::from_size_align(bs, 16).expect("layout");

    // Fill the magazine: alloc 4, free 2 back through the global allocator.
    // The deallocs must happen in a SEPARATE closure: GlobalAlloc::dealloc on
    // a fallback-owned block takes the fallback lock, and the closure already
    // holds it.
    let live = HeapCore::dbg_with_fallback_for_test(|heap| {
        let ptrs: Vec<*mut u8> = (0..4).map(|_| heap.alloc(layout)).collect();
        assert!(ptrs.iter().all(|p| !p.is_null()));
        ptrs
    })
    .expect("fallback heap available");
    for ptr in &live[2..4] {
        unsafe { GlobalAlloc::dealloc(&GLOBAL, *ptr, layout) };
    }
    HeapCore::dbg_with_fallback_for_test(|heap| {
        assert!(
            heap.dbg_tcache_count(class) > 0,
            "magazine should hold the two freed blocks"
        );
    })
    .expect("fallback heap available");

    // ACTIVE phase: poke the magazine each pass. Each poke takes the fallback
    // lock, which counts as external activity and keeps the worker on the
    // ingress-only step — the count must survive every pass.
    let active_deadline = Instant::now() + ACTIVE_DEADLINE;
    let mut visits_before = MaintenanceService::fallback_visits_for_test();
    let mut visits = 0u64;
    while visits < ACTIVE_VISITS {
        assert!(
            wait_one_pass(active_deadline),
            "worker pass did not complete"
        );
        let count = HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_tcache_count(class))
            .expect("fallback heap available");
        assert!(count > 0, "magazine drained while fallback was active");
        // One poke per pass = external activity. Count REAL fallback visits,
        // not passes: only they advance the worker's idle/trim trigger.
        visits = MaintenanceService::fallback_visits_for_test() - visits_before;
    }

    // HELD phase: regression for the skipped-acquisition case. While one
    // closure holds the fallback lock, every worker visit returns `None` and
    // must reset the idle streak; a mutant that lets `fallback_idle` grow
    // across failed visits reaches the trim threshold during the hold and
    // flushes the magazine on the first visit after release — the
    // post-release assert below goes red there. With the fixed accounting,
    // after release the worker needs a FULL fresh idle window before
    // trimming, so the magazine survives 1-2 passes.
    let held_deadline = Instant::now() + HELD_DEADLINE;
    let mut held_passes = 0u64;
    // dbg_with_fallback_for_test holds the fallback spinlock for the whole
    // closure (it delegates to fallback::with_heap), so every worker visit
    // inside fails its try-lock. Worker passes are still acked while the
    // lock is held, so pass acknowledgements work as a progress signal here.
    HeapCore::dbg_with_fallback_for_test(|heap| {
        // Sanity: the magazine is still populated entering the hold.
        assert!(
            heap.dbg_tcache_count(class) > 0,
            "magazine empty before HELD"
        );
        while held_passes < HELD_PASSES {
            assert!(
                wait_one_pass(held_deadline),
                "worker pass did not complete while lock held"
            );
            held_passes += 1;
        }
        // Re-read under the same hold: nothing could have flushed it.
        assert!(
            heap.dbg_tcache_count(class) > 0,
            "magazine drained while lock was held"
        );
    })
    .expect("fallback heap available");
    // Release, then give the worker 2 passes: with correct accounting the
    // idle streak was reset on every failed visit during the hold, so the
    // cold trim cannot fire yet. A mutant that accumulated idle across the
    // failed visits trims on the very first post-release visit.
    for _ in 0..2 {
        assert!(
            wait_one_pass(held_deadline),
            "worker pass did not complete after release"
        );
    }
    let count = HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_tcache_count(class))
        .expect("fallback heap available");
    assert!(
        count > 0,
        "magazine drained right after lock release: idle grew across skipped visits"
    );

    // IDLE phase: no pokes until the target is reached; count real fallback
    // visits, not time. Lock-order note: the test thread here holds the
    // fallback lock only inside `dbg_with_fallback_for_test` closures and
    // takes CONTROL (via `wait_after_for_test`) never while holding it in a
    // direction the worker reverses — the worker takes CONTROL only after
    // its try-lock returned (maintenance_service.rs), so no inversion.
    let idle_deadline = Instant::now() + IDLE_DEADLINE;
    visits_before = MaintenanceService::fallback_visits_for_test();
    // No pokes: every successful visit is a quiet one. Wait until a full
    // idle window PLUS slack of real fallback visits has elapsed (time-based
    // waits would assume the 10 ms period), then take exactly ONE reading.
    loop {
        assert!(
            wait_one_pass(idle_deadline),
            "worker stopped passing during idle phase"
        );
        let quiet_visits = MaintenanceService::fallback_visits_for_test() - visits_before;
        if quiet_visits >= IDLE_TARGET_VISITS {
            break;
        }
    }
    let count = HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_tcache_count(class))
        .expect("fallback heap available");
    assert_eq!(
        count, 0,
        "cold trim did not drain the idle fallback magazine"
    );
}
