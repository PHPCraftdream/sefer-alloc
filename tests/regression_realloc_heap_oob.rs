//! Realloc guard regressions for `registry::HeapCore::realloc`.
//!
//! The two oversized-old-layout tests deliberately violate this unsafe
//! method's caller contract. They probe defense-in-depth null returns, not
//! license for a caller to pass a false `Layout`. A standalone `AllocCore`
//! is not installed in the process-wide route directory, so its rejection
//! also cannot witness successful foreign routing. The correct-layout test
//! uses a TLS-bound `SeferAlloc` source and a separately claimed registry heap
//! to prove the supported cross-heap path copies and retires exactly once.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::alloc::Layout;
use core::sync::atomic::Ordering;

use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
#[cfg(all(feature = "alloc-xthread", feature = "alloc-core"))]
use sefer_alloc::AllocCore;
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

/// 8 MiB — exceeds one SEGMENT (4 MiB), so an 8 MiB claim cannot fit any
/// single-segment block's span.
const BOGUS_OLD: usize = 8 * 1024 * 1024;

/// Own-segment defense-in-depth probe: an oversized old-layout claim is
/// rejected before the move-leg copy. This deliberately violates the caller
/// contract and establishes no guarantee for arbitrary contract violations.
#[test]
fn heap_realloc_own_seg_oversized_layout_returns_null() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let small = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: `heap` was returned by `claim` and is the slot's sole writer for
    // the duration of this test (serialised by SERIAL).
    let p = unsafe { (*heap).alloc(small) };
    assert!(!p.is_null(), "setup: own-seg 16-byte alloc failed");
    // SAFETY: `p` is valid for 16 bytes.
    unsafe { core::ptr::write_bytes(p, 0xC3, 16) };

    let bogus = Layout::from_size_align(BOGUS_OLD, 16).unwrap();
    // SAFETY: exclusive heap access. The old Layout is intentionally false;
    // this test relies on the implementation's checked null-return path.
    let result = unsafe { (*heap).realloc(p, bogus, BOGUS_OLD) };
    assert!(
        result.is_null(),
        "own-seg realloc with a bogus oversized old_layout must return null \
         (R2-1 gap 2), not fall through to an 8 MiB out-of-segment read"
    );

    // The 16-byte block is intact (null return = failure, old block untouched).
    // SAFETY: `p` is still valid for 16 bytes.
    unsafe {
        assert_eq!(
            core::ptr::read(p),
            0xC3,
            "own-seg block disturbed by the rejected realloc"
        );
    }
    // SAFETY: same exclusive `heap` access; correct layout for the live block.
    unsafe { (*heap).dealloc(p, small) };
    // SAFETY: return the heap slot after exclusive use.
    unsafe { HeapRegistry::recycle(heap) };
}

/// A standalone substrate core is not registered in the process-wide route
/// directory. Its foreign realloc must reject without copying, regardless of
/// the deliberately false old Layout. The routed positive control below is
/// the non-vacuous cross-heap witness.
#[test]
#[cfg(all(feature = "alloc-xthread", feature = "alloc-core"))]
fn heap_realloc_foreign_sefer_ptr_oversized_layout_returns_null() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // A genuine sefer segment that is FOREIGN to the heap (owned by a
    // standalone substrate `AllocCore`, not the registry heap).
    let mut ac = AllocCore::new().expect("AllocCore::new");
    let small = Layout::from_size_align(16, 16).unwrap();
    let p = ac.alloc(small);
    assert!(!p.is_null(), "setup: substrate 16-byte alloc failed");
    // SAFETY: `p` is valid for 16 bytes.
    unsafe { core::ptr::write_bytes(p, 0x7E, 16) };

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let bogus = Layout::from_size_align(BOGUS_OLD, 16).unwrap();
    // SAFETY: exclusive heap access. The old Layout is intentionally false;
    // absence from the route directory rejects it before any payload read.
    let result = unsafe { (*heap).realloc(p, bogus, BOGUS_OLD) };
    assert!(
        result.is_null(),
        "unrouted standalone pointer must be rejected before any payload copy"
    );

    // Cleanup: the foreign leg returned null without freeing `p` (route lookup
    // rejected it before the copy/dealloc), so `p` is still owned by `ac`.
    // Reclaim it there with the CORRECT layout.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, small) };
    // SAFETY: return the heap slot after exclusive use.
    unsafe { HeapRegistry::recycle(heap) };
}

/// Gap 1 control: a correct-layout realloc from another routed registry heap
/// succeeds. A standalone `AllocCore::new()` has no foreign route, so it is
/// not a valid source for this cross-heap success oracle.
#[test]
#[cfg(all(feature = "alloc-xthread", feature = "alloc-core"))]
fn heap_realloc_foreign_sefer_ptr_correct_layout_succeeds() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let source = SeferAlloc::new();
    let small = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: the source allocator receives a valid nonzero Layout.
    let p = unsafe { source.alloc(small) };
    assert!(!p.is_null(), "setup: routed source allocation failed");
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("source allocation has a registered route");
    // SAFETY: `p` is valid for 16 bytes.
    unsafe { core::ptr::write_bytes(p, 0x99, 16) };

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "destination heap claim failed");

    // Correct layout (16), modest grow to 32: the source route validates
    // the payload span and the destination issues a fresh block.
    // SAFETY: the destination heap is exclusively claimed on this thread;
    // `p` is a current source allocation with exactly `small`'s layout.
    let new_ptr = unsafe { (*heap).realloc(p, small, 32) };
    assert!(
        !new_ptr.is_null(),
        "a legit cross-heap realloc (correct layout, modest grow) must succeed \
         under the R2-1 foreign-leg barrier"
    );
    // SAFETY: first 16 bytes are the preserved copy.
    unsafe {
        assert_eq!(
            core::ptr::read(new_ptr),
            0x99,
            "cross-heap realloc did not preserve the prefix"
        );
    }

    // The foreign leg transferred `p` once. The source TLS owner consumes
    // that terminal obligation on explicit trim, not by re-freeing `p`.
    assert!(route.pending_for_test(p));
    source.trim_current_thread();
    assert!(!route.pending_for_test(p));
    // SAFETY: exclusive `heap` access; `new_ptr` is a heap block of size 32.
    unsafe { (*heap).dealloc(new_ptr, Layout::from_size_align(32, 16).unwrap()) };
    // SAFETY: the destination heap is exclusively owned and its issued block
    // was retired. The source heap remains bound to this thread's TLS guard.
    unsafe { HeapRegistry::recycle(heap) };
}
