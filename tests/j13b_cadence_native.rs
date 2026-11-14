//! Real Auto controls drive the adaptive cadence: the level rises while
//! controls agree with the estimate, thins the actual selections to one per
//! `8 * 2^level` window, and snaps back to 1-in-8 on the first disagreement.
//! One scenario per binary: the cadence state is process-global.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpGradient, LargeThpMode};
use std::alloc::Layout;

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const GROUP: usize = 2;
const NO_LEVEL: u8 = 0xFF;

/// One consumed ticket: ordinal, selected, logged level (`NO_LEVEL` for a
/// non-candidate).
type Ticket = (u64, bool, u8);

fn cadence() -> [u64; 5] {
    AllocCore::dbg_thp_cadence_telemetry()[GROUP]
}

/// One aligned cohort of eight fresh requests, dense or untouched, then
/// freed on the owner thread. Checks every actual ticket against the oracle.
fn round(core: &mut AllocCore, dense: bool) -> Vec<Ticket> {
    let before = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(before % 8, 0, "rounds must open on an aligned cohort");
    AllocCore::dbg_thp_control_ticket_log_reset();
    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");
    let page = aligned_vmem::page_size();
    let mut blocks = [core::ptr::null_mut(); 8];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null());
        if dense {
            for offset in (0..BLOCK).step_by(page) {
                // SAFETY: live allocation; the offset is inside its payload.
                unsafe { block.add(offset).write_volatile(0x5a) };
            }
        }
    }
    for block in blocks {
        // SAFETY: each block is freed once with its exact layout.
        unsafe { core.dealloc(block, layout) };
    }
    assert_eq!(AllocCore::dbg_thp_control_ordinal() - before, 8);
    let (entries, cursor) = AllocCore::dbg_thp_control_ticket_log().expect("supported capture");
    assert_eq!(cursor, 8);
    let levels = AllocCore::dbg_thp_control_level_log();
    let tickets: Vec<Ticket> = (0..8)
        .map(|slot| (entries[slot] >> 2, entries[slot] & 2 != 0, levels[slot]))
        .collect();
    for (index, (ordinal, selected, level)) in tickets.iter().enumerate() {
        assert_eq!(*ordinal, before + index as u64);
        let candidate = AllocCore::dbg_thp_control_decision_at(*ordinal);
        assert_eq!(
            candidate,
            *level != NO_LEVEL,
            "only fixed-selector candidates read a level"
        );
        if candidate {
            assert_eq!(
                *selected,
                AllocCore::dbg_thp_control_select_for_ordinal(*ordinal, u32::from(*level)),
                "actual decision must equal the oracle at the logged level"
            );
        } else {
            assert!(!selected, "a non-candidate is never a control");
        }
    }
    tickets
}

fn controls(tickets: &[Ticket]) -> usize {
    tickets.iter().filter(|ticket| ticket.1).count()
}

fn candidate_levels(tickets: &[Ticket]) -> Vec<u8> {
    tickets
        .iter()
        .filter(|ticket| ticket.2 != NO_LEVEL)
        .map(|ticket| ticket.2)
        .collect()
}

#[test]
fn level_rises_under_agreeing_controls_thins_selections_and_resets_on_disagreement() {
    let config = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Auto)
        .huge_page_block_min_bytes(0)
        .huge_page_gradient(LargeThpGradient::CAUTIOUS);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    assert_eq!(AllocCore::dbg_thp_tuple().mode, LargeThpMode::Auto);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    core.dbg_set_large_cache_budget(Some(0));
    assert_eq!(cadence(), [0; 5], "a never-observed group starts at run 0");

    // Four dense controls from the optimistic prior all agree: level 0 keeps
    // today's exact 1-in-8 cohorts, one control per round.
    for expected_run in 0..4u64 {
        assert_eq!(cadence()[4], expected_run);
        let tickets = round(&mut core, true);
        assert_eq!(controls(&tickets), 1, "level 0 is the fixed selector");
        assert_eq!(candidate_levels(&tickets), [0]);
    }
    assert_eq!(cadence()[4], 4, "four agreements reach level one");

    // Level one: exactly one control per aligned 16 ordinals, so four rounds
    // (two super-cohorts) hold exactly two controls, each candidate at level 1.
    let mut selected = 0;
    for _ in 0..4 {
        let tickets = round(&mut core, true);
        assert!(candidate_levels(&tickets).iter().all(|level| *level == 1));
        selected += controls(&tickets);
    }
    assert_eq!(selected, 2, "level one thins to one control per 16");
    let state = cadence();
    assert_eq!(state[4], 6, "two more agreements, never above the run");
    let counters = AllocCore::dbg_thp_cadence_level_counters();
    assert_eq!(counters[0][GROUP][..2], [4, 4], "candidates per level");
    assert_eq!(counters[1][GROUP][..2], [4, 2], "selections per level");
    assert_eq!(
        &state[..4],
        &[64, 6, 0, 0],
        "eligible, agreed, resets, dropped"
    );

    // Untouched controls disagree with the dense estimate: the first one
    // clears the run, and the very next candidate is back at level 0.
    let mut rounds = 0;
    while cadence()[2] == 0 {
        assert!(rounds < 4, "a level-one cadence must reach a control");
        let tickets = round(&mut core, false);
        assert!(candidate_levels(&tickets).iter().all(|level| *level == 1));
        rounds += 1;
    }
    assert_eq!(cadence()[4], 0, "a disagreement clears the run at once");
    let tickets = round(&mut core, false);
    assert_eq!(candidate_levels(&tickets), [0], "next candidate is level 0");
    assert_eq!(
        controls(&tickets),
        1,
        "level 0 always selects its candidate"
    );
    assert_eq!(cadence()[3], 0, "no agreement was dropped single-threaded");

    // Other request groups never saw a control.
    let telemetry = AllocCore::dbg_thp_cadence_telemetry();
    let counters = AllocCore::dbg_thp_cadence_level_counters();
    for group in (0..6).filter(|group| *group != GROUP) {
        assert_eq!(telemetry[group], [0; 5]);
        assert_eq!(counters[0][group], [0; 6]);
        assert_eq!(counters[1][group], [0; 6]);
    }
    drop(core);
}
