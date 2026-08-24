//! NUMA OS-seam — thin wrapper over the `numa-shim` crate (`crates/numa-shim`).
//!
//! Preserves the in-tree call sites' API for backward-compat inside the
//! `sefer-alloc` crate; the actual unsafe OS FFI (mbind, VirtualAllocExNuma,
//! sysfs cpumap reads) lives entirely in `numa-shim`. This file contains NO
//! platform-specific unsafe code — and, since task #1306 removed the
//! test-only `bind_segment` seam (it forwarded to numa-shim's now-deleted
//! `bind_range`), no `unsafe` at all: this module is no longer one of the
//! crate's confined-`unsafe` seams.
//!
//! ## Gating
//!
//! Compiled only when `feature = "numa-aware"` is active (which implies
//! `dep:numa-shim` is enabled). Each function delegates straight to the shim.
//!
//! ## Backward compatibility
//!
//! The public items — `NO_NODE`, `current_node`, `reserve_aligned_on_node` —
//! keep the sentinel-`u32` signatures the in-tree call sites
//! (`alloc_core_small.rs` / `alloc_core_large.rs`) already use; all callers
//! compile without modification.

use core::ptr::NonNull;

/// Sentinel value: "no NUMA node / feature disabled / unsupported platform".
/// Re-exported from `numa_shim` to keep both values identical.
pub const NO_NODE: u32 = numa_shim::NO_NODE;

/// Return the NUMA node of the calling thread, or [`NO_NODE`] if unavailable.
///
/// Internally converts `Option<u32>` (the idiomatic shim API) to the sentinel
/// form used by the in-tree call sites.
#[must_use]
pub fn current_node() -> u32 {
    numa_shim::current_node().unwrap_or(NO_NODE)
}

/// Reserve a SEGMENT-aligned span of `usable` bytes with a NUMA preference for
/// `node`.
///
/// Delegates to `numa_shim::reserve_preferred_on_node` (requires the shim's
/// `vmem-integration` feature, enabled in this crate's `Cargo.toml` dep
/// declaration). Returns the legacy `(base, reservation_ptr, reservation_len)`
/// triple that the in-tree call sites expect, taking the allocation out of the
/// RAII handle so `sefer-alloc` can manage the lifetime through the segment
/// header's `(reservation, reservation_len)` pair.
///
/// ## Best-effort NUMA, by design (task #1306)
///
/// The shim's `reserve_preferred_on_node` is deliberately strict: no silent
/// fallback to an unbound reservation anywhere inside it. sefer-alloc's
/// production path is exactly the "caller wanting best-effort behavior" the
/// shim's docs describe — NUMA placement is a placement HINT for an
/// allocator; running out of memory is fatal, failing to install a mere hint
/// is not — so THIS seam composes the fallback visibly, at its own call site:
///
/// - `node == NO_NODE`: plain `aligned_vmem::reserve_aligned` — no NUMA
///   intent, no shim detour (the old shim API accepted the sentinel and
///   collapsed to this internally; the new one does not take a sentinel).
/// - Any `reserve_preferred_on_node` error (unsupported platform/architecture,
///   node out of the Linux nodemask range, mbind refusal, argument-contract
///   violation, OS refusal): fall back to a plain unbound reservation — the
///   same observable behavior the old silent internal paths had on Linux,
///   now uniform across platforms and localized in the one caller that wants
///   it, instead of hidden inside the shim.
///
/// Returns `None` only when the OS refuses the reservation itself (OOM) —
/// same contract as `os::Segment::reserve`.
#[must_use]
pub fn reserve_aligned_on_node(
    usable: usize,
    node: u32,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    use crate::alloc_core::os::SEGMENT;

    let r = if node == NO_NODE {
        // No NUMA preference expressed: plain reservation, no shim detour.
        aligned_vmem::reserve_aligned(usable, SEGMENT)?
    } else {
        // The enclosing `node == NO_NODE` branch proves the input is not
        // the sentinel, so NodeId::new cannot return None here.
        match numa_shim::reserve_preferred_on_node(
            usable,
            SEGMENT,
            numa_shim::NodeId::new(node).expect("node != NO_NODE, checked by the enclosing branch"),
        ) {
            Ok(r) => r,
            // task #1306: best-effort fallback — see this function's own doc.
            // Every error class (UnsupportedPlatform / UnsupportedArchitecture
            // / InvalidNode / InvalidArguments / Os) degrades to an unbound
            // reservation rather than failing the allocation.
            Err(_) => aligned_vmem::reserve_aligned(usable, SEGMENT)?,
        }
    };

    // Extract the triple fields BEFORE consuming the handle so we have both
    // the aligned usable base and the raw reservation coordinates.
    let base_ptr = r.as_ptr();
    let reservation_ptr = r.reservation_ptr();
    let reservation_len = r.reservation_len();

    // L-9e: validate BEFORE calling `into_parts()`. `r` still owns its RAII
    // Drop at this point, so if either pointer were somehow null, the `?`
    // below drops `r` normally (releasing the OS reservation) instead of
    // leaking it. Both pointers are guaranteed non-null by the reservation
    // contract in practice (a successful reservation is always non-null) —
    // this ordering just makes that guarantee load-bearing-free: a future
    // contract violation fails safe (release, then `None`) rather than
    // leaking.
    let base = NonNull::new(base_ptr)?;
    let reservation = NonNull::new(reservation_ptr)?;

    // Suppress the Drop so `aligned_vmem` does NOT call munmap/VirtualFree;
    // sefer-alloc takes ownership and releases via `os::release_segment`
    // later. Only reached now that both pointers are proven non-null.
    let _ = r.into_parts();

    // This path bypasses `os::Segment::reserve` (it goes through
    // `numa_shim::reserve_preferred_on_node` / `aligned_vmem` directly), so it
    // must bump the same process-wide reservation counter directly — otherwise
    // `segments_released_total` (which every release path funnels through
    // `os::release_segment`, including NUMA-pinned segments) would outpace
    // `segments_reserved_total` under `numa-aware`.
    crate::alloc_core::os::SEGMENTS_RESERVED_TOTAL
        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    Some((base, reservation, reservation_len))
}
