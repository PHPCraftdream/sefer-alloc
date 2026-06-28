//! Smoke tests for `numa-shim`.
//!
//! These tests verify the public API contracts without asserting any
//! platform-specific NUMA topology (which differs between hosts).

use numa_shim::{bind_range, current_node, NO_NODE};

/// `current_node()` must return either `None` (NUMA unavailable) or
/// `Some(n)` where `n < 64` (reasonable upper bound for NUMA node count).
#[test]
fn current_node_returns_valid_or_none() {
    match current_node() {
        None => {
            // NUMA unavailable on this host/platform — acceptable.
        }
        Some(node) => {
            assert!(
                node < 64,
                "NUMA node {node} is unreasonably large (expected < 64)"
            );
        }
    }
}

/// `bind_range` on a live owned allocation must not panic or cause UB.
///
/// This test allocates a page-sized buffer via `Box`, then calls `bind_range`
/// on it. On Linux the call issues `mbind(2)` (errors are silently ignored);
/// on Windows / macOS / miri it is a no-op. Either way the call must not panic.
#[test]
fn bind_range_on_owned_memory_does_not_panic() {
    // Allocate a buffer large enough to cover at least one OS page.
    let page = 4096usize;
    let mut buf: Vec<u8> = vec![0u8; page];

    let base = buf.as_mut_ptr();
    let len = buf.len();

    // Use NUMA node 0 as the target (always valid; no-op on single-node hosts).
    let node = current_node().unwrap_or(0);

    // SAFETY: `buf` is a live heap allocation owned exclusively by this scope.
    // `bind_range` never reads or writes the payload bytes — it only passes
    // `[base, base+len)` to `mbind(2)` (Linux) as kernel metadata. The Vec
    // outlives this call.
    unsafe { bind_range(base, len, node) };

    // Verify the buffer is still accessible after the call.
    buf[0] = 0xAB;
    assert_eq!(buf[0], 0xAB);
}

/// `bind_range` with `NO_NODE` sentinel must be a no-op and not panic.
#[test]
fn bind_range_no_node_is_noop() {
    let mut buf = [0u8; 16];
    // SAFETY: `buf` is a valid stack allocation; NO_NODE causes an early return
    // before any OS call, so no actual syscall is made.
    unsafe { bind_range(buf.as_mut_ptr(), buf.len(), NO_NODE) };
    // If we reach here without panic the test passes.
}

/// `bind_range` with `len == 0` must be a no-op and not panic.
#[test]
fn bind_range_zero_len_is_noop() {
    let mut buf = [0u8; 1];
    // SAFETY: len == 0 causes an early return before any OS call.
    unsafe { bind_range(buf.as_mut_ptr(), 0, 0) };
}

/// With `vmem-integration` feature: `reserve_on_node` returns a usable span.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_on_node_returns_valid_span() {
    use aligned_vmem::PAGE;
    use numa_shim::reserve_on_node;

    let size = PAGE * 4;
    let align = PAGE;
    let node = current_node().unwrap_or(0);

    let r = reserve_on_node(size, align, node)
        .expect("reserve_on_node returned None — OOM or contract violation");

    // Check alignment and size.
    assert_eq!(r.as_ptr() as usize % align, 0, "base is not align-aligned");
    assert_eq!(r.len(), size);

    // Write and read back to confirm the memory is accessible.
    // SAFETY: `r` owns the reservation; we write and read a single byte at
    // the start of the usable span before dropping.
    unsafe {
        r.as_ptr().write(0x5A);
        assert_eq!(r.as_ptr().read(), 0x5A);
    }

    // Drop releases the reservation back to the OS (RAII).
    drop(r);
}

/// Windows-specific: `reserve_on_node` with a large alignment (> PAGE)
/// exercises the over-reserve + trim path through `VirtualAllocExNuma` +
/// `Reservation::from_raw_parts`. Validates that the new direct-NUMA path
/// (task #83 follow-up) produces a usable, properly-aligned span and that
/// `Drop` releases the WHOLE over-reserved region without leaking.
///
/// On Linux/macOS this also runs (the over-reserve + trim is platform-
/// agnostic via the mmap-based `aligned_vmem::reserve_aligned`); only the
/// underlying syscall differs. The contract is identical.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_on_node_large_align_round_trip() {
    use aligned_vmem::PAGE;
    use numa_shim::reserve_on_node;

    // 4 MiB span aligned to 4 MiB — a realistic allocator-segment size that
    // exercises the over-reserve (size + align = 8 MiB on Windows) path.
    let span = 4 * 1024 * 1024;
    let align = span;
    let node = current_node().unwrap_or(0);

    let r = reserve_on_node(span, align, node)
        .expect("reserve_on_node 4 MiB aligned 4 MiB returned None");

    assert_eq!(r.as_ptr() as usize % align, 0, "base must be 4 MiB-aligned");
    assert_eq!(r.len(), span);
    assert!(r.reservation_len() >= span, "reservation_len must cover the span");

    // Page-stride write/readback fault-in: catches a wrong `len` (would SEGV
    // before the page-stride loop ends) and a wrong `align` (would mis-align
    // the writes).
    let pages = span / PAGE;
    // SAFETY: r owns `span` bytes at r.as_ptr(); we touch one byte per page.
    unsafe {
        for i in 0..pages {
            let p = r.as_ptr().add(i * PAGE);
            p.write(i as u8);
            assert_eq!(p.read(), i as u8);
        }
    }

    // Drop must release the WHOLE over-reservation, not just `span` bytes —
    // verified by absence of leaks under repeated reservation in a loop.
    drop(r);

    // Repeat 8× to surface any leak in the release path (loop OOMs quickly
    // if `Drop` only frees `span` instead of `reservation_len`).
    for _ in 0..8 {
        let r2 = reserve_on_node(span, align, node).expect("repeat reserve");
        drop(r2);
    }
}
