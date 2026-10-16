//! R6-T50 (`docs/agent_reviews_round6/memory_safety_review.md` R6-MS-3):
//! the foreign realloc leg must bound its move-leg read by the route
//! descriptor's registration-time COMMITTED span, not the reserved route
//! span. Under `large-reserved-capacity` a forged `old_layout` whose size
//! fits the reservation but exceeds the committed `span_usable` previously
//! passed the capacity check and drove the copy into reserved-but-
//! uncommitted VA (access violation on Windows; a garbage copy on
//! eager-commit backends). The descriptor bound turns both into a clean
//! null before any allocation. The metadata-address test pins the second
//! half of the fix: a Small/Primordial segment's metadata is not route
//! payload, so an address inside it is rejected at route lookup.
//!
//! These tests deliberately violate the `unsafe fn realloc` caller contract
//! (bogus `old_layout`). They probe defense-in-depth null returns, not a
//! license for false layouts — same posture as
//! `tests/regression_realloc_heap_oob.rs`, whose routed positive controls
//! this file mirrors for Large and Small sources.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "large-reserved-capacity"
))]

use core::alloc::Layout;
use core::sync::atomic::Ordering;

use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SeferAlloc;
use std::alloc::GlobalAlloc;

// Serialise: the registry (and its per-thread heap) is process-global.
static SERIAL: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// 1 MiB source allocation: classifies Large (well above the largest small
/// class), and under `large-reserved-capacity` reserves `4 x usable`
/// (capped at 64 MiB) while committing only `usable` — so the
/// reserved-but-uncommitted tail between ~1 MiB and ~4 MiB exists.
const LARGE_SIZE: usize = 1024 * 1024;
/// Forged old size: exceeds the committed span (~1 MiB) yet fits the
/// reservation (~4 MiB).
const FORGED_OLD: usize = 2 * 1024 * 1024;

#[test]
fn foreign_realloc_forged_layout_beyond_committed_span_returns_null() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let large = Layout::from_size_align(LARGE_SIZE, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(large) };
    assert!(!p.is_null(), "setup: 1 MiB large allocation failed");
    // The block must be foreign-routed (descriptor, not another owner's
    // header) for the committed-span bound to apply.
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("large allocation has a registered route");
    // SAFETY: `p` is valid for LARGE_SIZE bytes.
    unsafe { core::ptr::write_bytes(p, 0x5A, LARGE_SIZE) };

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");
    let heap = lease.core();

    // The old Layout is intentionally false: LARGE_SIZE < FORGED_OLD <=
    // reserved_capacity. The committed-span descriptor bound must reject
    // this BEFORE any allocation or copy — pre-fix, the move leg read the
    // reserved-but-uncommitted tail.
    // SAFETY: `p` is a live allocation; the forged old layout is the
    // deliberate caller-misuse scenario under test (R6-T50).
    let result = unsafe {
        heap.realloc(
            p,
            Layout::from_size_align(FORGED_OLD, 16).unwrap(),
            FORGED_OLD,
        )
    };
    assert!(
        result.is_null(),
        "foreign realloc with a layout beyond the committed span must return \
         null (R6-T50), not copy from reserved-but-uncommitted VA"
    );

    // Null return: the old block is intact and still owned by the source.
    // SAFETY: `p` is still valid for LARGE_SIZE bytes.
    unsafe {
        assert_eq!(
            core::ptr::read(p),
            0x5A,
            "source block disturbed by the rejected realloc"
        );
    }
    drop(lease);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was
    // returned by a prior matching alloc in this test, is live, and is freed
    // exactly once with its true layout.
    unsafe { source.dealloc(p, large) };
}

#[test]
fn foreign_realloc_true_layout_within_committed_span_succeeds() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let large = Layout::from_size_align(LARGE_SIZE, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(large) };
    assert!(!p.is_null(), "setup: 1 MiB large allocation failed");
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("large allocation has a registered route");
    // SAFETY: `p` is valid for LARGE_SIZE bytes.
    unsafe { core::ptr::write_bytes(p, 0x99, LARGE_SIZE) };

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");
    let heap = lease.core();

    // Positive control: the TRUE old layout fits the committed span, so the
    // foreign leg must keep working (alloc + copy + exactly one foreign
    // free publication).
    // SAFETY: `p` is a live allocation with exactly `large`'s layout.
    let new_ptr = unsafe { heap.realloc(p, large, LARGE_SIZE + 64) };
    assert!(
        !new_ptr.is_null(),
        "a legit cross-heap Large realloc must still succeed under the R6-T50 \
         committed-span bound"
    );
    // SAFETY: the first byte is the preserved prefix.
    unsafe {
        assert_eq!(core::ptr::read(new_ptr), 0x99, "prefix not preserved");
    }

    // The foreign leg transferred `p` exactly once; the source owner
    // consumes the terminal obligation on explicit trim.
    assert!(route.pending_for_test(p));
    source.trim_current_thread();
    assert!(!route.pending_for_test(p));
    // SAFETY: `new_ptr` is a live (LARGE_SIZE + 64)-byte allocation from
    // this realloc.
    unsafe {
        heap.dealloc(
            new_ptr,
            Layout::from_size_align(LARGE_SIZE + 64, 16).unwrap(),
        )
    };
    drop(lease);
}

#[test]
fn foreign_realloc_small_segment_metadata_address_is_rejected() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let small = Layout::from_size_align(64, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(small) };
    assert!(!p.is_null(), "setup: small allocation failed");
    // Segment base of the source's Small/Primordial segment. Its metadata
    // (header, page map, bins, bitmaps) occupies [base, base + meta_end);
    // the route descriptor's payload must start past it (R6-T50).
    let base = (p.addr() & !(sefer_alloc::SegmentLayout::SEGMENT - 1)) as *mut u8;
    assert_ne!(base, p, "setup: expected a non-base block address");

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");
    let heap = lease.core();

    // A metadata address is not a routable payload start: lookup rejects it
    // before any capacity check or copy.
    // SAFETY: `base` is never dereferenced by the allocator; the bogus old
    // layout is the deliberate caller-misuse scenario under test.
    let result = unsafe { heap.realloc(base, Layout::from_size_align(4096, 16).unwrap(), 4096) };
    assert!(
        result.is_null(),
        "foreign realloc at a Small segment's metadata address must be \
         rejected (R6-T50), not copy segment metadata"
    );

    drop(lease);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was
    // returned by a prior matching alloc in this test, is live (the rejected
    // realloc consumed nothing), and is freed exactly once here.
    unsafe { source.dealloc(p, small) };
}

#[test]
fn foreign_realloc_small_true_block_within_payload_succeeds() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let small = Layout::from_size_align(64, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(small) };
    assert!(!p.is_null(), "setup: small allocation failed");
    let base = (p.addr() & !(sefer_alloc::SegmentLayout::SEGMENT - 1)) as *mut u8;
    assert_ne!(p, base, "setup: block must not sit at the segment base");
    // SAFETY: `p` is valid for 64 bytes.
    unsafe { core::ptr::write_bytes(p, 0x77, 64) };

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");
    let heap = lease.core();

    // Positive control: a REAL small block address (>= meta_end) keeps
    // passing lookup and the foreign leg still works.
    // SAFETY: `p` is a live allocation with exactly `small`'s layout.
    let new_ptr = unsafe { heap.realloc(p, small, 128) };
    assert!(
        !new_ptr.is_null(),
        "a legit cross-heap Small realloc must still succeed under the R6-T50 \
         payload-start bound"
    );
    // SAFETY: the first byte is the preserved prefix.
    unsafe {
        assert_eq!(core::ptr::read(new_ptr), 0x77, "prefix not preserved");
    }

    // SAFETY: `p` is the live allocation transferred by the realloc.
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("small route still linked");
    assert!(route.pending_for_test(p));
    source.trim_current_thread();
    assert!(!route.pending_for_test(p));
    // SAFETY: `new_ptr` is a live 128-byte allocation from this realloc.
    unsafe { heap.dealloc(new_ptr, Layout::from_size_align(128, 16).unwrap()) };
    drop(lease);
}

#[test]
fn foreign_realloc_after_owner_inplace_grow_succeeds() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let one_mib = Layout::from_size_align(LARGE_SIZE, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(one_mib) };
    assert!(!p.is_null(), "setup: 1 MiB large allocation failed");
    // SAFETY: `p` is valid for LARGE_SIZE bytes.
    unsafe { core::ptr::write_bytes(p, 0x3C, LARGE_SIZE) };

    // Owner-side GROW IN PLACE into the reserved-but-uncommitted tail
    // (OPT-G + `try_grow_large_reserved_capacity` under
    // `large-reserved-capacity`): the SAME pointer must come back and the
    // segment's live `span_usable` advances past the registration-time
    // committed span.
    // SAFETY: `p` is a live allocation with exactly `one_mib`'s layout.
    let grown = unsafe { source.realloc(p, one_mib, FORGED_OLD) };
    assert_eq!(
        grown, p,
        "OPT-G grow-in-place must preserve the pointer under \
         large-reserved-capacity"
    );
    // SAFETY: `p` is valid for FORGED_OLD bytes after the grow.
    unsafe { core::ptr::write_bytes(p.add(LARGE_SIZE), 0x5C, FORGED_OLD - LARGE_SIZE) };

    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned None");
    let heap = lease.core();

    // Foreign realloc with the TRUE grown layout: the read bound is the
    // segment's CURRENT committed frontier (advanced by the grow above),
    // so this must succeed. A registration-time immutable bound would
    // reject this legitimate call with a bogus null — this test is the
    // counterfactual for that rejected design.
    // SAFETY: `p` is a live allocation with exactly the grown layout.
    let new_ptr = unsafe {
        heap.realloc(
            p,
            Layout::from_size_align(FORGED_OLD, 16).unwrap(),
            FORGED_OLD,
        )
    };
    assert!(
        !new_ptr.is_null(),
        "legit foreign realloc of an owner-grown block must succeed under \
         the R6-T50 live-frontier bound"
    );
    // SAFETY: the prefix byte and the grown-tail byte are the preserved
    // copy.
    unsafe {
        assert_eq!(core::ptr::read(new_ptr), 0x3C, "prefix not preserved");
        assert_eq!(
            core::ptr::read(new_ptr.add(LARGE_SIZE)),
            0x5C,
            "grown-tail bytes not preserved"
        );
    }

    // The foreign leg transferred `p` exactly once; the source owner
    // consumes the terminal obligation on explicit trim.
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("large route still linked");
    assert!(route.pending_for_test(p));
    source.trim_current_thread();
    assert!(!route.pending_for_test(p));
    // SAFETY: `new_ptr` is a live FORGED_OLD-byte allocation from this
    // realloc.
    unsafe { heap.dealloc(new_ptr, Layout::from_size_align(FORGED_OLD, 16).unwrap()) };
    drop(lease);
}
