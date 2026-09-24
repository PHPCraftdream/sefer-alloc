//! R2-05 (independent src review round 2, task #2007) — functional
//! regression for the `AllocCore`/`HeapCore` diagnostic accessors whose
//! feature gates (`numa-aware`, `page-map-diag`, `virgin-zero-skip`,
//! `primordial-lazy-commit`/`small-segment-lazy-commit`, `alloc-global`,
//! `alloc-segment-directory`) are too varied to combine into ONE `cargo miri`
//! target alongside `tests/regression_r2_05_diag_provenance_miri.rs`'s
//! feature-minimal set. See that file's module doc for the full mechanism
//! this fix closes (`SegmentTable::canonical_base_of` / the
//! `.find`-not-`.any` pattern) — this file proves the SAME property
//! (a provenance-less pointer sharing a live segment's address gets the
//! SAME answer as the real, provenance-carrying pointer) functionally,
//! under native `cargo test`, for the remaining accessors.

#![cfg(feature = "internals")]

#[cfg(all(feature = "alloc-core", feature = "numa-aware", feature = "internals"))]
#[test]
fn dbg_node_id_for_sound_under_provenance_less_input() {
    use core::alloc::Layout;

    use sefer_alloc::alloc_core::AllocCore;

    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = core.alloc(layout);
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    assert_eq!(core.dbg_node_id_for(real_ptr), core.dbg_node_id_for(stale));
}

#[cfg(all(
    feature = "alloc-core",
    feature = "page-map-diag",
    feature = "internals"
))]
#[test]
fn dbg_page_map_class_for_sound_under_provenance_less_input() {
    use core::alloc::Layout;

    use sefer_alloc::alloc_core::AllocCore;

    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = core.alloc(layout);
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    assert_eq!(
        core.dbg_page_map_class_for(real_ptr),
        core.dbg_page_map_class_for(stale)
    );
}

#[cfg(all(
    feature = "alloc-core",
    feature = "virgin-zero-skip",
    feature = "internals"
))]
#[test]
fn dbg_payload_virgin_for_sound_under_provenance_less_input() {
    use core::alloc::Layout;

    use sefer_alloc::alloc_core::AllocCore;

    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = core.alloc(layout);
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    assert_eq!(
        core.dbg_payload_virgin_for(real_ptr),
        core.dbg_payload_virgin_for(stale)
    );
}

#[cfg(all(
    feature = "alloc-core",
    any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ),
    feature = "internals"
))]
#[test]
fn dbg_committed_payload_end_for_sound_under_provenance_less_input() {
    use core::alloc::Layout;

    use sefer_alloc::alloc_core::AllocCore;

    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = core.alloc(layout);
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    assert_eq!(
        core.dbg_committed_payload_end_for(real_ptr),
        core.dbg_committed_payload_end_for(stale)
    );
}

#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[test]
fn dbg_owner_id_for_sound_under_provenance_less_input() {
    use core::alloc::Layout;
    use std::sync::atomic::{AtomicBool, Ordering};

    use sefer_alloc::registry::{bootstrap, HeapRegistry};

    // Serialise against other tests in this binary that also claim the
    // process-global registry (mirrors `tests/heap_core_tcache_stamp.rs`'s
    // established `SerialGuard` pattern for the same reason).
    static SERIAL: AtomicBool = AtomicBool::new(false);
    while SERIAL
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            SERIAL.store(false, Ordering::Release);
        }
    }
    let _g = Guard;

    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = unsafe { (*heap).alloc(layout) };
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    let via_real = unsafe { (*heap).dbg_owner_id_for(real_ptr) };
    let via_stale = unsafe { (*heap).dbg_owner_id_for(stale) };
    assert_eq!(via_real, via_stale);
    assert!(via_real.is_some(), "a freshly-allocated block must resolve");
}

#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-segment-directory",
    feature = "internals"
))]
#[test]
fn dbg_directory_bit_for_ptr_sound_under_provenance_less_input() {
    use core::alloc::Layout;
    use std::sync::atomic::{AtomicBool, Ordering};

    use sefer_alloc::registry::{bootstrap, HeapRegistry};

    static SERIAL: AtomicBool = AtomicBool::new(false);
    while SERIAL
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            SERIAL.store(false, Ordering::Release);
        }
    }
    let _g = Guard;

    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    let layout = Layout::from_size_align(64, 8).unwrap();
    let real_ptr = unsafe { (*heap).alloc(layout) };
    assert!(!real_ptr.is_null());
    let stale = core::ptr::without_provenance_mut::<u8>(real_ptr.expose_provenance());

    let via_real = unsafe { (*heap).dbg_directory_bit_for_ptr(real_ptr, 0) };
    let via_stale = unsafe { (*heap).dbg_directory_bit_for_ptr(stale, 0) };
    assert_eq!(via_real, via_stale);
}

/// Out-of-range/foreign input for a `None`-returning (non-panicking)
/// accessor: a genuinely foreign address must be rejected with `None`
/// (zero memory access), not a garbage/crashed read.
#[cfg(all(feature = "alloc-core", feature = "numa-aware", feature = "internals"))]
#[test]
fn dbg_node_id_for_returns_none_on_a_genuinely_foreign_address() {
    use sefer_alloc::alloc_core::AllocCore;

    let core = AllocCore::new().expect("primordial segment reservation");
    let foreign = core::ptr::without_provenance_mut::<u8>(0x1000_0000);
    assert_eq!(core.dbg_node_id_for(foreign), None);
}
