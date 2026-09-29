//! Nine simultaneously live Large spans fill eight base slots and force the
//! extension on deposit. Equal sizes cannot reuse one another while live.

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
fn overflow_past_base_eight_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    assert!(!ac.dbg_large_cache_extension_materialised());
    assert_eq!(ac.dbg_large_cache_total_slots(), 8);

    let bytes = bounded::large_request();
    let live = bounded::allocate_live(&mut ac, &[bytes; 9]);
    let original: Vec<_> = live.iter().map(|(p, _)| *p).collect();
    bounded::deposit_all(&mut ac, live);

    assert!(ac.dbg_large_cache_extension_materialised());
    assert_eq!(ac.dbg_large_cache_total_slots(), 40);
    assert_eq!(bounded::occupied(&ac), (8, 1));
    bounded::assert_used(&ac);

    let again = bounded::allocate_live(&mut ac, &[bytes; 9]);
    assert_eq!(bounded::occupied(&ac), (0, 0));
    for (p, _) in &again {
        assert!(
            original.contains(p),
            "every cached reservation must be reused"
        );
    }
    bounded::deposit_all(&mut ac, again);
}
