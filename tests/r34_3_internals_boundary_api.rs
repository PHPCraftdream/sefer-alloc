//! R34-3 (task #522, release-stabilization audit finding B1) — public-API
//! snapshot guard for the `internals` feature boundary.
//!
//! ## What this guards
//!
//! `alloc_core` / `global` / `registry` (the three internal modules housing
//! ~50 `pub unsafe fn dbg_*` diagnostic hooks and the rest of this crate's
//! white-box test surface) moved behind a new, additive `internals` feature
//! — see `internals`'s own doc comment in `Cargo.toml` and the `pub mod`
//! declarations in `src/lib.rs`. The intended STABLE surface (the types a
//! downstream `sefer-alloc = { features = ["production"] }` consumer is
//! meant to name) stays reachable at the crate root regardless of
//! `internals`, via the pre-existing `pub use alloc_core::{...}` / `pub use
//! global::{...}` re-exports.
//!
//! This file is compiled under PLAIN `alloc-core`/`alloc-decommit`/
//! `alloc-global` (no `internals` — see its own `required-features` below):
//! it is the POSITIVE half of the boundary check — every name below must
//! resolve at the crate root without `internals` enabled, so a future change
//! that accidentally moves one of these re-exports behind `internals` (or
//! removes it) fails to COMPILE here, not just silently ships a narrower
//! public surface.
//!
//! ## What this does NOT (and structurally cannot cheaply) guard
//!
//! The NEGATIVE half — `sefer_alloc::alloc_core::*` /
//! `sefer_alloc::registry::*` / `sefer_alloc::global::*` must NOT resolve
//! without `internals` — is a compile-FAILURE property, which a normal
//! `#[test]` cannot assert (a file that fails to compile fails the whole
//! test binary, not one test). A `trybuild`-style compile-fail harness would
//! need a new dev-dependency this crate does not otherwise carry. The
//! negative half is instead verified structurally and repeatedly by this
//! task's own zero-trust verification pass (see the R34-3 commit body):
//! `cargo clippy --all-targets --features production -- -D warnings` and
//! `cargo build --features production` both succeed ONLY because every
//! one of this repo's own ~106 white-box `tests/` files that reaches
//! `alloc_core`/`registry`/`global` directly is itself `internals`-gated
//! (module-level `#![cfg(feature = "internals")]`, `all(...)`-merged where a
//! narrower per-item `#[cfg]` pre-existed) — before that gating was added,
//! attempting to build under `production` alone produced explicit
//! `error[E0603]: module '<X>' is private` for every such file, which is the
//! actual Rust compiler proving the module is unreachable from outside the
//! crate. `cargo doc --features production --no-deps` was additionally
//! inspected by hand: no `registry/` directory is emitted at all, and the
//! `alloc_core/`/`global/` directories that DO exist contain ONLY the
//! physical HTML pages for the re-exported types this file names below
//! (zero `dbg_*` pages, zero module index page, zero crate-root link to
//! either directory).
//!
//! `tests/` files are NOT declared as explicit `[[test]]` Cargo.toml targets
//! in this repo (auto-discovered instead — confirmed zero `[[test]]` entries
//! exist), so `required-features` does not apply; this file instead gates
//! itself via the module-level `#![cfg(...)]` below, requiring
//! `alloc-core`/`alloc-global`/`alloc-decommit` (for every re-exported type
//! it names) and deliberately NOT `internals` — `internals`-off is exactly
//! the configuration this file exists to check under. Run via `cargo test
//! --features production --test r34_3_internals_boundary_api` (or any
//! feature string that turns on all three without `internals`).

#![cfg(all(feature = "alloc-core", feature = "alloc-global", feature = "alloc-decommit"))]

use sefer_alloc::{
    AllocCore, AllocStats, Handle, LargeCacheConfig, LargeCacheMode, LargeCachePolicy, Profile,
    Region, SeferAlloc, SegmentLayout, SmallPoolPolicy, SmallSegmentPoolConfig, SyncRegion,
};

/// Every name in the `use` above must resolve — that IS the test. A
/// reference to each keeps the import from being flagged unused and gives a
/// concrete, named assertion trail (which type disappeared, if one ever
/// does) rather than a bare "this file failed to compile".
#[test]
fn stable_surface_resolves_at_crate_root_without_internals() {
    fn assert_type_exists<T>() {}
    assert_type_exists::<SeferAlloc>();
    assert_type_exists::<AllocStats>();
    assert_type_exists::<Profile>();
    assert_type_exists::<LargeCacheConfig>();
    assert_type_exists::<LargeCacheMode>();
    assert_type_exists::<LargeCachePolicy>();
    assert_type_exists::<SmallPoolPolicy>();
    assert_type_exists::<SmallSegmentPoolConfig>();
    assert_type_exists::<AllocCore>();
    assert_type_exists::<SegmentLayout>();
    assert_type_exists::<Region<u8>>();
    assert_type_exists::<Handle<u8>>();
    assert_type_exists::<SyncRegion<u8>>();
}

/// `SeferAlloc`'s stable methods must also stay callable without
/// `internals` — a smoke check that the re-exported TYPE is not merely
/// nameable but genuinely usable (its inherent impl lives in the
/// `internals`-gated `global` module body, but the method set itself is
/// part of the stable surface regardless of that module's external
/// visibility — see `src/lib.rs`'s two-declaration `pub`/`pub(crate)` shape
/// for `mod global`).
#[test]
fn sefer_alloc_stable_methods_are_callable_without_internals() {
    let alloc = SeferAlloc::new();
    let stats = alloc.stats();
    // A genuinely non-vacuous, cheap invariant check (not just "did this
    // compile and not panic"): a freshly constructed, never-touched
    // `SeferAlloc` has not reserved or released any segments yet. Deeper
    // behavioral coverage of `stats()` already exists in
    // `tests/stats_reflects_activity.rs`; this call exists here only to
    // prove the STABLE method surface stays genuinely usable (not merely
    // nameable) without `internals`.
    assert_eq!(stats.segments_reserved_total, 0);
    assert_eq!(stats.segments_released_total, 0);
}
