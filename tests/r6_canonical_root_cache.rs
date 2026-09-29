//! Cache values must come from the hash table, even when an address-only key
//! comes from a one-byte user reborrow. Under Miri, storing that key instead
//! of the table entry makes the second-byte read below invalid. Verified RED
//! under strict-provenance Miri with the old `own_cache[idx] = base` fill.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::alloc_core::SegmentHashHarness;

#[test]
fn cache_fill_hit_evict_and_reuse_keep_stored_provenance() {
    let mut bytes = Box::new([0x12_u8, 0x34]);
    let root = bytes.as_mut_ptr();
    let mut table = SegmentHashHarness::new();
    table.insert(root);

    let narrow = &mut bytes[0] as *mut u8;
    assert!(table.contains_cached(narrow)); // hash fill
    assert!(table.contains_cached(narrow)); // cache hit
    let canonical = table.canonical(narrow).expect("live entry");
    assert_eq!(canonical.addr(), root.addr());
    // SAFETY: canonical must be the stored root pointer. The two initialized
    // bytes remain live in `bytes`; no write aliases this read.
    assert_eq!(unsafe { *canonical.add(1) }, 0x34);

    table.remove(root);
    assert!(table.canonical(narrow).is_none());

    table.insert(root);
    let address_only = core::ptr::without_provenance_mut::<u8>(root.addr());
    assert!(table.contains_cached(address_only));
    let canonical = table.canonical(address_only).expect("re-registered entry");
    // SAFETY: the hash table again stores `root`, which spans both initialized
    // bytes in the still-live `bytes` allocation.
    assert_eq!(unsafe { *canonical.add(1) }, 0x34);
}
