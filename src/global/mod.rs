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
//! - [`fallback`] -- the process-global always-live fallback heap; the
//!   `unsafe` is the `static mut MaybeUninit<HeapCore>` + atomic-init
//!   state-machine + spinlock-guarded `&mut` handout.
//! - [`sefer_alloc`] -- the `unsafe impl GlobalAlloc` trait obligation +
//!   pointer handoff to `HeapCore`.
//!
//! See each seam file for the M5 (reentrancy-freedom), no-panic, and never-
//! null (M10) proofs.
//!
//! [`tls_heap`]: self::tls_heap
//! [`fallback`]: self::fallback
//! [`sefer_alloc`]: self::sefer_alloc

mod fallback;
mod sefer_alloc;
mod tls_heap;

pub use sefer_alloc::SeferAlloc;
