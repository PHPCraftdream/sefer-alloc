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
//!   `pub(super)` — since `reserved_small_segment` is a direct child module
//!   of `alloc_core` (`src/alloc_core/mod.rs`), `pub(super)` here resolves
//!   to `pub(in crate::alloc_core)`: reachable from anywhere inside
//!   `alloc_core`, not only `alloc_core_small_pool.rs` — Rust has no
//!   sibling-module-only visibility, so this is the tightest expressible
//!   bound. In practice it is called from exactly one call site,
//!   `AllocCore::dbg_decomp_reserve_and_keep` (`alloc_core_small_pool.rs:1095`),
//!   immediately after a genuine `reserve_small_segment_impl()` call
//!   succeeds. The `base` field is
//!   private, so external code cannot construct a SECOND handle from a
//!   pointer it read out via [`Self::dbg_base`] (that method only reads the
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
    /// still possible via [`Self::dbg_base`] for measurement callers that need
    /// the address — see that method's doc for why this does not weaken
    /// the unforgeability or double-release guarantees.
    base: *mut u8,
}

#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
impl ReservedSmallSegment {
    /// Mint a handle around a freshly reserved segment base. `pub(super)` —
    /// reachable from anywhere inside `alloc_core` (Rust has no
    /// sibling-module-only visibility, so this is the tightest expressible
    /// bound); in practice called from exactly one call site
    /// (`AllocCore::dbg_decomp_reserve_and_keep`, `alloc_core_small_pool.rs:1095`,
    /// immediately after a real `reserve_small_segment_impl()` success). No
    /// module OUTSIDE `alloc_core` — no test, no bench, no example — can
    /// construct a `ReservedSmallSegment` around an address it merely
    /// computed or received from elsewhere.
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
    ///
    /// Named `dbg_base` (R31-14b, task #484, closing P2-12 filed in
    /// `docs/CORRECTNESS_OPEN_ITEMS.md` item 9), not the bare `base` this
    /// method was originally named: `tests/dbg_hook_safety_tripwire.rs`'s
    /// `scan_file` only matches `pub fn dbg_*` / `pub unsafe fn dbg_*` by
    /// name prefix, so a raw-pointer-returning method named without the
    /// `dbg_` prefix was invisible to that tripwire even though it returns
    /// exactly the shape (a raw pointer out of a measurement-only type) the
    /// tripwire exists to enumerate. The R31-4 retrofit that moved this
    /// pointer-return off `dbg_decomp_reserve_and_keep` (which the tripwire
    /// DID scan) onto this method silently narrowed the tripwire's
    /// coverage; the `dbg_` prefix restores it without widening the
    /// scanner itself. The repeated `#[cfg(...)]` immediately below
    /// (redundant with the enclosing `impl` block's own gate, kept anyway)
    /// is required for the SAME reason as the rename: the tripwire's
    /// `scan_file` reads the attribute block immediately preceding each
    /// `pub fn dbg_*` line, not the enclosing `impl`'s attributes — this
    /// was the exact gap the rename surfaced (`cargo test --features
    /// "production bench-internals alloc-stats" --test
    /// dbg_hook_safety_tripwire` failed with "NEW unaccounted-for SAFE,
    /// non-bench-internals-gated hooks: ...::dbg_base" until this
    /// per-method `#[cfg]` was added).
    #[doc(hidden)]
    #[must_use]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_base(&self) -> *mut u8 {
        self.base
    }

    /// Consume the handle, yielding the wrapped base for
    /// `dbg_decomp_release` to pass through to
    /// `release_or_pool_empty_segment`, and disarm the `Drop` leak-detector
    /// below (this IS the release path, not a leak). `pub(super)` —
    /// reachable from anywhere inside `alloc_core`, same bound as
    /// [`Self::new_from_reservation`] above (Rust has no
    /// sibling-module-only visibility); not exposed OUTSIDE `alloc_core`, so
    /// external code still cannot extract the pointer this way. The only
    /// caller is `AllocCore::dbg_decomp_release`
    /// (`alloc_core_small_pool.rs:1117`).
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
