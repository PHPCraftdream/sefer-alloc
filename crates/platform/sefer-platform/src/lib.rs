//! `sefer-platform` — the allocator's platform seam: the sanctioned process
//! terminator (`fatal`), the Windows process-exit probe (`process_exit`), and
//! the SEGMENT-aligned OS aperture ("backup" half of the former
//! `alloc_core::platform::os` seam: [`os::Segment`], the `SEGMENTS_*` counters,
//! release/decommit/commit wrappers and the depot-overflow storage).
//!
//! Extracted from `sefer-alloc` by plan step P1.4
//! (docs/plans/2026-11-15-workspace-restructure-plan.md) as a pure move: the
//! root crate re-exports this whole surface through its `alloc_core::os` /
//! `alloc_core::fatal` / `global::process_exit` conduits, so call-site paths
//! are unchanged. The directory-sidecar trio (`reserve_directory_sidecar`,
//! `read_directory_class_words`, `read_directory_node_bucket`) stays in the
//! root: it depends on `SegmentDirectory`/sidecar/segment-header internals.
//!
//! Feature names mirror the root crate one-for-one; the root forwards every
//! feature that gates moved code (see the root `Cargo.toml`).

/// The one sanctioned process terminator: writes a best-effort static
/// diagnostic and terminates without unwinding.
pub mod fatal;
/// The SEGMENT-aligned OS aperture (`Segment`, counters, release/commit/decommit; module `os`).
pub mod os;
/// The Windows loader-shutdown FFI probe (`process_exit_in_progress`).
pub mod process_exit;
