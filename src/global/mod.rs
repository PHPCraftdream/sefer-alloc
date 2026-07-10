//! Phase 12.3 -- the alloc face: [`SeferAlloc`] (`unsafe impl GlobalAlloc`),
//! routed through the global heap registry (Phase 12.2) via raw-pointer TLS
//! (Phase 12.3) with a never-null primordial fallback heap (§2.3).
//!
//! Re-exports only -- no logic lives here (per the one-export-per-file rule).
//! The confined-`unsafe` seams of this module are:
//! - [`tls_heap`] -- the raw-pointer TLS binding (`Cell<*mut HeapCore>` +
//!   `AbandonGuard`); the `unsafe` is the pointer handoff under the
//!   single-writer invariant + the `unsafe fn recycle`/`abandon_segments`
//!   calls in the guard's drop.
//! - `fallback` -- the process-global always-live fallback heap; the
//!   `unsafe` is the `static mut MaybeUninit<HeapCore>` + atomic-init
//!   state-machine + spinlock-guarded `&mut` handout.
//! - `sefer_alloc` -- the `unsafe impl GlobalAlloc` trait obligation +
//!   pointer handoff to `HeapCore`.
//!
//! See each seam file for the M5 (reentrancy-freedom), no-panic, and never-
//! null (M10) proofs.
//!
//! [`tls_heap`]: self::tls_heap

mod alloc_stats;
mod fallback;
mod sefer_alloc;
// `pub` (not private) so the task #129 teardown-ordering test can reach the
// `#[doc(hidden)]` test hook `tls_heap::dbg_teardown_then_resolve_is_fallback`
// through `sefer_alloc::global::tls_heap` (this module itself is already
// `#[doc(hidden)] pub` in `lib.rs` — see the comment there).
pub mod tls_heap;

pub use alloc_stats::AllocStats;
pub use sefer_alloc::SeferAlloc;

// `#[doc(hidden)]` test-only hook (task L4): lets the fallback panic-safety
// regression test (`tests/regression_fallback_panic_lock.rs`) reach the
// otherwise-private `fallback` spinlock behaviour. Same established pattern as
// `tls_heap::dbg_teardown_then_resolve_is_fallback` above; not stable API.
#[cfg(feature = "std")]
#[doc(hidden)]
pub use fallback::dbg_panic_in_with_heap_releases_lock;
