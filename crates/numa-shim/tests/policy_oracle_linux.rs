//! Linux-only oracle: verify `reserve_preferred_on_node` actually installed the VMA policy.
//!
//! task #1311 (F3): the smoke tests treat the `Ok` return of `reserve_preferred_on_node` as
//! proof the NUMA policy was installed — "checking the implementation by its own answer."
//! A regression that returns the reservation while silently skipping the `mbind(2)` would
//! stay green. This file provides an independent oracle that queries the KERNEL's own
//! record via `get_mempolicy(2)` with `MPOL_F_ADDR` for an address inside the usable span,
//! and asserts mode/nodemask match what `mbind_preferred_linux` installed (MPOL_PREFERRED,
//! single-node mask).
//!
//! ## Why raw syscall, not `extern "C" { fn get_mempolicy(...) }`?
//!
//! glibc and musl do NOT wrap `get_mempolicy` (man7 page lists it under libnuma, `-lnuma`) —
//! so a plain `extern "C" { fn get_mempolicy(...) }` would fail to LINK. Use raw
//! `syscall(SYS_get_mempolicy, ...)` — the exact same reasoning and mechanism as the
//! crate's own `mbind_preferred_linux`/`libc_mbind` in `src/lib.rs` (see the syscall
//! declaration and libc_mbind there for the precedent).
//!
//! ## Negative control (oracle is not vacuous)
//!
//! The second test in this file, `plain_unbound_reservation_is_not_reported_as_preferred_for_our_node`,
//! creates a reservation WITHOUT calling `reserve_preferred_on_node` and queries the kernel's
//! policy. If a plain never-NUMA'd mapping read back as "MPOL_PREFERRED, exactly our node,"
//! the positive oracle above could not distinguish bound from unbound — the exact F3 regression
//! it exists to catch.
//!
//! ## Verification status
//!
//! This dev host is Windows — the file is CROSS-COMPILE-CHECKED only
//! (`cargo clippy -p numa-shim --target x86_64-unknown-linux-gnu --all-features --all-targets -- -D warnings`)
//! and will first EXECUTE in CI (the `numa-shim-mock` job's `cargo test -p numa-shim --features vmem-integration`
//! row on ubuntu-latest, the same row that already executes smoke.rs against the real mbind). This matches
//! the task #1308/#1309 precedent for Linux-only tests.
//!
//! ## Scope cut
//!
//! The review's optional "alignment slack / reservation boundary" half is NOT probed
//! (nice-to-have; the core usable-span check is the improvement over the status quo of nothing).
//!
//! ## Compilation gating
//!
//! This file is compiled out under `numa_shim_mock` (the mock bypasses the real mbind,
//! so the kernel oracle is meaningless there) and under miri (no real kernel).

#![cfg(all(
    target_os = "linux",
    not(miri),
    not(numa_shim_mock),
    feature = "vmem-integration",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use numa_shim::{current_node, reserve_preferred_on_node, NodeId};

// ---------------------------------------------------------------------------
// Syscall numbers: per-arch constants matching the crate's own SYS_MBIND.
// ---------------------------------------------------------------------------

/// Syscall number for `get_mempolicy(2)` on x86_64.
///
/// Verified against kernel's own table: arch/x86/entry/syscalls/syscall_64.tbl —
/// the same table where mbind=237, matching the crate's own x86_64 `SYS_MBIND`.
#[cfg(target_arch = "x86_64")]
const SYS_GET_MEMPOLICY: i64 = 239;

/// Syscall number for `get_mempolicy(2)` on aarch64.
///
/// Verified against kernel's own table: include/uapi/asm-generic/unistd.h —
/// the same table where mbind=235, matching the crate's aarch64 `SYS_MBIND`.
#[cfg(target_arch = "aarch64")]
const SYS_GET_MEMPOLICY: i64 = 236;

/// `MPOL_PREFERRED`: soft preferred-node policy; kernel falls back on pressure.
///
/// Mirrors the crate-private constant of the same name in `src/lib.rs` — not
/// importable from an integration test.
///
/// Verified against `include/uapi/linux/mempolicy.h`: `MPOL_PREFERRED = 1`
/// (second member of `enum mempolicy_mode`).
const MPOL_PREFERRED: i32 = 1;

/// `MPOL_F_ADDR`: flag for `get_mempolicy(2)` to query the policy for a specific address.
///
/// Verified against `include/uapi/linux/mempolicy.h`: `MPOL_F_ADDR = (1<<1)`.
const MPOL_F_ADDR: u64 = 1 << 1;

// ---------------------------------------------------------------------------
// Errno values for the implementation-vs-environment classification
// (task #1318, P1 of the sixteenth independent review,
// `docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`).
//
// Local constants because this crate deliberately avoids the `libc` crate —
// the same precedent as the protocol constants above. Values verified
// against the kernel's own `include/uapi/asm-generic/errno-base.h`
// (EFAULT, EINVAL) and `include/uapi/asm-generic/errno.h` (ENOSYS);
// identical on the x86_64/aarch64 Linux targets this file is gated to.
// ---------------------------------------------------------------------------

/// `EFAULT` (14): bad address. Every pointer argument of the crate's
/// `mbind(2)` call comes from our own marshalling, so EFAULT can only mean
/// OUR wrapper handed the kernel a bad address — an implementation bug,
/// never an environment limitation.
const ERRNO_EFAULT: i32 = 14;

/// `EINVAL` (22): invalid argument. For `mbind(2)` this is a
/// non-page-aligned `addr`, a zero `len`, or a wrong `maxnode` — all three
/// are controlled by the crate's own wrapper (`mbind_preferred_linux`), so
/// EINVAL here is an implementation bug in the syscall marshalling, never
/// an environment limitation.
const ERRNO_EINVAL: i32 = 22;

/// `ENOSYS` (38): syscall not implemented. The crate bakes `SYS_MBIND` in
/// as a per-arch constant verified against the kernel's syscall tables, so
/// ENOSYS means the number is wrong for the running arch (e.g. a new arch
/// gate was added without adding its constant) — an implementation bug,
/// never an environment limitation.
const ERRNO_ENOSYS: i32 = 38;

// ---------------------------------------------------------------------------
// Raw syscall wrapper.
// ---------------------------------------------------------------------------

// `syscall(2)` from glibc/musl — same declaration the crate's own
// `libc_mbind` calls through; variadic, five args after the number.
extern "C" {
    fn syscall(number: i64, ...) -> i64;
}

/// Wrapper for `get_mempolicy(2)` via raw `syscall(2)`.
///
/// Queries the NUMA policy for a specific address `addr` using the `MPOL_F_ADDR` flag.
/// Returns the policy mode and the 64-bit nodemask on success.
///
/// # Safety
///
/// `addr` must be an address inside a live mapping owned by this process.
///
/// # Implementation notes
///
/// - `maxnode` quirk (output direction): the kernel's node-mask copy-out copies
///   `ALIGN(maxnode - 1, 64) / 8` bytes, so with an 8-byte `u64` nodemask we
///   MUST pass `maxnode = 65` (copies exactly 8 bytes = nodes 0..=63). This is
///   the output-direction twin of the `maxnode` quirk the crate's own
///   `mbind_preferred_linux` documents for task #697 (rust-intel audit §F1) and
///   compensates identically. A LARGER maxnode would make the kernel write MORE
///   than 8 bytes into the stack local (overflow); a smaller one silently drops
///   high node bits.
///
/// - Uses raw `syscall(2)` instead of `extern "C" { fn get_mempolicy(...) }` because
///   glibc/musl do NOT wrap `get_mempolicy` (man7 page lists it under libnuma,
///   `-lnuma`) — same reasoning as `libc_mbind` in `src/lib.rs`.
///
/// - Per-arch `SYS_GET_MEMPOLICY` constants follow the precedent of `SYS_MBIND`
///   in `src/lib.rs`, sourced from the same kernel syscall tables.
unsafe fn get_mempolicy_addr(addr: usize) -> std::io::Result<(i32, u64)> {
    let mut mode: core::ffi::c_int = 0;
    let mut nodemask: u64 = 0;
    // task #697 (rust-intel audit §F1): `maxnode` quirk — must be 65 for a 64-bit
    // mask to cover bits 0..=63. See the safety doc above for the full explanation.
    let maxnode: u64 = 65;
    let flags: u64 = MPOL_F_ADDR;

    // SAFETY:
    // - `&mut mode` is a valid out-pointer for `*mut c_int` per the man-page signature.
    // - `&mut nodemask` is a valid out-pointer for `*mut c_ulong`; it is 8 bytes,
    //   matching the copy-out length implied by `maxnode = 65`.
    // - `maxnode = 65` matches the 8-byte `nodemask` local exactly (the ALIGN quirk
    //   documented above — copies exactly 8 bytes, nodes 0..=63).
    // - `addr` is a valid address inside a live mapping owned by this process
    //   (caller's safety contract).
    // - `SYS_GET_MEMPOLICY` is the correct syscall number for this architecture
    //   (verified against kernel tables).
    // - Return value is checked; errno is captured immediately on -1
    //   (same discipline as `mbind_preferred_linux`).
    let rc = syscall(
        SYS_GET_MEMPOLICY,
        &mut mode as *mut i32,
        &mut nodemask as *mut u64,
        maxnode as usize,
        addr as *mut core::ffi::c_void,
        flags as usize,
    );

    if rc == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok((mode, nodemask))
    }
}

// ---------------------------------------------------------------------------
// Test 1: the positive oracle.
// ---------------------------------------------------------------------------

/// Verify that a successful `reserve_preferred_on_node` actually installed
/// the `MPOL_PREFERRED` policy on the usable span by querying the kernel's
/// own record via `get_mempolicy(2)`.
///
/// This is the F3 oracle — a regression that skips the mbind leaves the VMA at
/// the default policy and FAILS the mode assert.
#[test]
fn reserve_preferred_on_node_installs_mpol_preferred_on_the_usable_span() {
    // Skip discipline (same as the smoke tests' new convention, task #1311 F8.2):
    // if current_node() cannot resolve a node, the oracle cannot run, but that is
    // not a test failure — it's a topology limitation on this host.
    let Some(node) = current_node() else {
        eprintln!(
            "skip: current_node() could not resolve a node on this host (undetermined \
             topology — task #1308's fail-closed detection); the policy oracle needs a \
             genuinely resolved node"
        );
        return;
    };

    let page = aligned_vmem::page_size();
    let size = page * 4;
    let align = page;

    let node_id = NodeId::new(node)
        .expect("current_node()'s Some arm never yields the NO_NODE sentinel (task #1308)");

    let r = match reserve_preferred_on_node(size, align, node_id) {
        Ok(r) => r,
        Err(numa_shim::ReserveNumaError::Os(e)) => {
            // task #1318 (P1 of the sixteenth independent review): `Os` carries
            // BOTH legitimate environment refusals (a cgroup-restricted node)
            // AND implementation errors. The old unconditional skip here let a
            // regression that makes the crate's own `mbind` wrapper ALWAYS fail
            // leave this oracle green via skip — exactly the vacuous pass the
            // oracle (task #1311/F3) exists to prevent. So classify the errno
            // first: implementation errnos fail the test loudly; only
            // environment/capability refusals keep the F8.2 container-case skip
            // (now with the errno number printed, so a skipping CI log is
            // diagnosable).
            match e.raw_os_error() {
                Some(ERRNO_EINVAL) => panic!(
                    "reserve_preferred_on_node failed with EINVAL (errno {ERRNO_EINVAL}): \
                     implementation bug in its mbind(2) syscall marshalling (bad \
                     addr/len/maxnode), NOT an environment limitation — task #1318 \
                     (sixteenth review P1)"
                ),
                Some(ERRNO_EFAULT) => panic!(
                    "reserve_preferred_on_node failed with EFAULT (errno {ERRNO_EFAULT}): \
                     implementation bug in its mbind(2) syscall marshalling (bad pointer \
                     argument), NOT an environment limitation — task #1318 (sixteenth \
                     review P1)"
                ),
                Some(ERRNO_ENOSYS) => panic!(
                    "reserve_preferred_on_node failed with ENOSYS (errno {ERRNO_ENOSYS}): \
                     implementation bug — the SYS_MBIND number is wrong for this arch, \
                     NOT an environment limitation — task #1318 (sixteenth review P1)"
                ),
                Some(errno) => {
                    // Environment/capability refusal (EPERM, ENOMEM, a
                    // cgroup-restricted node, ...): skip rather than panic — the
                    // OS may legitimately refuse in constrained environments even
                    // though the implementation is correct. Loud skip with the
                    // errno number, per the smoke tests' F8.2 convention.
                    eprintln!(
                        "skip: reserve_preferred_on_node refused by the OS with errno \
                         {errno} (possibly a cgroup-restricted node — F8.2's container \
                         case): {e}"
                    );
                    return;
                }
                None => panic!(
                    "reserve_preferred_on_node failed with an Os error carrying NO raw \
                     errno ({e}) — itself suspicious, and unclassifiable for the task \
                     #1318 implementation-vs-environment split; failing loud"
                ),
            }
        }
        Err(other) => {
            // Any OTHER error variant is unexpected and should panic.
            panic!("unexpected reservation error: {other:?}");
        }
    };

    // Probe one page inside the usable span.
    // SAFETY: `r` owns `size` bytes at `r.as_ptr()`, where `size >= 2*page`
    // (we requested `page * 4`). Adding `page` stays within the reservation's
    // owned span. Any address in [base, base+len) shares the VMA policy that
    // mbind installed on the complete reservation span — mbind applies to the
    // VMA, not to individual pages, and the VMA covers the whole reservation.
    let probe = unsafe { r.as_ptr().add(page) } as usize;

    let (mode, nodemask) = unsafe {
        get_mempolicy_addr(probe)
            .expect("get_mempolicy(MPOL_F_ADDR) on a live reservation must succeed")
    };

    // Assert the mode is MPOL_PREFERRED and the nodemask matches our node.
    assert_eq!(
        mode, MPOL_PREFERRED,
        "expected mode {MPOL_PREFERRED} (MPOL_PREFERRED), got {mode}"
    );
    assert_eq!(
        nodemask,
        1u64 << node,
        "expected nodemask {:#x} (node {node}), got {:#x}",
        1u64 << node,
        nodemask
    );

    // Drop releases the reservation back to the OS.
    drop(r);
}

// ---------------------------------------------------------------------------
// Test 2: the negative control (oracle is not vacuous).
// ---------------------------------------------------------------------------

/// Negative control: verify that a plain unbound reservation is NOT reported
/// as `MPOL_PREFERRED` for our node.
///
/// If a plain never-NUMA'd mapping read back as "MPOL_PREFERRED, exactly our node,"
/// the positive oracle above could not distinguish bound from unbound — the exact
/// F3 regression it exists to catch.
///
/// We do NOT assert a specific mode for the plain mapping — kernel details of
/// what `MPOL_DEFAULT` reports in the mask are intentionally not relied on.
#[test]
fn plain_unbound_reservation_is_not_reported_as_preferred_for_our_node() {
    // Skip discipline (same as the positive test above).
    let Some(node) = current_node() else {
        eprintln!(
            "skip: current_node() could not resolve a node on this host (undetermined \
             topology — task #1308's fail-closed detection)"
        );
        return;
    };

    let page = aligned_vmem::page_size();
    let size = page * 4;
    let align = page;

    // Create a plain reservation WITHOUT calling `reserve_preferred_on_node`.
    //
    // task #1318 note: this negative control has NO `ReserveNumaError::Os`
    // skip arm to tighten — it never touches the NUMA policy machinery
    // (plain `aligned_vmem` reservation, no mbind), so there is no
    // environment-vs-implementation errno classification to apply. Its
    // failure mode is already loud (`.expect` below), which is the correct
    // posture here: a negative control only proves anything if it RUNS.
    let r =
        aligned_vmem::try_reserve_aligned(size, align).expect("plain reservation should succeed");

    // Probe one page inside the reservation (same SAFETY pattern as above).
    // SAFETY: `r` owns `size` bytes at `r.as_ptr()`; adding `page` stays within.
    let probe = unsafe { r.as_ptr().add(page) } as usize;

    let (mode, nodemask) = unsafe {
        get_mempolicy_addr(probe)
            .expect("get_mempolicy(MPOL_F_ADDR) on a live reservation must succeed")
    };

    // Assert that the plain mapping is NOT reported as "MPOL_PREFERRED, exactly our node."
    // If it were, the positive oracle would not distinguish bound from unbound.
    assert!(
        !(mode == MPOL_PREFERRED && nodemask == 1u64 << node),
        "plain unbound reservation unexpectedly reports MPOL_PREFERRED for node {node} \
         (mode={mode}, nodemask={:#x}); this would make the positive oracle vacuous",
        nodemask
    );

    drop(r);
}
