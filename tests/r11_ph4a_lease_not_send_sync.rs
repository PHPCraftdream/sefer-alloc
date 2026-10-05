//! Ph4a (task #2091): compile-time `!Send + !Sync` proof for `HeapLease`
//! plus the claim → drop → FREE → re-claim behavioural round-trip.
//!
//! The negative assertions use the ambiguity-probe pattern (the same
//! mechanism as `static_assertions::assert_not_impl_all!`, inlined to avoid
//! the dev-dependency): a blanket `AmbiguousIfImpl<()>` impl for every type
//! plus a `Send`/`Sync`-guarded `AmbiguousIfImpl<Invalid>` impl. If
//! `HeapLease` were `Send`/`Sync` (e.g. after a mutant `unsafe impl Send`),
//! type inference on `AmbiguousIfImpl<_>` becomes ambiguous (E0283) and this
//! file fails to compile. The exact recipe `const V: bool = <T as IsSend>::V`
//! from the task card is NOT green-able: for a `!Send` type the blanket impl
//! does not exist either, so the projection is E0277 in BOTH cases.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::bootstrap;
use sefer_alloc::registry::heap_registry::{HeapLease, HeapRegistry};
use sefer_alloc::registry::heap_slot::STATE_FREE;

// Negative auto-trait probes (ambiguity pattern, no external deps).
macro_rules! assert_not_impl {
    ($x:ty: $($t:path),+ $(,)?) => {
        const _: fn() = || {
            trait AmbiguousIfImpl<A> {
                fn some_item() {}
            }
            impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
            $({
                #[allow(dead_code)]
                struct Invalid;
                impl<T: ?Sized + $t> AmbiguousIfImpl<Invalid> for T {}
            })+
            // Resolves only when no `$t` impl applies to `$x`.
            let _ = <$x as AmbiguousIfImpl<_>>::some_item;
        };
    };
}

assert_not_impl!(HeapLease: Send);
assert_not_impl!(HeapLease: Sync);

// Negative control: the probe must still pass for a KNOWN `!Send`/`!Sync`
// type — proves the ambiguity machinery resolves the non-impl case.
assert_not_impl!(core::marker::PhantomData<*mut ()>: Send, Sync);

// Positive control of the probe family: a type that IS Send must make the
// NOT-Send probe fail — asserted here in the counterfactual form kept in a
// `#[cfg(any())]` block (it must NOT compile; flip to verify the mutant):
//
// ```
// // #[cfg(any())] remove wrapper to check the probe catches a Send type:
// struct SendMarker;
// assert_not_impl!(SendMarker: Send); // E0283 ambiguity, as expected
// ```
//
// And the mutant sample (verified in the task-#2091 mutant run): adding
// `unsafe impl Send for HeapLease {}` to `claim.rs` turns the
// `assert_not_impl!(HeapLease: Send)` line above into a compile error,
// making this test binary red.

#[test]
fn lease_slot_lifecycle_free_after_drop_and_regeneration() {
    let first = HeapRegistry::dbg_claim_lease().expect("fresh claimable slot");
    let index = first.slot_index() as usize;
    let gen = first.generation();
    assert!(gen >= 1, "generation snapshot is fetch_add result + 1");

    // Exclusive authority: while the lease lives the slot is LIVE; the Drop
    // must return it to FREE (Release publication).
    let reg = bootstrap::ensure();
    drop(first);
    assert_eq!(reg.dbg_slot_state(index), STATE_FREE, "Drop: LIVE -> FREE");

    // Re-claim wins the same slot (reuse hint) at a strictly newer
    // generation — numeric evidence of the Release/Acquire round trip.
    let second = HeapRegistry::dbg_claim_lease().expect("slot reclaimed");
    assert_eq!(second.slot_index() as usize, index, "reuse_hint steered");
    assert!(
        second.generation() > gen,
        "new generation {} must exceed previous {}",
        second.generation(),
        gen
    );
    drop(second);
    assert_eq!(reg.dbg_slot_state(index), STATE_FREE);
}
