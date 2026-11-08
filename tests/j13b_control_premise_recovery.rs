//! J13b control telemetry: invalid NOHUGEPAGE premise failure, its
//! failed-premise token consumed by realloc (counted as premise, exactly
//! once — never as realloc-invalidation, never double-counted), then
//! recovery.
#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

fn allocate_cohort(core: &mut AllocCore, layout: Layout) -> [*mut u8; COHORT] {
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "8 MiB allocation must succeed");
        // SAFETY: `block` is the live payload pointer returned for `layout`.
        unsafe { block.write_volatile(0x5a) };
    }
    blocks
}

fn free_cohort(core: &mut AllocCore, blocks: &[*mut u8; COHORT], layout: Layout) {
    for block in blocks.iter().rev() {
        // SAFETY: each pointer is live with the original layout.
        unsafe { core.dealloc(*block, layout) };
    }
}

#[test]
fn failed_nohugepage_premise_skips_sampling_and_next_cohort_recovers() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));
    let layout = Layout::from_size_align(BLOCK, 8).expect("valid block layout");

    aligned_vmem::fault_injection::arm_fail_next_advise_range(1);
    let failed = allocate_cohort(&mut core, layout);
    aligned_vmem::fault_injection::arm_fail_next_advise_range(0);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0]
    );

    // Failed-premise token through realloc: disarm is already done. The
    // one block whose premise-failed token is 11 now records the premise
    // failure exactly once through the token-derived direct index; the 8
    // fresh 16 MiB destinations form one new group-2 cohort.
    let moved_layout = Layout::from_size_align(16 * MIB, 8).expect("valid 16 MiB layout");
    let mut moved = [core::ptr::null_mut(); COHORT];
    for (slot, block) in moved.iter_mut().zip(failed.iter()) {
        // SAFETY: live 8 MiB allocation; a successful realloc consumes it.
        *slot = unsafe { core.realloc(*block, layout, 16 * MIB) };
        assert!(!slot.is_null(), "16 MiB moving realloc must succeed");
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [2, 1, 0, 0, 0, 0, 0, 0],
        "premise token consumed by realloc is counted as premise (not as \
         realloc-invalidation) and NOT double-counted"
    );

    // Free the moved blocks with the 16 MiB layout; the copy faults the
    // destination pages so resident > 0. Premise stays 1 (no double
    // consumption of the already-consumed token).
    for block in moved.iter().rev() {
        // SAFETY: each pointer is live with the 16 MiB layout.
        unsafe { core.dealloc(*block, moved_layout) };
    }
    let after_moves = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        &after_moves[..6],
        &[2, 1, 1, 0, 0, 0],
        "the moved allocations observe once; premise stays exactly 1"
    );
    assert!(
        after_moves[6] > 0,
        "copy faults fault the destination pages"
    );
    assert!(after_moves[7] > 0, "owner-free samples payload pages");

    let recovered = allocate_cohort(&mut core, layout);
    let before_recovered_free = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        before_recovered_free[0], 3,
        "the recovery cohort selects one more control"
    );
    assert_eq!(before_recovered_free[2], 1);
    free_cohort(&mut core, &recovered, layout);
    let final_counts = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(&final_counts[..6], &[3, 1, 2, 0, 0, 0]);
    assert!(
        final_counts[7] > 0,
        "recovered valid observation samples payload pages"
    );
    drop(core);
}
