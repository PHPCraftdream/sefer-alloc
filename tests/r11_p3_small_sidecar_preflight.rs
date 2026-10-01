#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteRegistration, SmallSidecar};
use sefer_alloc::AllocCore;

const LEAF: usize = 4096;

fn release(core: &mut AllocCore, ptr: *mut u8, size: usize) {
    // SAFETY: each caller passes one still-owned allocation and its exact layout.
    unsafe { core.dealloc(ptr, Layout::from_size_align(size, 16).unwrap()) };
}

#[test]
fn four_issue_transactions_preserve_state_before_commit_and_retry() {
    // These transaction faults do not fabricate a different live block class.
    // Real spill OOM is tested separately on scalar and magazine carve paths.
    let before = SmallSidecar::system_totals_for_test();
    scalar_pop();
    batch_drain();
    scalar_carve();
    batch_carve_crosses_leaf();
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.0 - before.0, after.2 - before.2);
    assert_eq!(after.3 - before.3, after.4 - before.4);
}

fn scalar_pop() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let mut blocks = [core::ptr::null_mut(); 2];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), 2);
    for ptr in blocks {
        release(&mut core, ptr, 16);
    }
    let state = core.dbg_r11_issue_state_for_test(0).unwrap();
    let directory = core.dbg_directory_get_bit(0, 0);
    let system = SmallSidecar::system_totals_for_test();
    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(core
        .alloc(Layout::from_size_align(16, 16).unwrap())
        .is_null());
    assert_eq!(core.dbg_r11_issue_state_for_test(0), Some(state));
    assert_eq!(core.dbg_directory_get_bit(0, 0), directory);
    assert_eq!(SmallSidecar::system_totals_for_test(), system);
    assert!(blocks.iter().all(|&ptr| core.dbg_is_free_for(ptr)));
    let ptr = core.alloc(Layout::from_size_align(16, 16).unwrap());
    assert_eq!(ptr, blocks[1]);
    assert_eq!(core.dbg_live_count_for(ptr), Some(state.1 + 1));
    assert!(!core.dbg_is_free_for(ptr));
    release(&mut core, ptr, 16);
}

fn batch_drain() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    let mut blocks = [core::ptr::null_mut(); 260];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), blocks.len());
    let first_leaf = blocks[0].addr() / LEAF;
    assert!(blocks.iter().any(|ptr| ptr.addr() / LEAF != first_leaf));
    for ptr in blocks {
        release(&mut core, ptr, 16);
    }
    let state = core.dbg_r11_issue_state_for_test(0).unwrap();
    let directory = core.dbg_directory_get_bit(0, 0);
    let system = SmallSidecar::system_totals_for_test();
    let mut out = [anchor; 260];
    SmallSidecar::fail_prepare_after_for_test(257);
    // SAFETY: anchor remains a live allocation in this exclusively owned core.
    assert_eq!(
        unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) },
        0
    );
    assert_eq!(out, [anchor; 260]);
    assert_eq!(core.dbg_r11_issue_state_for_test(0), Some(state));
    assert_eq!(core.dbg_directory_get_bit(0, 0), directory);
    assert_eq!(SmallSidecar::system_totals_for_test(), system);
    assert!(blocks.iter().all(|&ptr| core.dbg_is_free_for(ptr)));
    // SAFETY: anchor is still live; this consumes the unchanged freelist once.
    assert_eq!(
        unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) },
        out.len()
    );
    for (issued, freed) in out.iter().zip(blocks.iter().rev()) {
        assert_eq!(issued, freed);
        assert!(!core.dbg_is_free_for(*issued));
    }
    assert_eq!(
        core.dbg_live_count_for(anchor),
        Some(state.1 + out.len() as u32)
    );
    for ptr in out {
        release(&mut core, ptr, 16);
    }
    release(&mut core, anchor, 16);
}

fn scalar_carve() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    let state = core.dbg_r11_issue_state_for_test(1).unwrap();
    let system = SmallSidecar::system_totals_for_test();
    SmallSidecar::fail_spill_after_for_test(1);
    assert!(core.dbg_r11_scalar_carve_for_test(1).is_none());
    assert_eq!(core.dbg_r11_issue_state_for_test(1), Some(state));
    assert_eq!(SmallSidecar::system_totals_for_test(), system);
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(anchor.addr()),
        Some(0)
    );
    let second = core.dbg_r11_scalar_carve_for_test(1).unwrap();
    assert_eq!(second.addr() % (4 * 1024 * 1024), (state.0 + 31) & !31);
    assert_eq!(core.dbg_live_count_for(anchor), Some(state.1 + 1));
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(second.addr()),
        Some(1)
    );
    release(&mut core, second, 32);
    release(&mut core, anchor, 16);
}

fn batch_carve_crosses_leaf() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    let bump = core.dbg_r11_issue_state_for_test(0).unwrap().0;
    let mut target = (bump / LEAF + 1) * LEAF - 32;
    if target < bump {
        target += LEAF;
    }
    let mut fill = vec![core::ptr::null_mut(); (target - bump) / 16];
    assert!(fill.len() <= LEAF / 16);
    assert_eq!(core.dbg_carve_batch(0, &mut fill), fill.len());
    let state = core.dbg_r11_issue_state_for_test(1).unwrap();
    assert_eq!(state.0, target);
    let system = SmallSidecar::system_totals_for_test();
    let mut out = [anchor; 4];
    SmallSidecar::fail_prepare_after_for_test(2);
    assert_eq!(core.dbg_carve_batch(1, &mut out), 0);
    assert_eq!(out, [anchor; 4]);
    assert_eq!(core.dbg_r11_issue_state_for_test(1), Some(state));
    let promoted = SmallSidecar::system_totals_for_test();
    assert_eq!(promoted.0 - system.0, 256);
    assert_eq!(promoted.3 - system.3, 1);
    assert_eq!(
        RouteRegistration::class_at_global_address_for_test(anchor.addr()),
        Some(0)
    );
    assert_eq!(core.dbg_carve_batch(1, &mut out), 4);
    assert_ne!(out[0].addr() / LEAF, out[1].addr() / LEAF);
    assert_eq!(core.dbg_live_count_for(anchor), Some(state.1 + 4));
    for ptr in out {
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(ptr.addr()),
            Some(1)
        );
        release(&mut core, ptr, 32);
    }
    for ptr in fill {
        release(&mut core, ptr, 16);
    }
    release(&mut core, anchor, 16);
}
