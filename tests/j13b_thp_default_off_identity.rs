//! J13b: with no large-THP knob set the process tuple is the default (Off), so
//! a live Large block is never advised, never charged and its header charge
//! stays zero. One scenario per binary: the tuple and ledger are process-global.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;

#[test]
fn default_tuple_never_advises_or_charges() {
    let mut core = AllocCore::new().expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    assert_eq!(AllocCore::dbg_thp_tuple(), ThpTuple::DEFAULT);

    let layout = Layout::from_size_align(16 * MIB, 8).unwrap();
    let p = core.alloc(layout);
    assert!(!p.is_null());
    assert!(core.dbg_span_usable_of(p) >= 16 * MIB);

    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), 0);
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(core.large_thp_charge_for_test(p), Some(0));
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    // SAFETY: `p` is live with its original layout.
    unsafe { core.dealloc(p, layout) };
    drop(core);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), 0);
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
}
