//! SEC-03 boundary probes: deliberately invalid interior frees/reallocs must
//! reject before consuming a live Large credit. Removing the guards makes the
//! counter/membership/null assertions fail before any source-byte read.
//! R6-T55: an interior Large address whose numeric key lands in the
//! registered payload-key window resolves to this core — its reject leaves
//! the public `foreign_or_unroutable_frees` flat and lands in the
//! internals-gated `LOCAL_REJECT_FREES` instead. A biased Large pre-payload
//! address (`ptr - 64` under `align == SEGMENT`) does NOT resolve: it is the
//! documented "no matching live route" drop and counts in
//! `foreign_or_unroutable_frees` (standalone core: under
//! `alloc-global`/`alloc-stats`; global face: unconditionally).
#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(size: usize, align: usize) -> Layout {
    Layout::from_size_align(size, align).unwrap()
}

/// R6-T55: a rejected LOCAL interior probe (numeric key resolves to this
/// core) leaves the public foreign/unroutable stat flat and lands in
/// `LOCAL_REJECT_FREES` exactly once (the pre-R6-T55 code bumped the public
/// stat here — that is the red counterfactual).
fn assert_local_reject(before: u64, rejects_before: u64) {
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        before,
        "a local interior reject must not count as a foreign/unroutable drop"
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        rejects_before + 1,
        "the local interior reject must land in LOCAL_REJECT_FREES"
    );
}

/// R6-T55, review follow-up: a biased Large PRE-payload address (`ptr - 64`
/// under `align == SEGMENT`) carries a numeric key OUTSIDE every registered
/// payload-key window (Large registration keys on
/// `segment_base_of_ptr(payload)`), so the address-only lookup cannot resolve
/// it to this core. That is the DOCUMENTED `foreign_or_unroutable_frees`
/// case ("address-only route lookup finds no matching live route"): the
/// standalone core counts it in its unresolved-address branch (compiled
/// under `alloc-global`/`alloc-stats`), the global face in
/// `publish_foreign`'s lookup-miss branch (unconditional under
/// `alloc-global`). It never reaches a local reject site, so
/// `LOCAL_REJECT_FREES` stays flat.
fn assert_unroutable_reject(before: u64, rejects_before: u64) {
    assert_eq!(
        AllocCore::dbg_foreign_or_unroutable_frees(),
        before + u64::from(cfg!(any(feature = "alloc-global", feature = "alloc-stats"))),
        "an unresolved interior address must count only as the documented \
         foreign/unroutable drop",
    );
    assert_eq!(
        AllocCore::dbg_local_reject_frees(),
        rejects_before,
        "an unresolved interior address never reaches a local reject site",
    );
}

/// The R6-T55 rule for one probe address: `true` when the address's numeric
/// key lands in the registered payload-key window (both probes under
/// `align == 16`, and `ptr + 64` under `align == SEGMENT`); `false` for the
/// biased pre-payload `ptr - 64` under `align == SEGMENT`.
fn resolves_locally(align: usize, ptr: *mut u8, address: usize) -> bool {
    align < SegmentLayout::SEGMENT || address == ptr.addr() + 64
}

unsafe fn fill(ptr: *mut u8, size: usize) {
    // SAFETY: caller supplies an exclusively owned live allocation of `size` bytes.
    unsafe { ptr.write_bytes(0x5a, size) };
}

unsafe fn assert_bytes(ptr: *mut u8, size: usize) {
    // SAFETY: caller has first verified that the source remains registered/live;
    // its entire requested extent was initialized by `fill` or preserved by realloc.
    unsafe {
        for i in 0..size {
            assert_eq!(ptr.add(i).read(), 0x5a, "byte {i}");
        }
    }
}

// One test owns every process-global counter observation; no subprocesses,
// concurrent tests, global SeferAlloc installation, or background producers.
#[test]
fn large_interior_free_and_realloc_preserve_source() {
    // M2 witness: exactly one interior free followed immediately by valid free.
    // Geometry/count alone cannot expose a consumed LIVE credit.
    {
        let mut core = AllocCore::new().unwrap();
        let old = layout(4 * 1024 * 1024, 16);
        let ptr = core.alloc(old);
        assert!(!ptr.is_null());
        let geometry = core.large_geometry_for_test(ptr).unwrap();
        let before = AllocCore::dbg_foreign_or_unroutable_frees();
        let rejects_before = AllocCore::dbg_local_reject_frees();
        let interior = core::ptr::without_provenance_mut(ptr.addr() + 64);
        // SAFETY: deliberate rejection probe into a mapped live Large block.
        unsafe { core.dealloc(interior, old) };
        assert_local_reject(before, rejects_before);
        assert_eq!(core.large_geometry_for_test(ptr), Some(geometry));
        // SAFETY: the exact issued start remains owned after rejection; free
        // once. Moving rejection after claim_live aborts at this valid free.
        unsafe { core.dealloc(ptr, old) };
        assert!(core.large_geometry_for_test(ptr).is_none());
    }
    for align in [16, SegmentLayout::SEGMENT] {
        let mut core = AllocCore::new().unwrap();
        let old = layout(4 * 1024 * 1024, align);
        let ptr = core.alloc(old);
        assert!(!ptr.is_null());
        let geometry = core.large_geometry_for_test(ptr).unwrap();
        assert_eq!(geometry.2 + geometry.3, ptr.addr());
        assert!(geometry.3 >= 64);
        // SAFETY: successful allocation owns `old.size()` writable bytes.
        unsafe { fill(ptr, old.size()) };
        for address in [ptr.addr() - 64, ptr.addr() + 64] {
            // Address keys only: allocator resolves canonical root provenance.
            let interior = core::ptr::without_provenance_mut(address);
            let before = AllocCore::dbg_foreign_or_unroutable_frees();
            let rejects_before = AllocCore::dbg_local_reject_frees();
            let released = AllocCore::dbg_segments_released_total();
            #[cfg(feature = "alloc-decommit")]
            let cached = (
                core.dbg_large_cache_used(),
                core.dbg_large_cache_occupied_bits(),
            );
            // SAFETY: deliberate defence-in-depth contract-violation probe;
            // live owner root is mapped, but this address must be rejected.
            unsafe { core.dealloc(interior, old) };
            if resolves_locally(align, ptr, interior.addr()) {
                assert_local_reject(before, rejects_before);
            } else {
                assert_unroutable_reject(before, rejects_before);
            }
            assert_eq!(core.large_geometry_for_test(ptr), Some(geometry));
            assert_eq!(AllocCore::dbg_segments_released_total(), released);
            #[cfg(feature = "alloc-decommit")]
            assert_eq!(
                (
                    core.dbg_large_cache_used(),
                    core.dbg_large_cache_occupied_bits()
                ),
                cached
            );
            for new_size in [old.size() / 2, old.size() * 2] {
                let reserved = AllocCore::dbg_segments_reserved_total();
                // SAFETY: deliberate interior-address rejection probe. No
                // source read or ownership transfer is permitted on this path.
                let result = unsafe { core.realloc(interior, old, new_size) };
                assert!(result.is_null());
                assert_eq!(core.large_geometry_for_test(ptr), Some(geometry));
                assert_eq!(AllocCore::dbg_segments_reserved_total(), reserved);
                assert_eq!(AllocCore::dbg_segments_released_total(), released);
            }
            // SAFETY: membership/geometry checks above prove the original survived.
            unsafe { assert_bytes(ptr, old.size()) };
        }
        let released = AllocCore::dbg_segments_released_total();
        let count = AllocCore::dbg_foreign_or_unroutable_frees();
        // SAFETY: exact original start/layout, still owned, freed exactly once.
        unsafe { core.dealloc(ptr, old) };
        assert!(core.large_geometry_for_test(ptr).is_none());
        assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count);
        if align >= SegmentLayout::SEGMENT || !cfg!(feature = "alloc-decommit") {
            assert_eq!(AllocCore::dbg_segments_released_total(), released + 1);
        }

        // Positive grow and shrink controls must still preserve the prefix.
        let ptr = core.alloc(old);
        assert!(!ptr.is_null());
        // SAFETY: freshly issued, uniquely owned requested extent.
        unsafe { fill(ptr, old.size()) };
        // SAFETY: exact live start/layout; successful realloc transfers ownership.
        let grown = unsafe { core.realloc(ptr, old, old.size() * 2) };
        assert!(!grown.is_null());
        // SAFETY: successful growth preserved the initialized old prefix.
        unsafe { assert_bytes(grown, old.size()) };
        let grown_layout = layout(old.size() * 2, align);
        // SAFETY: exact grown allocation/layout; shrink transfers ownership.
        let shrunk = unsafe { core.realloc(grown, grown_layout, old.size() / 2) };
        assert!(!shrunk.is_null());
        // SAFETY: successful shrink preserved this initialized prefix; free once.
        unsafe {
            assert_bytes(shrunk, old.size() / 2);
            core.dealloc(shrunk, layout(old.size() / 2, align));
        }
        assert!(core.large_geometry_for_test(shrunk).is_none());
    }

    #[cfg(feature = "alloc-global")]
    heap_cases();
}

#[cfg(feature = "alloc-global")]
fn heap_cases() {
    use sefer_alloc::registry::{bootstrap, segment_route::RouteDirectory, HeapRegistry};

    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("exclusive heap lease");
    let heap = lease.core();
    for align in [16, SegmentLayout::SEGMENT] {
        let old = layout(4 * 1024 * 1024, align);
        let ptr = heap.alloc(old);
        assert!(!ptr.is_null());
        let root = heap.dbg_segment_base_of_ptr(ptr);
        // Membership oracle: dbg_segment_registered_for (canonical table
        // lookup) ⟺ ptr's segment is still registered in this heap's table
        // (the former owner-stamp `Some(_)` baseline).
        assert!(
            heap.dbg_segment_registered_for(ptr),
            "fresh Large alloc must resolve in the heap's table"
        );
        // SAFETY: live allocation owns the entire requested writable extent.
        unsafe { fill(ptr, old.size()) };
        for address in [ptr.addr() - 64, ptr.addr() + 64] {
            let interior = core::ptr::without_provenance_mut(address);
            assert!(RouteDirectory::global().lookup(interior).is_none());
            // Small layout deliberately reaches fastbin's early hardened guards.
            for free_layout in [old, layout(64, 16)] {
                let hardened_layout_reject = cfg!(all(feature = "fastbin", feature = "hardened"))
                    && free_layout != old
                    && heap.dbg_segment_registered_for(interior);
                let before = AllocCore::dbg_foreign_or_unroutable_frees();
                let rejects_before = AllocCore::dbg_local_reject_frees();
                let hardened_before =
                    sefer_alloc::registry::HeapCore::dbg_hardened_large_noop_count();
                #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
                let lookups_before =
                    sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_hits()
                        + sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_misses();
                let released = AllocCore::dbg_segments_released_total();
                let table_count = heap.dbg_table_count();
                // SAFETY: deliberate invalid-address/layout boundary probe;
                // only rejection, not payload access or free, is permitted.
                unsafe { heap.dealloc(interior, free_layout) };
                if resolves_locally(align, ptr, interior.addr()) {
                    assert_local_reject(before, rejects_before);
                } else {
                    assert_unroutable_reject(before, rejects_before);
                }
                assert_eq!(
                    sefer_alloc::registry::HeapCore::dbg_hardened_large_noop_count(),
                    hardened_before
                        + u64::from(hardened_layout_reject && cfg!(feature = "alloc-stats"))
                );
                #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
                if free_layout != old && heap.dbg_segment_registered_for(interior) {
                    // M5 oracle (PRF-02): the own free resolves the owned block
                    // exactly once; the former second canonical re-resolution
                    // is gone, as on the batch path below.
                    assert_eq!(
                        sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_hits()
                            + sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_misses(),
                        lookups_before + 1
                    );
                }
                assert!(
                    heap.dbg_segment_registered_for(ptr),
                    "still registered after the rejected interior probe"
                );
                assert_eq!(heap.dbg_segment_base_of_ptr(ptr), root);
                assert_eq!(heap.dbg_table_count(), table_count);
                assert_eq!(AllocCore::dbg_segments_released_total(), released);
                #[cfg(feature = "batch-api")]
                {
                    let before = AllocCore::dbg_foreign_or_unroutable_frees();
                    let rejects_before = AllocCore::dbg_local_reject_frees();
                    let hardened_before =
                        sefer_alloc::registry::HeapCore::dbg_hardened_large_noop_count();
                    #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
                    let lookups_before =
                        sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_hits()
                            + sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_misses();
                    // SAFETY: same deliberate rejection probe, through batch
                    // routing (both scalar Large and fastbin Small layout).
                    unsafe { heap.dealloc_batch(free_layout, &[interior]) };
                    if resolves_locally(align, ptr, interior.addr()) {
                        assert_local_reject(before, rejects_before);
                    } else {
                        assert_unroutable_reject(before, rejects_before);
                    }
                    assert_eq!(
                        sefer_alloc::registry::HeapCore::dbg_hardened_large_noop_count(),
                        hardened_before
                            + u64::from(hardened_layout_reject && cfg!(feature = "alloc-stats"))
                    );
                    #[cfg(all(feature = "fastbin", feature = "bench-internals"))]
                    if free_layout != old && heap.dbg_segment_registered_for(interior) {
                        // M5 batch oracle: one owner lookup, no substrate hop.
                        assert_eq!(
                            sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_hits()
                                + sefer_alloc::registry::HeapCore::dbg_contains_base_tier1_misses(),
                            lookups_before + 1
                        );
                    }
                    assert!(
                        heap.dbg_segment_registered_for(ptr),
                        "still registered after the rejected batch interior probe"
                    );
                    assert_eq!(heap.dbg_table_count(), table_count);
                    assert_eq!(AllocCore::dbg_segments_released_total(), released);
                }
            }
            for new_size in [old.size() / 2, old.size() * 2] {
                let reserved = AllocCore::dbg_segments_reserved_total();
                let released = AllocCore::dbg_segments_released_total();
                let table_count = heap.dbg_table_count();
                // SAFETY: deliberate interior realloc rejection probe; the
                // original live allocation must not be read or consumed.
                let result = unsafe { heap.realloc(interior, old, new_size) };
                assert!(result.is_null());
                assert!(
                    heap.dbg_segment_registered_for(ptr),
                    "still registered after the rejected interior realloc probe"
                );
                assert_eq!(heap.dbg_segment_base_of_ptr(ptr), root);
                assert_eq!(heap.dbg_table_count(), table_count);
                assert_eq!(AllocCore::dbg_segments_reserved_total(), reserved);
                assert_eq!(AllocCore::dbg_segments_released_total(), released);
            }
            // SAFETY: original membership has survived every rejection above.
            unsafe { assert_bytes(ptr, old.size()) };
        }
        let table_count = heap.dbg_table_count();
        let released = AllocCore::dbg_segments_released_total();
        let count = AllocCore::dbg_foreign_or_unroutable_frees();
        // SAFETY: exact original start/layout, freed once after no-op probes.
        unsafe { heap.dealloc(ptr, old) };
        assert!(
            !heap.dbg_segment_registered_for(ptr),
            "freed Large segment must leave this heap's table"
        );
        assert_eq!(heap.dbg_table_count(), table_count);
        assert!(RouteDirectory::global().lookup(ptr).is_none());
        assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count);
        if align >= SegmentLayout::SEGMENT || !cfg!(feature = "alloc-decommit") {
            assert_eq!(AllocCore::dbg_segments_released_total(), released + 1);
        }

        let ptr = heap.alloc(old);
        assert!(!ptr.is_null());
        // SAFETY: fresh live requested extent, exclusively owned.
        unsafe { fill(ptr, old.size()) };
        // SAFETY: exact live start/layout; growth transfers ownership on success.
        let grown = unsafe { heap.realloc(ptr, old, old.size() * 2) };
        assert!(!grown.is_null());
        // SAFETY: successful growth preserves initialized original bytes.
        unsafe { assert_bytes(grown, old.size()) };
        // SAFETY: exact grown layout/start; shrink transfers ownership on success.
        let shrunk = unsafe { heap.realloc(grown, layout(old.size() * 2, align), old.size() / 2) };
        assert!(!shrunk.is_null());
        // SAFETY: successful shrink preserved initialized prefix; free exactly once.
        unsafe {
            assert_bytes(shrunk, old.size() / 2);
            heap.dealloc(shrunk, layout(old.size() / 2, align));
        }
        assert!(
            !heap.dbg_segment_registered_for(shrunk),
            "freed Large segment must leave this heap's table"
        );
    }
}
