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
//!   publish a value (Release store) and returns the *current* generation. **Phase
//!   7b:** ANY thread (owner or remote) may call
//!   [`AtomicSlot::try_evict_at`] to perform a generation-CAS-checked eviction
//!   — the CAS is the single linearization point that prevents the
//!   lost-live-value hazard (see the method's SAFETY proof).
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

/// The outcome of a generation-checked eviction ([`AtomicSlot::try_evict_at`]).
///
/// This is the SINGLE linearization point of a removal: the
/// `compare_exchange(expected_gen → next)` on the slot's generation is the
/// atomic step that decides who owns the reclamation. See the `try_evict_at`
/// SAFETY argument for why this rules out the off-mutex "lost-live-value"
/// hazard (a remote remover that checks `generation == handle.gen` then swaps
/// value→null can destroy a NEWER value installed by the owner after an
/// intervening eviction — a use-after-free).
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum EvictOutcome {
    /// THIS call won the generation CAS and therefore uniquely owns the
    /// transition out of `expected_gen`: it has swapped the value to null and
    /// scheduled `defer_destroy` of the old pointer.
    ///
    /// `reusable` is `true` iff the POST-eviction generation (`expected_gen +
    /// 1`) is itself not saturated, i.e. `expected_gen != u32::MAX - 1`. The
    /// upfront `expected_gen == u32::MAX` check in `try_evict_at` only rejects
    /// a CAS attempted AT the sentinel (returning [`EvictOutcome::Stale`]); it
    /// does NOT cover the CAS that WINS and LANDS the slot on the sentinel
    /// (`expected_gen == u32::MAX - 1` → `next == u32::MAX`). That slot is
    /// retired: `reusable` is `false`, and the caller must NOT re-add it to a
    /// free list — no live handle will ever carry `u32::MAX`, so `install`
    /// will never run on it again, and a subsequent `try_evict_at` against it
    /// would immediately hit the upfront `Stale` guard rather than a real CAS.
    Evicted { reusable: bool },
    /// The slot was NO LONGER at `expected_gen` when the CAS ran: another
    /// remover already transitioned it (or the owner reinstalled at a later
    /// generation). No value was touched and no reclamation was scheduled —
    /// the caller treats this as a no-op `false` (the handle was already stale
    /// or already removed: I2).
    Stale,
}

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
/// - [`try_evict_at`](Self::try_evict_at) — generation-CAS-checked eviction
///   (Phase 7b); callable by ANY thread (owner or remote). The CAS is the
///   single linearization point of a removal.
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
    /// Pairing: an evicting thread stores the generation with `Release` in
    /// [`try_evict_at`](Self::try_evict_at); this `Acquire` load therefore sees
    /// the bump (and any value publication ordered before it). Used as a
    /// diagnostic accessor and by [`EpochRegion`](crate::concurrent::EpochRegion)'s
    /// `drain_remote_free` defensive re-check; the eviction path itself uses
    /// the CAS directly rather than a load-then-check (which would be the
    /// unsound off-mutex hazard).
    #[must_use]
    pub(crate) fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    /// **Diagnostics/testing only.** Force-sets the slot's generation with
    /// `Release` ordering, bypassing the normal install/evict protocol.
    ///
    /// This exists SOLELY so a test can reach the `expected_gen == u32::MAX -
    /// 1` saturation-retirement edge of [`try_evict_at`](Self::try_evict_at)
    /// without driving ~4 billion real insert/remove cycles (which the
    /// project's short-scenario test policy forbids). Production code MUST
    /// NEVER call this: it does not touch `value`, so calling it on an
    /// occupied slot desyncs the generation from the installed value's true
    /// generation (a self-inflicted ABA hazard) — the test call sites gate
    /// this to freshly-`with_capacity`-created, still-vacant slots only.
    pub(crate) fn set_generation_for_tests(&self, generation: u32) {
        self.generation.store(generation, Ordering::Release);
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
        //     owner via `install`, and an evicting thread — owner OR remote, in
        //     Phase 7b — has swapped the pointer to null before scheduling
        //     destruction, so no `&mut` coexists with this `&`).
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
    ///
    /// **Phase 7b note:** a slot popped from the free list is vacant AND at a
    /// generation no live handle carries (the generation was bumped on the
    /// eviction that freed it). `install` does not bump the generation, so the
    /// just-installed value is associated with that post-eviction generation.
    /// Because no remote remover can have a handle at this generation (none was
    /// ever minted here between the eviction and this install), `install` cannot
    /// race a `try_evict_at` — the install is the FIRST event at this
    /// generation. This is part of the [`try_evict_at`] no-reinstall proof.
    ///
    /// [`try_evict_at`]: AtomicSlot::try_evict_at
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

    /// Generation-checked eviction — the SINGLE linearization point of a
    /// removal (Phase 7b).
    ///
    /// Atomically transitions the slot's generation from `expected_gen` to
    /// `expected_gen + 1` via `compare_exchange` (saturation: if `expected_gen
    /// == u32::MAX` the attempt is rejected upfront as [`EvictOutcome::Stale`]
    /// — see below; if `expected_gen == u32::MAX - 1` the CAS wins and the
    /// generation LANDS on `u32::MAX`, and the slot is reported non-reusable —
    /// retired). On a SUCCESSFUL CAS the caller UNIQUELY owns the
    /// reclamation: this method then swaps the value pointer to null and
    /// schedules `guard.defer_destroy(old)`. On a FAILED CAS the slot was no
    /// longer at `expected_gen` (another remover won, or the owner
    /// reinstalled) — nothing is touched and [`EvictOutcome::Stale`] is
    /// returned.
    ///
    /// # Why this is the sound way to evict from a NON-OWNER thread
    ///
    /// A remote remover holds no writer mutex. The naive "load
    /// `generation()`, compare to `handle.gen`, then `swap(value → null)`"
    /// is **UNSOUND** off-mutex: between the check and the swap, the slot can
    /// be evicted AND reinstalled by the owner at `gen+1` with a NEW live
    /// value, and the stale remover's swap-to-null would then destroy that
    /// newer live value (a lost-live-value / use-after-free). The generation
    /// CAS makes the check-and-swap atomic relative to ALL other eviction
    /// attempts: exactly one remover can win the CAS at `expected_gen`, so
    /// exactly one remover can swap the value out, and it can only ever swap
    /// out the value that was published at `expected_gen` (see the SAFETY
    /// proof below for why no owner reinstall can slip between the CAS win and
    /// the swap).
    ///
    /// # Returns
    ///
    /// - [`EvictOutcome::Evicted { reusable }`] if THIS call won the CAS
    ///   (uniquely owns the reclamation). `reusable` is `false` iff the slot
    ///   saturated (caller retires it).
    /// - [`EvictOutcome::Stale`] if the CAS failed (caller treats as no-op).
    pub(crate) fn try_evict_at(&self, expected_gen: u32, guard: &Guard) -> EvictOutcome {
        // Saturation guard: a handle carrying `u32::MAX` cannot be live. A slot
        // reaches generation MAX only via eviction (which RETIRES it — never
        // re-adds to a free list), and `install` (which mints handles) never
        // runs on a retired slot. So no live handle ever carries MAX. We treat
        // `expected_gen == u32::MAX` as Stale upfront — this ALSO avoids a racy
        // idempotent MAX→MAX CAS (which two threads could both "win", causing a
        // double `fetch_sub` on `len` and a double `defer_destroy` report).
        if expected_gen == u32::MAX {
            return EvictOutcome::Stale;
        }
        // Normal path: CAS the generation `expected_gen → expected_gen + 1`.
        let next = expected_gen + 1;
        // AcqRel on success: Acquire to see prior publications (so we reclaim
        // the correct pointer in the swap below), Release so a concurrent
        // reader's Acquire load of the generation observes the bump BEFORE it
        // could load a stale value. Acquire on failure: we read the actual
        // current generation on failure for diagnostics, which we discard —
        // Relaxed would also suffice, but Acquire is harmless and matches the
        // reader's pairing.
        let cas = self.generation.compare_exchange(
            expected_gen,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if cas.is_err() {
            // Another thread transitioned the generation first (a concurrent
            // remover won, or the owner reinstalled at a later generation).
            // We touched NOTHING — no swap, no defer_destroy. The caller treats
            // this handle as already-removed (I2: a second remove is a no-op).
            return EvictOutcome::Stale;
        }
        // CAS WON: we uniquely own the transition out of `expected_gen`. Swap
        // the value to null and reclaim whatever was published at
        // `expected_gen`. AcqRel: Acquire pairs with the publisher's Release
        // store in `install` (we see the initialised value), Release so a
        // subsequent reader's Acquire load observes null (never the about-to-
        // be-destroyed pointer).
        let old: Shared<'_, T> = self.value.swap(Shared::null(), Ordering::AcqRel, guard);
        // SAFETY: identical contract to `evict`'s defer_destroy, restated for
        // the multi-thread eviction context:
        //  1. WE WON THE CAS — so we are the UNIQUE thread that may reclaim a
        //     value published at `expected_gen`. No other remover can reach
        //     this swap for `expected_gen` (they all failed the CAS and
        //     returned `Stale`), so there is no double-free.
        //  2. NO OWNER REINSTALL CAN RACE THE SWAP: the owner only ever
        //     `install`s into a slot it popped from its free list. A slot
        //     reaches the free list ONLY after a remover (this one, or a
        //     local one) enqueues the index AND the owner has drained that
        //     queue AND observed the slot vacant. Between our CAS win and our
        //     swap, the slot is at `next` (a generation NO live handle
        //     carries — handles are minted at install-time generations, and
        //     install does not bump the generation), so no concurrent
        //     `try_evict_at` can target it (their `expected_gen` would not
        //     match `next` unless a NEW install + handle-mint happened, which
        //     requires the free-list drain that has not yet occurred). Hence
        //     the pointer we swap out is exactly the one published at
        //     `expected_gen` — never a newer live value.
        //  3. LIFETIME (carries over from `evict`): after the swap-to-null no
        //     NEW reader can load `old` (they load null). Readers that already
        //     loaded `old` did so under a pinned `guard`; `defer_destroy(old)`
        //     frees the pointee only after the next epoch advance past every
        //     such reader's guard. We do NOT dereference `old` here.
        //  4. `old` MAY BE NULL: if the slot was already vacant at
        //     `expected_gen` (e.g. a retire-without-reuse left it vacant, or
        //     a prior `drop_value` ran under exclusive access — impossible
        //     concurrently but defensive), `defer_destroy(null)` is a no-op.
        //     A CAS win at `expected_gen` against a slot a live handle pointed
        //     at implies a value WAS installed (the handle was minted by an
        //     install), so in practice `old` is non-null; the null guard is
        //     belt-and-braces. We trust the caller (EpochRegion) to only call
        //     this for handles it minted.
        unsafe {
            guard.defer_destroy(old);
        }
        // Reusable iff the POST-eviction generation (`next`) is itself not
        // saturated. The upfront `expected_gen == u32::MAX` guard above only
        // rejects a CAS attempted AT the sentinel; it does NOT cover the CAS
        // that WINS and lands the slot ON the sentinel (`expected_gen ==
        // u32::MAX - 1` → `next == u32::MAX`). That slot must be retired here,
        // not handed back reusable: nothing can ever evict it again (the next
        // `try_evict_at` on it would see `expected_gen == u32::MAX` and hit the
        // upfront `Stale` guard, so a `reusable: true` here would mint a live
        // handle at gen `u32::MAX` that can never be removed — a permanent slot
        // leak and a `len` that never comes back down).
        EvictOutcome::Evicted {
            reusable: next != u32::MAX,
        }
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
//
// PHASE 7b RE-AUDIT (relaxed "any thread may evict via try_evict_at" contract):
// pre-7b the ONLY mutator was the single writer holding the region's writer
// mutex. In 7b a REMOTE thread may also call `try_evict_at`, which performs a
// generation CAS + a value swap-to-null + `defer_destroy`. This does NOT
// broaden the aliasing surface: every mutation is still an atomic operation
// (`compare_exchange`, `swap`) on the atomic fields, and the reclamation is
// still routed through `crossbeam_epoch`'s `defer_destroy` (never a raw
// `drop`). The `try_evict_at` SAFETY proof establishes that exactly ONE thread
// can win the generation CAS at a given `expected_gen`, so exactly one thread
// schedules `defer_destroy` for the value published there — no double-free, no
// `&mut` racing a reader's `&`. The invariants below are therefore unchanged
// in substance; only the "single writer" framing widens to "the unique CAS
// winner among all evicting threads".
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
// same value — sound only when `T: Sync`. Evicting threads (owner OR remote in
// 7b) mutate ONLY via atomic operations (`compare_exchange`, `swap`); no `&mut
// T` ever coexists with a reader `&T` (the pointer is swapped to null before
// `defer_destroy` is scheduled). `T: Send` is required because a value
// installed on one thread may be dropped (reclaimed) on another. Matches
// `Atomic<T>: Sync`.
unsafe impl<T: Send + Sync> Sync for AtomicSlot<T> {}

// `AtomicSlot<T>` has no hand-written `Drop`: its default drop drops the
// `Atomic<T>` handle without touching the pointee (which the `Atomic` does not
// own — the epoch collector does). LIVE values are dropped explicitly by
// [`EpochRegion`](crate::concurrent::EpochRegion)'s `Drop`, which calls
// [`AtomicSlot::drop_value`] on every slot under `&mut` exclusivity (upholding
// I5). Values already `remove`d are reclaimed by `crossbeam-epoch` at an epoch
// boundary; if the process exits before that boundary they may not run their
// destructors — the standard epoch-reclamation caveat, documented on the tier.
