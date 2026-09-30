#![cfg(all(
    feature = "alloc-core",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn carved_index_is_within_primordial_metadata_and_bootstrap_initializes_it() {
    let end = SegmentLayout::PRIMORDIAL_ACTIVE_KIND_END;
    let meta = SegmentLayout::PRIMORDIAL_META_END;
    assert!(end > SegmentLayout::SMALL_META_END);
    assert!(end <= meta);
    assert!(meta - end < SegmentLayout::PAGE);
    let core = AllocCore::new().unwrap();
    assert_eq!(core.dbg_active_kind_census(), (1, 0, true));
}

#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
#[test]
fn lazy_commit_covers_index_on_supported_real_page_geometries() {
    for page in [4096, 16384, 65536] {
        let initial = SegmentLayout::primordial_lazy_initial_commit(page);
        assert_eq!(initial % page, 0);
        assert!(initial >= SegmentLayout::PRIMORDIAL_ACTIVE_KIND_END);
        assert!(initial <= SegmentLayout::SEGMENT);
    }
}

#[cfg(feature = "alloc-global")]
#[test]
fn heap_slot_and_chunk_geometry_is_measured_not_assumed() {
    use core::mem::{align_of, size_of};
    use sefer_alloc::registry::{HeapCore, HeapSlot};

    let alloc_core = size_of::<AllocCore>();
    let heap_core = size_of::<HeapCore>();
    let slot = size_of::<HeapSlot>();
    // RegistryChunk is repr(C) and contains exactly [HeapSlot; 64].
    let chunk_raw = slot * 64;
    let chunk_reserved = chunk_raw.div_ceil(SegmentLayout::PAGE) * SegmentLayout::PAGE;
    assert_eq!(slot % align_of::<HeapSlot>(), 0);
    assert!(slot >= heap_core && heap_core >= alloc_core);
    assert!(chunk_reserved >= chunk_raw);
    eprintln!(
        "AllocCore={alloc_core} HeapCore={heap_core} HeapSlot={slot} RegistryChunk={chunk_raw} CHUNK_SIZE={chunk_reserved}"
    );
}
