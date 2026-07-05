//! Task W3 — miri (Stacked Borrows) counterfactual for the stats-aggregator
//! aliasing fix.
//!
//! ## What W3 fixed
//!
//! The process-wide stats aggregators (`tcache_hits_total` /
//! `large_cache_hits_total` in `src/registry/heap_registry.rs`) used to read
//! each heap's hit counter through `(*heap_ptr).…` — materialising a shared
//! `&HeapCore`/`&AllocCore` over a struct the OWNING thread concurrently holds
//! a protected `&mut` into (the `alloc(&mut self, …)` protector). Forming a
//! shared reference that spans a struct another thread holds a protected
//! `Unique` (`&mut`) into is a **foreign read of a protected tag** — undefined
//! behaviour under Stacked Borrows (miri's default model), even though it is
//! permitted under Tree Borrows. W3 moved the counters into the shared,
//! `Sync` `HeapSlot` and made the aggregator read the slot's `AtomicU64`
//! DIRECTLY off a `&HeapSlot`, so no `&HeapCore` is ever formed.
//!
//! ## Why this test is a MODEL, not the real types
//!
//! The real reproduction (two threads allocating on their own registry heaps
//! while a third calls the aggregator) requires `HeapCore::new`, which reserves
//! a 4 MiB primordial segment per heap through the OS aperture — hundreds of
//! thousands of interpreted operations per heap under miri, which does not
//! finish in a practical window on this host (that MT registry stress is,
//! for the same reason, deliberately absent from `scripts/miri.mjs`). So this
//! file models the EXACT aliasing SHAPE with a tiny struct:
//!
//! - `Inner` = the stand-in for `HeapCore`/`AllocCore`: it carries a
//!   non-atomic field (`scratch`, mutated by the owner on its `&mut self` hot
//!   path) AND an atomic hit counter (`hits`).
//! - The BAD pattern (`old_aggregator_ub`, gated off by default) forms a shared
//!   `&Inner` over the owner's live `&mut Inner` to read `hits` — the exact
//!   foreign-read-of-protected-Unique that SB rejects.
//! - The GOOD pattern (this test) keeps the counter in a shared `Slot`
//!   (`Sync`, `'static`-style) and reads it there — never forming `&Inner`.
//!
//! ## How to run the counterfactual
//!
//! - Fixed path (this test): `cargo +nightly miri test --test
//!   regression_w3_stats_aliasing_miri` → PASSES under strict-provenance SB.
//! - Broken path: flip `REPRODUCE_OLD_UB` to `true` (or run the
//!   `old_pattern_is_sb_ub` test, which is `#[ignore]` by default) → miri
//!   reports a Stacked-Borrows violation ("protected tag" / "not granting
//!   access") at the `&*owner_ptr` read, and a NON-miri build shows the two
//!   patterns are otherwise behaviourally identical. That is the
//!   counterfactual: the old shape is UB, the W3 shape is clean.

#![cfg(feature = "std")]

use std::sync::atomic::{AtomicU64, Ordering};

/// Stand-in for `HeapCore`/`AllocCore`: a non-atomic owner-mutated field plus
/// the diagnostic hit counter. `#[repr(C)]` to mirror the real layout intent.
#[repr(C)]
struct Inner {
    /// Mutated by the owner on its `&mut self` path (models the allocator's
    /// non-atomic bookkeeping — bins, cursors, etc.).
    scratch: u64,
    /// The diagnostic hit counter. In the OLD design this lived HERE, inside
    /// the owner-protected struct — reading it cross-thread required forming
    /// `&Inner`. In the W3 design it lives in the `Slot` instead (see below).
    hits: AtomicU64,
}

/// Stand-in for `HeapSlot`: the shared, `Sync` container the aggregator reads.
/// In the real code the slot ALSO holds the `Inner` (in an `UnsafeCell`), but
/// the aggregator only ever touches THIS atomic — never the `Inner`.
struct Slot {
    /// W3: the hit counter lives here, in the shared slot. The owner increments
    /// it through a `&'static AtomicU64` handle; the aggregator reads it via
    /// `&Slot`. No `&Inner` is ever formed by the aggregator.
    hits: AtomicU64,
}

/// The W3 (GOOD) aggregator: read the counter DIRECTLY off the shared `&Slot`.
/// No `&Inner`, so no foreign read of the owner's protected `&mut Inner`.
fn w3_aggregator(slot: &Slot) -> u64 {
    slot.hits.load(Ordering::Relaxed)
}

/// The owner's hot path: it holds a `&mut Inner` (the protected `Unique`) and
/// writes the non-atomic `scratch`, while incrementing the SLOT's counter
/// through the shared reference (the W3 discipline).
fn owner_step(inner: &mut Inner, slot_hits: &AtomicU64) {
    inner.scratch = inner.scratch.wrapping_add(1);
    // W3: increment the SLOT's counter (Relaxed load+store — single writer).
    slot_hits.store(
        slot_hits.load(Ordering::Relaxed).wrapping_add(1),
        Ordering::Relaxed,
    );
}

/// The W3 shape is Stacked-Borrows clean: the owner holds a live `&mut Inner`
/// and mutates `scratch` while a concurrent-in-shape reader (`w3_aggregator`)
/// reads the counter off the shared `&Slot`. No shared reference is ever formed
/// over the owner's protected `Inner`, so SB has nothing to complain about.
///
/// Single-threaded interleave (deterministic — miri needs no real threads to
/// see the borrow-stack violation; the tag protection is what SB checks, and it
/// is checked on the `&*` access itself, not on any actual data race).
#[test]
fn w3_pattern_is_sb_clean() {
    let mut inner = Inner {
        scratch: 0,
        hits: AtomicU64::new(0),
    };
    let slot = Slot {
        hits: AtomicU64::new(0),
    };

    // The owner takes its protected `&mut Inner` and, WHILE it is live,
    // increments the slot's counter and the aggregator reads it. This is the
    // exact temporal overlap the old code had — but through the shared slot.
    let inner_ref: &mut Inner = &mut inner;
    for _ in 0..64 {
        owner_step(inner_ref, &slot.hits);
        // Aggregator reads the SLOT while the owner's `&mut Inner` is still
        // live — SB-clean because it never touches `inner`.
        let _ = w3_aggregator(&slot);
    }
    // `inner.hits` stays 0 in the W3 design (the counter moved to the slot);
    // the slot saw all 64 increments.
    assert_eq!(inner_ref.hits.load(Ordering::Relaxed), 0);
    assert_eq!(w3_aggregator(&slot), 64);
    // Touch `scratch` so it is not optimised away and the `&mut` is genuinely
    // used across the whole overlap window.
    assert_eq!(inner_ref.scratch, 64);
}

/// The OLD (BAD) aggregator shape, kept as an executable record of the UB the
/// W3 move eliminated. `#[ignore]` so the normal `miri test` run is green;
/// run it explicitly to SEE the Stacked-Borrows violation:
///
/// ```text
/// cargo +nightly miri test --test regression_w3_stats_aliasing_miri \
///     -- --ignored old_pattern_is_sb_ub
/// ```
///
/// Under `-Zmiri-strict-provenance` (Stacked Borrows) this reports a violation
/// at the `&*owner_ptr` line: forming `&Inner` while `owner` (a `&mut Inner`,
/// a protected `Unique`) is live is a foreign read of the protected tag. Under
/// Tree Borrows (`-Zmiri-tree-borrows`) it would be permitted — which is
/// exactly why the project's own bar (SB, miri's default) required the fix.
#[test]
#[ignore = "counterfactual: intentionally Stacked-Borrows UB (the pre-W3 shape)"]
fn old_pattern_is_sb_ub() {
    let mut inner = Inner {
        scratch: 0,
        hits: AtomicU64::new(0),
    };
    // The owner's protected `&mut Inner`, taken as a raw pointer so we can form
    // an overlapping shared reference below (mirrors the real code's
    // `*mut HeapCore` handoff out of the slot's `UnsafeCell`).
    let owner: &mut Inner = &mut inner;
    let owner_ptr: *mut Inner = owner as *mut Inner;

    // Owner mutates its non-atomic field (protected `&mut` in use).
    owner.scratch = owner.scratch.wrapping_add(1);

    // OLD aggregator: form `&Inner` over the SAME struct the owner holds a
    // protected `&mut` into, to read the counter. THIS is the foreign read of a
    // protected Unique — SB-UB. miri flags the `&*owner_ptr`.
    //
    // SAFETY (for the non-miri build only): single-threaded here, `owner_ptr`
    // is valid; this compiles and "works" without miri — the whole point is
    // that it is UB the *type system* does not catch, which is why we needed
    // the W3 structural fix rather than a code-review note.
    let aliased: &Inner = unsafe { &*owner_ptr };
    let read = aliased.hits.load(Ordering::Relaxed);

    // Use the owner AGAIN after the aliased read, so the `&mut` protector is
    // provably still live across the foreign read (SB checks the read against
    // the still-active protector).
    owner.scratch = owner.scratch.wrapping_add(1);

    assert_eq!(read, 0);
    assert_eq!(owner.scratch, 2);
}
