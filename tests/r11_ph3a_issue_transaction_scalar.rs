#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ph3a scalar-input witnesses: `pop_free` and `carve_block` must run their
//! fallible sidecar prepare BEFORE any freelist/bitmap/bump/credit mutation,
//! so a prepare refusal leaves the whole owner state bit-for-bit identical
//! and a later retry serves the very same block.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteRegistration, SmallSidecar};
use sefer_alloc::{AllocCore, SegmentLayout};

const CLASS0: usize = 16;
const CLASS1: usize = 32;
const POP_N: usize = 24;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

fn release(core: &mut AllocCore, ptr: *mut u8, size: usize) {
    // SAFETY: each caller passes one still-owned allocation and its exact layout.
    unsafe { core.dealloc(ptr, layout(size)) };
}

/// Snapshot of every observable the transaction protocol may touch on the
/// scalar paths: bump cursor, live credits, class freelist head, commit
/// frontier, the segment-directory bit, and the process-wide sidecar totals.
struct ScalarState {
    issue: (usize, u32, u32, usize),
    directory: Option<bool>,
    system: (usize, usize, usize, usize, usize),
}

fn capture(core: &AllocCore, class_idx: usize, slot_idx: usize) -> ScalarState {
    ScalarState {
        issue: core.dbg_r11_issue_state_for_test(class_idx).unwrap(),
        directory: core.dbg_directory_get_bit(class_idx, slot_idx),
        system: SmallSidecar::system_totals_for_test(),
    }
}

fn assert_unchanged(core: &AllocCore, before: &ScalarState, class_idx: usize, slot_idx: usize) {
    assert_eq!(
        core.dbg_r11_issue_state_for_test(class_idx),
        Some(before.issue),
        "prepare refusal must not move bump/live/head/frontier",
    );
    assert_eq!(
        core.dbg_directory_get_bit(class_idx, slot_idx),
        before.directory,
        "prepare refusal must not move the directory bit",
    );
    assert_eq!(
        SmallSidecar::system_totals_for_test(),
        before.system,
        "prepare refusal must not promote or allocate sidecar storage",
    );
}

fn scalar_pop_prepare_refusal_is_a_full_rollback_and_retry_serves_the_same_block() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    // Two blocks on the class-0 free list: the pop path (not carve) serves the
    // retry, and the untouched anchor keeps its free bit for the whole test.
    let mut blocks = [core::ptr::null_mut(); 2];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), 2);
    for ptr in blocks {
        release(&mut core, ptr, CLASS0);
    }
    let anchor = blocks[0];
    let before = capture(&core, 0, 0);

    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(
        core.alloc(layout(CLASS0)).is_null(),
        "an uncommitted class spill must surface as OOM",
    );
    assert_unchanged(&core, &before, 0, 0);
    assert!(
        core.dbg_is_free_for(anchor) && core.dbg_is_free_for(blocks[1]),
        "both free blocks must keep their free bit through the refusal",
    );

    // The refusal is one-shot: the next attempt commits the same pop.
    let ptr = core.alloc(layout(CLASS0));
    assert_eq!(ptr, blocks[1], "LIFO freelist order must be preserved");
    assert_eq!(
        core.dbg_live_count_for(ptr),
        Some(before.issue.1 + 1),
        "exactly one live credit for one issued block",
    );
    assert!(!core.dbg_is_free_for(ptr));
    assert!(core.dbg_is_free_for(anchor), "anchor bit must be untouched");
    assert_eq!(
        core.dbg_r11_issue_state_for_test(0).unwrap().0,
        before.issue.0,
        "pop never moves the bump cursor",
    );
    release(&mut core, ptr, CLASS0);
    release(&mut core, anchor, CLASS0);
}

fn scalar_carve_spill_refusal_is_a_full_rollback_and_retry_carves_the_block() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    // Class 1 has never been carved: its sidecar leaf still needs a promotion,
    // which is exactly what `fail_spill_after` refuses.
    let before = capture(&core, 1, 0);

    SmallSidecar::fail_spill_after_for_test(1);
    assert!(
        core.dbg_r11_scalar_carve_for_test(1).is_none(),
        "an uncommitted leaf spill must refuse the carve",
    );
    assert_unchanged(&core, &before, 1, 0);
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(anchor.addr()),
        Some(0),
        "the anchor keeps its committed class-0 encoding",
    );

    let second = core
        .dbg_r11_scalar_carve_for_test(1)
        .expect("retry after the one-shot failure must carve");
    assert_eq!(
        second.addr() % SegmentLayout::SEGMENT,
        (before.issue.0 + 31) & !31,
        "the retry carves at the alignment-up bump the rollback preserved",
    );
    assert_eq!(
        core.dbg_live_count_for(anchor),
        Some(before.issue.1 + 1),
        "exactly one live credit for the retried carve",
    );
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(second.addr()),
        Some(1),
        "the retry commits the class-1 publication",
    );
    release(&mut core, second, CLASS1);
    release(&mut core, anchor, CLASS0);
}

fn scalar_pop_zeroed_refusal_rolls_back_and_the_retry_zeroes_the_requested_extent() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let mut blocks = [core::ptr::null_mut(); 2];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), 2);
    // Poison the blocks so a successful `alloc_zeroed` must visibly erase it:
    // a refusal that half-issued would leave the pattern behind.
    for ptr in blocks {
        // SAFETY: both pointers are live, exclusively owned, 16-byte blocks.
        unsafe { core::slice::from_raw_parts_mut(ptr, CLASS0) }.fill(0xA5);
    }
    for ptr in blocks {
        release(&mut core, ptr, CLASS0);
    }
    let before = capture(&core, 0, 0);

    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(core.alloc_zeroed(layout(CLASS0)).is_null());
    assert_unchanged(&core, &before, 0, 0);

    let ptr = core.alloc_zeroed(layout(CLASS0));
    assert_eq!(ptr, blocks[1]);
    // SAFETY: `ptr` is a live 16-byte block from this exclusively owned core.
    let extent = unsafe { core::slice::from_raw_parts(ptr, CLASS0) };
    assert!(
        extent.iter().all(|&byte| byte == 0),
        "the poisoned pattern must be erased over the whole requested extent",
    );

    // A smaller request over a larger class must zero exactly the requested
    // extent: the first 24 bytes of a 32-byte block, no more, no less.
    let mut wide = [core::ptr::null_mut(); 2];
    assert_eq!(core.dbg_carve_batch(1, &mut wide), 2);
    for ptr in wide {
        // SAFETY: both pointers are live, exclusively owned, 32-byte blocks.
        unsafe { core::slice::from_raw_parts_mut(ptr, CLASS1) }.fill(0x5A);
    }
    for ptr in wide {
        release(&mut core, ptr, CLASS1);
    }
    let wide_before = capture(&core, 1, 0);
    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(core.alloc_zeroed(layout(POP_N)).is_null());
    assert_unchanged(&core, &wide_before, 1, 0);
    let wide_ptr = core.alloc_zeroed(layout(POP_N));
    assert_eq!(wide_ptr, wide[1]);
    // SAFETY: `wide_ptr` is a live 32-byte block; the request covered 24 bytes.
    let wide_extent = unsafe { core::slice::from_raw_parts(wide_ptr, CLASS1) };
    assert!(
        wide_extent[..POP_N].iter().all(|&byte| byte == 0),
        "the first 24 bytes of the requested extent must be zero",
    );
    assert_eq!(
        core.dbg_live_count_for(wide_ptr),
        Some(wide_before.issue.1 + 1),
    );

    release(&mut core, wide_ptr, CLASS1);
    release(&mut core, wide[0], CLASS1);
    release(&mut core, ptr, CLASS0);
    release(&mut core, blocks[0], CLASS0);
}

// One test by design: `SmallSidecar::system_totals_for_test` is process-wide,
// so the scenarios must run sequentially inside a single test (the same
// pattern `r11_p3_small_sidecar_preflight.rs` uses).
#[test]
fn scalar_issue_transactions_roll_back_before_commit_and_retry_after() {
    scalar_pop_prepare_refusal_is_a_full_rollback_and_retry_serves_the_same_block();
    scalar_carve_spill_refusal_is_a_full_rollback_and_retry_carves_the_block();
    scalar_pop_zeroed_refusal_rolls_back_and_the_retry_zeroes_the_requested_extent();
}
