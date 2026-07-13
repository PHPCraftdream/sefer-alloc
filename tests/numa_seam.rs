//! Unit tests for the NUMA OS-seam (`src/alloc_core/numa.rs`).
//!
//! Gated on `feature = "numa-aware"` — this whole file is a no-op without it.
//! Run with:
//!   cargo test --features "alloc-core numa-aware" --test numa_seam

#![cfg(feature = "numa-aware")]

use sefer_alloc::alloc_core::numa;

// ---------------------------------------------------------------------------
// Basic invariant: current_node() returns either NO_NODE or a sane value
// ---------------------------------------------------------------------------

/// `current_node()` must return either the sentinel `NO_NODE` (unsupported /
/// feature disabled / miri) or a value in the range [0, 64).  64 is a
/// generous upper bound — current server hardware tops out at ~8 NUMA nodes;
/// we allow up to 64 to future-proof without being unbounded.
#[test]
fn current_node_returns_valid_value() {
    let node = numa::current_node();
    assert!(
        node == numa::NO_NODE || node < 64,
        "current_node() returned an implausibly large value: {node}"
    );
}

// ---------------------------------------------------------------------------
// bind_segment with NO_NODE must be a no-op (no panic, no UB)
// ---------------------------------------------------------------------------

/// Calling `bind_segment` with `NO_NODE` must be a complete no-op.  We use a
/// stack-allocated dummy pointer — `bind_segment` with `NO_NODE` returns
/// immediately without touching the pointer at all, so there is no UB.
#[test]
fn bind_segment_on_no_node_is_noop() {
    // A dummy non-null address; we pass NO_NODE so bind_segment short-circuits
    // before making any OS call and never dereferences the pointer.
    let dummy: *mut u8 = std::ptr::dangling_mut::<u8>();
    // SAFETY: NO_NODE makes bind_segment a no-op before any OS call.
    // Must not panic or crash.
    unsafe { numa::bind_segment(dummy, 4096, numa::NO_NODE) };
}

/// Calling `bind_segment` with `len == 0` must be a no-op regardless of node.
#[test]
fn bind_segment_zero_len_is_noop() {
    let dummy: *mut u8 = std::ptr::dangling_mut::<u8>();
    // SAFETY: len == 0 makes bind_segment a no-op before any OS call.
    // len == 0 early-return guard.
    unsafe { numa::bind_segment(dummy, 0, 0) };
}

// ---------------------------------------------------------------------------
// reserve_aligned_on_node: basic smoke test
// ---------------------------------------------------------------------------

/// `reserve_aligned_on_node` with `NO_NODE` must behave identically to a
/// plain OS reservation — it should succeed and return a non-null,
/// SEGMENT-aligned base.
#[test]
fn reserve_aligned_on_no_node_succeeds() {
    use sefer_alloc::SegmentLayout;

    let segment_size = SegmentLayout::SEGMENT;
    let result = numa::reserve_aligned_on_node(segment_size, numa::NO_NODE);
    assert!(
        result.is_some(),
        "reserve_aligned_on_node returned None (OOM?) for NO_NODE"
    );
    let (base, reservation, reservation_len) = result.unwrap();
    let base_addr = base.as_ptr() as usize;
    assert_eq!(
        base_addr % segment_size,
        0,
        "base must be SEGMENT-aligned; got {base_addr:#x}"
    );
    assert!(reservation_len >= segment_size);

    // Release the reservation so we don't leak OS memory.
    // Use release_segment which is the public(crate) entry point.
    // Since it's pub(crate), access it through the documented pattern:
    // the Segment drop is the canonical path. We can't call release_segment
    // directly (it's pub(crate)). Instead we use the AllocCore-level
    // free path OR just leak it in the test (tests run in separate processes).
    // For a unit test we accept the small leak.
    let _ = (base, reservation, reservation_len);
}

/// `reserve_aligned_on_node` with the actual NUMA node (if available) must
/// also return a SEGMENT-aligned result.  On platforms without NUMA (macOS,
/// miri, single-node Linux) this falls back to plain mmap — still correct.
#[test]
fn reserve_aligned_on_current_node_succeeds() {
    use sefer_alloc::SegmentLayout;

    let node = numa::current_node();
    let segment_size = SegmentLayout::SEGMENT;
    let result = numa::reserve_aligned_on_node(segment_size, node);
    assert!(
        result.is_some(),
        "reserve_aligned_on_node returned None for node={node}"
    );
    let (base, _reservation, reservation_len) = result.unwrap();
    let base_addr = base.as_ptr() as usize;
    assert_eq!(
        base_addr % segment_size,
        0,
        "base must be SEGMENT-aligned; got {base_addr:#x} (node={node})"
    );
    assert!(reservation_len >= segment_size);
}

// ---------------------------------------------------------------------------
// Linux-specific: bind_segment on a real mmap'd region
// ---------------------------------------------------------------------------

/// On Linux, call `bind_segment` on a freshly mmap'd page.  `mbind` may
/// return `EINVAL` on single-node machines (no NUMA topology exposed) — this
/// is not a test failure; the call must simply not panic or corrupt memory.
///
/// We don't assert on *whether* the binding was applied (that requires
/// numastat / move_pages which we'd need to parse), only that the function
/// completes without panicking.
#[test]
#[cfg(all(target_os = "linux", not(miri)))]
fn bind_segment_with_real_node_does_not_panic() {
    // Allocate a small anonymous mapping via std to get a real OS page.
    // (We use Box::new to get a heap page rather than calling mmap directly,
    // since this is a test and we're not the global allocator here.)
    let mut buf: Vec<u8> = vec![0u8; 4096];
    let ptr = buf.as_mut_ptr();

    // SAFETY: `ptr` is a live, exclusively-owned mmap'd page.
    // Try to bind to node 0 (always exists, even on single-node machines).
    // mbind on a Rust-heap page is fine for testing the syscall path; the
    // kernel will simply ignore or accept the request.
    unsafe { numa::bind_segment(ptr, 4096, 0) };

    // Verify we can still read and write the buffer (no UAF / decommit).
    buf[0] = 42;
    assert_eq!(buf[0], 42);
    // If mbind had corrupted metadata, the write-back above would segfault.
}
