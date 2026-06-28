//! NUMA OS-seam — thin wrapper over the `numa-shim` crate (`crates/numa`).
//!
//! Preserves the in-tree call sites' API for backward-compat inside the
//! `sefer-alloc` crate; the actual unsafe OS FFI (mbind, VirtualAllocExNuma,
//! sysfs cpumap reads) lives entirely in `numa-shim`. This file contains NO
//! platform-specific unsafe code.
//!
//! ## Gating
//!
//! Compiled only when `feature = "numa-aware"` is active (which implies
//! `dep:numa-shim` is enabled). Each function delegates straight to the shim.
//!
//! ## Backward compatibility
//!
//! The three public items — `NO_NODE`, `current_node`, `bind_segment`,
//! `reserve_aligned_on_node` — have identical signatures to the 742-line
//! in-tree implementation they replace; all callers in `alloc_core.rs` compile
//! without modification.

// `numa_shim` is the one crate that is allowed unsafe; we are safe here.
#![allow(unsafe_code)] // needed only for the SAFETY-documented unsafe block in bind_segment

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

/// Bind a memory range to a NUMA node.
///
/// Safe wrapper: `sefer-alloc` guarantees that `(base, len)` is its own OS
/// reservation. The shim's `bind_range` is `unsafe` only because external
/// callers may pass arbitrary pointers; inside this crate the invariant is
/// established by the callers (`reserve_small_segment` / `alloc_large_slow`).
///
/// No-op when `node == NO_NODE`, `len == 0`, or `base` is null.
// Currently exercised only by `tests/numa_seam.rs` (NO_NODE / zero-len no-op
// invariants) — `reserve_aligned_on_node` binds at reservation time. Kept
// pub so the seam stays callable, with the SAFETY proof intact.
// `not_unsafe_ptr_arg_deref` is conservative here: this function never
// dereferences `base`; it forwards it to `numa_shim::bind_range` (an unsafe
// fn) whose entire safety story is about an OS reservation existing, not
// about Rust-level UB.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn bind_segment(base: *mut u8, len: usize, node: u32) {
    if node == NO_NODE || len == 0 || base.is_null() {
        return;
    }
    // SAFETY: callers (sefer-alloc internals) only call this immediately after
    // receiving `(base, len)` from their own OS reservation; the range is live
    // and exclusively owned by this allocator for the duration of the call.
    // `bind_range` only issues an `mbind(2)` syscall (kernel metadata); it
    // never reads or writes the payload bytes.
    unsafe { numa_shim::bind_range(base, len, node) };
}

/// Reserve a SEGMENT-aligned span of `usable` bytes with a NUMA preference for
/// `node`.
///
/// Delegates to `numa_shim::reserve_on_node` (requires the `vmem-integration`
/// feature, enabled in `Cargo.toml`). Returns the legacy
/// `(base, reservation_ptr, reservation_len)` triple that the in-tree call
/// sites expect, taking the allocation out of the RAII handle so
/// `sefer-alloc` can manage the lifetime through the segment header's
/// `(reservation, reservation_len)` pair.
///
/// Returns `None` on OOM (same contract as `os::Segment::reserve`).
#[must_use]
pub fn reserve_aligned_on_node(
    usable: usize,
    node: u32,
) -> Option<(NonNull<u8>, NonNull<u8>, usize)> {
    use crate::alloc_core::os::SEGMENT;

    let r = numa_shim::reserve_on_node(usable, SEGMENT, node)?;

    // Extract the triple fields BEFORE consuming the handle so we have both
    // the aligned usable base and the raw reservation coordinates.
    let base_ptr = r.as_ptr();
    let reservation_ptr = r.reservation_ptr();
    let reservation_len = r.reservation_len();

    // Suppress the Drop so `aligned_vmem` does NOT call munmap/VirtualFree;
    // sefer-alloc takes ownership and releases via `os::release_segment` later.
    let _ = r.into_parts();

    // Both pointers are guaranteed non-null by the `reserve_on_node` contract
    // (it returns None on OOM; a successful Some(r) is always non-null).
    let base = NonNull::new(base_ptr)?;
    let reservation = NonNull::new(reservation_ptr)?;

    Some((base, reservation, reservation_len))
}
