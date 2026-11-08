//! J13b control telemetry: the successful Large OOM-shrink fallback in the
//! registry realloc path must consume a pending first-life control token
//! exactly once with reason/group-correct accounting, while a truly failed
//! realloc PRESERVES a pending token (proven by the later consumption that
//! still finds it). One scenario per binary.
//!
//! Scenarios:
//! - A: a selected success-token control on an 8 MiB cohort survives a
//!   failed-move shrink realloc (8 -> 4 MiB, injected OOM) as exactly one
//!   realloc invalidation in the token's group; the fallback keeps the
//!   pointer and the recorded size.
//! - D1: with the cohort's failed-premise token still PENDING (injected
//!   MADV_NOHUGEPAGE failure at allocation), failed growth reallocs
//!   (12 -> 16 MiB, injected OOM) must leave telemetry byte-identical.
//!   Preservation is proven by the NEXT scenario, whose successful fallback
//!   still finds the token: premise failures land exactly once.
//! - B: the successful fallback shrink (12 -> 8 MiB, injected OOM) consumes
//!   the pending failed-premise token as exactly one premise failure —
//!   never as a realloc invalidation. This closes D1: a mutant that cleared
//!   pending tokens on failed growth leaves premise 0 here and is red.
//! - C: a second fallback shrink of the already-consumed cohort accounts
//!   nothing (exactly-once).
//! - D2: with a fresh cohort's success token still PENDING, failed growth
//!   reallocs (16 -> 32 MiB, injected OOM) must leave telemetry
//!   byte-identical; the subsequent consumption shrink (16 -> 8 MiB) still
//!   records exactly one realloc invalidation, closing D2 the same way.
//! - The non-selected blocks exercise the token == 0 arm: the fallback must
//!   clear nothing to account, so the exact per-scenario counts are the
//!   discriminators (a mutant accounting per-block instead of per-token
//!   makes scenarios A/B/D2-closure red; a mutant accounting a consumed
//!   token twice makes the exactly-once scenarios red).
//!
//! Every successful fallback shrink records the NEW size, and every cleanup
//! dealloc uses the layout of the LAST successful allocation for that
//! cohort (all cohorts end at 4 MiB here), honouring the GlobalAlloc
//! contract that a successful realloc establishes the new layout even when
//! the address is unchanged.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const COHORT: usize = 8;
const GROUP_MIB1_TO_8: usize = 1;
const GROUP_MIB8_TO_32: usize = 2;

#[test]
fn oom_shrink_fallback_accounts_tokens_exactly_once_and_failed_realloc_preserves() {
    // Bootstrap the process tuple through an explicit config, then drop it.
    drop(
        AllocCore::new_with_config(LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto))
            .expect("primordial bootstrap"),
    );

    let layout4 = Layout::from_size_align(4 * MIB, 8).expect("valid 4 MiB layout");
    let layout8 = Layout::from_size_align(8 * MIB, 8).expect("valid 8 MiB layout");
    let layout12 = Layout::from_size_align(12 * MIB, 8).expect("valid 12 MiB layout");
    let layout16 = Layout::from_size_align(16 * MIB, 8).expect("valid 16 MiB layout");

    // --- Scenario A: success token through the OOM-shrink fallback.
    let mut cohort_a = [core::ptr::null_mut(); COHORT];
    for block in &mut cohort_a {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout8) };
        assert!(!block.is_null(), "8 MiB allocation must succeed");
    }
    let after_alloc = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        after_alloc[GROUP_MIB8_TO_32],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "the fresh 8 MiB cohort selects exactly one control"
    );
    assert!(after_alloc
        .iter()
        .enumerate()
        .all(|(group, counters)| group == GROUP_MIB8_TO_32 || *counters == [0; 8]));

    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_a {
        // SAFETY: live owned pointer with its exact old layout; the OOM
        // fallback keeps it valid and owned at the NEW size.
        let shrunk = unsafe { GLOBAL.realloc(*block, layout8, 4 * MIB) };
        assert!(
            !shrunk.is_null(),
            "the OOM-shrink fallback must keep the block"
        );
        assert_eq!(
            shrunk, *block,
            "the fallback must keep the original pointer"
        );
        *block = shrunk;
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "the selected control consumed by the fallback is exactly one realloc \
         invalidation in the token's group"
    );

    // Every cohort_a block is now a 4 MiB allocation (the last successful
    // realloc), so cleanup uses layout4.
    for block in cohort_a.iter().rev() {
        // SAFETY: each pointer is the live 4 MiB shrunk allocation.
        unsafe { GLOBAL.dealloc(*block, layout4) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "freeing fallback-consumed controls must not observe them"
    );

    // --- Cohort B: failed-premise token still PENDING. A 12 MiB request
    // cannot hit the cached 8 MiB-class slots, so every block is fresh.
    aligned_vmem::fault_injection::arm_fail_next_advise_range(1);
    let mut cohort_b = [core::ptr::null_mut(); COHORT];
    for block in &mut cohort_b {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout12) };
        assert!(!block.is_null(), "12 MiB allocation must succeed");
    }
    aligned_vmem::fault_injection::arm_fail_next_advise_range(0);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [2, 0, 0, 0, 1, 0, 0, 0],
        "the second fresh cohort selects one more control; the failed premise \
         is counted only when the token is consumed"
    );

    // --- Scenario D1: failed growth reallocs with the premise token
    // PENDING must preserve it. Exactly one cohort block carries the token
    // (unknown which), so every block is attempted; each must fail without
    // touching telemetry.
    let before_failed_growth_b = AllocCore::dbg_thp_control_telemetry();
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_b {
        // SAFETY: live owned pointer with its exact old layout; a failed
        // realloc leaves it valid, owned, and (for the token-bearing block)
        // with its pending token untouched.
        let failed = unsafe { GLOBAL.realloc(*block, layout12, 16 * MIB) };
        assert!(
            failed.is_null(),
            "the injected OOM must fail the growth realloc"
        );
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        before_failed_growth_b,
        "failed growth reallocs must not select, invalidate, or observe anything"
    );
    // SAFETY: cohort_b[0] stayed a live 12 MiB allocation after the failed
    // realloc.
    unsafe { cohort_b[0].write_volatile(0x5a) };

    // --- Scenario B: the successful fallback shrink consumes the pending
    // premise token exactly once. This CLOSES scenario D1: a mutant that
    // cleared pending tokens on failed growth would leave premise 0 here.
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_b {
        // SAFETY: live owned pointer with its exact old layout; the OOM
        // fallback keeps it valid and owned at the NEW size.
        let shrunk = unsafe { GLOBAL.realloc(*block, layout12, 8 * MIB) };
        assert!(
            !shrunk.is_null(),
            "the OOM-shrink fallback must keep the block"
        );
        assert_eq!(
            shrunk, *block,
            "the fallback must keep the original pointer"
        );
        *block = shrunk;
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [2, 1, 0, 0, 1, 0, 0, 0],
        "the pending failed-premise token survived the failed growth reallocs \
         and is consumed by the fallback as exactly one premise failure, \
         never a realloc invalidation"
    );

    // --- Scenario C: exactly-once — a second fallback shrink of the
    // already-consumed cohort must account nothing. Every cohort_b block is
    // now an 8 MiB allocation, so this shrink's old layout is layout8.
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_b {
        // SAFETY: live owned pointer with its exact 8 MiB layout.
        let twice = unsafe { GLOBAL.realloc(*block, layout8, 4 * MIB) };
        assert!(
            !twice.is_null() && twice == *block,
            "the second fallback keeps the block"
        );
        *block = twice;
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [2, 1, 0, 0, 1, 0, 0, 0],
        "an already-consumed token must not be recounted by a second fallback"
    );

    // Every cohort_b block is now a 4 MiB allocation; cleanup uses layout4.
    for block in cohort_b.iter().rev() {
        // SAFETY: each pointer is the live 4 MiB shrunk allocation.
        unsafe { GLOBAL.dealloc(*block, layout4) };
    }

    // --- Cohort C: a fresh 16 MiB cohort with a PENDING success token
    // (16 MiB cannot hit the cached 12/16 MiB-usable slots, so every block
    // is fresh).
    let mut cohort_c = [core::ptr::null_mut(); COHORT];
    for block in &mut cohort_c {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout16) };
        assert!(!block.is_null(), "16 MiB allocation must succeed");
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [3, 1, 0, 0, 1, 0, 0, 0],
        "the third fresh cohort selects one more control; scenario B's premise failure is cumulative and must stay 1"
    );

    // --- Scenario D2: failed growth reallocs with a success token PENDING
    // must preserve it; the consumption shrink below closes this scenario.
    let before_failed_growth_c = AllocCore::dbg_thp_control_telemetry();
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_c {
        // SAFETY: live owned pointer with its exact old layout; a failed
        // realloc leaves it valid, owned, and (for the token-bearing block)
        // with its pending token untouched.
        let failed = unsafe { GLOBAL.realloc(*block, layout16, 32 * MIB) };
        assert!(
            failed.is_null(),
            "the injected OOM must fail the growth realloc"
        );
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry(),
        before_failed_growth_c,
        "failed growth reallocs must not select, invalidate, or observe anything"
    );
    // SAFETY: cohort_c[0] stayed a live 16 MiB allocation after the failed
    // realloc.
    unsafe { cohort_c[0].write_volatile(0x5a) };

    // --- D2 closure: the successful fallback shrink (16 -> 8 MiB) must
    // still find the pending success token and account it exactly once as
    // the SECOND realloc invalidation.
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_c {
        // SAFETY: live owned pointer with its exact old layout; the OOM
        // fallback keeps it valid and owned at the NEW size.
        let shrunk = unsafe { GLOBAL.realloc(*block, layout16, 8 * MIB) };
        assert!(
            !shrunk.is_null(),
            "the OOM-shrink fallback must keep the block"
        );
        assert_eq!(
            shrunk, *block,
            "the fallback must keep the original pointer"
        );
        *block = shrunk;
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [3, 1, 0, 0, 2, 0, 0, 0],
        "the pending success token survived the failed growth reallocs and is \
         consumed by the fallback as the second realloc invalidation"
    );

    // --- Exactly-once for cohort C: a second fallback shrink (8 -> 4 MiB)
    // of the already-consumed cohort must account nothing.
    AllocCore::dbg_set_inject_large_alloc_oom(true);
    for block in &mut cohort_c {
        // SAFETY: live owned pointer with its exact 8 MiB layout.
        let twice = unsafe { GLOBAL.realloc(*block, layout8, 4 * MIB) };
        assert!(
            !twice.is_null() && twice == *block,
            "the second fallback keeps the block"
        );
        *block = twice;
    }
    AllocCore::dbg_set_inject_large_alloc_oom(false);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_MIB8_TO_32],
        [3, 1, 0, 0, 2, 0, 0, 0],
        "an already-consumed token must not be recounted by a second fallback"
    );

    // Every cohort_c block is now a 4 MiB allocation; cleanup uses layout4.
    for block in cohort_c.iter().rev() {
        // SAFETY: each pointer is the live 4 MiB shrunk allocation.
        unsafe { GLOBAL.dealloc(*block, layout4) };
    }

    let final_counts = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        final_counts[GROUP_MIB8_TO_32],
        [3, 1, 0, 0, 2, 0, 0, 0],
        "final accounting: three selections, one premise failure from the \
         pending token that survived D1, two realloc invalidations from the \
         tokens that survived scenario A's own shrink and D2"
    );
    assert_eq!(
        final_counts[GROUP_MIB1_TO_8], [0; 8],
        "group 1 must stay untouched by every scenario"
    );
    for (group, counters) in final_counts.iter().enumerate() {
        assert!(
            group == GROUP_MIB8_TO_32 || *counters == [0; 8],
            "every other group must be all zeros, got group {group}: {counters:?}"
        );
    }
}
