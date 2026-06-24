//! Concurrent lock-free tier (Phase 3b-I).
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).

mod lock_free_handle;
mod lock_free_region;

pub use lock_free_handle::LockFreeHandle;
pub use lock_free_region::LockFreeRegion;
