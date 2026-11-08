//! `large_thp_observation` — the J13b bounded-observation PURE foundation:
//! the deterministic control selector and the bounded residency sampling
//! planner. Everything here is `core`-only arithmetic: no atomics, no
//! unsafe, no allocation, no OS access, no configuration surface, no
//! allocator lifecycle, no runtime wiring.
//!
//! Contracts are fixed by `docs/perf/J13B_BOUNDED_THP_PREREG.md` (its
//! 2026-11-06 amendment takes precedence over the older text): control
//! selection is stratified systematic over aligned cohorts of 8 eligible
//! fresh requests (a hard at-most-one-per-cohort bound, expected rate
//! 1/8); residency observation plans at most 16 windows of at most 2 MiB
//! each, and the future metric is resident BASE pages divided by sampled
//! BASE pages — never "regions with any resident page".
//!
//! ## Family overview
//!
//! - [`bounded_mix`]: the shared bounded seeded decorrelation primitive
//!   (one splitmix64-style finalization per call; no rejection loops).
//! - [`LargeThpControlSelector`](large_thp_control_selector::LargeThpControlSelector):
//!   the stateful deterministic control selector — exactly one control
//!   per complete, aligned cohort of 8 eligible fresh requests.
//! - [`LargeThpSpanGeometry`](large_thp_span_geometry::LargeThpSpanGeometry):
//!   validated live-span geometry (span size, base-page size) normalized
//!   into whole-page accounting.
//! - [`LargeThpResidencyPlan`](large_thp_residency_plan::LargeThpResidencyPlan):
//!   the pure sampling planner — up to 16 disjoint, page-aligned window
//!   locations for a FUTURE owning `mincore` walk.
//! - [`LargeThpResidencyWindow`](large_thp_residency_window::LargeThpResidencyWindow):
//!   one planned sampling location.
//! - [`LargeThpObservationError`](large_thp_observation_error::LargeThpObservationError):
//!   the family's single explicit rejection vocabulary.
//!
//! ## Current scope: declared and consumed by the crate
//!
//! This module IS declared from the parent `config/policy/mod.rs` as of
//! the #48 runtime integration. The mechanism-gated runtime consumers
//! live in `src/alloc_core/alloc_core/mem/observe_large_control.rs`
//! together with the fresh-path advice. The `#[path]`-included tests
//! (`tests/j13b_observation_*.rs`) continue to compile their own copy of
//! the pure surface. No estimator feedback exists and no runtime claim
//! beyond the pinned pure contracts is made.

pub mod bounded_mix;
pub mod large_thp_control_selector;
pub mod large_thp_observation_error;
pub mod large_thp_residency_plan;
pub mod large_thp_residency_window;
pub mod large_thp_span_geometry;
