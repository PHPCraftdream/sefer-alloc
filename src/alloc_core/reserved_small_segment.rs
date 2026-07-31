//! [`ReservedSmallSegment`] — a typed, non-forgeable, move-consumed handle
//! standing in place of a bare `*mut u8` for exactly one measurement-only
//! hook pair: `AllocCore::dbg_decomp_reserve_and_keep` /
//! `AllocCore::dbg_decomp_release` (`alloc_core_small_pool.rs`).
//!
//! ## Why this type exists (R31-4, task #467)
//!
//! `docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §5 surveyed
//! every `dbg_*` hook in the crate and found this ONE pair is the only
//! current hook that both (a) mints a NEW raw pointer via a `dbg_*` call
//! (not merely accepting a pointer that already existed) and (b) requires
//! the caller to hold that exact value and hand it back later — the
//! mint-then-redeem shape a typed handle exists to make safe. Before this
//! type, `dbg_decomp_reserve_and_keep` returned a bare `Option<*mut u8>`
//! and `dbg_decomp_release` was an `unsafe fn(&mut self, base: *mut u8)`
//! guarded only by a `debug_assert!` (compiled out in `--release`) checking
//! `base != self.small_cur` — nothing stopped a caller from forging an
//! arbitrary pointer, and nothing stopped a caller from releasing the same
//! base twice.
//!
//! This type closes both gaps structurally:
//!
//! - **Unforgeable.** The only constructor is [`Self::new_from_reservation`],
//!   `pub(super)` — reachable only from inside `alloc_core_small_pool.rs`
//!   (the same module `AllocCore`'s own reservation methods live in), and
//!   even there it is called exactly once, from
//!   `AllocCore::dbg_decomp_reserve_and_keep`, immediately after a genuine
//!   `reserve_small_segment_impl()` call succeeds. The `base` field is
//!   private, so external code cannot construct a SECOND handle from a
//!   pointer it read out via [`Self::base`] (that method only reads the
//!   value; it is not a constructor) — the unforgeability guarantee is
//!   about minting handles, not about the pointer value being opaque.
//! - **Double-release is a compile error, not a runtime hazard.**
//!   `AllocCore::dbg_decomp_release` takes the handle BY VALUE and consumes
//!   it. The only way to obtain a `ReservedSmallSegment` is
//!   `dbg_decomp_reserve_and_keep`; the only way to consume one is
//!   `dbg_decomp_release` (or letting it drop — see the `Drop` impl below).
//!   Calling `dbg_decomp_release(handle)` a second time with the same
//!   variable does not compile: the first call MOVES `handle`, so a second
//!   call has no value left to move (rustc E0382, "use of moved value").
//!   This is verified externally by `tests/r31_4_reserved_small_segment_handle.rs`,
//!   whose module doc explains exactly why a second call cannot be written
//!   as a runtime test (see that file for the full argument).
//!
//! The existing `debug_assert!(base != self.small_cur, ...)` in
//! `dbg_decomp_release` stays as defence-in-depth (R30-1's own hazard
//! class — a reservation that somehow WAS published as the live cursor —
//! is still worth a cheap local check), but it is no longer the PRIMARY
//! guarantee against double-release; the type system is.

/// Opaque handle to a small segment reserved via
/// `AllocCore::dbg_decomp_reserve_and_keep`. See the module doc for the full
/// rationale. `#[doc(hidden)]`: not stable public API, same status as every
/// other `dbg_*` measurement type in this crate (e.g.
/// `SegmentStateReconciliation`).
#[doc(hidden)]
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#[derive(Debug)]
#[must_use = "a ReservedSmallSegment must be passed to AllocCore::dbg_decomp_release (or \
              consumed via into_base) — dropping it without releasing fires a debug-only \
              leak assertion at runtime instead of a compile-time warning"]
pub struct ReservedSmallSegment {
    /// Private FIELD: no external code can move/replace the field, or
    /// construct a `ReservedSmallSegment` via struct-literal syntax from an
    /// arbitrary pointer — the only way to CONSTRUCT one is
    /// [`Self::new_from_reservation`] (`pub(super)`). Reading the current
    /// value out (without constructing a new handle) is intentionally
    /// still possible via [`Self::base`] for measurement callers that need
    /// the address — see that method's doc for why this does not weaken
    /// the unforgeability or double-release guarantees.
    base: *mut u8,
}

#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
impl ReservedSmallSegment {
    /// Mint a handle around a freshly reserved segment base. `pub(super)` —
    /// callable only from within `alloc_core_small_pool.rs`'s own module
    /// tree, and in practice called from exactly one call site
    /// (`AllocCore::dbg_decomp_reserve_and_keep`, immediately after a real
    /// `reserve_small_segment_impl()` success). No other module — no test,
    /// no bench, no example — can construct a `ReservedSmallSegment` around
    /// an address it merely computed or received from elsewhere.
    pub(super) fn new_from_reservation(base: *mut u8) -> Self {
        Self { base }
    }

    /// Read the wrapped base WITHOUT consuming the handle — `#[doc(hidden)]
    /// pub`, the established "test-only export pattern" (see
    /// `src/lib.rs`'s and this module's own crate-doc notes), so a
    /// measurement harness in `examples/`/`tests/` can touch the reserved
    /// segment's payload pages (e.g. `write_volatile` for a first-touch
    /// page-fault measurement) between reserving and releasing, without
    /// being able to construct a SECOND handle from the returned address:
    /// this only reads the pointer, it does not mint a new
    /// `ReservedSmallSegment`. The double-release guarantee is unaffected —
    /// that guarantee comes from [`Self::into_base`] consuming `self` by
    /// value, not from hiding the pointer value itself.
    #[doc(hidden)]
    #[must_use]
    pub fn base(&self) -> *mut u8 {
        self.base
    }

    /// Consume the handle, yielding the wrapped base for
    /// `dbg_decomp_release` to pass through to
    /// `release_or_pool_empty_segment`, and disarm the `Drop` leak-detector
    /// below (this IS the release path, not a leak). `pub(super)` — not
    /// exposed outside this module tree, so external code still cannot
    /// extract the pointer; the only caller is
    /// `AllocCore::dbg_decomp_release`.
    pub(super) fn into_base(self) -> *mut u8 {
        let base = self.base;
        // Disarm Drop's leak-detector: this consumption IS the release,
        // not an accidental drop. `mem::forget` skips `Drop::drop` entirely
        // (no unsafe needed — `ReservedSmallSegment` holds no owned
        // resource `Drop` would otherwise need to release; the raw pointer
        // itself is not freed here, `release_or_pool_empty_segment` does
        // that separately in the caller).
        core::mem::forget(self);
        base
    }
}

/// Defence-in-depth: reaching `drop` without having gone through
/// `AllocCore::dbg_decomp_release` (i.e. without going through
/// [`ReservedSmallSegment::into_base`], which forgets `self` first) means
/// the handle was leaked by its caller (a measurement-harness bug — forgot
/// to release), not a soundness hazard by itself (the segment stays
/// correctly registered in the allocator's own table; nothing is
/// corrupted). Loud in debug builds only — a diagnostic tool aborting a
/// benchmark process over its own leak would be a worse failure mode than a
/// silent (but debug-visible) leak.
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
impl Drop for ReservedSmallSegment {
    fn drop(&mut self) {
        debug_assert!(
            false,
            "ReservedSmallSegment dropped without going through \
             AllocCore::dbg_decomp_release — measurement-harness bug (reservation leaked, \
             not an allocator-soundness issue: the segment stays correctly registered)"
        );
    }
}
