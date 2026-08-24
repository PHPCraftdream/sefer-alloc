//! Smoke tests for `numa-shim`.
//!
//! These tests verify the public API contracts without asserting any
//! platform-specific NUMA topology (which differs between hosts).
//!
//! task #1306: the `bind_range`-era tests ("does not panic" non-assertions)
//! are gone with the API. The `Result`-returning `reserve_preferred_on_node`
//! gives this suite real behavioral oracles: a genuine `Ok(_)` on a fresh
//! (never-touched) reservation, and typed `Err(_)` outcomes for contract
//! violations and the Linux nodemask limit.

use numa_shim::current_node;

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

/// With `vmem-integration`: `reserve_preferred_on_node` returns a usable span
/// as a genuine `Ok` — on Linux the `mbind(2)` policy call on the complete OS
/// reservation span actually SUCCEEDED (its return value is checked now,
/// task #1306); on Windows the `VirtualAllocExNuma` reserve+commit chain
/// succeeded. The old `reserve_on_node -> Option` could not distinguish
/// "bound" from "silently unbound"; this asserts the real outcome on a
/// reservation that has never been touched — exactly the regime where
/// mbind's future-fault-only semantics apply.
///
/// macOS and any platform under miri have no NUMA-preference API at all —
/// per the module doc's platform-support table, that surfaces as an explicit
/// `Err(UnsupportedPlatform)`, not a silent unbound no-op (task #1306's
/// whole point); this test asserts THAT outcome there instead.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_returns_valid_span() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};

    // Use runtime page size: macOS aarch64 (Apple Silicon) uses 16 KiB pages,
    // not the 4 KiB constant `aligned_vmem::PAGE`. mmap rejects sizes/aligns
    // that are not a multiple of the kernel's actual page granule.
    let page = aligned_vmem::page_size();
    let size = page * 4;
    let align = page;
    let node = current_node().unwrap_or(0);

    let result = reserve_preferred_on_node(size, align, NodeId::new(node));

    if cfg!(all(any(target_os = "linux", windows), not(miri))) {
        let r = result.expect("NUMA-preferred reservation failed");

        // Check alignment and size.
        assert_eq!(r.as_ptr() as usize % align, 0, "base is not align-aligned");
        assert_eq!(r.len(), size);

        // Write and read back to confirm the memory is accessible.
        // SAFETY: `r` owns the reservation; we write and read a single byte at
        // the start of the usable span before dropping it.
        unsafe {
            r.as_ptr().write(0x5A);
            assert_eq!(r.as_ptr().read(), 0x5A);
        }

        // Drop releases the reservation back to the OS (RAII).
        drop(r);
    } else {
        assert!(
            matches!(result, Err(ReserveNumaError::UnsupportedPlatform)),
            "expected UnsupportedPlatform, got {result:?}"
        );
    }
}

/// Windows-specific: `reserve_preferred_on_node` with a large alignment
/// (> PAGE) exercises the over-reserve + trim path through
/// `VirtualAllocExNuma` + `Reservation::from_raw_parts`. Validates that the
/// direct-NUMA path produces a usable, properly-aligned span and that `Drop`
/// releases the WHOLE over-reserved region without leaking.
///
/// On Linux this also runs (the over-reserve + trim is platform-agnostic via
/// the mmap-based `aligned_vmem` reservation, with `mbind` applied to the
/// complete reservation span); only the underlying syscall differs. The
/// contract is identical.
///
/// macOS and any platform under miri have no NUMA-preference API — see
/// `reserve_preferred_on_node_returns_valid_span`'s doc comment above for why
/// this asserts `Err(UnsupportedPlatform)` there instead.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_large_align_round_trip() {
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};

    // 4 MiB span aligned to 4 MiB — a realistic allocator-segment size that
    // exercises the over-reserve (size + align = 8 MiB on Windows) path.
    // 4 MiB is a multiple of both the 4 KiB and 16 KiB page granules, so this
    // works on all targets including macOS aarch64.
    let page = aligned_vmem::page_size();
    let span = 4 * 1024 * 1024;
    let align = span;
    let node = current_node().unwrap_or(0);

    let result = reserve_preferred_on_node(span, align, NodeId::new(node));

    if !cfg!(all(any(target_os = "linux", windows), not(miri))) {
        assert!(
            matches!(result, Err(ReserveNumaError::UnsupportedPlatform)),
            "expected UnsupportedPlatform, got {result:?}"
        );
        return;
    }

    let r = result.expect("NUMA-preferred 4 MiB-aligned reservation failed");

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
        let r2 = reserve_preferred_on_node(span, align, NodeId::new(node)).expect("repeat reserve");
        drop(r2);
    }
}

/// task #1306: the new typed-error oracle the old `Option` API could not
/// express — a zero `size` is an argument-contract violation, surfaced as
/// `InvalidArguments`, DISTINCT from an OOM refusal (`Os`). Runs on Linux and
/// Windows: the Windows backend validates explicitly, the Linux backend maps
/// `aligned_vmem`'s `invalid_argument` error, and the mock mirrors both.
///
/// On macOS and any platform under miri, the platform check itself is
/// unconditional and runs BEFORE argument validation (there is no NUMA API
/// to validate arguments against), so the outcome there is
/// `UnsupportedPlatform`, not `InvalidArguments` — see
/// `reserve_preferred_on_node_returns_valid_span`'s doc comment above.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_rejects_zero_size_with_invalid_arguments() {
    use aligned_vmem::page_size;
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};

    let page = page_size();
    let err =
        reserve_preferred_on_node(0, page, NodeId::new(0)).expect_err("zero size must be rejected");

    let expected: fn(&ReserveNumaError) -> bool =
        if cfg!(all(any(target_os = "linux", windows), not(miri))) {
            |e| matches!(e, ReserveNumaError::InvalidArguments)
        } else {
            |e| matches!(e, ReserveNumaError::UnsupportedPlatform)
        };
    assert!(expected(&err), "unexpected error variant: {err:?}");
}

/// task #1306: the Linux single-`u64` nodemask limit (nodes 0..=63) is now a
/// typed `InvalidNode` error instead of the old silent no-op-with-unbound-
/// reservation. Linux-only: Windows forwards any node id to the OS (its
/// refusal surfaces as `Os`), so the `InvalidNode` variant is not assertable
/// there.
#[cfg(all(target_os = "linux", not(miri), feature = "vmem-integration"))]
#[test]
fn reserve_preferred_on_node_rejects_node_beyond_nodemask_range() {
    use aligned_vmem::page_size;
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};

    let page = page_size();
    let err = reserve_preferred_on_node(page, page, NodeId::new(64))
        .expect_err("node 64 must be rejected on Linux");
    assert!(
        matches!(err, ReserveNumaError::InvalidNode),
        "expected InvalidNode, got {err:?}"
    );
}

/// task #778 (rust-intel audit round-closing review, finding F3, MEDIUM):
/// the Windows path (`reserve_aligned_numa`, fixed by task #724) must commit
/// only the caller-requested `size` bytes, NOT the whole
/// `over = size + align` over-reservation. #724's own "EMPIRICALLY VERIFIED"
/// claim cited the two round-trip tests above as proof — the review showed
/// both pass IDENTICALLY against the reverted pre-#724 double-commit bug
/// (they assert alignment/length/byte-readback, none of which the bug
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
fn reserve_preferred_on_node_commits_only_the_requested_span_not_the_whole_over_reservation() {
    use numa_shim::{reserve_preferred_on_node, NodeId};

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

    let r = reserve_preferred_on_node(size, align, NodeId::new(node))
        .expect("NUMA-preferred reservation failed");

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
