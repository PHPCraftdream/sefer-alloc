//! Owning backing storage of one `SmallSidecar` (task #223): an OS-zeroed
//! anonymous VM span, or the System-heap fallback.
//!
//! ## Why
//!
//! Registering a Small/Primordial route used to pay a full ~40 KiB
//! userspace zero-fill: `System.alloc` returned fresh heap bytes that
//! `SmallSidecar::initialize_at` then wrote over field by field (LLVM folds
//! the pair into a `calloc`-shaped glibc zero-fill below the mmap
//! threshold). Every field of the sidecar EXCEPT the backing tag itself is
//! valid at all-zero (null leaf pointers, zero pending words —
//! `SidecarBitmap::publish` rejects a zero encoded class — and zero
//! presence bits), so a backing that is already zeroed by the OS removes
//! the whole fill: every real OS backend hands back zeroed anonymous pages
//! (Linux `mmap`, Windows `VirtualAlloc`); there is no userspace zero-fill
//! and no memset on the registration path.
//!
//! ## Fallbacks (required for correctness, not performance)
//!
//! - **Interpreter build**: the aligned-vmem backend of the interpreter
//!   build is `System`-backed and does NOT hand back zeroed memory; `SmallSidecar::allocate` explicitly
//!   zeroes the span in that build, so the observed initial state is
//!   identical.
//! - **Model-checker build**: its atomics are tracked cells that must be
//!   built by `AtomicU64::new`/`AtomicPtr::new`; raw OS-zeroed bytes
//!   reinterpreted as atomics are not valid tracked cells. That build never
//!   constructs the `Vmem` variant (see the availability constant in
//!   `small_sidecar.rs`) and take the explicit-construction `System` path.
//! - **OOM**: if the reservation fails, `allocate` falls back to the
//!   historical `System.alloc` + `initialize_at` path; the tag records
//!   which backing is live so the OWNER releases it exactly once, in both
//!   cases after `ClassLeaves`' promoted-leaf chunk Drop has run (both
//!   `backing` and `classes` are `ManuallyDrop`; the release order is:
//!   `SmallSidecar::drop` drops only the payload `classes` while the
//!   storage is still live; `EntryHandle::free_sidecar` then moves the tag
//!   out with `take_backing`, runs `drop_in_place`, and only after that
//!   returns releases the storage via `release_taken_backing` —
//!   `Reservation` munmap for `Vmem`, `System.dealloc` for `System`; no
//!   automatic field glue runs and no release happens while any reference
//!   into the storage is live).
//!
//! ## Kernel cost (stated bound)
//!
//! The VM path enters the kernel exactly twice per segment cycle: one
//! reserve (`mmap` / `VirtualAlloc`) at registration and one release
//! (`munmap` / `VirtualFree`) at last-pin unlink; there is no per-page
//! decommit and no recommit call. Page faults stay proportional to the
//! pages actually touched (registration touches only the page holding the
//! backing tag; the historical path memset-dirtied all ~11 pages).
/// Owning backing of one `SmallSidecar`. All-zero bytes are the documented
/// valid initial state of every other field; this tag is the one field that
/// is written explicitly (after the backing is in place) and it decides how
/// the OWNER releases the storage exactly once (moved out by
/// `SmallSidecar::take_backing`, released after the payload drop by
/// `SmallSidecar::release_taken_backing`).
// The `Vmem` payload is never read through: the inner `Reservation` is
// moved out by `SmallSidecar::take_backing` and dropped by
// `SmallSidecar::release_taken_backing` only AFTER the payload drop
// returned, so the field is written and moved, never dereferenced — the
// lint cannot see that path.
#[allow(dead_code)]
pub(crate) enum SmallSidecarBacking {
    /// An anonymous OS reservation from `aligned_vmem::reserve_aligned`:
    /// already zeroed by the OS (or explicitly in the interpreter build, see the module
    /// doc), so no userspace zero-fill runs on the registration path. The
    /// inner `Reservation` is moved out by `SmallSidecar::take_backing` and
    /// dropped (munmap) by `SmallSidecar::release_taken_backing` only after
    /// the payload drop returned; no automatic field glue runs afterwards.
    Vmem(aligned_vmem::Reservation),
    /// `System`-heap fallback (`reserve_aligned` refused, or a model-checker build):
    /// constructed fully in place by `SmallSidecar::initialize_at`; released
    /// by `SmallSidecar::release_taken_backing` with
    /// `Layout::new::<SmallSidecar>()` after the payload drop returned.
    System,
}

impl SmallSidecarBacking {
    /// Reserve an OS-zeroed anonymous span of `ceil(bytes / PAGE) * PAGE`
    /// bytes for one sidecar. `None` only on OOM — the caller falls back to
    /// the `System` heap path and must treat that as success, not failure.
    /// `bytes` is `size_of::<SmallSidecar>()`, never zero.
    pub(crate) fn reserve_zeroed_span(bytes: usize) -> Option<aligned_vmem::Reservation> {
        let page = aligned_vmem::PAGE;
        let size = bytes.checked_add(page - 1)? & !(page - 1);
        aligned_vmem::reserve_aligned(size, page)
    }
}
