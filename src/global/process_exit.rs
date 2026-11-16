//! P1.4 conduit (plan step P1.4,
//! docs/plans/2026-11-15-workspace-restructure-plan.md): the Windows
//! loader-shutdown FFI probe moved verbatim to the `sefer-platform` crate
//! (`crates/platform/sefer-platform/src/process_exit.rs`). This file is the
//! conduit that keeps the historical `global::process_exit` module surface —
//! including the ghost guard `super::process_exit_in_progress()` in
//! `tls_heap.rs` and the `cache/process-exit` ghost rule — unchanged.
pub use sefer_platform::process_exit::*;
