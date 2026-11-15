//! P1.4 conduit (plan step P1.4,
//! docs/plans/2026-11-15-workspace-restructure-plan.md): the one sanctioned
//! process terminator (`fatal(msg)`) moved verbatim to the `sefer-platform`
//! crate (`crates/platform/sefer-platform/src/fatal.rs`). This file is the
//! sanctioned conduit exception that keeps the historical
//! `crate::alloc_core::fatal::fatal` path alive for every call site.
pub(crate) use sefer_platform::fatal::fatal;
