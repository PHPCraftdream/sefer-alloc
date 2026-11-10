//! J13b: a charged Large block freed REMOTELY keeps its charge until the
//! owner's terminal release. The owner drains the remote free and the cache
//! trim releases the mapping: the charge returns exactly once and the ledger
//! empties. The cache-refusing variant of the owner-side reclaim cannot be
//! forced from a root test with the existing hooks (the claimed heap exposes
//! no cache-budget setter), so only the admitting path is covered. One
//! scenario per binary.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpAdvice;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const BUDGET: usize = 8 * MIB;

struct SendPtr(*mut u8);
// SAFETY: ownership crosses the channel once; only the receiver frees it.
unsafe impl Send for SendPtr {}

#[test]
fn remote_free_keeps_charge_until_owner_terminal_release() {
    // Publish the process tuple through an explicit config, then drop it.
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    drop(AllocCore::new_with_config(cfg).expect("primordial bootstrap"));
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);

    let l = Layout::from_size_align(16 * MIB, 8).unwrap();
    // SAFETY: valid non-zero layout.
    let p = unsafe { GLOBAL.alloc(l) };
    assert!(!p.is_null());
    let charged = AllocCore::dbg_thp_charged_total();
    let expect = if cfg!(target_os = "linux") {
        // The span is at least the 16 MiB request, so the budget is the cap.
        ThpAdvice::extent(
            16 * MIB,
            LargeThpShare::ONE,
            Some(BUDGET),
            aligned_vmem::page_size(),
        )
    } else {
        0
    };
    assert_eq!(charged, expect as u64);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), expect);

    let (tx, rx) = mpsc::sync_channel::<SendPtr>(1);
    let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(1);
    let reaper = std::thread::spawn(move || {
        for SendPtr(ptr) in rx {
            // SAFETY: freed exactly once with the allocation layout.
            unsafe { GLOBAL.dealloc(ptr, l) };
            let _ = ack_tx.send(());
        }
    });
    tx.send(SendPtr(p)).unwrap();
    ack_rx.recv().unwrap();
    drop(tx);
    reaper.join().unwrap();

    // A remote free alone releases nothing.
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), expect);

    // The owner's trim drains the remote free, reclaims it into the cache and
    // evicts the cache: the terminal release returns the charge once.
    GLOBAL.dbg_trim_current_thread();
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), expect as u64);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
}
