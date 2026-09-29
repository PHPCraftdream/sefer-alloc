//! Fresh sidecar slots must be typed `None`, except for the overflowing deposit.
//! OS-zeroed bytes happen to decode as `None` on today's compiler; this is a
//! postcondition guard, not a witness against that unspecified-layout bug.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended",
    feature = "internals"
))]

#[path = "support/r6_bounded_large_cache.rs"]
mod bounded;

use sefer_alloc::AllocCore;

#[test]
fn all_extension_slots_are_none_immediately_after_materialisation() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    assert!(!ac.dbg_large_cache_extension_materialised());

    let live = bounded::allocate_live(&mut ac, &[bounded::large_request(); 9]);
    bounded::deposit_all(&mut ac, live);

    assert!(ac.dbg_large_cache_extension_materialised());
    assert_eq!(bounded::occupied(&ac), (8, 1));
    bounded::assert_used(&ac);
    let ext = ac.dbg_large_cache_extended_slot_sizes();
    assert!(
        ext[0].is_some(),
        "ninth deposit occupies the first extension slot"
    );
    assert!(ext[1..].iter().all(Option::is_none));
}
