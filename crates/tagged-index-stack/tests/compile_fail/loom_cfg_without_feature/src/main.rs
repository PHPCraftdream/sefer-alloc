//! Negative oracle: building `tagged-index-stack` with
//! `--cfg loom` but WITHOUT its `loom` feature must fail with ONLY the
//! crate's own named `compile_error!` — no secondary unresolved-import
//! errors, because the entire implementation module is gated off under that
//! configuration. This file deliberately references NO crate items: the
//! fixture declares the dependency (so the crate compiles), and the crate's
//! `compile_error!` is the only diagnostic expected. Pinned failing by
//! `tests/compile_fail_loom_cfg_without_feature.rs`.
fn main() {}
