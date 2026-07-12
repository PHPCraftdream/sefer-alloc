//! R2-2 regression: the abandoned-segments stack API is `unsafe`.
//!
//! `HeapRegistry::push_abandoned_segment` / `pop_abandoned_segment`
//! (`src/registry/heap_registry.rs`) were SAFE `pub fn`s reachable from
//! downstream safe code via the `#[doc(hidden)] pub mod registry`. A safe caller
//! could publish a raw segment base onto the process-global abandoned stack and
//! then free the segment's memory (`AllocCore::drop` never purges the stack); a
//! subsequent `pop_abandoned_segment` dereferences that freed base to follow the
//! Treiber chain (`SegmentMeta::new(base).next_abandoned_atomic().load(...)`) —
//! a use-after-free (round2 finding R2-2, HIGH/potentially CRITICAL).
//!
//! ## The fix
//!
//! Both functions are now `pub unsafe fn`, matching the existing discipline of
//! `abandon_segments` (`pub unsafe fn`, the internal push path) and `try_adopt`
//! (`pub unsafe fn`, the internal adopt/pop path). The `# Safety` contract on
//! `push_abandoned_segment` makes the caller responsible for keeping the segment
//! mapped while its base is on the global stack — exactly what the internal
//! `abandon_segments` path already upholds (registry heaps are recycled, not
//! dropped, so their abandoned segments stay mapped until adopted).
//!
//! ## What this file proves
//!
//! This is a COMPILE-TIME soundness fix, so the RED→GREEN regression guard is a
//! pair of `compile_fail` doctests on the two functions in
//! `src/registry/heap_registry.rs`: they prove safe code can no longer reach the
//! API. (Before the fix the safe call compiled, so each `compile_fail` doctest
//! was RED — `compile_fail` is violated by a successful compile; after the fix
//! the call is rejected with E0133 "call to unsafe function", so the doctests
//! are GREEN. The RED→GREEN was verified by reverting the `unsafe` markers.)
//!
//! This file pins the BEHAVIOURAL contract: a caller that UPHOLDS the `# Safety`
//! contract (a real, still-mapped segment) round-trips the exact base through
//! push→pop, UB-free. It is deliberately tiny so it ALSO serves as the
//! plain-miri target for the abandoned-stack pop dereference — the
//! abandoned-segs intrusive stack packs real pointer addresses via
//! `expose_provenance` and re-derives them via `with_exposed_provenance_mut` BY
//! DESIGN, so it runs under PLAIN miri (Stacked Borrows, non-strict provenance),
//! not the strict-provenance matrix (see `scripts/miri.mjs` PLAIN_MATRIX).

#![cfg(feature = "alloc-global")]

use core::sync::atomic::Ordering;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise tests in this file: the registry and its abandoned-segs stack are
// process-global; the round-trip below asserts exact stack contents, which
// requires no interleaving with another test touching the same statics under
// `cargo test`'s default multi-threaded runner.
static SERIAL: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// RAII guard that holds the serial flag for the duration of a test (mirrors
/// `tests/registry_basic.rs`'s pattern — the one-shot mutex equivalent of the
/// `serial_test` crate, without adding a dev-dep).
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

/// Drain any leftover abandoned-segment entries so a test starts from a
/// known-empty stack.
fn drain_abandoned() {
    // SAFETY: drain-only cleanup; every base on the global abandoned stack was
    // pushed by the internal `abandon_segments` path, which keeps the segment
    // mapped until adoption, so each popped base is safe to dereference here.
    while unsafe { HeapRegistry::pop_abandoned_segment() }.is_some() {}
}

/// R2-2: a caller that UPHOLDS `push_abandoned_segment`'s `# Safety` contract (a
/// real, SEGMENT-aligned base of a segment that stays mapped while it is on the
/// global stack) round-trips the exact base through push→pop. The pop
/// DEREFERENCES the pushed base to read its `next_abandoned` link — the exact
/// read that is a UAF when the contract is violated (the R2-2 bug) — and this
/// test exercises that read on a VALID, still-mapped segment. Under plain miri
/// it validates that dereference is UB-free on the legitimate path.
///
/// NON-VACUOUS: the `popped == base` assertion fails if the abandoned-head
/// packing truncates the address (FINDINGS №1) or the push/pop CAS protocol
/// drops/reshuffles the entry; the `is_none()` assertions fail if the stack
/// state leaks across the round-trip.
#[test]
fn abandoned_stack_unsafe_api_round_trips_a_live_segment() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    drain_abandoned();

    // Empty stack: pop returns None (no base to dereference).
    // SAFETY: the stack was just drained — empty — so the pop returns None
    // without dereferencing any base.
    assert!(
        unsafe { HeapRegistry::pop_abandoned_segment() }.is_none(),
        "pop on an empty abandoned stack must return None"
    );

    // Obtain a REAL segment base from a claimed heap: the pop reads the segment
    // header, so a fake address would crash. A fresh heap owns its primordial
    // segment.
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "claim must succeed");
    // SAFETY: `heap` was returned by `claim` and is the slot's sole writer.
    let heap_ref: &mut sefer_alloc::registry::HeapCore = unsafe { &mut *heap };
    let base = heap_ref
        .segment_bases()
        .next()
        .expect("a fresh heap owns at least its primordial segment");
    assert!(!base.is_null(), "segment base must be non-null");

    // THE R2-2 CONTRACT IN ACTION: push the base onto the global stack under the
    // `unsafe` signature. The segment stays mapped for the whole test (`heap` is
    // neither dropped nor recycled, matching the `abandon_pop_round_trip` pattern
    // in `tests/registry_basic.rs`), so the stack never holds a stale pointer —
    // precisely the discipline the old SAFE signature let a caller bypass.
    // SAFETY: `base` is a real, SEGMENT-aligned base of a segment that stays
    // mapped until the pop below removes it from the stack.
    unsafe { HeapRegistry::push_abandoned_segment(base) };

    // The pop dereferences `base` to read `next_abandoned` (the R2-2 UAF site).
    // SAFETY: the only base on the stack is the one pushed above (still mapped).
    let popped =
        unsafe { HeapRegistry::pop_abandoned_segment() }.expect("pop must return the pushed base");
    assert_eq!(
        popped, base,
        "pop must return the exact base that was pushed (no address truncation)"
    );

    // After the pop the stack is empty again.
    // SAFETY: empty stack — the single entry was just popped.
    assert!(
        unsafe { HeapRegistry::pop_abandoned_segment() }.is_none(),
        "after popping the only entry, the stack must be empty"
    );
}
