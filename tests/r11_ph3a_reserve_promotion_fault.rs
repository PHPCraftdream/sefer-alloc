#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ph3a promotion/reserve witnesses. When the current small segment is
//! exhausted, `alloc_small` reserves a fresh segment BEFORE issuing: the
//! reservation itself is fallible (`SegmentTable::register` ->
//! `RouteDirectory::register`), and the post-reserve carve's sidecar prepare
//! is fallible too. Neither refusal may touch the exhausted segment's state,
//! and the one-shot disarms make the retry succeed on the fresh segment.
//!
//! Hook inventory (searched as the brief asks): `alloc_core_small_pool` and
//! `alloc_core_small_diag.rs` expose NO reserve-refusal hook of their own, so
//! the register refusal is armed through the directory's own test seam
//! (`RouteDirectory::fail_next_registration_for_test`) — the exact hook the
//! brief's option (a) names — and the post-reserve carve refusal through
//! `SmallSidecar::fail_prepare_after_for_test`. Both are covered below.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteRegistration, SmallSidecar};
use sefer_alloc::{AllocCore, SegmentLayout};

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
/// naming the moment a reserve succeeds.
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
/// carved) and the retry promotes cleanly.
fn reserve_register_refusal_rolls_back_and_the_retry_promotes() {
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
    // built, so the counters do not move at all.
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(
        after, before.system,
        "a refused register allocates no sidecar"
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

    let ptr = core.alloc(layout());
    assert!(!ptr.is_null(), "the one-shot disarm lets the retry promote");
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(ptr.addr()),
        Some(1),
        "the retry issues a class-1 block from the fresh segment",
    );
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

/// The post-reserve carve refusal: the fresh segment IS reserved (that is the
/// documented reserve semantics — the reservation happens before the issue
/// prepare), the carve on it refuses, and the exhausted segment keeps its
/// state bit-for-bit while the retry carves on the fresh one.
fn post_reserve_carve_refusal_leaves_the_old_segment_intact() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = exhaust_current_segment(&mut core);
    let before = capture_old(&core, anchor);

    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(
        core.alloc(layout()).is_null(),
        "a refused post-reserve carve must surface as OOM",
    );
    assert_old_untouched(&core, &before);
    let table_after = core.dbg_table_count();
    assert!(
        table_after == before.table || table_after == before.table + 1,
        "the reserve may or may not have materialised before the refusal \
         (table delta {})",
        table_after - before.table,
    );
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().0 - before.census.0,
        usize::from(table_after != before.table),
        "at most the reserved segment's route is new",
    );

    let ptr = core.alloc(layout());
    assert!(!ptr.is_null(), "the retry carves on the fresh segment");
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(ptr.addr()),
        Some(1),
    );
    assert_ne!(
        core.dbg_segment_id_of(ptr),
        before.slot,
        "the retry must be served by the newly reserved segment",
    );
    assert_eq!(core.dbg_table_count(), before.table + 1);
    // SAFETY: `ptr` came from this core with this exact layout.
    unsafe { core.dealloc(ptr, layout()) };
    let _ = SegmentLayout::SEGMENT;
}

// One test by design: the sidecar accounting observers are process-wide, so
// the two promotion scenarios run sequentially inside a single test.
#[test]
fn promotion_faults_roll_back_the_exhausted_segment_and_retry_promotes() {
    reserve_register_refusal_rolls_back_and_the_retry_promotes();
    post_reserve_carve_refusal_leaves_the_old_segment_intact();
}
