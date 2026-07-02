//! Regression test for task A2 (0.3.0) — `fastbin` must never build without
//! `alloc-xthread`.
//!
//! ## What this guards against
//!
//! Pre-fix, `Cargo.toml` declared `fastbin = ["alloc-global"]`. A build with
//! `--features fastbin` (without also requesting `alloc-xthread`) compiled
//! successfully, but was UNSOUND: a cross-thread free of a small block has no
//! ownership-checked routing path without `alloc-xthread` (`dealloc_routing`'s
//! owner-identity stamp and the per-segment `RemoteFreeRing` both live behind
//! that feature) — so nothing would have stopped a cross-thread free from
//! writing directly into a magazine/free-list it does not own: a genuine,
//! unsynchronised data race.
//!
//! Post-fix, `Cargo.toml` declares `fastbin = ["alloc-global", "alloc-xthread"]`
//! (Cargo feature unification), and `src/lib.rs` carries a
//! `#[cfg(all(feature = "fastbin", not(feature = "alloc-xthread")))]
//! compile_error!(...)` as defence-in-depth for any build path that manages
//! to route around the `Cargo.toml` dependency (e.g. a stale vendored copy).
//!
//! ## Two valid outcomes — both are the fix working correctly
//!
//! This test file has no way to "fail" in the traditional sense once
//! `fastbin` is requested at all, by construction:
//!
//! 1. **The crate fails to compile** — if some build path requested
//!    `fastbin` without `alloc-xthread`, the `compile_error!` guard in
//!    `src/lib.rs` fires and NOTHING in the crate (including this test)
//!    compiles. That is the guard doing its job: an unsound configuration is
//!    rejected at compile time rather than silently shipped.
//! 2. **The crate compiles AND this test passes** — feature unification
//!    worked (`fastbin` pulled `alloc-xthread` in), so `cfg!(feature =
//!    "alloc-xthread")` is `true` whenever `cfg!(feature = "fastbin")` is
//!    `true`, and the assertion below holds.
//!
//! There is no third outcome ("compiles but `alloc-xthread` is off while
//! `fastbin` is on") — that is exactly the bug this task fixes, and it is
//! now unreachable both via `Cargo.toml`'s declared dependency AND via the
//! `compile_error!` backstop.
//!
//! Only compiled at all when `fastbin` is requested (mirrors the crate's own
//! `#[cfg(feature = "fastbin")]` gating style) — with `fastbin` off this file
//! is entirely skipped, which is correct: there is nothing to assert about a
//! feature that was not requested.

#![cfg(feature = "fastbin")]

#[test]
fn fastbin_implies_xthread() {
    assert!(
        cfg!(feature = "alloc-xthread"),
        "fastbin was enabled without alloc-xthread — Cargo.toml's \
         `fastbin = [\"alloc-global\", \"alloc-xthread\"]` feature \
         unification failed to pull it in (task A2 regression). This \
         combination is unsound: cross-thread frees would race the \
         per-thread magazine/free-list with no ownership check."
    );
}
