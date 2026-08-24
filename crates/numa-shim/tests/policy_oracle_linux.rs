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
// Require-oracle gate (task #1324, P2-1 of the seventeenth independent
// review, docs/reviews/2026-08-24-223343-numa-shim-publication-audit-oh.md).
// ---------------------------------------------------------------------------

/// Returns `true` if `NUMA_SHIM_REQUIRE_ORACLE=1` is set in the environment.
///
/// The inverse of the root crate's `SEFER_NUMA_TEST=1` gate
/// (`tests/numa_alloc.rs`): that env var OPTS IN to running the expensive
/// real-multi-NUMA-hardware tests; this one declares that this process runs
/// on a host that is SUPPOSED to have working NUMA detection — the
/// `numa-shim-mock` CI job's real-Linux
/// `cargo test -p numa-shim --features vmem-integration` row is the only
/// place that sets it. Under it, the `current_node() == None` skip arms in
/// this file PANIC instead of skipping: `current_node()` is this crate's
/// own detection chain (`sched_getcpu` → sysfs cpumap → `ReverseIndex`
/// lookup), so a None there means detection regressed, not that this host
/// lacks NUMA hardware. Unset (local/dev runs and every other CI row):
/// today's tolerant skip, unchanged.
fn require_oracle() -> bool {
    std::env::var("NUMA_SHIM_REQUIRE_ORACLE").as_deref() == Ok("1")
}

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

/// `MPOL_F_MEMS_ALLOWED`: flag for `get_mempolicy(2)` to return the set of
/// nodes the calling thread may name in a subsequent `mbind(2)`/`set_mempolicy(2)`
/// call (the thread's `cpuset_current_mems_allowed` context).
///
/// Verified against `include/uapi/linux/mempolicy.h` — the same header as
/// `MPOL_F_ADDR` above: `MPOL_F_MEMS_ALLOWED (1<<2)` (line 48; `MPOL_F_NODE`
/// is `(1<<0)`, `MPOL_F_ADDR` is `(1<<1)`).
const MPOL_F_MEMS_ALLOWED: u64 = 1 << 2;

/// Words in the oracle's nodemask arrays (task #1329, F6): 16 `u64` words =
/// 1024 bits, covering the maximum `nr_node_ids` buildable on the two arches
/// this file gates to — `CONFIG_NODES_SHIFT` is `range 1 10` in BOTH
/// arch/x86/Kconfig and arch/arm64/Kconfig, so `nr_node_ids <= 1024` on every
/// kernel this test can execute on.
const NODEMASK_WORDS: usize = 16;

/// `maxnode` for every `get_mempolicy(2)` probe in this file: 16 * 64 = 1024.
///
/// Two constraints, both verified against mm/mempolicy.c:
/// - `kernel_get_mempolicy` rejects the whole call with `EINVAL` when
///   `maxnode < nr_node_ids` BEFORE any copy-out, so the old single-`u64`
///   probe (`maxnode = 65`) spuriously failed on any host with more than 64
///   possible nodes even when the target node is safely within 0..=63 (F6).
/// - `copy_nodes_to_user` copies `ALIGN(maxnode - 1, 64) / 8` bytes:
///   `ALIGN(1023, 64) / 8 = 128` = exactly `sizeof([u64; 16])` — a 1024-node
///   kernel fills the array exactly; a smaller kernel clamps the copy to its
///   own `BITS_TO_LONGS(nr_node_ids) * 8` (and zero-fills the tail), so the
///   copy can never exceed the array.
const MAXNODE: u64 = NODEMASK_WORDS as u64 * 64;

// ---------------------------------------------------------------------------
// Errno values for the implementation-vs-environment classification
// (task #1318, P1 of the sixteenth independent review,
// `docs/reviews/2026-08-24-204022-numa-shim-publication-audit-Sol-codex.md`;
// refined by task #1329, F1.1+F1.2 of the eighteenth independent review,
// `docs/reviews/2026-08-24-224323-numa-shim-publication-audit-Sol-codex.md`).
//
// Local constants because this crate deliberately avoids the `libc` crate —
// the same precedent as the protocol constants above. Values verified
// against the kernel's own `include/uapi/asm-generic/errno-base.h`
// (EFAULT, EINVAL, ENOMEM, EPERM) and `include/uapi/asm-generic/errno.h`
// (ENOSYS); identical on the x86_64/aarch64 Linux targets this file is
// gated to.
//
// Classification model (task #1329 refinement of #1318): known-safe skips
// explicitly enumerated (allowlist), everything else fails loud. The
// eighteenth review chose fail-closed PANIC for implementation errors and
// for unexpected errnos rather than hiding them behind green skips.
// ---------------------------------------------------------------------------

/// `EFAULT` (14): bad address. Every pointer argument of the crate's
/// `mbind(2)` call comes from our own marshalling, so EFAULT can only mean
/// OUR wrapper handed the kernel a bad address — an implementation bug,
/// never an environment limitation.
const ERRNO_EFAULT: i32 = 14;

/// `EINVAL` (22): invalid argument. Since the task #1329 MPOL_F_MEMS_ALLOWED
/// preflight runs BEFORE the reserve call and rules out the documented
/// environment-EINVAL (nodemask naming no online / cpuset-allowed /
/// memory-bearing node), EINVAL reaching the mbind error arm means bad
/// addr/len/maxnode marshalling in the crate's own wrapper — an
/// implementation bug. Residual: a cpuset reconfiguration racing between
/// preflight and mbind (TOCTOU) would also land here; that is accepted
/// fail-closed per the eighteenth review.
const ERRNO_EINVAL: i32 = 22;

/// `ENOSYS` (38): syscall not implemented. The crate bakes `SYS_MBIND` in
/// as a per-arch constant verified against the kernel's syscall tables, so
/// on the x86_64/aarch64 Linux targets this file gates to, ENOSYS is either
/// a seccomp/sandbox policy denying the syscall (a legitimate environment)
/// or a wrong syscall number (an implementation bug); the eighteenth review
/// chose fail-closed PANIC over a separate sandbox-probe, so this arm is
/// deliberately loud.
const ERRNO_ENOSYS: i32 = 38;

/// `ENOMEM` (12): the ONLY errno mbind(2)'s ERRORS section documents for
/// exactly the call form the crate issues (`flags = 0`, no MF_MOVE family):
/// "Insufficient kernel memory was available." Genuine resource exhaustion —
/// an environment condition, not an implementation bug. (Value verified
/// against include/uapi/asm-generic/errno-base.h, same source as the
/// constants above.)
const ERRNO_ENOMEM: i32 = 12;

/// `EPERM` (1): NOT documented by mbind(2) for the crate's `flags = 0` form
/// (the man page ties EPERM to MPOL_MF_MOVE_ALL without CAP_SYS_NICE, which
/// the crate never passes) — kept on the skip allowlist anyway because
/// seccomp-based sandboxes (e.g. docker's default profile) deny mbind with
/// exactly EPERM: the F8.2 container case this skip arm has always existed
/// for. (Value verified against include/uapi/asm-generic/errno-base.h.)
const ERRNO_EPERM: i32 = 1;

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
/// Queries the NUMA policy for the address `addr` using the `MPOL_F_ADDR` flag.
/// Returns the policy mode and the 1024-bit nodemask on success.
///
/// # Safety
///
/// `addr` must point inside a live mapping owned by this process.
///
/// # Implementation notes
///
/// - `maxnode` quirk (output direction): the kernel's node-mask copy-out copies
///   `ALIGN(maxnode - 1, 64) / 8` bytes. With a 128-byte `[u64; 16]` nodemask we
///   MUST pass `maxnode = 1024` (copies exactly 128 bytes = nodes 0..=1023). This
///   is the output-direction twin of the `maxnode` quirk the crate's own
///   `mbind_preferred_linux` documents for task #697 (rust-intel audit §F1) and
///   compensates identically. A LARGER maxnode would make the kernel write MORE
///   than 128 bytes into the stack local (overflow); a smaller one silently drops
///   high node bits.
///
/// - Uses raw `syscall(2)` instead of `extern "C" { fn get_mempolicy(...) }` because
///   glibc/musl do NOT wrap `get_mempolicy` (man7 page lists it under libnuma,
///   `-lnuma`) — same reasoning as `libc_mbind` in `src/lib.rs`.
///
/// - Per-arch `SYS_GET_MEMPOLICY` constants follow the precedent of `SYS_MBIND`
///   in `src/lib.rs`, sourced from the same kernel syscall tables.
unsafe fn get_mempolicy_addr(
    addr: *mut core::ffi::c_void,
) -> std::io::Result<(i32, [u64; NODEMASK_WORDS])> {
    let mut mode: core::ffi::c_int = 0;
    let mut nodemask = [0u64; NODEMASK_WORDS];
    let maxnode: u64 = MAXNODE;
    let flags: u64 = MPOL_F_ADDR;

    // SAFETY:
    // - `&mut mode` is a valid out-pointer for `*mut c_int` per the man-page signature.
    // - `nodemask.as_mut_ptr()` is a valid out-pointer for `*mut c_ulong`; it is 128 bytes,
    //   matching the copy-out length implied by `maxnode = 1024` (see MAXNODE's doc).
    // - `maxnode = 1024` matches the 128-byte `nodemask` local exactly (the ALIGN quirk
    //   documented above — copies exactly 128 bytes, nodes 0..=1023).
    // - `addr` points inside a live mapping owned by this process (caller's
    //   safety contract).
    // - `SYS_GET_MEMPOLICY` is the correct syscall number for this architecture
    //   (verified against kernel tables).
    // - Return value is checked; errno is captured immediately on -1
    //   (same discipline as `mbind_preferred_linux`).
    let rc = syscall(
        SYS_GET_MEMPOLICY,
        &mut mode as *mut i32,
        nodemask.as_mut_ptr(),
        maxnode as usize,
        addr,
        flags as usize,
    );

    if rc == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok((mode, nodemask))
    }
}

/// Preflight: the set of nodes this thread may actually name in a subsequent
/// `mbind(2)` — `get_mempolicy(2)` with `MPOL_F_MEMS_ALLOWED` (task #1329,
/// F1.1 of the eighteenth review).
///
/// Calling convention (DIFFERENT from the `MPOL_F_ADDR` probe above, which is
/// address-based and requires a live mapping): under this flag the man page
/// specifies that the `mode` argument is IGNORED and the allowed set is
/// returned in `nodemask`; `addr` must be passed as NULL whenever flags does
/// not specify `MPOL_F_ADDR`, and the kernel's own `do_get_mempolicy`
/// (mm/mempolicy.c) rejects combining this flag with `MPOL_F_NODE` or
/// `MPOL_F_ADDR`. The kernel returns `cpuset_current_mems_allowed`, which
/// `guarantee_online_mems` (kernel/cgroup/cpuset.c) maintains intersected
/// with `node_states[N_MEMORY]` — so every SET bit in the returned mask is
/// BOTH cpuset-allowed AND memory-bearing, which is exactly the precondition
/// `mbind(2)` demands (its ERRORS section documents EINVAL for a nodemask
/// naming no node that is online, allowed by the thread's cpuset, and
/// memory-bearing).
///
/// # Safety
///
/// No caller obligations beyond the syscall's own contract: both output
/// pointers are kernel-write-only locals of matching size, and neither input
/// pointer is dereferenced by the kernel on this path.
unsafe fn get_mems_allowed() -> std::io::Result<[u64; NODEMASK_WORDS]> {
    let mut nodemask = [0u64; NODEMASK_WORDS];
    // SAFETY:
    // - `null_mut::<i32>()` for `mode`: ignored under MPOL_F_MEMS_ALLOWED per
    //   the man page; the kernel guards its `put_user` with `if (policy && ...)`
    //   (kernel_get_mempolicy), so NULL is safe.
    // - `nodemask.as_mut_ptr()` is a valid 128-byte out-buffer; the copy-out
    //   length implied by MAXNODE is exactly 128 bytes (see MAXNODE's doc).
    // - `null_mut::<c_void>()` for `addr`: required when flags does not
    //   specify MPOL_F_ADDR (get_mempolicy(2) ERRORS).
    // - flags is MPOL_F_MEMS_ALLOWED alone — no F_ADDR/F_NODE combination.
    // - SYS_GET_MEMPOLICY is the per-arch-verified number (see its constant).
    // - Return value checked; errno captured immediately on -1, same
    //   discipline as `mbind_preferred_linux`.
    let rc = syscall(
        SYS_GET_MEMPOLICY,
        std::ptr::null_mut::<i32>(),
        nodemask.as_mut_ptr(),
        MAXNODE as usize,
        std::ptr::null_mut::<core::ffi::c_void>(),
        MPOL_F_MEMS_ALLOWED as usize,
    );
    if rc == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(nodemask)
    }
}

/// Bit-test across the multi-word nodemask: is `node` set in `mask`?
fn node_bit_set(mask: &[u64; NODEMASK_WORDS], node: u32) -> bool {
    let node = node as usize;
    node < NODEMASK_WORDS * 64 && (mask[node / 64] >> (node % 64)) & 1 == 1
}

/// The single-node nodemask `mbind_preferred_linux` installs for `node`.
fn single_node_mask(node: u32) -> [u64; NODEMASK_WORDS] {
    let mut mask = [0u64; NODEMASK_WORDS];
    mask[(node / 64) as usize] = 1u64 << (node % 64);
    mask
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

    // task #1329 (F1.1): preflight — may this thread actually name `node` in
    // an mbind? `current_node()` proves only the CPU→node sysfs mapping; it
    // does NOT prove the node is in this thread's allowed-memory mask (a
    // CPU-only/memoryless node, or a cpuset splitting CPU and memory node
    // masks, is a real topology shape), and mbind(2) documents EINVAL for
    // exactly that case — which task #1318's classification wrongly treated
    // as impossible. Probe first; skip loudly if disallowed.
    let allowed = unsafe {
        get_mems_allowed().expect(
            "get_mempolicy(MPOL_F_MEMS_ALLOWED) preflight must succeed; if this \
             environment sandbox-denies the NUMA policy syscalls (seccomp \
             EPERM/ENOSYS), the policy oracle cannot run here at all — \
             task #1329 (eighteenth review F1.1)",
        )
    };
    if !node_bit_set(&allowed, node) {
        // Legitimate environment restriction, NOT a regression: the node
        // current_node() resolved is outside this thread's allowed-memory
        // mask, so the positive mbind would be a documented-environment
        // EINVAL. Skipping BEFORE the reserve call keeps the #1318 errno
        // classification from false-reding a correctly-behaving environment.
        eprintln!(
            "skip: node {node} (resolved by current_node()) is NOT in this \
             thread's allowed-memory nodemask (MPOL_F_MEMS_ALLOWED = {allowed:?}); \
             the positive mbind would be a legitimate EINVAL on this environment — \
             task #1329 (eighteenth review F1.1)"
        );
        return;
    }

    let r = match reserve_preferred_on_node(size, align, node_id) {
        Ok(r) => r,
        Err(numa_shim::ReserveNumaError::Os(e)) => {
            // task #1329 (F1.1+F1.2) refines task #1318's classification: the
            // MPOL_F_MEMS_ALLOWED preflight above now rules out the
            // legitimate-EINVAL case before this arm is reached, and the
            // catch-all inverted from skip to panic. Only explicitly allowed
            // environment errnos (allowlist) skip; everything else panics,
            // fail-closed.
            match e.raw_os_error() {
                // -- ALLOWLIST: the only errno classified as environment
                //    (task #1329, F1.2). Everything not here panics below.
                Some(ERRNO_ENOMEM) => {
                    // Documented for exactly this call form (flags=0):
                    // kernel memory exhausted. Genuine environment refusal.
                    eprintln!(
                        "skip: reserve_preferred_on_node refused with ENOMEM \
                         (errno {ERRNO_ENOMEM}, documented kernel-memory exhaustion): {e}"
                    );
                    return;
                }
                Some(ERRNO_EPERM) => {
                    // Not documented for flags=0, but seccomp sandboxes
                    // (docker default profile) deny mbind with EPERM — the
                    // F8.2 container case. See ERRNO_EPERM's doc comment.
                    eprintln!(
                        "skip: reserve_preferred_on_node refused with EPERM \
                         (errno {ERRNO_EPERM}, seccomp/container denial of mbind): {e}"
                    );
                    return;
                }
                // -- Fail-closed below (task #1329, F1.1+F1.2).
                Some(ERRNO_EINVAL) => panic!(
                    "reserve_preferred_on_node failed with EINVAL (errno {ERRNO_EINVAL}) \
                     AFTER the MPOL_F_MEMS_ALLOWED preflight confirmed node {node} is in \
                     this thread's allowed-memory mask: the documented environment-EINVAL \
                     (node offline / outside cpuset / memoryless) is ruled out, so this is \
                     an implementation bug in the mbind(2) marshalling (bad addr/len/maxnode) \
                     — task #1329 (eighteenth review F1.1); residual accepted fail-closed: \
                     a cpuset reconfiguration racing between preflight and mbind"
                ),
                Some(ERRNO_EFAULT) => panic!(
                    "reserve_preferred_on_node failed with EFAULT (errno {ERRNO_EFAULT}): \
                     implementation bug — every pointer argument comes from the crate's own \
                     marshalling, never an environment limitation — task #1329 (eighteenth \
                     review F1.1)"
                ),
                Some(ERRNO_ENOSYS) => panic!(
                    "reserve_preferred_on_node failed with ENOSYS (errno {ERRNO_ENOSYS}): \
                     SYS_MBIND is verified against the kernel's syscall tables, so this is \
                     either an implementation bug (wrong number for this arch) or a \
                     seccomp/sandbox policy denying the syscall — deliberately fail-closed, \
                     not auto-skipped — task #1329 (eighteenth review F1.1); if this host \
                     sandboxes mbind, run the oracle on an unrestricted host"
                ),
                Some(errno) => panic!(
                    "reserve_preferred_on_node failed with UNCLASSIFIED errno {errno}: {e} \
                     — not on the task #1329 environment allowlist (ENOMEM/EPERM only), so \
                     it fails loud instead of hiding behind a green skip (eighteenth review \
                     F1.2: an implementation regression surfacing an unexpected errno must \
                     not be able to pass this oracle)"
                ),
                None => panic!(
                    "reserve_preferred_on_node failed with an Os error carrying NO raw \
                     errno ({e}) — itself suspicious, and unclassifiable for the task \
                     #1318/#1329 implementation-vs-environment split; failing loud"
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
    let probe = unsafe { r.as_ptr().add(page) }.cast::<core::ffi::c_void>();

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
        single_node_mask(node),
        "expected nodemask with only node {node} set (task #1311 F3 oracle), got {nodemask:?}"
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
        // task #1324 (seventeenth review P2-1): same fatal-under-CI treatment
        // as the positive oracle above — a None on the real-Linux CI row
        // (NUMA_SHIM_REQUIRE_ORACLE=1) means detection regressed.
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
    let probe = unsafe { r.as_ptr().add(page) }.cast::<core::ffi::c_void>();

    let (mode, nodemask) = unsafe {
        get_mempolicy_addr(probe)
            .expect("get_mempolicy(MPOL_F_ADDR) on a live reservation must succeed")
    };

    // Assert that the plain mapping is NOT reported as "MPOL_PREFERRED, exactly our node."
    // If it were, the positive oracle would not distinguish bound from unbound.
    assert!(
        !(mode == MPOL_PREFERRED && nodemask == single_node_mask(node)),
        "plain unbound reservation unexpectedly reports MPOL_PREFERRED for node {node} \
         (mode={mode}, nodemask={nodemask:?}); this would make the positive oracle vacuous"
    );

    drop(r);
}
