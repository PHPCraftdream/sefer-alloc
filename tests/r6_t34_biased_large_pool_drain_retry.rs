//! R6-T34 (R6-CR-9) — the non-NUMA biased Large reserve path must drain the
//! pooled small segments and retry ONCE when the OS refuses the reservation,
//! mirroring the NUMA branch and the aligned (align < SEGMENT) path.
//!
//! Counterfactual: without the fix, the injected first-attempt failure makes
//! `reserve_biased` return None once and `alloc_large` concede null — the
//! `!p.is_null()` assertion goes red. With the fix, the pool is drained
//! (releasing the pooled segments' VA back to the OS) and the REAL retry
//! succeeds.
//!
//! Pool precondition is asserted directly via `HeapCore::dbg_pooled_count`
//! (alloc-decommit + internals); if that assertion proves unreachable in some
//! feature combination, the drain attempt still runs and this doc records it.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::{AllocCore, SegmentLayout};
use std::alloc::Layout;

const SEGMENT: usize = SegmentLayout::SEGMENT;
const SMALL: usize = 256;

/// Drop guard: the injection hook is process-global; a test that sets it MUST
/// clear it even on panic.
struct HookGuard;

impl Drop for HookGuard {
    fn drop(&mut self) {
        AllocCore::dbg_set_inject_biased_reserve_fail(false);
    }
}

#[test]
fn biased_large_reserve_drains_pool_and_retries() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // Fill the hysteresis pool: spread small allocations over several fresh
    // small segments, then free them all so each emptied segment is pooled.
    let small_layout = Layout::from_size_align(SMALL, 8).expect("small layout");
    let mut batches: Vec<Vec<*mut u8>> = Vec::new();
    for _ in 0..3 {
        let mut ptrs = Vec::new();
        // One batch comfortably exceeds one fresh small segment's capacity,
        // forcing multiple segments.
        for _ in 0..20_000 {
            let p = (*heap).alloc(small_layout);
            assert!(!p.is_null(), "small alloc");
            ptrs.push(p);
        }
        batches.push(ptrs);
    }
    for ptrs in &batches {
        for &p in ptrs {
            // SAFETY: p is a live small allocation with exactly small_layout.
            unsafe { (*heap).dealloc(p, small_layout) };
        }
    }
    drop(batches);
    let pooled = (*heap).dbg_pooled_count();
    assert!(pooled > 0, "expected pooled segments after emptying, got 0");

    // Inject: the FIRST biased Large reserve attempt fails; the post-drain
    // retry must be a real, unhooked call.
    AllocCore::dbg_set_inject_biased_reserve_fail(true);
    let _guard = HookGuard;
    let biased_layout = Layout::from_size_align(SEGMENT, SEGMENT * 2).expect("biased layout");
    let p = (*heap).alloc(biased_layout);
    drop(_guard);
    assert!(
        !p.is_null(),
        "R6-CR-9: biased Large must drain the pool and retry once"
    );
    // SAFETY: p is valid for biased_layout.size(); write and read back.
    unsafe {
        core::ptr::write_bytes(p, 0xC3, biased_layout.size());
        assert_eq!(p.add(biased_layout.size() - 1).read(), 0xC3);
    }
    // SAFETY: p is a live allocation from this heap with biased_layout.
    unsafe { (*heap).dealloc(p, biased_layout) };
}
