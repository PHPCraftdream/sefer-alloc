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

/// Helper for parsing `/proc/self/maps`: parse a hex `usize` without a
/// `0x` prefix. Returns `None` if the input is empty or exceeds twice the
/// platform pointer width in hex digits (16 on 64-bit targets)
/// (fail-closed, matching `crate::cpumap::parse_hex_u32`'s style).
#[cfg(all(target_os = "linux", feature = "vmem-integration", not(miri)))]
fn parse_hex_usize(s: &[u8]) -> Option<usize> {
    if s.is_empty() || s.len() > 2 * core::mem::size_of::<usize>() {
        return None;
    }
    let mut n: usize = 0;
    for &b in s {
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        };
        n = n.wrapping_mul(16).wrapping_add(digit as usize);
    }
    Some(n)
}

/// Helper for parsing `/proc/self/maps`: extract the first field
/// `<hexstart>-<hexend>` (dash then hex up to the first space or end of line).
/// Returns `None` if the format is malformed (no dash or invalid hex).
#[cfg(all(target_os = "linux", feature = "vmem-integration", not(miri)))]
fn parse_maps_range(line: &[u8]) -> Option<(usize, usize)> {
    let dash_pos = line.iter().position(|&b| b == b'-')?;
    let start = parse_hex_usize(&line[..dash_pos])?;
    let after_dash = &line[dash_pos + 1..];
    let space_or_end = after_dash
        .iter()
        .position(|&b| b == b' ')
        .unwrap_or(after_dash.len());
    let end = parse_hex_usize(&after_dash[..space_or_end])?;
    Some((start, end))
}

/// Returns `true` if `NUMA_SHIM_REQUIRE_ORACLE=1` is set in the environment.
///
/// task #1324 (P2-1 of the seventeenth independent review,
/// `docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md`).
/// The inverse of the root crate's `SEFER_NUMA_TEST=1` gate
/// (`tests/numa_alloc.rs`): that env var OPTS IN to running the expensive
/// real-multi-NUMA-hardware tests; this one declares that this process runs
/// on a host that is SUPPOSED to have working NUMA detection — the
/// `numa-shim-mock` CI job's real-Linux
/// `cargo test -p numa-shim --features vmem-integration` row is the only
/// place that sets it. Under it, the `current_node() == None` skip arms in
/// this file's positive tests PANIC instead of skipping: `current_node()`
/// is this crate's own detection chain, so a None there means detection
/// regressed, not that this host lacks NUMA hardware. Unset (local/dev
/// runs and every other CI row): today's tolerant skip, unchanged.
#[cfg(feature = "vmem-integration")]
fn require_oracle() -> bool {
    std::env::var("NUMA_SHIM_REQUIRE_ORACLE").as_deref() == Ok("1")
}

#[cfg(all(target_os = "linux", feature = "vmem-integration", not(miri)))]
#[test]
fn parse_maps_range_normal_line() {
    let line = b"7f0abc000000-7f0abd000000 rw-p 00000000 00:00 0   /path/to/thing";
    assert_eq!(
        parse_maps_range(line),
        Some((0x7f0abc000000, 0x7f0abd000000))
    );
}

#[cfg(all(target_os = "linux", feature = "vmem-integration", not(miri)))]
#[test]
fn parse_maps_range_no_path() {
    let line = b"7f0abc000000-7f0abd000000 rw-p 00000000 00:00 0";
    assert_eq!(
        parse_maps_range(line),
        Some((0x7f0abc000000, 0x7f0abd000000))
    );
}

#[cfg(all(target_os = "linux", feature = "vmem-integration", not(miri)))]
#[test]
fn parse_maps_range_malformed() {
    let line = b"7f0abc000000 7f0abd000000 rw-p 00000000 00:00 0"; // no dash
    assert_eq!(parse_maps_range(line), None);
}

/// `current_node()` must return either `None` (NUMA unavailable) or
/// `Some(n)` where the bound is platform-conditional:
/// - On Linux: `n < 64` (the Linux nodemask API uses a `u64` bitset,
///   so the detection topology scan only covers nodes 0..=63)
/// - On Windows: `n <= u16::MAX as u32` (the `GetNumaProcessorNodeEx`
///   API returns a `u16` node number)
/// - On other platforms (macOS, etc.): `None` is the only outcome
///
/// The Linux bound is NOT an arbitrary "any u32" check: it reflects
/// the detection contract's dependence on the reservation API's
/// single-u64 nodemask limit (finding F8, point 3 of
/// docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md).
#[test]
fn current_node_returns_valid_or_none() {
    match current_node() {
        None => {
            // NUMA unavailable on this host/platform — acceptable.
        }
        Some(node) => {
            if cfg!(target_os = "linux") {
                assert!(
                    node < 64,
                    "NUMA node {node} exceeds Linux nodemask limit (expected < 64)"
                );
            } else {
                assert!(
                    node <= u16::MAX as u32,
                    "NUMA node {node} exceeds Windows API bound (expected <= u16::MAX)"
                );
            }
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
///
/// task #1325 (P2-2 of the seventeenth review, F2 of the eighteenth): that
/// expectation is REAL-backend-only. The platform branch below is
/// backend-aware — under the `numa_shim_mock` cfg the mock dispatch arm
/// performs no platform check at all (task #1311/F6's Linux-shaped
/// contract), so this test takes the positive path on EVERY target,
/// including macOS and miri, matching what the mock actually returns.
///
/// Self-skip discipline: positive NUMA policy tests run only when
/// `current_node()` genuinely returns `Some(_)`; otherwise they skip loudly
/// (task #1311, F8.2). This covers the container/cgroup case where the
/// detected node may not be in allowed memory nodes.
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

    if !cfg!(any(
        numa_shim_mock,
        all(any(target_os = "linux", windows), not(miri))
    )) {
        // Unsupported platform: must error, not succeed.
        let result = reserve_preferred_on_node(
            size,
            align,
            NodeId::new(0).expect("literal 0, not the NO_NODE sentinel"),
        );
        assert!(
            matches!(result, Err(ReserveNumaError::UnsupportedPlatform)),
            "expected UnsupportedPlatform, got {result:?}"
        );
        return;
    }

    let Some(node) = current_node() else {
        // task #1324 (seventeenth review P2-1): on the real-Linux CI row
        // (NUMA_SHIM_REQUIRE_ORACLE=1) this None is a regression in the
        // crate's OWN detection chain, not a topology limitation.
        if require_oracle() {
            panic!(
                "current_node() returned None but NUMA_SHIM_REQUIRE_ORACLE=1 is set — \
                 this CI row exists specifically to prove NUMA detection works; a None \
                 here means the crate's own detection chain (sched_getcpu -> sysfs \
                 cpumap -> ReverseIndex::lookup) regressed, not that this host lacks \
                 NUMA hardware (task #1324, seventeenth review P2-1)"
            );
        }
        eprintln!(
            "skip: current_node() could not resolve a node on this host \
             (undetermined topology — task #1308's fail-closed detection); \
             the NUMA-preference path needs a genuinely resolved node, not \
             a node-0 guess (task #1311, F8.2)"
        );
        return;
    };
    let node =
        NodeId::new(node).expect("current_node()'s Some arm never yields the NO_NODE sentinel");

    let result = reserve_preferred_on_node(size, align, node);

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
///
/// Under the `numa_shim_mock` cfg the platform branch below is backend-aware
/// and takes the positive path on every target (task #1325) — see that same
/// doc comment.
///
/// Self-skip discipline: positive NUMA policy tests run only when
/// `current_node()` genuinely returns `Some(_)`; otherwise they skip loudly
/// (task #1311, F8.2).
///
/// Release oracle (task #1311, F8.1): after `drop(r)`, the OS's own
/// bookkeeping is queried to confirm the entire over-reservation is freed.
/// Four probes are checked (reservation start, usable span start, usable
/// span last page, reservation last page) — this catches the exact regression
/// where Drop frees only `span` instead of `reservation_len`.
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

    if !cfg!(any(
        numa_shim_mock,
        all(any(target_os = "linux", windows), not(miri))
    )) {
        // Unsupported platform: must error, not succeed.
        let result = reserve_preferred_on_node(
            span,
            align,
            NodeId::new(0).expect("literal 0, not the NO_NODE sentinel"),
        );
        assert!(
            matches!(result, Err(ReserveNumaError::UnsupportedPlatform)),
            "expected UnsupportedPlatform, got {result:?}"
        );
        return;
    }

    let Some(node) = current_node() else {
        // task #1324 (seventeenth review P2-1): on the real-Linux CI row
        // (NUMA_SHIM_REQUIRE_ORACLE=1) this None is a regression in the
        // crate's OWN detection chain, not a topology limitation.
        if require_oracle() {
            panic!(
                "current_node() returned None but NUMA_SHIM_REQUIRE_ORACLE=1 is set — \
                 this CI row exists specifically to prove NUMA detection works; a None \
                 here means the crate's own detection chain (sched_getcpu -> sysfs \
                 cpumap -> ReverseIndex::lookup) regressed, not that this host lacks \
                 NUMA hardware (task #1324, seventeenth review P2-1)"
            );
        }
        eprintln!(
            "skip: current_node() could not resolve a node on this host \
             (undetermined topology — task #1308's fail-closed detection); \
             the NUMA-preference path needs a genuinely resolved node, not \
             a node-0 guess (task #1311, F8.2)"
        );
        return;
    };
    let node =
        NodeId::new(node).expect("current_node()'s Some arm never yields the NO_NODE sentinel");

    let r = reserve_preferred_on_node(span, align, node)
        .expect("NUMA-preferred 4 MiB-aligned reservation failed");

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

    // Drop must release the WHOLE over-reservation, not just `span` bytes.
    // Capture values before drop for the release oracle.
    let base = r.as_ptr() as usize;
    let raw = r.reservation_ptr() as usize;
    let over = r.reservation_len();

    drop(r);

    // Windows oracle: query VirtualQuery to verify all four probes report MEM_FREE.
    #[cfg(windows)]
    {
        // Duplicate of the struct in `reserve_preferred_on_node_commits_only_the_requested_span_not_the_whole_over_reservation`.
        // This duplication is deliberate and historical: task #1311 added this second copy rather than touching that test's
        // struct while the 32-bit Windows policy from task #1313/F11 was still undecided. That policy is now DECIDED — 64-bit
        // Windows only (task #1313, fifteenth review F11) — and compile-enforced (task #1321, sixteenth review P2: a 32-bit
        // Windows build fails with `compile_error!`), so the freeze's original justification is gone and a future cleanup may
        // consolidate the two copies into one shared Windows-only test helper (eighteenth review F9). Task #1333 is docs-only
        // and leaves both copies structurally untouched; they still share one fate — any future layout change applies to both.
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

        const MEM_FREE: u32 = 0x0001_0000;

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
            // SAFETY: `addr` is a valid address (just freed, but still a valid
            // address in this process's address space); `&mut mbi` is a valid,
            // correctly-sized out-pointer. `VirtualQuery` never fails for an
            // address inside the calling process's own address space.
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

        // Probes: reservation start, usable span start, usable span last page, reservation last page.
        let probes = [raw, base, base + span - page, raw + over - page];
        for probe in probes {
            let state = query_state(probe as *const core::ffi::c_void);
            assert_eq!(
                state, MEM_FREE,
                "probe at {:#x} is not MEM_FREE (got {:#x}); \
                 MEM_RESERVE/MEM_COMMIT here means the release did not \
                 cover the whole reservation (task #1311, F8.1)",
                probe, state
            );
        }

        // Note: there is a residual raciness — cargo-test runs tests in parallel
        // threads, so a foreign mapping could theoretically land inside the just-
        // freed region between `drop(r)` and the probes. This is unlikely because
        // the four probes are megabytes apart while every other reservation in
        // this test binary is tiny, and the window is sub-millisecond.
    }

    // Linux oracle: read /proc/self/maps and assert NO mapping covers ANY probe.
    #[cfg(all(target_os = "linux", not(miri)))]
    {
        let maps =
            std::fs::read_to_string("/proc/self/maps").expect("failed to read /proc/self/maps");

        // Probes: reservation start, usable span start, usable span last page, reservation last page.
        let probes = [raw, base, base + span - page, raw + over - page];
        for probe in probes {
            for line in maps.lines() {
                let (s, e) = parse_maps_range(line.as_bytes())
                    .unwrap_or_else(|| panic!("malformed /proc/self/maps line: {line}"));
                // Fail if the probe is covered by any mapping.
                assert!(
                    !(s <= probe && probe < e),
                    "probe at {:#x} is covered by mapping {:#x}-{:#x} (task #1311, F8.1)",
                    probe,
                    s,
                    e
                );
            }
        }

        // Note: there is a residual raciness — cargo-test runs tests in parallel
        // threads, so a foreign mapping could theoretically land inside the just-
        // freed region between `drop(r)` and the read. This is unlikely because
        // the four probes are megabytes apart while every other reservation in
        // this test binary is tiny, and the window is sub-millisecond.
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
///
/// task #1325: real-backend-only expectation — under `numa_shim_mock` the
/// mock validates arguments (mirroring the real backends' error mapping)
/// with no platform check in front, so the `InvalidArguments` arm is taken
/// on EVERY target.
#[cfg(feature = "vmem-integration")]
#[test]
fn reserve_preferred_on_node_rejects_zero_size_with_invalid_arguments() {
    use aligned_vmem::page_size;
    use numa_shim::{reserve_preferred_on_node, NodeId, ReserveNumaError};

    let page = page_size();
    let err = reserve_preferred_on_node(0, page, NodeId::new(0).expect("literal 0, not NO_NODE"))
        .expect_err("zero size must be rejected");

    let expected: fn(&ReserveNumaError) -> bool = if cfg!(any(
        numa_shim_mock,
        all(any(target_os = "linux", windows), not(miri))
    )) {
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
    // 64 is not the sentinel, so construction succeeds; the rejection under
    // test happens at reserve_preferred_on_node's Linux nodemask check.
    let err = reserve_preferred_on_node(
        page,
        page,
        NodeId::new(64).expect("literal 64 is not the NO_NODE sentinel"),
    )
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
///
/// Self-skip discipline: positive NUMA policy tests run only when
/// `current_node()` genuinely returns `Some(_)`; otherwise they skip loudly
/// (task #1311, F8.2).
#[cfg(all(windows, feature = "vmem-integration"))]
#[test]
fn reserve_preferred_on_node_commits_only_the_requested_span_not_the_whole_over_reservation() {
    use numa_shim::{reserve_preferred_on_node, NodeId};

    // Mirrors the real `MEMORY_BASIC_INFORMATION` (winnt.h) on 64-bit
    // Windows -- the only supported Windows pointer width for this crate
    // since task #1313 (fifteenth review F11) made 64-bit-only an explicit
    // policy decision (compile-time enforced since task #1321, sixteenth
    // review P2); before that it was an unstated assumption. This
    // repo's own Windows platform code elsewhere assumes a 64-bit pointer
    // width too. `PartitionId` is conditionally present in the C header
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

    let Some(node) = current_node() else {
        eprintln!(
            "skip: current_node() could not resolve a node on this host \
             (undetermined topology — task #1308's fail-closed detection); \
             the NUMA-preference path needs a genuinely resolved node, not \
             a node-0 guess (task #1311, F8.2)"
        );
        return;
    };
    let node =
        NodeId::new(node).expect("current_node()'s Some arm never yields the NO_NODE sentinel");

    let r =
        reserve_preferred_on_node(size, align, node).expect("NUMA-preferred reservation failed");

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
