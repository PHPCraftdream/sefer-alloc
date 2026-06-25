//! [`HeapCore`] — the thin heap value that lives inside a registry slot.
//!
//! This is the type the Phase 12.3 raw-pointer TLS will cache as
//! `*mut HeapCore`. Per §2.0 of `MALLOC_PLAN_PHASE12-13.md` the heap is now
//! thin (segment-centric free state lives in each segment's `BinTable`, not
//! in a heap-local array), so the per-slot heap needs to carry only:
//!
//! - its **id** (its slot index + the slot's `generation`), used by the 12.3
//!   ownership stamping on segment headers (`owner = heap id + generation`)
//!   and the M8/M9 coherence checks, and
//! - the **segment substrate** ([`AllocCore`]) that owns this heap's segments
//!   and performs all per-segment `BinTable` arithmetic.
//!
//! ## Scope (Phase 12.2)
//!
//! Phase 12.2 ships the registry *structure* and the claim/recycle/abandon
//! API, single-threaded. The cross-thread [`ThreadFreeStack`] is intentionally
//! NOT wired into `HeapCore` here: it is `Box`-allocated (goes through
//! `std::alloc`), which would violate the M5-clean bootstrap path of the
//! registry. The TFS + raw-pointer TLS + `owner_thread_free` stamping arrive
//! together in Phase 12.3 (where the TLS bind — not the registry bootstrap —
//! performs the lazy `HeapCore` init that may touch `std`). `HeapCore` is
//! therefore the *minimal* sound heap value: no half-wired fields, every
//! field is consumed by a 12.2 code path or is a documented 12.3 hook (the
//! `id`, read by `HeapRegistry::recycle` to find the slot back).
//!
//! ## `Heap` vs `HeapCore`
//!
//! The existing [`Heap`](crate::heap::Heap) is the Phase 9/10/12.1
//! thread-local heap (owns `AllocCore` + the cross-thread stack, with the
//! abandon-on-drop leak discipline). `HeapCore` is NOT a rewrite of `Heap`:
//! it is the *slot-resident* value the registry stores. The 12.3 step will
//! migrate the live `Heap`'s state into `HeapCore` (or have `Heap` own a
//! `HeapCore`); until then they coexist — the registry is exercised only by
//! its own tests.

use crate::alloc_core::AllocCore;

/// The thin, slot-resident heap value.
///
/// Lives inside a [`HeapSlot`](super::heap_slot::HeapSlot)'s `UnsafeCell` and
/// is handed out to a thread via
/// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) as a
/// `*mut HeapCore`. Single-writer invariant (the owning thread is the only
/// mutator of its heap's bins) makes the `UnsafeCell` sound.
pub struct HeapCore {
    /// The owning slot's index in the registry. Used by `recycle`/`abandon`
    /// to find the slot back from a `*mut HeapCore` (12.3 will stamp this +
    /// the slot `generation` into segment headers as the ownership key).
    /// `u32::MAX` is reserved as "not yet bound to a slot" (a freshly-init'd
    /// slot has `id = u32::MAX` until `claim` overwrites it).
    pub(crate) id: u32,
    /// The segment substrate this heap owns. Owns the primordial + any
    /// additionally-reserved small/large segments. Phase 12.1: free-list
    /// state lives in each segment's `BinTable`, so this is the heap's entire
    /// small-allocation engine.
    #[allow(dead_code)] // Read by Phase 12.3's TLS-bound allocator path.
    pub(crate) core: AllocCore,
}

impl HeapCore {
    /// Construct a fresh heap value bound to slot `id`. Bootstraps the
    /// segment substrate via [`AllocCore::new`] (which goes through the OS
    /// aperture — `mmap`/`VirtualAlloc` — and never `std::alloc`, upholding
    /// M5). Returns `None` only on primordial OOM (the OS refused the
    /// reservation).
    ///
    /// Called lazily by [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// when it transitions a slot `FREE → LIVE` and needs to materialise the
    /// heap value in the slot's `UnsafeCell`.
    #[must_use]
    pub(crate) fn new(id: u32) -> Option<Self> {
        let core = AllocCore::new()?;
        Some(Self { id, core })
    }

    /// The slot index this heap is bound to. Read by `recycle`/`abandon` to
    /// locate the owning slot from a `*mut HeapCore`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
}
