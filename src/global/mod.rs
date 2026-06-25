//! Phase 11 -- the `malloc` face: [`SeferMalloc`] (`unsafe impl GlobalAlloc`).
//!
//! Re-exports only -- no logic lives here (per the one-export-per-file rule).
//! The single confined-`unsafe` seam of this module is [`sefer_malloc`] (the
//! `GlobalAlloc` trait obligation + pointer handoff). See [`sefer_malloc`] for
//! the M5 (reentrancy-freedom) and no-panic proofs.
//!
//! [`sefer_malloc`]: self::sefer_malloc

mod sefer_malloc;

pub use sefer_malloc::SeferMalloc;
