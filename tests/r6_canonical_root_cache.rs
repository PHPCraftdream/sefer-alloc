//! A biased payload key and its stored root can have different addresses.
//! Cache hits must match the numeric key but return only the stored root.
//! A one-byte reborrow used as a key cannot supply the second byte's
//! provenance. The old root-as-key cache misses the warm-hit oracle below.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

#[cfg(feature = "bench-internals")]
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::alloc_core::SegmentHashHarness;

#[test]
fn cache_fill_hit_evict_and_reuse_keep_stored_provenance() {
    let mut bytes = Box::new([0x12_u8, 0x34]);
    let root = bytes.as_mut_ptr();
    let mut table = SegmentHashHarness::new();
    let key_addr = SegmentHashHarness::base_for_index(7).addr();
    assert_ne!(key_addr, root.addr());
    let narrow = (&mut bytes[0] as *mut u8).with_addr(key_addr);
    table.insert_root_for_key(narrow, root);

    #[cfg(feature = "bench-internals")]
    let before = (
        AllocCore::dbg_contains_base_tier1_hits(),
        AllocCore::dbg_contains_base_tier1_misses(),
    );
    assert!(table.contains_cached(narrow)); // hash fill
    assert!(table.contains_cached(narrow)); // cache hit
    #[cfg(feature = "bench-internals")]
    assert_eq!(
        (
            AllocCore::dbg_contains_base_tier1_hits() - before.0,
            AllocCore::dbg_contains_base_tier1_misses() - before.1,
        ),
        (1, 1),
        "a second lookup of the same payload key must hit the root cache"
    );
    let canonical = table.canonical(narrow).expect("live entry");
    assert_eq!(canonical.addr(), root.addr());
    // SAFETY: canonical must be the stored root pointer. The two initialized
    // bytes remain live in `bytes`; no write aliases this read.
    assert_eq!(unsafe { *canonical.add(1) }, 0x34);

    table.remove_root_for_key(narrow, root);
    assert!(table.canonical(narrow).is_none());

    let mut replacement = Box::new([0x56_u8, 0x78]);
    let replacement_root = replacement.as_mut_ptr();
    table.insert_root_for_key(narrow, replacement_root);
    let address_only = core::ptr::without_provenance_mut::<u8>(key_addr);
    assert!(table.contains_cached(address_only));
    let canonical = table.canonical(address_only).expect("re-registered entry");
    assert_eq!(canonical.addr(), replacement_root.addr());
    // SAFETY: the table now stores the replacement's live, two-byte root.
    assert_eq!(unsafe { *canonical.add(1) }, 0x78);
}

#[test]
fn hash_identity_without_a_stored_root_is_not_a_live_member() {
    let mut bytes = Box::new([0x12_u8, 0x34]);
    let root = bytes.as_mut_ptr();
    let key = SegmentHashHarness::base_for_index(13);
    let mut table = SegmentHashHarness::new();
    table.insert_root_for_key(key, root);
    table.hide_root_for_test(root);

    assert!(!table.contains_cached(key));
    assert!(table.canonical(key).is_none());
}
