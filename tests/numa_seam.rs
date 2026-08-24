//! Unit tests for the NUMA OS-seam (`src/alloc_core/numa.rs`).
//!
//! Gated on `feature = "numa-aware"` — this whole file is a no-op without it.
//! Run with:
//!   cargo test --features "alloc-core numa-aware" --test numa_seam
//!
//! task #1306: the old `bind_segment` tests are gone with `bind_segment`
//! itself (the seam forwarded to numa-shim's removed `bind_range`). What
//! remains exercises the production surface: `current_node` and
//! `reserve_aligned_on_node` — now including the seam's new best-effort
//! fallback contract.

#![cfg(all(feature = "numa-aware", feature = "internals"))]

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
// reserve_aligned_on_node: basic smoke tests
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
/// miri, single-node Linux) the seam's best-effort fallback yields a plain
/// reservation — still correct.
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

/// task #1306: the seam is deliberately best-effort — a node id NO platform
/// can address (Linux: out of the single-`u64` nodemask range, rejected as
/// `InvalidNode` by the shim; Windows: the OS refuses it as `Os`; macOS/miri:
/// `UnsupportedPlatform`) must NOT fail the allocation: the seam falls back
/// to a plain unbound reservation and still returns `Some`. The old API
/// reached the same observable outcome only through silent internal no-ops
/// hidden inside the shim; this pins the fallback as the seam's own
/// documented contract, on every platform.
#[test]
fn reserve_aligned_on_node_unaddressable_node_falls_back_to_unbound() {
    use sefer_alloc::SegmentLayout;

    let segment_size = SegmentLayout::SEGMENT;
    // Any value no real host addresses and that is NOT the NO_NODE sentinel
    // (which takes the seam's dedicated no-preference path instead).
    let absurd_node = u32::MAX - 1;
    let result = numa::reserve_aligned_on_node(segment_size, absurd_node);
    assert!(
        result.is_some(),
        "best-effort seam must return Some even when the NUMA preference \
         cannot be installed (node={absurd_node})"
    );
    let (base, _reservation, reservation_len) = result.unwrap();
    let base_addr = base.as_ptr() as usize;
    assert_eq!(
        base_addr % segment_size,
        0,
        "fallback reservation must still be SEGMENT-aligned; got {base_addr:#x}"
    );
    assert!(reservation_len >= segment_size);
}
