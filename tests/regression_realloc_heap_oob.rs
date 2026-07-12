//! R2-1 regression (gap 1 + gap 2) for `registry::HeapCore::realloc` — the
//! registry-level SAFE `pub fn` reached via the `#[doc(hidden)] pub mod
//! registry`.
//!
//! ## Gap 1 — foreign leg had no membership barrier
//!
//! `HeapCore::realloc`'s foreign leg (a `ptr` whose segment base is NOT in
//! THIS heap's table) used to unconditionally alloc a fresh block and
//! `Node::copy_nonoverlapping` `old_layout.size().min(new_size)` bytes out of
//! `ptr`. Under `alloc-xthread` this leg is the deliberately-designed
//! cross-heap path, but it had NO check that `ptr` actually resolves to a
//! live sefer segment before copying — a bogus/foreign pointer was read out
//! of arbitrary caller-supplied memory under a safe fn.
//!
//! ## Gap 2 — caller-controlled copy length (own-seg leg)
//!
//! The own-segment move leg (and the foreign leg) trusted `old_layout.size()`
//! for the copy length exactly as the substrate `AllocCore::realloc` did.
//!
//! ## The fix
//!
//! The foreign leg now validates the segment-header magic BEFORE copying
//! (mirrors `dealloc_foreign_slow`'s first guard) and bounds the read by the
//! segment's committed span; the own-seg move leg applies the same span bound.
//! A bogus/oversized layout is rejected (null), never read out of bounds.
//!
//! ## Counterfactual (non-vacuity)
//!
//! RED (fix reverted): both scenarios reach an unbounded
//! `copy_nonoverlapping` of 8 MiB out of a 4 MiB segment — a read that
//! escapes the segment's OS allocation (miri: OOB; native: segfault or
//! non-null). GREEN (fix present): the membership/size check returns null
//! before any copy.

#![cfg(feature = "alloc-global")]

use core::alloc::Layout;
use core::sync::atomic::Ordering;

use sefer_alloc::registry::{bootstrap, HeapRegistry};
#[cfg(all(feature = "alloc-xthread", feature = "alloc-core"))]
use sefer_alloc::AllocCore;

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

/// Gap 2 (own-seg leg): a safe caller reallocs a 16-byte block OWNED by this
/// heap, claiming it is 8 MiB and asking to grow to 8 MiB. The move leg's read
/// is bounded by the segment's committed span (~4 MiB); the oversized claim is
/// rejected → null, no out-of-segment read.
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
    // SAFETY: same exclusive `heap` access.
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

/// Gap 1 (foreign leg): a REAL sefer pointer allocated by a substrate
/// `AllocCore` (so its segment base is NOT in this heap's table — foreign to
/// the heap) with a bogus oversized layout. The fix's magic check PASSES (it
/// is a genuine sefer segment) and the span bound REJECTS the 8 MiB claim →
/// null. Without the membership barrier (RED) the foreign leg would copy 8 MiB
/// out of the 4 MiB segment. Gated on `alloc-xthread` (the magic check is
/// xthread-only) plus `alloc-core` (to construct the substrate `AllocCore`).
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
    // SAFETY: exclusive `heap` access (serialised). `p` is foreign to this
    // heap, so this exercises the foreign leg.
    let result = unsafe { (*heap).realloc(p, bogus, BOGUS_OLD) };
    assert!(
        result.is_null(),
        "foreign-leg realloc of a real sefer pointer with a bogus oversized \
         layout must return null (R2-1 gap 1: magic barrier + span bound), not \
         copy 8 MiB out of a 4 MiB segment"
    );

    // Cleanup: the foreign leg returned null without freeing `p` (magic+bound
    // rejected it before the copy/dealloc), so `p` is still owned by `ac`.
    // Reclaim it there with the CORRECT layout.
    ac.dealloc(p, small);
    // SAFETY: return the heap slot after exclusive use.
    unsafe { HeapRegistry::recycle(heap) };
}

/// Gap 1 control: a CORRECT-layout realloc of a foreign (substrate-owned) sefer
/// pointer does NOT trip the span bound — the magic check passes (real sefer
/// segment) and the small claimed size is well within the segment span, so the
/// cross-heap copy proceeds (non-null). Proves the barrier is not a blanket
/// reject of legitimate cross-heap reallocs. Same gating as the test above.
#[test]
#[cfg(all(feature = "alloc-xthread", feature = "alloc-core"))]
fn heap_realloc_foreign_sefer_ptr_correct_layout_succeeds() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let mut ac = AllocCore::new().expect("AllocCore::new");
    let small = Layout::from_size_align(16, 16).unwrap();
    let p = ac.alloc(small);
    assert!(!p.is_null(), "setup: substrate 16-byte alloc failed");
    // SAFETY: `p` is valid for 16 bytes.
    unsafe { core::ptr::write_bytes(p, 0x99, 16) };

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Correct layout (16), modest grow to 32: magic passes, 16 <= ~4 MiB span,
    // so the foreign leg copies and returns a fresh non-null pointer.
    // SAFETY: exclusive `heap` access.
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

    // `p` was freed cross-thread by the foreign leg's `self.dealloc` (it
    // routes to `ac`'s segment owner; for a substrate-`AllocCore` segment the
    // owner-thread-free stamp is null so it degrades to a defensive no-op —
    // i.e. `p` stays live in `ac`). Reclaim it there. `new_ptr` lives on the
    // heap; free it with the new layout.
    // SAFETY: `p` is still valid for 16 bytes in `ac` (see comment).
    ac.dealloc(p, small);
    // SAFETY: exclusive `heap` access; `new_ptr` is a heap block of size 32.
    unsafe { (*heap).dealloc(new_ptr, Layout::from_size_align(32, 16).unwrap()) };
    // SAFETY: return the heap slot after exclusive use.
    unsafe { HeapRegistry::recycle(heap) };
}
