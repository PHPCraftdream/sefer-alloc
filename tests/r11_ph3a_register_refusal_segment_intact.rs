#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ported from the removed `tests/r11_ph3a_reserve_promotion_fault.rs`
//! (294ff32a, IssueTransaction era): the NON-leaf promotion witness. When the
//! current small segment is exhausted, `alloc_small` reserves a fresh segment
//! BEFORE issuing; the reservation itself is fallible
//! (`SegmentTable::register` -> `RouteDirectory::register`). A refused
//! register must surface as OOM (never a panic) and must leave the EXHAUSTED
//! segment bit-for-bit intact: bump / live / head (the owner state triple),
//! its directory bit, and no sidecar / route / table-slot movement.
//!
//! Mechanism under D1′ (one-class): there is no prepare-transaction any
//! more — the only publication steps are the reserve's `register_route`
//! followed by the carve's `set_bump` -> per-block `issue` -> `add_live`. The
//! refusal is armed through the directory's own test seam
//! (`RouteDirectory::fail_next_registration_for_test`), which is exactly the
//! hook the promotion fault always used for the register leg; the commit
//! leg's refusal (`dbg_arm_commit_fail`) is covered by
//! `tests/r11_ph3a_magazine_refill_partial.rs`.
//!
//! Mutant reasoning (why this must fail under the mutants):
//! - a mutant that lets the refused register still advance the exhausted
//!   segment's bump/live (`set_bump`/`add_live` before the register check)
//!   is caught by the `owner_state_for_test` triple equality;
//! - a mutant that clears the exhausted segment's directory bit or moves its
//!   bin head on refusal is caught by the directory-bit observer;
//! - a mutant that publishes the route (or takes a table slot / allocates
//!   the sidecar) despite the refusal is caught by the census /
//!   `dbg_table_count` / `system_totals_for_test` equality assertions;
//! - a mutant that demotes the refusal to a silent success is caught by the
//!   null-return assertion (OOM contract).

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, SmallSidecar};
use sefer_alloc::AllocCore;

const CLASS_SIZE: usize = 32;

fn layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 16).unwrap()
}

/// Fill the current small segment for the 32B class until the bump cursor
/// cannot fit one more block. The carved blocks are intentionally leaked:
/// the segment's room, not their storage, is what the test consumes.
fn exhaust_current_segment(core: &mut AllocCore) -> *mut u8 {
    let anchor = core
        .dbg_r11_scalar_carve_for_test(1)
        .expect("a fresh segment must fit one 32B block");
    while core.dbg_r11_scalar_carve_for_test(1).is_some() {}
    anchor
}

/// Observer bundle for the EXHAUSTED (old) segment, which `small_cur` stops
/// naming the moment a reserve succeeds. `owner_state_for_test` reads exactly
/// the three D1′-relevant fields — bump, live_count, bin head — through the
/// segment header, so the oracle is the "bump/live/head untouched" property
/// from the original promotion-fault witness, unchanged.
struct OldSegment {
    anchor: *mut u8,
    slot: u32,
    owner: (usize, u32, u32),
    directory: Option<bool>,
    system: (usize, usize, usize, usize, usize),
    census: (usize, usize, usize),
    table: u32,
}

fn capture_old(core: &AllocCore, anchor: *mut u8) -> OldSegment {
    OldSegment {
        anchor,
        slot: core.dbg_segment_id_of(anchor),
        // SAFETY: the exhausted segment stays registered and owned by this
        // core for the whole read; the anchor is one of its live blocks.
        owner: unsafe { SmallSidecar::owner_state_for_test(anchor.addr(), 1) }.unwrap(),
        directory: core.dbg_directory_get_bit(1, core.dbg_segment_id_of(anchor) as usize),
        system: SmallSidecar::system_totals_for_test(),
        census: RouteDirectory::global().live_route_census_for_test(),
        table: core.dbg_table_count(),
    }
}

fn assert_old_untouched(core: &AllocCore, before: &OldSegment) {
    // SAFETY: same exhausted segment, same live anchor, same exclusive owner.
    let owner = unsafe { SmallSidecar::owner_state_for_test(before.anchor.addr(), 1) }.unwrap();
    assert_eq!(
        owner, before.owner,
        "the exhausted segment's bump/live/head must not move",
    );
    assert_eq!(
        core.dbg_directory_get_bit(1, before.slot as usize),
        before.directory,
    );
}

/// The register refusal: `reserve_small_segment` cannot publish the fresh
/// segment, so the promotion is a full rollback (nothing reserved, nothing
/// carved) and the retry promotes cleanly on a fresh segment.
#[test]
fn refused_register_leaves_the_exhausted_segment_intact_and_retry_promotes() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = exhaust_current_segment(&mut core);
    let before = capture_old(&core, anchor);
    RouteDirectory::fail_next_registration_for_test();
    assert!(
        core.alloc(layout()).is_null(),
        "a refused register must surface as OOM, never a panic",
    );
    assert_old_untouched(&core, &before);
    // The refusal happens before the entry (and therefore the sidecar) is
    // built, so the sidecar's System counters do not move at all.
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(
        after, before.system,
        "a refused register allocates no sidecar",
    );
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test(),
        before.census,
        "a refused register publishes no route",
    );
    assert_eq!(
        core.dbg_table_count(),
        before.table,
        "a refused register takes no table slot",
    );

    // The one-shot seam disarms itself, so the retry promotes: a fresh
    // segment serves the block, and exactly one segment was reserved.
    let ptr = core.alloc(layout());
    assert!(!ptr.is_null(), "the one-shot disarm lets the retry promote");
    assert_ne!(
        core.dbg_segment_id_of(ptr),
        before.slot,
        "the retry must be served by the newly reserved segment",
    );
    assert_eq!(
        core.dbg_table_count(),
        before.table + 1,
        "the successful promotion reserves exactly one segment",
    );
    // SAFETY: `ptr` came from this core with this exact layout.
    unsafe { core.dealloc(ptr, layout()) };
}
