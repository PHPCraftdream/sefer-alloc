//! `sefer-ghost`: the `ghost_assert!` / `ghost_event!` checking-build
//! infrastructure of the sefer allocator (docs/GHOST_CHECKS_PLAN.md §3),
//! extracted from `sefer-alloc`'s former `src/ghost` module (plan step
//! P1.1 of docs/plans/2026-11-15-workspace-restructure-plan.md).
//!
//! Everything here exists only in checking builds —
//! `cfg(any(loom, miri, kani, sefer_ghost))` — and expands to nothing in
//! production: the macros compile to no code, no fields are added to
//! production structs, nothing allocates (rule R9), and the shadow side
//! table uses `core` atomics only, so it is invisible to loom's model.
//! No `unsafe` anywhere: the crate is `#![forbid(unsafe_code)]`.
//!
//! `sefer-alloc` re-exports the [`ghost`] module so every in-tree
//! `crate::ghost::…` call site keeps resolving unchanged; the ghost cfg
//! flags are global `--cfg` RUSTFLAGS, so they apply to this crate and
//! to its consumer alike.
#![forbid(unsafe_code)]
#![no_std]

pub mod ghost;
