//! oxx R2-07: static proof that `AllocCore` is NOT `Send` (nor, by the same
//! construction applied a second time, `Sync`).
//!
//! Several doc comments in `src/alloc_core/` used to assert `AllocCore` "is
//! `Send`" (`src/alloc_core/alloc_core/mod.rs`,
//! `src/alloc_core/alloc_core/alloc_core_impl.rs`) — false: `AllocCore` holds
//! raw pointers (`*mut u8` segment bases, `directory_sidecar`, the large-cache
//! sidecar, etc.) and carries no `unsafe impl Send`/`unsafe impl Sync`, so it
//! is neither. This is a *static*, compile-time pin, not a runtime assertion:
//! a future edit that (incorrectly) adds `unsafe impl Send for AllocCore`
//! would make this file fail to COMPILE, not merely fail to pass — exactly
//! the failure mode the review's finding warns a doc/code mismatch here could
//! invite ("a future edit … may introduce unsoundness").
//!
//! ## The trick
//!
//! `AmbiguousIfSend<A>` has two blanket impls: one for every `T: ?Sized`
//! (associating `A = ()`), and one for every `T: ?Sized + Send` (associating
//! `A = u8`). For a `Send` type, BOTH impls apply, so
//! `<T as AmbiguousIfSend<_>>::some_item()` cannot infer `A` and is a hard
//! compile error (E0283, "type annotations needed"). For a non-`Send` type,
//! only the first impl applies, so the call resolves unambiguously and
//! compiles (and, trivially, runs — the body is a no-op). This is the
//! standard `static_assertions`-style negative-trait-bound trick, inlined
//! here (no new dependency) because `std`/this crate provide no direct
//! `!Send` assertion primitive.
//!
//! ## Counterfactual (verified manually, not committed)
//!
//! Compiling the identical trick against a plain `Send` struct
//! (`struct SendCounterfactual(u8);`) produces exactly the E0283 ambiguity
//! this file relies on catching; compiling it against a `*mut u8`-holding
//! struct (structurally the same shape as `AllocCore`'s pointer fields)
//! compiles and runs cleanly — confirming the trick is red exactly when the
//! type IS `Send` and green exactly when it is not.

#![cfg(feature = "alloc-core")]

use sefer_alloc::AllocCore;

trait AmbiguousIfSend<A> {
    fn some_item() {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

#[test]
fn alloc_core_is_not_send() {
    // Compiles ONLY because `AllocCore: !Send`. If a future edit adds
    // `unsafe impl Send for AllocCore`, this line stops compiling (E0283).
    <AllocCore as AmbiguousIfSend<_>>::some_item();
}

trait AmbiguousIfSync<A> {
    fn some_item() {}
}
impl<T: ?Sized> AmbiguousIfSync<()> for T {}
impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

#[test]
fn alloc_core_is_not_sync() {
    // Same trick, `Sync` instead of `Send` — `AllocCore`'s own doc comment
    // (`src/alloc_core/alloc_core/mod.rs`) also claims NOT `Sync`; pinned
    // here for the same reason.
    <AllocCore as AmbiguousIfSync<_>>::some_item();
}
