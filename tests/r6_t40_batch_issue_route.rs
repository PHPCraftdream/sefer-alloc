#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! R6-T40 OPT05 witnesses: the freelist-drain batch producer resolves its
//! small-issue route context ONCE and then prepares/commits per block through
//! that context. The oracle is BEHAVIORAL, not a resolve-count measurement:
//! a one-shot prepare refusal (`SmallSidecar::fail_prepare_after_for_test(2)`
//! — the 2nd per-block prepare fails) must abort the batch BEFORE any state
//! change (`drain_freelist_batch` surfaces the `Err` as `0`), leaving the
//! output pattern, freelist head, live credits, bitmap and directory bit
//! untouched; after the injection's countdown is exhausted the retry commits
//! the full chain. The happy path commits exactly the chain length with every
//! pointer valid and bitmap-allocated. The pre-OPT05 path (per-block
//! `prepare_small_issue` resolution) passes the same asserts — this test
//! pins the rollback/retry contract the batch-context refactor must keep,
//! it does NOT observe how many times the route is resolved.

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::SmallSidecar;
use sefer_alloc::AllocCore;

const CLASS0: usize = 16;
const SMALL_OUT: usize = 8;
const FREE_LIST_NULL: u32 = u32::MAX;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

fn release(core: &mut AllocCore, ptr: *mut u8, size: usize) {
    // SAFETY: each caller passes one still-owned allocation and its exact layout.
    unsafe { core.dealloc(ptr, layout(size)) };
}

/// Routed core + a non-empty freelist chain of `SMALL_OUT` class-0 blocks:
/// carve a full batch (the batched routed prepare path itself), then free
/// every block so the drain has a whole chain to walk.
fn routed_core_with_freed_chain() -> (AllocCore, *mut u8, [*mut u8; SMALL_OUT]) {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    let mut blocks = [core::ptr::null_mut(); SMALL_OUT];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), SMALL_OUT);
    for ptr in blocks {
        release(&mut core, ptr, CLASS0);
    }
    (core, anchor, blocks)
}

/// A one-shot prepare refusal at the 2nd per-block prepare surfaces as `0`
/// with the output pattern, head/live/bump state, bitmap and directory bit
/// all unchanged; the retry after the countdown expires commits the full
/// chain (equivalence with the never-injected drain).
#[test]
fn batch_prepare_refusal_at_second_block_rolls_back_and_retries() {
    let (mut core, anchor, blocks) = routed_core_with_freed_chain();
    let state = core.dbg_r11_issue_state_for_test(0).unwrap();
    let directory = core.dbg_directory_get_bit(0, 0);

    SmallSidecar::fail_prepare_after_for_test(2);
    let mut out = [anchor; SMALL_OUT];
    // SAFETY: `anchor` is a live allocation of this exclusively owned core.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) };

    assert_eq!(k, 0, "an uncommitted prepare must issue nothing");
    assert_eq!(out, [anchor; SMALL_OUT], "the pattern must survive");
    assert_eq!(
        core.dbg_r11_issue_state_for_test(0),
        Some(state),
        "head, live credits and the bump cursor must not move",
    );
    assert_eq!(core.dbg_directory_get_bit(0, 0), directory);
    assert!(
        blocks.iter().all(|&ptr| core.dbg_is_free_for(ptr)),
        "the whole chain must stay free through the refusal",
    );

    // The injection is a one-shot countdown: the retry runs with the same
    // single-resolve batch context and must commit the entire chain.
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
        assert!(!ptr.is_null());
        assert!(!core.dbg_is_free_for(*ptr), "issued blocks stay allocated");
    }
    for ptr in &retry[..k] {
        release(&mut core, *ptr, CLASS0);
    }
    release(&mut core, anchor, CLASS0);
}

/// Happy path: a full drain of the freed chain commits every block — valid,
/// distinct, bitmap-allocated pointers; live grows by exactly `k`; the chain
/// ends at `FREE_LIST_NULL`; the bump cursor never moves.
#[test]
fn batch_drain_full_chain_commits_every_block_valid_and_class_sized() {
    let (mut core, anchor, blocks) = routed_core_with_freed_chain();
    let state = core.dbg_r11_issue_state_for_test(0).unwrap();

    let mut out = [anchor; SMALL_OUT];
    // SAFETY: `anchor` is a live allocation of this exclusively owned core.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) };

    assert_eq!(k, SMALL_OUT, "the whole freed chain must be committed");
    for (i, &ptr) in out.iter().enumerate() {
        assert!(!ptr.is_null(), "every issued pointer must be valid");
        assert!(!core.dbg_is_free_for(ptr), "issued blocks stay allocated");
        assert!(
            (ptr as usize).is_multiple_of(CLASS0),
            "issued blocks stay class-0 aligned",
        );
        for &other in &out[..i] {
            assert!(!core::ptr::eq(ptr, other), "issued pointers must be distinct");
        }
    }
    assert_eq!(
        core.dbg_r11_issue_state_for_test(0).unwrap().0,
        state.0,
        "drain never moves the bump cursor",
    );
    assert_eq!(
        core.dbg_live_count_for(anchor),
        Some(state.1 + k as u32),
        "live credits grow by exactly k",
    );
    assert_eq!(
        core.dbg_freelist_head_for(anchor, 0),
        FREE_LIST_NULL,
        "the exhausted chain must end at FREE_LIST_NULL",
    );

    for ptr in &out[..k] {
        release(&mut core, *ptr, CLASS0);
    }
    release(&mut core, anchor, CLASS0);
}
