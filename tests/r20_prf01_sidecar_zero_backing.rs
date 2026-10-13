//! OPT-04 counterfactual: `SmallSidecar::initialize_at` no longer rewrites
//! the pending/summary words because the only creation path
//! (`directory.rs` `EntryHandle::new`) uses `System.alloc_zeroed` and never
//! recycles sidecar storage. The zeroed-backing contract is kept observable:
//! debug builds verify the backing. This test poisons the storage, calls the
//! construction hook and REQUIRES the verification to reject it — if someone
//! later swaps `alloc_zeroed` for `alloc`, adds a recycle path without a
//! fresh `alloc_zeroed`, or deletes the verification, this test turns red.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals",
    debug_assertions
))]

use sefer_alloc::registry::segment_route::SmallSidecar;
use std::alloc::{GlobalAlloc, Layout, System};

const POISON: u64 = 0xDEAD_BEEF_1234_5678;

fn poison(raw: *mut u8, layout: Layout) {
    for i in 0..layout.size() / 8 {
        // Every aligned 8-byte word, including pending and summary fields.
        unsafe { raw.add(i * 8).cast::<u64>().write_volatile(POISON) };
    }
}

#[test]
fn initialize_at_rejects_non_zeroed_backing() {
    let layout = Layout::new::<SmallSidecar>();
    let raw = unsafe { System.alloc(layout) };
    assert!(!raw.is_null());
    poison(raw, layout);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: test-only exclusive raw storage with SmallSidecar's exact
        // layout; never published and freed below without dropping.
        unsafe { SmallSidecar::dbg_initialize_at_for_test(raw.cast()) };
    }));
    unsafe { System.dealloc(raw, layout) };
    assert!(
        caught.is_err(),
        "initialize_at accepted non-zeroed pending/summary backing: \
         the alloc_zeroed-only contract was broken without restoring the \
         zero-writing (or the verification was deleted)"
    );
}

#[test]
fn initialize_at_accepts_zeroed_backing() {
    let layout = Layout::new::<SmallSidecar>();
    let raw = unsafe { System.alloc_zeroed(layout) };
    assert!(!raw.is_null());
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: same test-only exclusive-storage contract as above.
        unsafe { SmallSidecar::dbg_initialize_at_for_test(raw.cast()) };
    }));
    unsafe { System.dealloc(raw, layout) };
    assert!(caught.is_ok(), "zeroed backing must pass the verification");
}
