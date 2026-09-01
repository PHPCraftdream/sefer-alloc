//! Test-helper module for `tagged-index-stack`'s integration tests.
//!
//! Following the workspace's established `tests/common/` convention (see
//! `crates/size-classes/tests/common/` and `crates/sefer-region/tests/`):
//! `tests/common/mod.rs` is NOT itself a test binary — cargo only auto-
//! discovers plain `tests/*.rs` files, and a `mod.rs` inside a subdirectory
//! is invisible to target discovery — so this file holds reexports only.
//! The actual helper code lives in the child modules it declares.

pub mod compile_fail;
