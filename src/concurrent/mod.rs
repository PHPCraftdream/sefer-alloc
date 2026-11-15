//! Concurrent tier re-export shim (workspace-restructure plan step P1.2).
//!
//! The tier itself moved to the `sefer-concurrent` crate
//! (`crates/containers/sefer-concurrent`). This module exists only to keep the
//! crate-root re-exports in `src/lib.rs` (`pub use concurrent::{...}`) and the
//! public paths behind them working unchanged — re-exports only, no logic
//! (per the one-export-per-file rule the old module header documented).

#[allow(deprecated)]
pub use sefer_concurrent::{
    EpochHandle, EpochRegion, LockFreeHandle, LockFreeRegion, ShardedHandle, ShardedRegion,
};

#[cfg(feature = "pinning")]
pub use sefer_concurrent::PinnedRunner;
