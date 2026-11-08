//! J13b control telemetry: concurrent competing eligible allocations through
//! the installed GlobalAlloc, with the runtime's ACTUAL tickets and
//! decisions observed through the bounded bench-internals diagnostic ticket
//! log. One scenario per binary.
//!
//! Oracle: aggregate telemetry alone does NOT prove ticket uniqueness or
//! per-cohort placement. The discriminating oracles read what the runtime
//! actually consumed, not ideal replays:
//! (1) the captured ticket records are exactly the consumed ordinal window
//!     `[before, after)`, pairwise distinct — a duplicated, lost, or folded
//!     ticket (e.g. feeding the selector the fetch_add return divided by the
//!     cohort length, so ordinals repeat 0..7 across cohorts) changes the
//!     captured multiset even though the counter delta and the aggregate
//!     selection count stay unchanged;
//! (2) every captured decision equals the fixed-seed pure selector evaluated
//!     at that captured ticket (`dbg_thp_control_decision_at`), and the
//!     per-cohort census over the ACTUAL captured decisions holds exactly
//!     one control per complete aligned cohort — seed, input, or placement
//!     drift turns this red;
//! (3) closure: the number of true captured decisions equals the telemetry
//!     selections (g2[0] == 8), tying the actual decisions to the counters.
//!
//! The pure standalone selector tests and sequential allocations do NOT
//! cover the competing read-modify-write property this binary does: here the
//! ordinal fetch_add, the decision, and the ticket-log record all run under
//! genuine concurrent contention. Valid telemetry: every selected control is
//! owner-freed and sampled exactly once; premise failures 0 and errors 0 on
//! the healthy native path. Every OTHER group must be all zeros (bootstrap
//! and heap materialization allocate only non-eligible spans).

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Barrier;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 16 * MIB;
const THREADS: usize = 4;
const PER_THREAD: usize = 16;
const TOTAL: usize = THREADS * PER_THREAD; // 64 = 8 complete aligned cohorts
const COHORTS: usize = TOTAL / 8;
const GROUP_16_TO_64_MIB: usize = 2;

#[test]
fn concurrent_eligible_allocations_select_exactly_one_control_per_cohort() {
    // Bootstrap the process tuple through an explicit config, then drop it.
    drop(
        AllocCore::new_with_config(LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto))
            .expect("primordial bootstrap"),
    );

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 16 MiB layout");
    let barrier = Barrier::new(THREADS);

    // Quiescent capture: reset before the threads spawn (no control
    // decisions in flight) and snapshot only after the join below; the
    // fresh 64-record window stays strictly below the log cap (128).
    AllocCore::dbg_thp_control_ticket_log_reset();
    let ordinal_before = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(
        ordinal_before % 8,
        0,
        "the consumed ordinal window must open on an aligned cohort boundary \
         (bootstrap consumes no eligible ordinals)"
    );

    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                barrier.wait();
                let mut blocks = [core::ptr::null_mut(); PER_THREAD];
                for block in &mut blocks {
                    // SAFETY: valid non-zero layout; the returned pointer is retained.
                    *block = unsafe { GLOBAL.alloc(layout) };
                    assert!(!block.is_null(), "16 MiB allocation must succeed");
                    // SAFETY: in-bounds byte of the owned allocation.
                    unsafe { block.write_volatile(0x5a) };
                }
                // Free the thread's OWN blocks in reverse: owner-free sampling
                // happens on the allocating thread's heap.
                for block in blocks.iter().rev() {
                    // SAFETY: each pointer is live with the allocation layout.
                    unsafe { GLOBAL.dealloc(*block, layout) };
                }
            });
        }
    });

    let ordinal_after = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(
        ordinal_after - ordinal_before,
        TOTAL as u64,
        "exactly {TOTAL} next_control calls consumed the window: a skipped or \
         extra call changes this total"
    );

    // --- Oracle 1: the ACTUAL consumed tickets.
    let (log, cursor) = AllocCore::dbg_thp_control_ticket_log().expect(
        "the fresh 64-record window must be fully supported capture: no \
         truncated ticket and no record dropped past the cap",
    );
    assert_eq!(
        cursor, TOTAL as u64,
        "every consumed ticket was captured within the bounded diagnostic log"
    );
    let mut tickets = [0u64; TOTAL];
    let mut decided = [false; TOTAL];
    for i in 0..TOTAL {
        let entry = log[i];
        assert!(
            entry & 1 == 1,
            "ticket record {i} must carry the valid bit; 0 is the explicit \
             invalid marker for top-truncated capture"
        );
        tickets[i] = entry >> 2;
        decided[i] = entry & 2 != 0;
    }
    // Pairwise-distinct set equality with the window: sorted captured
    // tickets must be exactly [before, before + 64). A folded ticket
    // (`fetch_add(...) / 8`-style mutant) repeats ordinals and is caught
    // here even though the delta, the pure replay, and the aggregate
    // selection count all stay unchanged.
    let mut sorted = tickets;
    sorted.sort_unstable();
    for (i, ticket) in sorted.iter().enumerate() {
        assert_eq!(
            *ticket,
            ordinal_before + i as u64,
            "captured ticket {ticket} breaks window distinctness: the captured \
             tickets must be exactly the consumed window [{ordinal_before}, \
             {ordinal_after}), pairwise distinct"
        );
    }

    // --- Oracle 2: the ACTUAL decisions, per ticket and per cohort.
    let mut per_cohort = [0u32; COHORTS];
    let mut true_decisions = 0u32;
    for i in 0..TOTAL {
        assert_eq!(
            decided[i],
            AllocCore::dbg_thp_control_decision_at(tickets[i]),
            "the decision the runtime actually consumed for ticket {} must be \
             the fixed-seed pure selector's decision, not an assumed replay",
            tickets[i]
        );
        let cohort = (tickets[i] - ordinal_before) as usize / 8;
        per_cohort[cohort] += u32::from(decided[i]);
        true_decisions += u32::from(decided[i]);
    }
    for (cohort, count) in per_cohort.iter().enumerate() {
        assert_eq!(
            *count, 1,
            "actual cohort {cohort} of the consumed window holds exactly one \
             control decision regardless of thread interleaving"
        );
    }

    // --- Oracle 3: closure against telemetry.
    let group2 = AllocCore::dbg_thp_control_telemetry()[GROUP_16_TO_64_MIB];
    assert_eq!(
        true_decisions, 8,
        "the actual captured decisions total eight controls"
    );
    assert_eq!(
        group2[0], 8,
        "64 eligible requests form exactly 8 complete aligned cohorts; the \
         runtime's actual selections match the captured decisions"
    );
    assert_eq!(
        group2[1], 0,
        "no premise failures on the healthy native path"
    );
    assert_eq!(
        group2[2], 8,
        "every selected control was owner-freed and observed exactly once"
    );
    assert_eq!(group2[3], 0, "no remote-reclaim invalidations");
    assert_eq!(group2[4], 0, "no realloc invalidations");
    assert_eq!(group2[5], 0, "no errors on the healthy native path");
    assert!(group2[6] > 0, "owner-freed controls report resident pages");
    assert!(group2[7] > 0, "owner-freed controls sample payload pages");

    let learned = AllocCore::dbg_thp_learning_telemetry();
    let learning = learned[GROUP_16_TO_64_MIB];
    assert_eq!(
        learning[3] + learning[4],
        COHORTS as u64,
        "after join every valid control was applied or explicitly dropped, never lost"
    );
    assert!(
        learning[3] > 0,
        "bounded strong CAS must admit at least one observation"
    );
    assert!(learning[0] < u64::from(sefer_alloc::LargeThpShare::SCALE));
    for (group, state) in learned.iter().enumerate() {
        if group != GROUP_16_TO_64_MIB {
            assert_eq!(&state[3..], &[0, 0], "other groups must not be trained");
        }
    }

    let telemetry = AllocCore::dbg_thp_control_telemetry();
    for (group, counters) in telemetry.iter().enumerate() {
        assert!(
            group == GROUP_16_TO_64_MIB || *counters == [0; 8],
            "every other group must be all zeros, got group {group}: {counters:?}"
        );
    }
}
