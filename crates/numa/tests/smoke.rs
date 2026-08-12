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
///
/// task #727 (rust-intel audit §D1): on a non-`mock` build this test has NO
/// observable postcondition to assert -- a no-op leaves nothing to inspect,
/// so it structurally cannot fail for the specific property its name claims
/// ("is a no-op"). Its real, load-bearing value here is narrower: a
/// cross-platform "doesn't crash the real dispatch path" probe (does
/// `bind_range` itself panic/UB when built for whichever real OS this test
/// runs on, before the mock ever enters the picture). The actual short-
/// circuit POSTCONDITION -- that NO_NODE prevents any call from reaching the
/// platform backend at all -- is asserted for real in
/// `tests/mock_dispatch.rs` (`bind_range_no_node_short_circuits`, via
/// `drain().is_empty()`); do not read this test as covering that property.
#[test]
fn bind_range_no_node_is_noop() {
    let mut buf = [0u8; 16];
    // SAFETY: `buf` is a valid stack allocation; NO_NODE causes an early return
    // before any OS call, so no actual syscall is made.
    unsafe { bind_range(buf.as_mut_ptr(), buf.len(), NO_NODE) };
    // If we reach here without panic the test passes.
}

/// `bind_range` with `len == 0` must be a no-op and not panic.
///
/// task #727 (rust-intel audit §D1): same caveat as
/// `bind_range_no_node_is_noop` above -- this is a cross-platform doesn't-
/// crash-the-real-dispatch-path probe, not coverage of the short-circuit
/// postcondition itself (that lives in `tests/mock_dispatch.rs`'s
/// `bind_range_zero_len_short_circuits`).
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
    use numa_shim::reserve_on_node;

    // Use runtime page size: macOS aarch64 (Apple Silicon) uses 16 KiB pages,
    // not the 4 KiB constant `aligned_vmem::PAGE`. mmap rejects sizes/aligns
    // that are not a multiple of the kernel's actual page granule.
    let page = aligned_vmem::page_size();
    let size = page * 4;
    let align = page;
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
    use numa_shim::reserve_on_node;

    // 4 MiB span aligned to 4 MiB — a realistic allocator-segment size that
    // exercises the over-reserve (size + align = 8 MiB on Windows) path.
    // 4 MiB is a multiple of both the 4 KiB and 16 KiB page granules, so this
    // works on all targets including macOS aarch64.
    let page = aligned_vmem::page_size();
    let span = 4 * 1024 * 1024;
    let align = span;
    let node = current_node().unwrap_or(0);

    let r = reserve_on_node(span, align, node)
        .expect("reserve_on_node 4 MiB aligned 4 MiB returned None");

    assert_eq!(r.as_ptr() as usize % align, 0, "base must be 4 MiB-aligned");
    assert_eq!(r.len(), span);
    assert!(
        r.reservation_len() >= span,
        "reservation_len must cover the span"
    );

    // Page-stride write/readback fault-in: catches a wrong `len` (would SEGV
    // before the page-stride loop ends) and a wrong `align` (would mis-align
    // the writes).
    let pages = span / page;
    // SAFETY: r owns `span` bytes at r.as_ptr(); we touch one byte per page.
    unsafe {
        for i in 0..pages {
            let p = r.as_ptr().add(i * page);
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

/// task #778 (rust-intel audit round-closing review, finding F3, MEDIUM):
/// `reserve_on_node`'s Windows path (`reserve_aligned_numa`, fixed by task
/// #724) must commit only the caller-requested `size` bytes, NOT the whole
/// `over = size + align` over-reservation. #724's own "EMPIRICALLY VERIFIED"
/// claim cited `reserve_on_node_returns_valid_span` /
/// `reserve_on_node_large_align_round_trip` above as proof -- the review
/// showed both pass IDENTICALLY against the reverted pre-#724 double-commit
/// bug (they assert alignment/length/byte-readback, none of which the bug
/// affects), so neither is a real regression test for the specific defect
/// #724 fixed. This test IS: it inspects the OS's own bookkeeping via
/// `VirtualQuery` and asserts the region strictly beyond `[base, base+size)`
/// -- which is ALWAYS non-empty, since `over - size == align > 0` by
/// construction -- reports `MEM_RESERVE`, not `MEM_COMMIT`. Zero-trust
/// counterfactual verified (see the task #778 commit message): reverting to
/// the pre-#724 single `MEM_RESERVE | MEM_COMMIT` call makes this test fail
/// with the tail region reporting `MEM_COMMIT`.
#[cfg(all(windows, feature = "vmem-integration"))]
#[test]
fn reserve_on_node_commits_only_the_requested_span_not_the_whole_over_reservation() {
    use numa_shim::reserve_on_node;

    // Mirrors the real `MEMORY_BASIC_INFORMATION` (winnt.h) on 64-bit
    // Windows, the only realistic target for this crate (this repo's own
    // Windows platform code elsewhere assumes a 64-bit pointer width too).
    // `PartitionId` is conditionally present in the C header
    // (`#if defined(_WIN64)`) -- included here since every supported target
    // triple for this crate is 64-bit.
    #[repr(C)]
    struct MemoryBasicInformation {
        base_address: *mut core::ffi::c_void,
        allocation_base: *mut core::ffi::c_void,
        allocation_protect: u32,
        partition_id: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        type_: u32,
    }

    extern "system" {
        fn VirtualQuery(
            lp_address: *const core::ffi::c_void,
            lp_buffer: *mut MemoryBasicInformation,
            dw_length: usize,
        ) -> usize;
    }

    const MEM_COMMIT: u32 = 0x0000_1000;
    const MEM_RESERVE: u32 = 0x0000_2000;

    // SAFETY: `MemoryBasicInformation` is `#[repr(C)]` and zero-initialized
    // fields are all valid bit patterns (pointers, u32s, usize, u16) --
    // `VirtualQuery` overwrites every field it succeeds on before this
    // value is read.
    fn query_state(addr: *const core::ffi::c_void) -> u32 {
        let mut mbi = MemoryBasicInformation {
            base_address: core::ptr::null_mut(),
            allocation_base: core::ptr::null_mut(),
            allocation_protect: 0,
            partition_id: 0,
            region_size: 0,
            state: 0,
            protect: 0,
            type_: 0,
        };
        // SAFETY: `addr` is a valid address inside a live reservation this
        // process owns (the caller passes either `base` or `base + size`,
        // both within `[reservation_ptr, reservation_ptr + reservation_len)`);
        // `&mut mbi` is a valid, correctly-sized out-pointer for the exact
        // struct size passed as `dw_length`. `VirtualQuery` never fails for
        // an address inside the calling process's own address space.
        let n = unsafe {
            VirtualQuery(
                addr,
                &mut mbi,
                core::mem::size_of::<MemoryBasicInformation>(),
            )
        };
        assert_ne!(n, 0, "VirtualQuery failed for {addr:p}");
        mbi.state
    }

    let page = aligned_vmem::page_size();
    let size = page * 4;
    // 128 KiB > WIN_ALLOCATION_GRANULARITY (64 KiB) guarantees the Windows two-call
    // path actually over-reserves (aligned-vmem task #848 changed this to be conditional
    // on align > 64 KiB). This keeps the test's purpose (probing tail slack) intact.
    let align = 128 * 1024;
    let node = current_node().unwrap_or(0);

    let r = reserve_on_node(size, align, node)
        .expect("reserve_on_node returned None — OOM or contract violation");

    let base = r.as_ptr();
    let committed_len = r.len();
    let raw = r.reservation_ptr();
    let over = r.reservation_len();
    assert_eq!(committed_len, size);

    // With align fixed at 128 KiB (above WIN_ALLOCATION_GRANULARITY), the
    // Windows two-call path over-reserves (over = size + align), making
    // [base + size, raw + over) non-empty. This is the condition the test probes.
    let front_slack = (base as usize) - (raw as usize);
    let tail_slack = over - front_slack - committed_len;
    assert!(
        tail_slack > 0,
        "tail slack must be non-empty by construction"
    );

    // SAFETY: `base` is valid for `committed_len` bytes (the reservation's
    // own contract); `base.add(committed_len)` stays within the `over`-byte
    // reservation since `committed_len + tail_slack <= over - front_slack`.
    let tail_probe = unsafe { base.add(committed_len) };

    let committed_state = query_state(base.cast());
    assert_eq!(
        committed_state, MEM_COMMIT,
        "the requested [base, base+size) span must be committed"
    );

    let tail_state = query_state(tail_probe.cast());
    assert_eq!(
        tail_state, MEM_RESERVE,
        "the slack beyond [base, base+size) must stay MEM_RESERVE, not \
         MEM_COMMIT -- MEM_COMMIT here is exactly the #724 double-commit \
         regression (committing the whole `over = size + align` span \
         instead of only the requested `size`)"
    );

    drop(r);
}
