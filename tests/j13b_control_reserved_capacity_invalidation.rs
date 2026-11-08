//! J13b control telemetry: the reserved-capacity invalidation boundary
//! previously feature-excluded. This binary is the genuinely activating
//! reserved-capacity + exact-span positive case plus the failure-preservation
//! oracle (a failed reserved grow preserves the token for the moving leg).
//!
//! Scenarios:
//! - A: successful in-place reserved grow (8 -> 12 MiB) accounts exactly once.
//! - A2: successful in-place reserved grow (8 -> 32 MiB) crossing the
//!   group-2 -> group-3 size boundary while the TOKEN still encodes group 2;
//!   the invalidation must be attributed by the token's original group, not
//!   the new size and not group 0.
//! - B: failed reserved grow (8 -> 40 MiB, required span exceeds the >=4x
//!   reserved target) preserves the token; the moving leg accounts once in
//!   the destination group.
//! - C: original group-3 control invalidated by a moving realloc (40 -> 128
//!   MiB) into group 4; the token-derived index must land in group 3.
//!
//! Byte-classifier discriminator: a mutant that classifies the raw token
//! value as a byte size maps token 3 (A2) and token 4 (C) to group 0
//! (`Under1MiB`, since 3 or 4 bytes / 1 MiB = 0), never to the token's own
//! group. The final whole-telemetry assert keeps group 0 all zeros, so such
//! a mutant lands the invalidation there and is caught; the token-derived
//! production path keeps groups 2 and 3 exact.
//!
//! Memory bound: peak live is roughly 8 x (40 + 128) MiB = 1.34 GiB in
//! Scenario C — fine for CI runners.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    feature = "large-reserved-capacity",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 8 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;
const GROUP_32_PLUS_MIB: usize = 3;

fn allocate_cohort(core: &mut AllocCore, layout: Layout) -> [*mut u8; COHORT] {
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        *block = core.alloc(layout);
        assert!(!block.is_null(), "large allocation must succeed");
    }
    blocks
}

#[test]
fn reserved_capacity_grow_accounts_once_and_failed_grow_preserves_the_token() {
    let config = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto);
    let mut core = AllocCore::new_with_config(config).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 8 MiB layout");
    let grown_layout = Layout::from_size_align(12 * MIB, 8).expect("valid 12 MiB layout");

    // --- Scenario A: successful reserved-capacity grow invalidates exactly once.
    // In-place proof (page-general): usable = 8 MiB + p, so the reserved
    // target is min(4 * (8 MiB + p), 64 MiB) = 32 MiB + 4p for every
    // supported page p (<= 64 KiB), while growing to 12 MiB requires only
    // 12 MiB + p — the margin 20 MiB + 3p is positive on every host.
    let mut cohort = allocate_cohort(&mut core, layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "fresh 8 MiB cohort selects one control"
    );

    for block in &mut cohort {
        let old_ptr = *block;
        // SAFETY: live 8 MiB allocation; a successful realloc consumes it.
        let grown_ptr = unsafe { core.realloc(old_ptr, layout, 12 * MIB) };
        assert!(!grown_ptr.is_null(), "reserved grow must succeed");
        assert_eq!(
            grown_ptr, old_ptr,
            "12 MiB + header exceeds the ~8 MiB exact span so the committed-span \
             arm declines; the reserved-capacity arm grows in place because \
             required_end fits the >=4x reserved target"
        );
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "in-place reserved grow must record realloc invalidation exactly once; \
         the token captured before the helper survives into the accounting"
    );

    for block in cohort.iter().rev() {
        // SAFETY: each pointer is live with the post-realloc layout.
        unsafe { core.dealloc(*block, grown_layout) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 0, 1, 0, 0, 0],
        "freeing the invalidated control must not observe it"
    );

    // --- Scenario A2: in-place reserved grow crossing the group-2 -> group-3
    // boundary. The token still encodes group 2; the invalidation must be
    // attributed to the TOKEN's original group (field 4 index 2), not the
    // new-size group (field 4 index 3) and not group 0.
    // In-place proof (page-general): reserved = 32 MiB + 4p (as above) and
    // required = 32 MiB + p, so the grow stays in place on every supported
    // page size; 33751296 is only the 64 KiB-page value of the reserved
    // target, not a cross-host constant.
    let mut boundary = allocate_cohort(&mut core, layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB][0],
        2,
        "a second fresh cohort selects one more control"
    );

    for block in &mut boundary {
        let old_ptr = *block;
        // SAFETY: live 8 MiB allocation; a successful realloc consumes it.
        let grown_ptr = unsafe { core.realloc(old_ptr, layout, 32 * MIB) };
        assert!(!grown_ptr.is_null(), "boundary reserved grow must succeed");
        assert_eq!(
            grown_ptr, old_ptr,
            "32 MiB still fits the >=4x reserved target computed from the 8 MiB \
             span: required (32 MiB + p) <= reserved (32 MiB + 4p) for every \
             supported page size, so the grow is in place"
        );
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [2, 0, 0, 0, 2, 0, 0, 0],
        "the boundary-crossing invalidation must land in the TOKEN's original \
         group (field 4 index 2), not the new-size group (index 3) and not \
         group 0"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB],
        [0; 8],
        "group 3 stays all zeros: a mutant attributing by the new recorded size \
         (32 MiB -> group 3) would land the boundary-crossing invalidation here; \
         token-derived attribution keeps it in group 2"
    );

    let grown_32_layout = Layout::from_size_align(32 * MIB, 8).expect("valid 32 MiB layout");
    for block in boundary.iter().rev() {
        // SAFETY: each pointer is live with the 32 MiB layout.
        unsafe { core.dealloc(*block, grown_32_layout) };
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [2, 0, 0, 0, 2, 0, 0, 0],
        "freeing the boundary-crossing controls must not observe them (tokens \
         were already consumed)"
    );

    // --- Scenario B: failed reserved grow preserves the token; the moving leg
    // accounts once.
    // Moving proof (page-general): required = 40 MiB + p exceeds the
    // reserved target 32 MiB + 4p whenever 8 MiB > 3p — true for every
    // supported page p (<= 64 KiB) — so the in-place arm declines and the
    // realloc moves deterministically on every host.
    let fresh = allocate_cohort(&mut core, layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB][0],
        3,
        "a third fresh cohort selects one more control"
    );

    let moved_layout = Layout::from_size_align(40 * MIB, 8).expect("valid 40 MiB layout");
    let mut moved = [core::ptr::null_mut(); COHORT];
    for (slot, block) in moved.iter_mut().zip(fresh.iter()) {
        // SAFETY: live 8 MiB allocation; a successful realloc consumes it.
        *slot = unsafe { core.realloc(*block, layout, 40 * MIB) };
        assert!(!slot.is_null(), "40 MiB moving realloc must succeed");
        assert_ne!(
            *slot, *block,
            "40 MiB exceeds the <=4x usable reserved target: the helper returns \
             false leaving the header untouched and the moving leg consumes the \
             preserved token"
        );
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [3, 0, 0, 0, 3, 0, 0, 0],
        "if the token had been lost at the failed grow, field 4 would be 2, not 3"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "the 8 fresh 40 MiB destinations form one full group-3 cohort"
    );

    for block in moved.iter().rev() {
        // SAFETY: each pointer is live with the 40 MiB layout.
        unsafe { core.dealloc(*block, moved_layout) };
    }
    let group3 = AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB];
    assert_eq!(
        &group3[..6],
        &[1, 0, 1, 0, 0, 0],
        "owner-free of the moved controls must record valid observations"
    );
    assert!(group3[6] > 0, "copy faults fault the destination pages");
    assert!(group3[7] > 0, "owner-free sampling covers payload pages");
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [3, 0, 0, 0, 3, 0, 0, 0],
        "group 2 must be unchanged by the group-3 owner-free"
    );

    // --- Scenario C: an original group-3 control invalidated by a MOVING
    // realloc into group 4. Peak live here is about 8 x (40 + 128) MiB =
    // 1.34 GiB — fine for CI runners.
    // Moving proof: required = page + 128 MiB >= 134221824 > reserved
    // min(4*(40 MiB + page), 64 MiB) = 64 MiB cap, so the realloc moves
    // deterministically on every supported page size.
    let large_layout = Layout::from_size_align(40 * MIB, 8).expect("valid 40 MiB layout");
    let mut large_cohort = allocate_cohort(&mut core, large_layout);
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB][0],
        2,
        "a second fresh 40 MiB cohort selects one more group-3 control"
    );

    let huge_layout = Layout::from_size_align(128 * MIB, 8).expect("valid 128 MiB layout");
    let mut huge = [core::ptr::null_mut(); COHORT];
    for (slot, block) in huge.iter_mut().zip(large_cohort.iter()) {
        // SAFETY: live 40 MiB allocation; a successful realloc consumes it.
        *slot = unsafe { core.realloc(*block, large_layout, 128 * MIB) };
        assert!(!slot.is_null(), "128 MiB moving realloc must succeed");
        assert_ne!(
            *slot, *block,
            "128 MiB exceeds the reserved-capacity cap (64 MiB): the in-place \
             arm declines and the moving leg consumes the preserved token"
        );
    }
    let group3_after_c = AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB];
    assert_eq!(
        &group3_after_c[..6],
        &[2, 0, 1, 0, 1, 0],
        "the original group-3 control's token (4) must land in field 4 of GROUP 3 \
         (direct token-derived index), not the destination group 4"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[1],
        [0; 8],
        "group 1 stays all zeros: a byte-classifier mutant maps the raw token \
         value 4 to group 0 (4 bytes / 1 MiB = 0), so group 1 can only ever \
         move through genuine MiB1To8 requests"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[4],
        [1, 0, 0, 0, 0, 0, 0, 0],
        "the 8 fresh 128 MiB destinations form one full group-4 cohort"
    );

    for block in huge.iter().rev() {
        // SAFETY: each pointer is live with the 128 MiB layout.
        unsafe { core.dealloc(*block, huge_layout) };
    }
    let group4 = AllocCore::dbg_thp_control_telemetry()[4];
    assert_eq!(&group4[..2], &[1, 0], "group-4 selections stay at one");
    assert_eq!(
        group4[2], 1,
        "owner-free of the moved 128 MiB controls observes once"
    );
    assert_eq!(group4[3], 0, "no remote-reclaim invalidations in group 4");
    assert_eq!(group4[4], 0, "no realloc invalidations in group 4");
    assert_eq!(group4[5], 0, "no errors in group 4");
    assert!(
        group4[6] > 0,
        "copy faults fault the first 40 MiB of each destination"
    );
    assert!(
        group4[7] > 0,
        "owner-free sampling covers the 128 MiB payload pages"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_32_PLUS_MIB],
        [2, 0, 1, 0, 1, 0, group3[6], group3[7]],
        "group 3 must be unchanged by the group-4 owner-free"
    );

    // --- FINAL whole-telemetry assert. Groups 2, 3, 4 carry the scenario
    // accounting asserted above; groups 0, 1, 5 must be all zeros. Group 0
    // all-zeros is the byte-classifier discriminator: a mutant classifying
    // the raw token value as bytes lands the token 3/4 invalidations there.
    let telemetry = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(
        telemetry[GROUP_8_TO_32_MIB],
        [3, 0, 0, 0, 3, 0, 0, 0],
        "final group-2 accounting"
    );
    let group3 = telemetry[GROUP_32_PLUS_MIB];
    assert_eq!(
        &group3[..6],
        &[2, 0, 1, 0, 1, 0],
        "final group-3 accounting (first six fields exact)"
    );
    assert!(group3[6] > 0, "final group-3 copy faults must be positive");
    assert!(group3[7] > 0, "final group-3 sampling must be positive");
    let group4 = telemetry[4];
    assert_eq!(
        &group4[..6],
        &[1, 0, 1, 0, 0, 0],
        "final group-4 accounting"
    );
    assert!(group4[6] > 0, "final group-4 copy faults must be positive");
    assert!(group4[7] > 0, "final group-4 sampling must be positive");
    for (group, counters) in telemetry.iter().enumerate() {
        if group == GROUP_8_TO_32_MIB || group == GROUP_32_PLUS_MIB || group == 4 {
            continue;
        }
        assert!(
            *counters == [0; 8],
            "all remaining groups must be all zeros, got group {group}: {counters:?}; \
             group 0 all-zeros is the byte-classifier discriminator"
        );
    }
    drop(core);
}
