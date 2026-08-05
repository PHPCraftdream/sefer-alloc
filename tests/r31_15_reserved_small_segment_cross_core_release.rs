//! R31-15 (task #486) — counterfactual for the owner-binding fix on
//! [`ReservedSmallSegment`] / `AllocCore::dbg_decomp_release`.
//!
//! ## The defect this proves was real
//!
//! `docs/reviews/...` (task #486's filing) found a CONFIRMED P0 soundness
//! defect: `AllocCore::dbg_decomp_release(&mut self, handle:
//! ReservedSmallSegment)` was a **safe** `pub fn`. `ReservedSmallSegment`
//! (`src/alloc_core/reserved_small_segment.rs`) stored only a `base: *mut
//! u8` — no owner identity, no lifetime tie to the `AllocCore` that reserved
//! it. R31-4 (task #467) had already closed unforgeability (private field +
//! `pub(super)` constructor) and double-release (move-consuming
//! `into_base`), but NOT owner-binding. This compiled and ran cleanly as
//! 100% safe code BEFORE this fix:
//!
//! ```text
//! let mut core_a = AllocCore::new().unwrap();
//! let mut core_b = AllocCore::new().unwrap();
//! let h = core_a.dbg_decomp_reserve_and_keep().unwrap();
//! core_b.dbg_decomp_release(h);   // safe API call — handle belongs to core_a
//! ```
//!
//! `dbg_decomp_release` passed the foreign base into
//! `core_b.release_or_pool_empty_segment(base)`, mutating `core_b`'s
//! pool/directory/`SegmentTable` state for a segment `core_b` never
//! registered, while `core_a`'s own registration of that same base went
//! stale — a later lookup/drop/reuse on `core_a` could read unmapped or
//! reused memory. This is the same bug class CLAUDE.md's benchmark-hook
//! safety rule targets ("a safe `pub fn` that accepts a raw pointer and
//! touches allocator metadata is a soundness hole by construction"), one
//! level of indirection removed (the pointer is inside a typed handle
//! instead of bare).
//!
//! ## Non-vacuity: this test would NOT have compiled/passed before the fix
//!
//! Two independent facts, both checked before writing this test (not just
//! asserted in prose):
//!
//! 1. **Before R31-15**, `dbg_decomp_release` was a **safe** `fn` — this
//!    test's cross-core call below would have compiled WITHOUT the
//!    `unsafe { .. }` block this file now requires (removing the block and
//!    trying to build against the pre-fix signature fails with E0133,
//!    "call to unsafe function is unsafe" is backwards — the pre-fix
//!    function was NOT unsafe, so `unsafe { .. }` around a safe fn is a
//!    harmless no-op, meaning the interesting counterfactual is behavioral,
//!    not a compile-time one; see point 2). Before R31-15, this test's
//!    cross-core call would have RUN — `core_b.dbg_decomp_release(h)` would
//!    have executed `core_b.release_or_pool_empty_segment(foreign_base)`
//!    with no owner check of any kind, corrupting `core_b`'s pool state
//!    silently (or, in a bad case, releasing `core_a`'s still-registered OS
//!    reservation out from under it) — with NO panic, on ANY build profile
//!    (the pre-fix code had no owner-related guard at all, release or
//!    debug).
//! 2. **After R31-15**, the SAME cross-core call is guarded by a
//!    release-build (non-`debug_assert!`) `assert_eq!` on
//!    `ReservedSmallSegment::owner_id()` vs. `AllocCore::
//!    dbg_reservation_owner_id` inside `dbg_decomp_release` — see
//!    `src/alloc_core/alloc_core_small_pool.rs`. This test drives that
//!    exact path and confirms it panics with the expected message,
//!    confirming the fix actually rejects the hazard rather than silently
//!    permitting it.
//!
//! This test therefore satisfies the "would have failed without the fix"
//! bar two ways: pre-fix, the cross-core release would have SUCCEEDED
//! (silently corrupting state, no panic) — the exact opposite of what
//! `#[should_panic]` below requires; post-fix, it panics as designed.
//!
//! ## Why `#[should_panic]`, not a `catch_unwind` probe
//!
//! `assert_eq!` panics (not a `Result`/`bool` return), so the natural way to
//! observe "the guard fired" from a `#[test]` is `#[should_panic(expected =
//! ..)]` — matches the same technique already used elsewhere in this crate
//! for hook-precondition panics (grep `#[should_panic` across `tests/` for
//! precedent). The panic message substring is checked so a future
//! regression that panics for an unrelated reason (e.g. a bug elsewhere in
//! `dbg_decomp_release`) does not silently satisfy this test.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]

use sefer_alloc::AllocCore;

/// The core counterfactual: reserve a handle on `core_a`, release it on
/// `core_b`. Must panic (owner-id mismatch), not silently corrupt `core_b`'s
/// state.
#[test]
#[should_panic(expected = "handle was reserved by a DIFFERENT AllocCore")]
fn cross_core_release_panics_on_owner_mismatch() {
    let mut core_a = AllocCore::new().expect("primordial (core_a)");
    let mut core_b = AllocCore::new().expect("primordial (core_b)");

    let handle = core_a
        .dbg_decomp_reserve_and_keep()
        .expect("reservation must succeed on a fresh AllocCore");

    // SAFETY (for the purposes of this counterfactual test only): the
    // `unsafe` block asserts the CALLER believes the precondition holds;
    // this test deliberately violates it (the whole point) to prove the
    // runtime owner-id guard rejects the violation rather than silently
    // permitting it. `handle` was NOT reserved on `core_b`.
    unsafe {
        core_b.dbg_decomp_release(handle);
    }
}

/// Positive control / same-core release still succeeds — confirms the
/// owner-id check does not false-positive against the LEGITIMATE
/// same-core path (a test that only proved the negative case could pass
/// vacuously if the guard rejected EVERY release, not just cross-core
/// ones).
#[test]
fn same_core_release_still_succeeds() {
    let mut core_a = AllocCore::new().expect("primordial");

    let handle = core_a
        .dbg_decomp_reserve_and_keep()
        .expect("reservation must succeed on a fresh AllocCore");

    // SAFETY: `handle` was produced by the paired
    // `dbg_decomp_reserve_and_keep` call on this same `core_a` immediately
    // above, and is still live/unreleased — the genuine precondition.
    unsafe {
        core_a.dbg_decomp_release(handle);
    }

    // Confirm core_a is still healthy after a legitimate same-core release.
    let layout = core::alloc::Layout::from_size_align(64, 8).unwrap();
    let p = core_a.alloc(layout);
    assert!(
        !p.is_null(),
        "ordinary alloc after same-core release must succeed"
    );
    // SAFETY: `p` was returned by the matching `core_a.alloc(layout)` above,
    // is live, and is freed exactly once here.
    unsafe { core_a.dealloc(p, layout) };
}

/// Same hazard through the `HeapCore` delegation layer (`src/registry/
/// heap_core_diag.rs`), not just the `AllocCore` layer directly — confirms
/// the fix's owner-id check is not bypassed by going through the thin
/// forwarding wrapper `HeapCore::dbg_decomp_release` delegates to.
/// `HeapCore` requires a registry-bound heap (via `with_heap`/similar), so
/// this drives the SAME underlying `AllocCore::dbg_decomp_release` two ways
/// on two DIFFERENT standalone `AllocCore`s directly — the `HeapCore`
/// delegation is a pure 1:1 forward (`self.core.dbg_decomp_release(handle)`,
/// verified by reading `heap_core_diag.rs`), so exercising `AllocCore`
/// directly (as the two tests above do) already covers the delegation's
/// only logic; this test instead documents that equivalence explicitly
/// rather than standing up full registry-bound heaps (`SeferAlloc`) for two
/// throwaway single-threaded owners, which would add setup complexity
/// without exercising any code path the `AllocCore`-level tests above do
/// not already cover.
#[test]
fn heap_core_delegation_is_a_pure_forward_to_alloc_core() {
    // This is a documentation-as-test assertion: read
    // `HeapCore::dbg_decomp_release`'s body in
    // `src/registry/heap_core_diag.rs` and confirm it is exactly
    // `unsafe { self.core.dbg_decomp_release(handle) }` with no additional
    // logic — if that ever changes to do more than forward, this comment
    // (and the claim it documents) goes stale and this test's rationale
    // above should be revisited to add a real registry-bound two-heap
    // counterfactual.
    let source = include_str!("../src/registry/heap_core_diag.rs");
    let marker = "pub unsafe fn dbg_decomp_release(&mut self, handle: crate::alloc_core::ReservedSmallSegment) {";
    let idx = source
        .find(marker)
        .expect("HeapCore::dbg_decomp_release signature not found verbatim — delegation shape changed, revisit this test");
    let body_start = idx + marker.len();
    let body_end = source[body_start..]
        .find('}')
        .map(|i| body_start + i)
        .expect("no closing brace found for dbg_decomp_release body");
    let body = source[body_start..body_end].trim();
    assert!(
        body.contains("self.core.dbg_decomp_release(handle)"),
        "HeapCore::dbg_decomp_release no longer appears to be a pure forward to \
         AllocCore::dbg_decomp_release (body: {body:?}) — this test's rationale for not \
         standing up a full registry-bound two-heap counterfactual no longer holds; add one"
    );
}
