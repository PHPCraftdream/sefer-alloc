#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! Ph3a batch-input witnesses: `try_drain_freelist_batch` and
//! `try_carve_batch` run EVERY fallible prepare before the first mutation, so
//! a prepare refusal returns `Err` (the `0` the `carve_batch`/
//! `drain_freelist_batch` wrappers surface) with the output buffer, the
//! freelist head, the bitmap, the live credits and the bump cursor all
//! untouched — while a successful batch commits exactly `k` blocks, `k` being
//! the chain/room length, never the caller's slice length.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::SmallSidecar;
use sefer_alloc::{AllocCore, SegmentLayout};

const CLASS0: usize = 16;
const CLASS1: usize = 32;
const FREE_BLOCKS: usize = 100;
const OUT_LEN: usize = 260;
const SMALL_OUT: usize = 8;
const LEAF: usize = 4096;
const FREE_LIST_NULL: u32 = u32::MAX;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

fn release(core: &mut AllocCore, ptr: *mut u8, size: usize) {
    // SAFETY: each caller passes one still-owned allocation and its exact layout.
    unsafe { core.dealloc(ptr, layout(size)) };
}

/// Drain a freelist that is SHORTER than the output slice: the chain (not the
/// slice) bounds the batch, and every committed step stays consistent.
fn batch_drain_shorter_chain_commits_exactly_the_chain_length() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    // A chain longer than `FREE_BLOCKS` so the freelist really is the bound.
    let mut blocks = [core::ptr::null_mut(); OUT_LEN];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), OUT_LEN);
    let first_leaf = blocks[0].addr() / LEAF;
    assert!(
        blocks.iter().any(|ptr| ptr.addr() / LEAF != first_leaf),
        "the chain must span more than one leaf for a mixed-leaf drain",
    );
    // Free exactly the first FREE_BLOCKS; the rest stay live and allocated.
    for ptr in &blocks[..FREE_BLOCKS] {
        release(&mut core, *ptr, CLASS0);
    }
    let state = core.dbg_r11_issue_state_for_test(0).unwrap();
    let directory = core.dbg_directory_get_bit(0, 0);
    let system = SmallSidecar::system_totals_for_test();

    let mut out = [anchor; OUT_LEN];
    // SAFETY: `anchor` is a live allocation of this exclusively owned core.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) };

    assert_eq!(k, FREE_BLOCKS, "a partial drain commits the chain length");
    assert_eq!(
        core.dbg_r11_issue_state_for_test(0).unwrap().0,
        state.0,
        "drain never moves the bump cursor",
    );
    assert_eq!(
        core.dbg_live_count_for(anchor),
        Some(state.1 + k as u32),
        "live credits grow by exactly k, not by out.len()",
    );
    assert_eq!(
        core.dbg_freelist_head_for(anchor, 0),
        FREE_LIST_NULL,
        "the exhausted chain must end at FREE_LIST_NULL",
    );
    // Every issued block ends bitmap-allocated, and no never-freed block was
    // ever free in the first place.
    for ptr in &blocks[..FREE_BLOCKS] {
        assert!(!core.dbg_is_free_for(*ptr));
    }
    for ptr in &blocks[FREE_BLOCKS..] {
        assert!(!core.dbg_is_free_for(*ptr));
    }
    // The single non-empty -> empty transition is the only directory change.
    match (directory, core.dbg_directory_get_bit(0, 0)) {
        (None, after) => assert_eq!(after, None),
        (Some(true), after) => assert_eq!(after, Some(false)),
        (Some(false), after) => assert_eq!(after, Some(false)),
    }
    assert_eq!(
        SmallSidecar::system_totals_for_test().0 - system.0,
        0,
        "a drain of already-prepared blocks allocates no sidecar storage",
    );

    for ptr in &out[..k] {
        release(&mut core, *ptr, CLASS0);
    }
    for ptr in &blocks[FREE_BLOCKS..] {
        release(&mut core, *ptr, CLASS0);
    }
    release(&mut core, anchor, CLASS0);
}

/// Carve into a slice longer than the segment's remaining room: the batch
/// commits `room` blocks and stops, never overrunning the segment end.
fn batch_carve_short_room_commits_exactly_the_room() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    // Fill the current segment with class-1 carves until fewer than OUT_LEN
    // blocks of room remain, so the next batch is bounded by the segment, not
    // by the output slice. The fill blocks are intentionally never freed: the
    // bump cursor, not their storage, is what consumes the room.
    let mut fill = vec![core::ptr::null_mut(); OUT_LEN];
    loop {
        let (bump, ..) = core.dbg_r11_issue_state_for_test(1).unwrap();
        let aligned = (bump + CLASS1 - 1) & !(CLASS1 - 1);
        let room = (SegmentLayout::SEGMENT - aligned) / CLASS1;
        assert!(room > 0, "the fill loop must stop before the segment ends");
        if room < OUT_LEN {
            break;
        }
        assert!(core.dbg_carve_batch(1, &mut fill) > 0);
    }
    let state = core.dbg_r11_issue_state_for_test(1).unwrap();
    let aligned = (state.0 + CLASS1 - 1) & !(CLASS1 - 1);
    let room = (SegmentLayout::SEGMENT - aligned) / CLASS1;
    let system = SmallSidecar::system_totals_for_test();
    let table_before = core.dbg_table_count();

    let mut out = [anchor; OUT_LEN];
    let n = core.dbg_carve_batch(1, &mut out);

    assert_eq!(n, room, "a partial carve commits the segment's room");
    assert!(n > 0 && n < OUT_LEN, "the fixture must really be partial");
    assert_eq!(
        core.dbg_r11_issue_state_for_test(1).unwrap().0,
        aligned + n * CLASS1,
        "the bump advances by exactly n blocks",
    );
    assert_eq!(
        core.dbg_live_count_for(anchor),
        Some(state.1 + n as u32),
        "live credits grow by exactly n",
    );
    for ptr in &out[..n] {
        assert!(!ptr.is_null());
        assert!(!core.dbg_is_free_for(*ptr), "carved blocks stay allocated");
    }
    for slot in &out[n..] {
        assert_eq!(*slot, anchor, "untouched output slots keep their pattern");
    }
    assert_eq!(
        core.dbg_table_count(),
        table_before,
        "a carve needs no new segment",
    );
    assert_eq!(
        SmallSidecar::system_totals_for_test().0 - system.0,
        0,
        "the promoted leaf is already material, so no new sidecar storage",
    );

    for ptr in &out[..n] {
        release(&mut core, *ptr, CLASS1);
    }
    release(&mut core, anchor, CLASS0);
}

/// A prepare refusal inside a batch producer returns `Err` (surfaced as `0`)
/// with the output buffer still holding the caller's pattern, then a retry
/// after the one-shot disarm commits a full batch.
fn batch_prepare_refusal_rolls_back_state_and_leaves_the_output_pattern() {
    // (a) drain producer over a non-empty chain.
    {
        let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
        let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
        let mut blocks = [core::ptr::null_mut(); SMALL_OUT];
        assert_eq!(core.dbg_carve_batch(0, &mut blocks), SMALL_OUT);
        for ptr in blocks {
            release(&mut core, ptr, CLASS0);
        }
        let state = core.dbg_r11_issue_state_for_test(0).unwrap();
        let directory = core.dbg_directory_get_bit(0, 0);
        let system = SmallSidecar::system_totals_for_test();
        let table_before = core.dbg_table_count();

        SmallSidecar::fail_prepare_after_for_test(1);
        let mut out = [anchor; SMALL_OUT];
        // SAFETY: `anchor` is a live allocation of this exclusively owned core.
        let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) };

        assert_eq!(k, 0, "an uncommitted prepare must issue nothing");
        assert_eq!(out, [anchor; SMALL_OUT], "the pattern must survive");
        assert_eq!(core.dbg_r11_issue_state_for_test(0), Some(state));
        assert_eq!(core.dbg_directory_get_bit(0, 0), directory);
        assert_eq!(SmallSidecar::system_totals_for_test(), system);
        assert_eq!(core.dbg_table_count(), table_before);
        assert!(
            blocks.iter().all(|&ptr| core.dbg_is_free_for(ptr)),
            "the whole chain must stay free through the refusal",
        );

        let mut retry = [anchor; SMALL_OUT];
        // SAFETY: `anchor` still names the same live allocation.
        let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut retry) };
        assert_eq!(k, SMALL_OUT, "the retry drains the untouched chain");
        assert_eq!(
            core.dbg_live_count_for(anchor),
            Some(state.1 + k as u32),
            "live credits grow by exactly the committed batch size",
        );
        for ptr in &retry[..k] {
            release(&mut core, *ptr, CLASS0);
        }
        release(&mut core, anchor, CLASS0);
    }
    // (b) carve producer from the bump cursor.
    {
        let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
        let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
        let state = core.dbg_r11_issue_state_for_test(1).unwrap();
        let system = SmallSidecar::system_totals_for_test();
        let table_before = core.dbg_table_count();

        SmallSidecar::fail_prepare_after_for_test(1);
        let mut out = [anchor; SMALL_OUT];
        let n = core.dbg_carve_batch(1, &mut out);

        assert_eq!(n, 0, "an uncommitted prepare must carve nothing");
        assert_eq!(out, [anchor; SMALL_OUT], "the pattern must survive");
        assert_eq!(core.dbg_r11_issue_state_for_test(1), Some(state));
        assert_eq!(SmallSidecar::system_totals_for_test(), system);
        assert_eq!(core.dbg_table_count(), table_before);

        let mut retry = [anchor; SMALL_OUT];
        let k = core.dbg_carve_batch(1, &mut retry);
        assert_eq!(k, SMALL_OUT, "the retry carves the full batch");
        assert_eq!(
            core.dbg_live_count_for(anchor),
            Some(state.1 + k as u32),
            "live credits grow by exactly the committed batch size",
        );
        for ptr in &retry[..k] {
            release(&mut core, *ptr, CLASS1);
        }
        release(&mut core, anchor, CLASS0);
    }
}

// One test by design: `SmallSidecar::system_totals_for_test` is process-wide,
// so the scenarios must run sequentially inside a single test (the same
// pattern `r11_p3_small_sidecar_preflight.rs` uses).
#[test]
fn batch_issue_transactions_commit_only_the_prepared_run() {
    batch_drain_shorter_chain_commits_exactly_the_chain_length();
    batch_carve_short_room_commits_exactly_the_room();
    batch_prepare_refusal_rolls_back_state_and_leaves_the_output_pattern();
}
