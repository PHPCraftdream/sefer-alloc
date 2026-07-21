//! R11-5: NUMA `current_node()` cache invalidation regression.
//!
//! Proves the per-`AllocCore` cached NUMA-node value is invalidated at
//! registry-slot `claim()` time so a recycled slot never hands the previous
//! owner's stale node to the new owning thread. See
//! `docs/PHASE_NUMA_DESIGN.md` §4.1 for the design note this test enforces.
//!
//! Mechanism: drive `HeapRegistry::claim` / `recycle` / `claim` against
//! `numa-shim`'s `mock` backend (gated on the `numa-aware-mock` feature,
//! which enables `numa-shim/mock` for deterministic scripting of what
//! `current_node()` returns). Without the `invalidate_numa_node_cache` call
//! in `HeapRegistry::claim`, the second claim would inherit the first
//! claim's cached value (a stale node from a now-recycled slot) and never
//! re-query — the exact bug §4.1's "slot-recycle correctness point" exists
//! to prevent.
//!
//! The cache populate step uses `dbg_populate_numa_cache_for_test` rather
//! than relying on `find_segment_with_free` to fire on a real alloc — a
//! leftover free block on a recycled slot (left by whatever test happened
//! to claim the same slot earlier in this binary) would make `pop_free`
//! succeed and skip `find_segment_with_free` (and thus skip the cache
//! populate), making the test flaky across test orderings. The deterministic
//! hook removes that dependency while still exercising the same
//! `current_node_cached` code path. A real `alloc` call ALSO fires in each
//! claim for end-to-end coverage.
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-global" --test numa_cache_invalidation

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-global"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use numa_shim::mock;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static
// and the mock's thread-local call log is per-thread; running these tests in
// parallel would let one test's claim/recycle race against another's, making
// the slot-index / cached-value assertions flaky. Same discipline as
// `tests/stamp_cache.rs`.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// Script the mock to return `node`, then clear the call log so any later
/// `mock::drain()` observes only calls made after this point.
fn script_node(node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
}

/// The headline regression: across a `claim → recycle → claim` cycle, the
/// newly-claimed slot's cached NUMA node must reflect the NEW mock value,
/// not the stale one from the previous owner.
#[test]
fn cached_node_invalidates_across_slot_recycle() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // ── First claim: cache should populate with the first mock node (7) ──
    script_node(7);
    let heap_a = HeapRegistry::claim();
    assert!(!heap_a.is_null(), "first claim returned null");

    // Deterministically populate the cache by directly invoking the cached
    // accessor (see the file doc for why we don't rely on a real alloc here).
    // SAFETY: `heap_a` is the live, sole-writer slot we just claimed.
    unsafe { (*heap_a).dbg_populate_numa_cache_for_test() };
    assert_eq!(
        unsafe { (*heap_a).dbg_cached_numa_node() },
        Some(7),
        "first claim: cache must be populated with the mock node 7"
    );

    // Also do a real alloc for end-to-end coverage of the cached path inside
    // `find_segment_with_free` — if it fires (it may not, depending on
    // free-list state), it must NOT overwrite the already-cached value.
    let layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: live claimed slot, single-writer.
    let p = unsafe { (*heap_a).alloc(layout) };
    assert!(!p.is_null(), "first alloc returned null");
    assert_eq!(
        unsafe { (*heap_a).dbg_cached_numa_node() },
        Some(7),
        "first claim: cache must still hold mock node 7 after a real alloc"
    );

    // Recycle the slot — the `HeapCore` stays whole (whole-slot reuse), but
    // `HeapRegistry::claim`'s R11-5 invalidation hook must reset the cache
    // on the NEXT claim.
    // SAFETY: `heap_a` was returned by `claim` above and not yet recycled.
    unsafe { HeapRegistry::recycle(heap_a) };

    // ── Second claim against a DIFFERENT mock node (11) ──
    script_node(11);
    let heap_b = HeapRegistry::claim();
    assert!(!heap_b.is_null(), "second claim returned null");

    // Immediately after claim, BEFORE any populate/alloc on the new claim,
    // the cache must be `None` — invalidation fired. (This is the assertion
    // that would fail against a naively-unconditional cache.)
    // SAFETY: raw-pointer deref of the live, claimed slot.
    assert_eq!(
        unsafe { (*heap_b).dbg_cached_numa_node() },
        None,
        "post-claim: cache must be invalidated (None) before any populate \
         on the new claim — a stale Some(7) here is the exact bug \
         invalidate_numa_node_cache exists to prevent"
    );

    // After populating on the new claim, the cache must reflect the NEW
    // mock node (11), NOT the stale 7.
    // SAFETY: live claimed slot, single-writer.
    unsafe { (*heap_b).dbg_populate_numa_cache_for_test() };
    assert_eq!(
        unsafe { (*heap_b).dbg_cached_numa_node() },
        Some(11),
        "post-populate on new claim: cache must hold the NEW mock node (11), \
         not the stale 7 from the previous owner"
    );

    // Real alloc on the new claim for end-to-end coverage.
    let layout2 = Layout::from_size_align(128, 8).unwrap();
    // SAFETY: same as above.
    let p2 = unsafe { (*heap_b).alloc(layout2) };
    assert!(!p2.is_null(), "second alloc returned null");
    assert_eq!(
        unsafe { (*heap_b).dbg_cached_numa_node() },
        Some(11),
        "post-alloc on new claim: cache must still hold the NEW mock node (11)"
    );

    // SAFETY: `heap_b` was returned by the second `claim` and not yet
    // recycled.
    unsafe { HeapRegistry::recycle(heap_b) };
    // Touch `p`/`p2` to suppress a dead-code warning; we intentionally leak
    // them (the slot has been recycled with the segments whole).
    let _ = (p, p2);
}

/// Companion: a single claim's repeated cache populate calls reuse the
/// cached value without re-querying the OS. Proves the cache itself (not
/// just the invalidation) works — `current_node()` is queried exactly once
/// on the first populate, then never again on the same claim. This is the
/// "amortisation" half of the design: if every populate re-queried, the
/// cache would be a no-op and the regression above would still pass.
#[test]
fn cached_node_amortises_within_a_claim() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    script_node(5);
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "claim returned null");

    // SAFETY: live claimed slot, single-writer for all of these.
    unsafe {
        // First populate: queries the mock, populates cache with 5.
        (*heap).dbg_populate_numa_cache_for_test();
        assert_eq!(
            (*heap).dbg_cached_numa_node(),
            Some(5),
            "first populate must query the mock and cache node 5"
        );

        // Subsequent populates: must NOT re-query the mock — the cache
        // already holds 5. (If they re-queried, the mock state could be
        // changed underneath us mid-claim, defeating the cache.)
        for _ in 0..5 {
            (*heap).dbg_populate_numa_cache_for_test();
            assert_eq!(
                (*heap).dbg_cached_numa_node(),
                Some(5),
                "subsequent populate: cache must stay at 5 (no re-query)"
            );
        }
    }

    // The mock records every `current_node()` dispatch. With the cache
    // working, exactly ONE dispatch fired (the first populate); the
    // bootstrap stamp in `AllocCore::new_inner` is one more, so the total
    // observed in this test's thread-local mock log is at most 2.
    let calls = mock::drain();
    let current_node_calls = calls
        .iter()
        .filter(|c| matches!(c, mock::MockCall::CurrentNode(_)))
        .count();
    assert!(
        current_node_calls <= 2,
        "expected at most 2 CurrentNode mock calls (1 first-populate cache \
         fill + 1 bootstrap stamp); cache appears not to be amortising, saw \
         {current_node_calls}"
    );

    // SAFETY: `heap` was returned by `claim` above.
    unsafe { HeapRegistry::recycle(heap) };
}
