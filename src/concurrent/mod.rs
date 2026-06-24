//! Concurrent tier (Phase 3b).
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).

mod epoch_handle;
mod epoch_region;
mod hand;
mod lock_free_handle;
mod lock_free_region;

pub use epoch_handle::EpochHandle;
pub use epoch_region::EpochRegion;
pub use lock_free_handle::LockFreeHandle;
pub use lock_free_region::LockFreeRegion;
