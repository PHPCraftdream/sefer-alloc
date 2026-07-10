//! F7 (task #25) — the HARDENED Large-segment kind guard on the OWN-THREAD
//! magazine free path (`HeapCore::dealloc_own_thread_with_base`).
//!
//! ## The hole this guards
//!
//! The own-thread fastbin free path keys entirely off the *layout*: if
//! `SizeClasses::class_for(layout)` returns `Some(c)`, it proceeds straight to
//! the magazine / bitmap M2 oracles WITHOUT ever consulting the segment's
//! `kind`. If a caller frees a pointer that actually lives in a LARGE segment
//! but passes a SMALL layout (a `GlobalAlloc`-contract violation — the real UB
//! is on the caller side), the oracles read the "bitmap"/magazine state out of
//! the bytes of the Large allocation's PAYLOAD. Those bytes can read as
//! "not-in-magazine, not-free" → the block is pushed into the small magazine
//! and a later small alloc hands out an address INSIDE the still-live Large
//! allocation → silent aliasing / double-issue.
//!
//! The substrate path (`AllocCore::dealloc`) routes by segment `kind` FIRST and
//! so degrades to a clean no-op on the same violation. The `hardened` feature
//! restores that symmetry on the magazine path: a `kind_at(base) == Large`
//! check before the oracles turns the mismatched free into a detected no-op.
//!
//! ## Counterfactual (RED without the guard)
//!
//! Comment out the `#[cfg(feature = "hardened")]` Large-kind block in
//! `heap_core.rs::dealloc_own_thread_with_base` and re-run under
//! `--features "production hardened"`: `large_ptr_small_layout_free_is_noop`
//! goes RED — the Large payload address is pushed into the small magazine and
//! re-issued by a later small alloc, appearing as a duplicate / an address
//! inside the live Large block. Restoring the guard → GREEN.
//!
//! Gated to `hardened` (which pulls `fastbin`): only that build compiles the
//! guard and the magazine path it defends.

#![cfg(all(feature = "hardened", feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

static SERIAL: AtomicBool = AtomicBool::new(false);

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

/// Free a LARGE allocation via a SMALL layout on the OWN thread. The hardened
/// Large-kind guard must make it a no-op: the Large payload address must NOT be
/// pushed into the small magazine and re-issued, and the live Large block must
/// stay intact (no aliasing).
#[test]
fn large_ptr_small_layout_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // 512 KiB, align 8 (< SEGMENT) — unambiguously routed to the dedicated
    // Large-segment path (class_for → None), yet owned by THIS heap so the
    // own-thread routing (`contains_base`) reaches the magazine free body.
    const LARGE_SIZE: usize = 512 * 1024;
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    // A small layout whose `class_for` is `Some(c)` — the mismatched size the
    // buggy caller uses to free the Large pointer.
    //
    // Size 64 (block_size 64) is chosen deliberately so this test ISOLATES the
    // F7 Large-kind guard from the sibling interior-pointer guard: the Large
    // payload starts at segment offset `hdr_aligned` = `align_up(SegmentHeader,
    // PAGE)` = 4096 (for align 8, PAGE 4096), and 4096 is a whole multiple of
    // the 64 B block size, so the interior-pointer guard's `off % block_size ==
    // 0` check PASSES the pointer through — only the F7 kind check stops it.
    // (A non-page-multiple block size like 48 would be caught by the interior
    // guard first, making this test vacuous for F7.)
    let small_layout = Layout::from_size_align(64, 8).unwrap();

    let large = unsafe { (*heap).alloc(large_layout) };
    assert!(!large.is_null(), "large alloc returned null");
    // Fill the payload so the oracles, if they (wrongly) ran, would read our
    // pattern out of the payload bytes rather than incidental zeros.
    unsafe { std::ptr::write_bytes(large, 0xCC, LARGE_SIZE) };

    // The hazardous own-thread free of the Large pointer with a SMALL layout —
    // must be a NO-OP under the hardened Large-kind guard.
    unsafe { (*heap).dealloc(large, small_layout) };

    // The Large payload must be untouched (the free must not have mutated it).
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload mutated by the mismatched small-layout free"
        );
    }

    // Cold-storm of small allocations: none may alias any byte of the live
    // Large block. If the guard were missing, the Large payload address would
    // have entered the small magazine and been re-issued here.
    const N: usize = 8192;
    let large_lo = large as usize;
    let large_hi = large_lo + LARGE_SIZE;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(small_layout) };
        assert!(!p.is_null(), "cold-storm small alloc returned null");
        let a = p as usize;
        assert!(
            !(large_lo..large_hi).contains(&a),
            "LARGE-KIND GUARD BROKEN: a small alloc was handed an address \
             inside the live Large block — the mismatched free aliased it"
        );
        issued.push(p);
    }
    // Distinctness: no small pointer may repeat (a Large payload re-issue would
    // still be distinct from small blocks, but a general double-issue would
    // surface here as a duplicate).
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE small pointer during the cold-storm"
    );

    // The Large block is still ours to free legitimately — heap stays sound.
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload corrupted by the cold-storm"
        );
        (*heap).dealloc(large, large_layout);
    }
    for &p in &issued {
        unsafe { (*heap).dealloc(p, small_layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
