use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

pub fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).expect("valid Large layout")
}

pub fn large_request() -> usize {
    let small_max = AllocCore::dbg_block_size(AllocCore::dbg_small_class_count() - 1);
    small_max + SegmentLayout::PAGE
}

pub fn allocate_live(ac: &mut AllocCore, sizes: &[usize]) -> Vec<(*mut u8, Layout)> {
    sizes
        .iter()
        .map(|&bytes| {
            let l = layout(bytes);
            let p = ac.alloc(l);
            assert!(
                !p.is_null(),
                "bounded Large allocation of {bytes} bytes failed"
            );
            (p, l)
        })
        .collect()
}

pub fn deposit_all(ac: &mut AllocCore, live: Vec<(*mut u8, Layout)>) {
    for (p, l) in live {
        // SAFETY: each pointer is live from allocate_live, with its matching layout.
        unsafe { ac.dealloc(p, l) };
    }
}

pub fn occupied(ac: &AllocCore) -> (usize, usize) {
    (
        ac.dbg_large_cache_slot_sizes().iter().flatten().count(),
        ac.dbg_large_cache_extended_slot_sizes()
            .iter()
            .flatten()
            .count(),
    )
}

pub fn assert_used(ac: &AllocCore) {
    let base: usize = ac.dbg_large_cache_slot_sizes().iter().flatten().sum();
    let extended: usize = ac
        .dbg_large_cache_extended_slot_sizes()
        .iter()
        .flatten()
        .sum();
    assert_eq!(ac.dbg_large_cache_used(), base + extended);
}
