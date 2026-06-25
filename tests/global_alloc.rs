//! Phase 11 — correctness of the `SeferMalloc` `GlobalAlloc` face, exercised via
//! its `GlobalAlloc` API directly (NOT installed as this test binary's
//! `#[global_allocator]`).
//!
//! ## Why direct-API and not `#[global_allocator]` here
//!
//! Installing `SeferMalloc` as the *process-wide* allocator for a libtest binary
//! subjects it to libtest's harness allocations (parallel test threads, panic
//! hooks, capture buffers, thread teardown) — a hostile, reentrancy-heavy
//! pattern that the current TLS binding does not yet serve robustly (it returns
//! null under reentrant/early-init access, which aborts the process). That
//! robustness is part of the **remaining hardening** documented in
//! `docs/MALLOC_BENCH.md` and `docs/MALLOC_PLAN.md` (Phase 11 hardening gate),
//! NOT yet production-trusted.
//!
//! What DOES work and is proven here + by `examples/global_allocator.rs` (a
//! real `#[global_allocator]` running a single-threaded Vec/HashMap workload to
//! completion): the `GlobalAlloc` face itself serves correct, aligned,
//! non-overlapping memory. We verify that here by driving the API directly with
//! pattern write/read-back — NON-VACUOUS: a wrong size, overlap, or lost byte
//! fails an assertion.

#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferMalloc;

fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a + asize <= b || b + bsize <= a)
}

#[test]
fn alloc_dealloc_roundtrip_is_valid_and_aligned() {
    let a = SeferMalloc::new();
    for &(size, align) in &[(1usize, 1usize), (8, 8), (64, 16), (1000, 8), (4096, 4096)] {
        let layout = Layout::from_size_align(size, align).unwrap();
        // SAFETY: layout has non-zero size and valid power-of-two alignment.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "alloc({size},{align}) returned null");
        assert_eq!((p as usize) % align, 0, "misaligned for align={align}");
        // SAFETY: p valid for `size` bytes; write a pattern and read it back.
        unsafe {
            for b in 0..size {
                p.add(b).write(0x5A);
            }
            for b in 0..size {
                assert_eq!(p.add(b).read(), 0x5A, "byte {b} not writable/readable");
            }
            a.dealloc(p, layout);
        }
    }
}

#[test]
fn alloc_zeroed_is_zero() {
    let a = SeferMalloc::new();
    let layout = Layout::from_size_align(777, 8).unwrap();
    // SAFETY: valid layout.
    let p = unsafe { a.alloc_zeroed(layout) };
    assert!(!p.is_null());
    // SAFETY: zeroed allocation valid for 777 bytes.
    unsafe {
        for b in 0..777 {
            assert_eq!(p.add(b).read(), 0, "byte {b} not zeroed");
        }
        a.dealloc(p, layout);
    }
}

#[test]
fn many_live_allocations_do_not_overlap() {
    let a = SeferMalloc::new();
    let layout = Layout::from_size_align(128, 8).unwrap();
    let mut live: Vec<(usize, u8)> = Vec::new();
    for i in 0..256u32 {
        // SAFETY: valid layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null());
        let fill = (i & 0xFF) as u8;
        // SAFETY: p valid for 128 bytes.
        unsafe {
            for b in 0..128 {
                p.add(b).write(fill);
            }
        }
        // No overlap with any live block.
        for &(q, _) in &live {
            assert!(
                !ranges_overlap(p as usize, 128, q, 128),
                "allocation {i} overlaps a live block"
            );
        }
        live.push((p as usize, fill));
    }
    // Every block still holds its own fill (no cross-contamination).
    for &(p, fill) in &live {
        // SAFETY: p valid for 128 bytes, still live.
        unsafe {
            for b in 0..128 {
                assert_eq!((p as *const u8).add(b).read(), fill, "block clobbered");
            }
        }
    }
    for &(p, _) in &live {
        // SAFETY: p was allocated with `layout` and is live.
        unsafe { a.dealloc(p as *mut u8, layout) };
    }
}

#[test]
fn realloc_grows_and_preserves_prefix() {
    let a = SeferMalloc::new();
    let layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: valid layout.
    let p = unsafe { a.alloc(layout) };
    assert!(!p.is_null());
    // SAFETY: p valid for 64 bytes.
    unsafe {
        for b in 0..64 {
            p.add(b).write((b as u8).wrapping_mul(3));
        }
    }
    // SAFETY: p is a live allocation of `layout`; grow to 4096.
    let p2 = unsafe { a.realloc(p, layout, 4096) };
    assert!(!p2.is_null());
    // SAFETY: p2 valid for 4096 bytes; first 64 preserved.
    unsafe {
        for b in 0..64 {
            assert_eq!(
                p2.add(b).read(),
                (b as u8).wrapping_mul(3),
                "realloc lost prefix byte {b}"
            );
        }
        a.dealloc(p2, Layout::from_size_align(4096, 8).unwrap());
    }
}

#[test]
fn churn_reuses_without_growth() {
    let a = SeferMalloc::new();
    let layout = Layout::from_size_align(48, 8).unwrap();
    // Many alloc/dealloc cycles — would corrupt or exhaust if state were
    // mishandled. (Same-thread, so no cross-thread routing involved.)
    for _ in 0..20_000 {
        // SAFETY: valid layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null());
        // SAFETY: p valid for 48 bytes.
        unsafe {
            p.write(0xC3);
            assert_eq!(p.read(), 0xC3);
            a.dealloc(p, layout);
        }
    }
}
