#![cfg(all(
    feature = "alloc-core",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

#[test]
fn terminal_words_are_outside_the_copy_header_and_before_the_page_map() {
    let (header_size, words_off, head_off, state_off, page_map_off) =
        AllocCore::terminal_header_layout_for_test();
    assert_eq!(header_size, 128);
    assert_eq!(words_off, 128);
    assert_eq!(head_off, 128);
    assert_eq!(state_off, 136);
    assert_eq!(page_map_off, 4096);
    assert!(words_off >= header_size);
    assert!(head_off >= words_off);
    assert!(head_off + 4 <= state_off);
    assert!(state_off + 8 <= page_map_off);
}

#[test]
fn fresh_small_and_large_have_independent_atomic_snapshots() {
    let mut core = AllocCore::new().expect("primordial bootstrap");
    assert_eq!(
        core.terminal_primordial_snapshot_for_test(),
        Some((u32::MAX, 0))
    );
    let small = Layout::from_size_align(64, 8).unwrap();
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small_ptr = core.alloc(small);
    let large_ptr = core.alloc(large);
    assert!(!small_ptr.is_null());
    assert!(!large_ptr.is_null());
    assert_eq!(
        core.terminal_header_snapshot_for_test(small_ptr),
        Some((u32::MAX, 0))
    );
    assert_eq!(
        core.terminal_header_snapshot_for_test(large_ptr),
        Some((u32::MAX, (1 << 3) | 2))
    );
    // SAFETY: both pointers are live allocations from this core with their
    // original layouts, and each is freed exactly once.
    unsafe {
        core.dealloc(large_ptr, large);
        core.dealloc(small_ptr, small);
    }
}

#[test]
fn large_generation_exhaustion_never_wraps() {
    let max = u64::MAX >> 3;
    assert_eq!(
        AllocCore::terminal_next_generation_for_test(max - 1),
        Some(max)
    );
    assert_eq!(AllocCore::terminal_next_generation_for_test(max), None);
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn large_cache_hit_resets_atomic_phase_and_advances_generation() {
    let mut core = AllocCore::new().expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    assert_eq!(
        core.terminal_header_snapshot_for_test(first),
        Some((u32::MAX, (1 << 3) | 2))
    );
    // SAFETY: `first` is live and this is its original layout.
    unsafe { core.dealloc(first, layout) };
    assert!(core
        .dbg_large_cache_slot_sizes()
        .iter()
        .any(Option::is_some));
    assert_eq!(
        core.terminal_cached_large_state_for_test(),
        Some((1 << 3) | 5)
    );

    let second = core.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(second, first, "same cached reservation must be reused");
    assert_eq!(
        core.terminal_header_snapshot_for_test(second),
        Some((u32::MAX, (2 << 3) | 2))
    );
    assert!(core
        .dbg_large_cache_slot_sizes()
        .iter()
        .all(Option::is_none));
    // SAFETY: `second` is the current live allocation with its original layout.
    unsafe { core.dealloc(second, layout) };
}
