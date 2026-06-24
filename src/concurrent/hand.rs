//! [`AtomicSlot<T>`] — the crate's single confined `unsafe` organ (Phase 3b-II).
//!
//! This is the **only** module in the whole crate with
//! `#![allow(unsafe_code)]`. The crate is `#![forbid(unsafe_code)]` everywhere
//! else, so the structural promise "the `unsafe` is one module" is
//! **compiler-checked**, not asserted in prose.
//!
//! [`AtomicSlot<T>`] hides ALL pointer/`unsafe` work behind a minimal, total,
//! safe-to-use API. [`EpochRegion`](crate::concurrent::EpochRegion) is then
//! written in 100% safe code on top of it. Every `unsafe` block below carries a
//! `// SAFETY:` comment naming the invariant it relies on — no exceptions.
//!
//! ## Design
//!
//! A slot is a `(generation, value)` pair where `value` is a
//! `crossbeam_epoch::Atomic<T>` (null = vacant). Readers and writers coordinate
//! via a publication protocol:
//!
//! - A **writer** (holding the writer mutex) calls [`AtomicSlot::install`] to
//!   publish a value (Release store) and returns the *current* generation; or
//!   [`AtomicSlot::evict`] to swap the value to null and bump the generation.
//! - A **reader** calls [`AtomicSlot::read_with`] with the generation baked into
//!   its handle: it loads the generation (Acquire), compares, then loads the
//!   value pointer (Acquire) under a pinned epoch `Guard`. The generation/value
//!   ordering guarantees a reader never sees a value belonging to a different
//!   generation (no torn generation/value pair, no ABA).
//!
//! Reclamation is delegated to `crossbeam-epoch`: an evicted pointer is
//! scheduled for destruction via `guard.defer_destroy` and is freed only once
//! every reader that could still be holding it has unpinned. Readers therefore
//! dereference a pointer that is provably alive for the duration of their
//! pinned guard.

// The crate is `#![deny(unsafe_code)]` with `experimental` on (see
// `src/lib.rs`); this is the ONE documented exception: the confined `Hand`
// organ. `allow` lifts the crate-level `deny` for this file only, so the
// confinement is enforced structurally by the compiler — `unsafe` anywhere
// else is a hard error. (With no features the crate is `forbid` and this
// module is not compiled at all.)
#![allow(unsafe_code)]

use core::sync::atomic::Ordering;

use crossbeam_epoch::{Atomic, Guard, Owned, Shared};

/// The generation a vacant slot starts at, and the lowest generation a handle
/// may carry. Install leaves the generation unchanged; only eviction bumps it.
const INITIAL_GENERATION: u32 = 0;

/// A single slot of an [`EpochRegion`](crate::concurrent::EpochRegion): a
/// generation counter plus an epoch-managed value pointer (null = vacant).
///
/// This is the safe-to-use membrane over ALL pointer/`unsafe` work in the
/// crate. It exposes a minimal, total API:
///
/// - [`vacant`](Self::vacant) — construct a vacant slot.
/// - [`generation`](Self::generation) — read the current generation (Acquire).
/// - [`read_with`](Self::read_with) — lock-free read under a pinned guard.
/// - [`install`](Self::install) — writer-only publish (caller holds the writer
///   mutex); requires the slot be vacant.
/// - [`evict`](Self::evict) — writer-only publish of a tombstone; bumps
///   generation, schedules reclamation.
///
/// `T` is stored on the heap behind a `crossbeam_epoch::Atomic<T>`; the slot
/// itself is plain data (an atomic `u32` and an atomic-pointer word) and is
/// `Send + Sync` for every `T` (the slot does not own a `T` until `install`,
/// and the pointed-to `T` is reclaimed by the epoch collector, not dropped by
/// the slot).
pub(crate) struct AtomicSlot<T> {
    /// Generation of the current occupant. Bumped (Release) on every eviction
    /// so handles minted at an older generation go stale (I3 — no ABA). A
    /// reader loads this first (Acquire) and refuses the value if it does not
    /// match the handle's generation.
    generation: core::sync::atomic::AtomicU32,
    /// The value pointer. `null` ⇒ vacant. Stored/published with Release;
    /// loaded by readers with Acquire under a pinned guard, then reclaimed via
    /// `guard.defer_destroy` on eviction.
    value: Atomic<T>,
}

impl<T> AtomicSlot<T> {
    /// Creates a vacant slot at generation [`INITIAL_GENERATION`] with a null
    /// value pointer.
    #[must_use]
    pub(crate) fn vacant() -> Self {
        Self {
            generation: core::sync::atomic::AtomicU32::new(INITIAL_GENERATION),
            value: Atomic::null(),
        }
    }

    /// Loads the slot's current generation with `Acquire` ordering.
    ///
    /// Pairing: a writer stores the generation with `Release` in
    /// [`evict`](Self::evict); this `Acquire` load therefore sees the bump (and
    /// any value publication ordered before it).
    #[must_use]
    pub(crate) fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// Lock-free read under a pinned epoch [`Guard`].
    ///
    /// Uses a **seqlock-style validation** to guarantee a reader never observes
    /// a value belonging to a different generation (the torn-read / ABA hazard
    /// that loom catches):
    ///
    /// 1. Load the generation (Acquire) → `g1`. If `g1 != expected_gen`, the
    ///    handle is stale (I3) → `None`.
    /// 2. Load the value pointer (Acquire) under `guard`. If null → `None`.
    /// 3. **Re-load** the generation (Acquire) → `g2`. If `g2 != g1`, a writer
    ///    evicted (and possibly reinstalled) the slot between steps 1 and 2, so
    ///    the value we loaded may not belong to `g1` → `None` (reject, like a
    ///    stale handle).
    ///
    /// Only if `g1 == expected_gen == g2` do we dereference and call `f`. This
    /// closes the window where a reader loads an old generation, a writer
    /// evicts + reinstalls, and the reader then loads the new value.
    ///
    /// `guard` keeps the pointed-to `T` alive until at least the next epoch
    /// advance after the reader unpins, so the dereference is valid for the
    /// whole call to `f`.
    pub(crate) fn read_with<R>(
        &self,
        expected_gen: u32,
        guard: &Guard,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        // Step 1: Acquire-load the generation. If it does not match the handle,
        // the handle is stale (or the slot was reused at a later generation):
        // refuse to read. This is the ABA guard (I3).
        let g1 = self.generation.load(Ordering::Acquire);
        if g1 != expected_gen {
            return None;
        }
        // Step 2: Acquire-load the value pointer under the pinned guard. Acquire
        // pairs with the writer's Release store in `install`, so the pointee
        // `T` is fully initialised and visible.
        let shared: Shared<'_, T> = self.value.load(Ordering::Acquire, guard);
        if shared.is_null() {
            // Vacant (or evicted): generation matched but no value — I2.
            return None;
        }
        // Step 3 (seqlock validation): re-load the generation. If a writer
        // evicted (bumping the generation) between step 1 and now, the value we
        // loaded may belong to a later reinstall — reject to avoid a torn read.
        // This is the fix loom demanded: without it, a reader can load the old
        // generation, then load a value installed after an intervening eviction.
        let g2 = self.generation.load(Ordering::Acquire);
        if g2 != g1 {
            return None;
        }
        // SAFETY: `shared` is a non-null pointer obtained from `self.value` by
        // an Acquire load under `guard`. Three invariants make dereferencing it
        // sound for the duration of `f`:
        //  1. VALID INIT: the pointer was published by `install` via an
        //     `Owned::new(value)` (a heap allocation of a fully-initialised
        //     `T`) stored with `Release`; the Acquire load here synchronises
        //     with that Release, so the pointee is initialised and visible.
        //  2. LIFETIME: the value is reclaimed ONLY by `guard.defer_destroy`,
        //     which runs no sooner than the next epoch advance after EVERY
        //     currently-pinned guard (including this one) has unpinned. Since
        //     `guard` is pinned for the whole body of `f`, the pointee is
        //     alive for the whole call — it cannot be freed underneath us.
        //  3. NO ALIASING VIOLATION: `f` takes a shared `&T`; multiple readers
        //     may concurrently hold `&T` to the same value, which is sound
        //     (shared references are `Sync`-free; `T` is only mutated by the
        //     writer, and the writer has swapped the pointer to null before
        //     scheduling destruction, so no `&mut` coexists with this `&`).
        //  4. GENERATION COHERENCE (seqlock): the g1==g2 re-check above proves
        //     no eviction occurred between the generation load and the value
        //     load, so `shared` belongs to generation `g1 == expected_gen`.
        // The reference is bound to `f` and never escapes.
        let r = f(unsafe {
            // SAFETY (deref validity): `shared` is non-null (checked above) and
            // points to a `T` published with Release in `install`; the Acquire
            // load above synchronises with that Release. The lifetime proof is
            // the four-point argument in the surrounding SAFETY comment.
            shared
                .as_ref()
                .expect("non-null Shared yields a valid reference")
        });
        // `Shared` is `Copy` and borrows the `Guard`; the pointee stays alive
        // until the next epoch advance past `guard` (see SAFETY point 2). No
        // explicit drop needed.
        Some(r)
    }

    /// Writer-only: publish `value` into a currently-vacant slot.
    ///
    /// The caller MUST hold the writer mutex (writers are serialised), and the
    /// slot MUST be vacant. Boxes `value` into an [`Owned`], stores it with
    /// `Release`, and returns the slot's current generation (unchanged on
    /// install) — a handle minted now carries that generation.
    ///
    /// We do not check vacancy here (that is the caller's invariant under the
    /// writer lock); `install` unconditionally overwrites. The
    /// [`EpochRegion`](crate::concurrent::EpochRegion) only ever calls this on a
    /// slot popped from the free list, which is vacant by construction.
    pub(crate) fn install(&self, value: T, _guard: &Guard) -> u32 {
        let owned = Owned::new(value);
        // Release-publish the pointer. A reader's Acquire load of this pointer
        // (and the Acquire load of the generation, which precedes it in
        // `read_with`) synchronises with this Release, seeing the initialised
        // value.
        self.value.store(owned, Ordering::Release);
        // Generation is unchanged on install — a handle minted now carries the
        // slot's current generation.
        self.generation.load(Ordering::Acquire)
    }

    /// Writer-only: tombstone the slot, bump the generation, and schedule
    /// reclamation of the old value.
    ///
    /// The caller MUST hold the writer mutex. Swaps the value pointer to null
    /// (`AcqRel`); if it was non-null, schedules `guard.defer_destroy(old)` and
    /// bumps the generation (`Release`). Returns `true` if the slot may be
    /// reused (generation was bumped below saturation), `false` if it saturated
    /// (generation at `u32::MAX`, slot must be retired) OR if the slot was
    /// already vacant (nothing to evict).
    ///
    /// **Reclamation:** after the swap-to-null, no NEW reader can obtain the
    /// old pointer (they load null). Readers that already loaded the old
    /// pointer are protected by their pinned guard; `defer_destroy` frees the
    /// pointee only after the next epoch advance past all such guards.
    pub(crate) fn evict(&self, guard: &Guard) -> bool {
        // AcqRel: Acquire to see prior publications (so we reclaim the right
        // pointer), Release so a subsequent reader's Acquire load observes the
        // null (and never the about-to-be-destroyed pointer).
        let old: Shared<'_, T> = self.value.swap(Shared::null(), Ordering::AcqRel, guard);
        if old.is_null() {
            // Already vacant — nothing to evict. Report "not reusable" so the
            // caller does not double-count a free slot.
            return false;
        }
        // SAFETY: `old` is the non-null pointer that WAS published in this slot.
        // After the `swap` to null, NO new reader can load `old` (they load
        // null). Any reader that ALREADY loaded `old` did so under a pinned
        // guard; `guard.defer_destroy(old)` defers the deallocation until at
        // least two epoch advances later, by which time every such reader has
        // unpinned. Therefore scheduling destruction here is sound — the memory
        // is freed only when no reader can still be holding `old`. We do NOT
        // dereference `old` here.
        unsafe {
            guard.defer_destroy(old);
        }
        // Bump the generation so handles minted at the old generation go stale
        // (I3 — no ABA). Saturation: at u32::MAX we RETIRE the slot (leave
        // generation at MAX, signal the caller not to reuse it) rather than
        // wrap into alias with a prior generation.
        let cur = self.generation.load(Ordering::Acquire);
        if cur == u32::MAX {
            // Saturated: retire. Generation stays at MAX; old handles are stale
            // (slot is now vacant) and no fresh handle is ever minted here.
            return false;
        }
        self.generation.store(cur + 1, Ordering::Release);
        true
    }

    /// Drops the value this slot holds (if any), given EXCLUSIVE access.
    ///
    /// Called only from [`EpochRegion`](crate::concurrent::EpochRegion)'s `Drop`
    /// to run live values' destructors instead of leaking them — upholding I5
    /// (every value is dropped exactly once, on `remove` or on region drop).
    /// `&mut self` proves no reader or writer can race, so we may take ownership
    /// of the pointer directly.
    pub(crate) fn drop_value(&mut self) {
        // SAFETY: `&mut self` is exclusive access to this slot — there is no
        // concurrent reader (no pinned guard can reference this slot) and no
        // concurrent writer. `crossbeam_epoch::unprotected()` is therefore a
        // sound guard here (its contract is "no other thread is accessing the
        // collection"), and swapping the pointer out and reconstructing the
        // `Owned` via `into_owned` takes ownership of the heap `T` and drops it
        // exactly once. A null pointer (vacant/retired slot) is a no-op.
        unsafe {
            let guard = crossbeam_epoch::unprotected();
            let shared = self.value.swap(Shared::null(), Ordering::Relaxed, guard);
            if !shared.is_null() {
                drop(shared.into_owned());
            }
        }
    }
}

// Hand-written `Send`/`Sync`: an `AtomicSlot<T>` does not own a `T` while
// vacant, and while occupied the `T` is shared (read-only to readers) and
// reclaimed by the epoch collector (not dropped by the slot). The slot is
// therefore `Send + Sync` for every `T` (matches `crossbeam_epoch::Atomic<T>`,
// which is unconditionally `Send + Sync`).
// SAFETY (Send): an `AtomicSlot<T>` may hold a heap `T` behind its `Atomic<T>`,
// so sending it to another thread sends that `T` — hence the `T: Send` bound.
// `T: Sync` is also required because readers on multiple threads share `&T`
// concurrently (see the Sync impl). With both bounds the impl matches exactly
// what `crossbeam_epoch::Atomic<T>: Send` requires, and is what the compiler
// would auto-derive — stated explicitly here for clarity at the unsafe seam.
// An UNBOUNDED impl would be unsound (it would let a non-`Send` `T`, e.g.
// `Rc`, cross threads).
unsafe impl<T: Send + Sync> Send for AtomicSlot<T> {}
// SAFETY (Sync): `&AtomicSlot<T>` grants shared access to the generation
// counter (atomic) and the `Atomic<T>` pointer (lock-free under a guard).
// Readers run `read_with` concurrently, each obtaining a shared `&T` to the
// same value — sound only when `T: Sync`. The single writer (under the region's
// writer mutex) is the only mutator, via atomic operations; no `&mut T`
// coexists with a reader `&T`. `T: Send` is required because a value installed
// on one thread may be dropped (reclaimed) on another. Matches `Atomic<T>: Sync`.
unsafe impl<T: Send + Sync> Sync for AtomicSlot<T> {}

// `AtomicSlot<T>` has no hand-written `Drop`: its default drop drops the
// `Atomic<T>` handle without touching the pointee (which the `Atomic` does not
// own — the epoch collector does). LIVE values are dropped explicitly by
// [`EpochRegion`](crate::concurrent::EpochRegion)'s `Drop`, which calls
// [`AtomicSlot::drop_value`] on every slot under `&mut` exclusivity (upholding
// I5). Values already `remove`d are reclaimed by `crossbeam-epoch` at an epoch
// boundary; if the process exits before that boundary they may not run their
// destructors — the standard epoch-reclamation caveat, documented on the tier.
