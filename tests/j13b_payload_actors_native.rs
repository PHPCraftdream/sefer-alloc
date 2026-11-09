//! Real facade actor ownership: System bookkeeping, no campaign timing.
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
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_payload_actors.rs"]
mod payload_actors;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use payload_actors::PayloadActors;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};
use workload::{Shape, TouchPlan};

#[test]
fn real_actors_return_owner_samples_and_reject_remote_training_in_both_directions() {
    const MIB: usize = 1024 * 1024;
    const GROUP: usize = 1;
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && page < MIB);
    let bytes = 8 * MIB - page;
    let allocator = SeferAlloc::with_config(
        LargeCacheConfig::new()
            .budget_bytes(0)
            .huge_page_mode(LargeThpMode::Auto)
            .huge_page_block_min_bytes(bytes)
            .huge_page_budget_bytes(0),
    );
    let plans = [TouchPlan {
        bytes,
        shape: Shape::Sparse,
    }; 8];
    let initial = AllocCore::dbg_thp_control_telemetry();
    let learning = AllocCore::dbg_thp_learning_telemetry();
    PayloadActors::with(
        &allocator,
        2,
        page,
        |_, actual| {
            assert_eq!(actual.dbg_current_large_cache_budget(), Some(0));
            assert_eq!(actual.stats().config_conflicts, 0);
            // On the true owner actor, consume prior foreign retirements before
            // checking quiescent counters. This never samples a live foreign mapping.
            actual.dbg_trim_current_thread();
            Ok(())
        },
        |actors| {
            let mut identities = [None, None];
            for (owner, toucher) in [(0, 1), (1, 0)] {
                let (allocated, owner_thread) = actors.allocate(owner, &plans)?;
                assert_eq!(allocated.allocations, 8);
                identities[owner] = Some(owner_thread);
                let (from, to) = actors.transfer(owner, toucher)?;
                assert_eq!(from, owner_thread);
                assert_ne!(from, to);
                let (touched, touching_thread) = actors.touch(toucher)?;
                assert_eq!(touching_thread, to);
                assert_eq!(touched.bytes_written, 8);
                assert_eq!(actors.transfer(toucher, owner)?, (to, from));
                let (freed, retiring_thread) = actors.free(owner)?;
                assert_eq!(retiring_thread, owner_thread);
                assert_eq!(freed.frees, 8);
            }
            let owner_rows = AllocCore::dbg_thp_control_telemetry();
            let learned = AllocCore::dbg_thp_learning_telemetry();
            assert_eq!(owner_rows[GROUP][2] - initial[GROUP][2], 2);
            assert_eq!(learned[GROUP][3] - learning[GROUP][3], 2);
            assert!(learned[GROUP][0] < learning[GROUP][0]);
            for (owner, recipient) in [(0, 1), (1, 0)] {
                assert_eq!(
                    actors.allocate(owner, &plans)?.1,
                    identities[owner].unwrap()
                );
                actors.transfer(owner, recipient)?;
                assert_eq!(actors.touch(recipient)?.0.bytes_written, 8);
                assert_eq!(actors.free(recipient)?.1, identities[recipient].unwrap());
                assert_eq!(
                    AllocCore::dbg_thp_learning_telemetry(),
                    learned,
                    "foreign retirement cannot supply owner evidence"
                );
                // A counted real small allocation binds the owner request; preflight
                // then drains its foreign queue on that same persistent actor.
                actors.allocate(
                    owner,
                    &[TouchPlan {
                        bytes: 32,
                        shape: Shape::Dense,
                    }],
                )?;
                actors.free(owner)?;
                assert_eq!(AllocCore::dbg_thp_learning_telemetry(), learned);
            }
            let final_rows = AllocCore::dbg_thp_control_telemetry();
            assert_eq!(final_rows[GROUP][0] - initial[GROUP][0], 4);
            assert_eq!(final_rows[GROUP][2] - initial[GROUP][2], 2);
            assert_eq!(final_rows[GROUP][3] - initial[GROUP][3], 2);
            for column in [1, 4, 5] {
                assert_eq!(final_rows[GROUP][column], initial[GROUP][column]);
            }
            assert_eq!(AllocCore::dbg_thp_control_ordinal(), 32);
            assert_eq!(AllocCore::dbg_thp_tuple().mode, LargeThpMode::Auto);
            assert_eq!(AllocCore::dbg_thp_tuple().min_bytes, bytes);
            assert_eq!(AllocCore::dbg_thp_tuple().budget_bytes, 0);
            assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
            Ok(())
        },
    )
    .expect("persistent actual payload ownership");
    assert_eq!(allocator.stats().config_conflicts, 0);
}
