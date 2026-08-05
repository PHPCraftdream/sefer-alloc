//! Sol-F1 (task #563, release-readiness review finding F1) — the NEGATIVE
//! half of the `internals` boundary: this file exists SOLELY as compile-fail
//! bait, never to be executed. It does NOT list `internals` in its own
//! `required-features` (see `Cargo.toml`'s `[[example]]` entry for this
//! file) — deliberately, since `internals`-off is exactly the configuration
//! this probe exists to prove fails.
//!
//! ## What this proves
//!
//! `tests/r34_3_internals_boundary_api.rs` (R34-3/task #522) proves the
//! POSITIVE half of the `internals` boundary — the stable crate-root
//! re-exports (`SeferAlloc`, `AllocCore`, `SegmentLayout`, etc.) resolve
//! WITHOUT `internals`. It explicitly does NOT (and, per its own module doc,
//! structurally cannot cheaply) prove the NEGATIVE half: that
//! `AllocCore::dbg_*` diagnostic hooks do NOT resolve without `internals`.
//! Before Sol-F1 (task #563), that negative half was simply false —
//! `AllocCore` is re-exported at the crate root unconditionally (`pub use
//! alloc_core::{AllocCore, SegmentLayout}` in `src/lib.rs`, gated only on
//! `alloc-core`), so gating the `alloc_core` MODULE PATH behind `internals`
//! (R34-3) did not hide `AllocCore`'s own inherent `dbg_*` methods: they
//! stayed reachable as `sefer_alloc::AllocCore::dbg_*` regardless of
//! `internals`. This file, plus
//! `scripts/verify-internals-negative-boundary.mjs` (which builds it BOTH
//! ways and asserts the expected outcome each time), is the compile-fail
//! oracle that makes the negative half a checked, reproducible property
//! instead of an unverified claim:
//!
//! - `cargo build --example sol_f1_dbg_carve_batch_negative_probe --features
//!   "alloc-core alloc-global alloc-decommit"` (NO `internals`) MUST FAIL to
//!   compile — `AllocCore::dbg_carve_batch` must not resolve.
//! - The SAME command PLUS `internals` MUST SUCCEED — proving the failure
//!   above is caused specifically by the `internals` gate (Sol-F1's fix in
//!   `src/alloc_core/alloc_core_small_diag.rs`), not by some unrelated
//!   breakage in this file.
//!
//! `AllocCore::dbg_carve_batch` (`src/alloc_core/alloc_core_small_diag.rs`)
//! is used as the representative probe method: it is a plain safe `pub fn`
//! (no `unsafe`, no extra feature gate beyond the file's own `internals`
//! gate), so a failure here is unambiguously about the `internals` boundary
//! and not about some OTHER feature/safety gate being unmet.

#[cfg(feature = "alloc-core")]
fn main() {
    let mut core = sefer_alloc::AllocCore::new().unwrap();
    let mut buf = [core::ptr::null_mut::<u8>(); 4];
    // This call is the actual probe: it resolves iff `internals` is on (see
    // this file's module doc). Deliberately not asserting on the result —
    // this example is never meant to be RUN, only built.
    let _ = core.dbg_carve_batch(0, &mut buf);
}

#[cfg(not(feature = "alloc-core"))]
fn main() {}
