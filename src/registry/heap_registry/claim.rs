//! Slot claim, maintenance lease, recycle, and failed-init rollback.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented registry seam (the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`). R6-OPT-P0-2 (round 1): the
// former `get_unchecked` on a `'static` inline slot array is gone — every
// slot-array access now goes through `Registry`'s chunk-resolving accessors
// (`bootstrap::registry`), which are safe
// (range-checked via `debug_assert!` and array-index, not `get_unchecked`).
// `allow` lifts the crate-level `deny` for this file only — `unsafe`
// anywhere else in the crate is a hard error. Every remaining `unsafe` block
// carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::marker::PhantomData;
use core::sync::atomic::Ordering;

#[cfg(feature = "alloc-decommit")]
use super::counters::CONFIG_CONFLICTS;
use super::stack::{
    pick_with_saturation, scan_claim_recovery_free, scan_claimable_slot, scan_free_slot,
};
use crate::registry::bootstrap::{ensure, Registry, MAX_HEAPS};
use crate::registry::heap_core::HeapCore;
use crate::registry::heap_slot::{
    HeapSlot, STATE_EMPTY, STATE_FREE, STATE_INITIALIZING, STATE_LIVE, STATE_MAINTENANCE,
};

/// The global heap slot table. All methods operate on the process-global
/// [`Registry`] returned by [`ensure`]; the type itself carries no state (it
/// is a name-space for the API, mirroring how `Node` / `Layout` are organised
/// elsewhere in the crate).
#[doc(hidden)]
pub struct HeapRegistry;

impl HeapRegistry {
    /// Ph4a (task #2091): safe test observer — same semantics as
    /// [`claim_lease`](Self::claim_lease) but test-gated.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_claim_lease() -> Option<HeapLease> {
        Self::claim_impl(HeapCore::new, |_| {})
    }

    /// Safe test/diagnostic (`internals` + `alloc-decommit`) variant of
    /// [`dbg_claim_lease`](Self::dbg_claim_lease) with the decommit config
    /// plumbing of `claim_lease_with_config` (first-materialisation config,
    /// N2 config-conflict hook). Test-only; never a production API.
    #[cfg(all(feature = "internals", feature = "alloc-decommit"))]
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_claim_lease_with_config(
        config: crate::alloc_core::LargeCacheConfig,
    ) -> Option<HeapLease> {
        Self::claim_lease_with_config(config)
    }

    /// Acquire one already-materialised FREE heap without allocating chunks.
    /// A failed CAS, including MAINTENANCE, grants no access.
    ///
    /// Ph4c (ADR addendum §2.6): crate-only after the surface narrowing;
    /// external tests use the `internals`-gated `dbg_try_maintenance` alias.
    #[doc(hidden)]
    #[must_use]
    pub(crate) fn try_maintenance() -> Option<MaintenanceLease> {
        let (index, _) = scan_free_slot(ensure())?;
        Self::try_maintenance_at(index)
    }

    /// Test-only alias of [`Self::try_maintenance`] after the Ph4c narrowing
    /// made it crate-only (ADR addendum §2.6). Gated on `internals` like the
    /// other `dbg_*` observers.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_try_maintenance() -> Option<MaintenanceLease> {
        Self::try_maintenance()
    }

    /// Acquire exactly this existing initialized slot; never materialize a
    /// chunk and never accept a failed ownership CAS.
    pub(super) fn try_maintenance_at(index: usize) -> Option<MaintenanceLease> {
        let reg = ensure();
        if index >= (reg.count.load(Ordering::Acquire) as usize).min(MAX_HEAPS) {
            return None;
        }
        let slot = reg.slot_if_materialised(index)?;
        if !slot.initialised.load(Ordering::Acquire) {
            return None;
        }
        if slot
            .cas_state(
                STATE_FREE,
                STATE_MAINTENANCE,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return None;
        }
        Some(MaintenanceLease {
            index,
            slot,
            _not_send: PhantomData,
        })
    }

    /// Ph4b (#2092): typed bind path for `tls_heap` — the [`HeapLease`]
    /// authority stays with the caller until its `Drop` publishes FREE.
    #[must_use]
    pub(crate) fn claim_lease() -> Option<HeapLease> {
        // No re-claim-time check: a plain `claim_lease` carries no config to
        // conflict with, so the "already initialised" hook is a no-op.
        Self::claim_impl(HeapCore::new, |_| {})
    }

    /// `claim_lease` with the alloc-decommit config plumbing
    /// (same first-materialisation
    /// config, same N2 config-conflict hook).
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub(crate) fn claim_lease_with_config(
        config: crate::alloc_core::LargeCacheConfig,
    ) -> Option<HeapLease> {
        Self::claim_impl(
            // First materialisation: use the caller's config.
            |idx| HeapCore::new_with_config(idx, config),
            |heap_ptr| {
                // N2 (task #95): re-claim of an already-materialised slot.
                // The slot's existing config (set at first materialisation)
                // silently wins. Compare the requested config against the
                // slot's live policy; on mismatch, count + signal.
                //
                // SAFETY: `claim_impl` only calls this hook when the slot is
                // LIVE and initialised, with the caller being the sole
                // writer (just won the FREE→LIVE CAS). The comparison is a
                // read-only `&self` method on `HeapCore` — no mutation, no
                // hazard.
                let matches = unsafe { (*heap_ptr).live_config_matches(&config) };
                if !matches {
                    // The counter is the ONLY signal, in every build profile
                    // (always compiled in — one increment per mismatched
                    // bind on this cold path, not a hot-path RMW worth gating
                    // behind `alloc-stats`). R2-08 (task #2010): this branch
                    // is the cold bind behind every `GlobalAlloc` method and
                    // is reachable by a legitimate multi-instance config
                    // collision, so it must not panic — a former
                    // `debug_assert!` here unwound out of `GlobalAlloc::alloc`
                    // in debug builds (UB per the trait's contract). The slot
                    // stays LIVE and is returned below: first-wins.
                    CONFIG_CONFLICTS.fetch_add(1, Ordering::Relaxed);
                }
            },
        )
    }

    /// Shared CAS/materialise/bind protocol behind [`claim_lease`](Self::claim_lease)
    /// and [`claim_lease_with_config`](Self::claim_lease_with_config) — factored
    /// out because
    /// the two were near-verbatim copies of the reservation/materialisation/
    /// NUMA-invalidation sequence (R1-11, src review round 1).
    ///
    /// `materialise(idx)` is called exactly once, only on the FIRST claim of
    /// a given slot (`!initialised`), to produce the `HeapCore` to write into
    /// it — `HeapCore::new` for [`claim_lease`](Self::claim_lease),
    /// `HeapCore::new_with_config` for
    /// [`claim_lease_with_config`](Self::claim_lease_with_config). Returning `None`
    /// (OOM) releases the slot to `FREE` (see
    /// [`push_back_after_oom`]) and this function returns `null`. A chunk
    /// materialisation OOM happens earlier: it leaves its minted index
    /// retryable and cold-scans materialised FREE alternatives, preferring
    /// initialized heaps over constructor retries.
    ///
    /// `on_already_initialised(heap_ptr)` is called exactly once, only on a
    /// RE-claim of an already-materialised slot (the `else` of the same
    /// branch) — [`claim_lease`](Self::claim_lease) passes a no-op (a plain
    /// claim has no config to conflict with),
    /// [`claim_lease_with_config`](Self::claim_lease_with_config)
    /// passes the N2 config-conflict comparison. `heap_ptr` is the slot's
    /// live `*mut HeapCore`; the hook is responsible for any `unsafe`
    /// dereference it needs, under the same sole-writer/LIVE/initialised
    /// contract documented at each call site below.
    ///
    /// Every other step — slot picking, the `FREE → LIVE` CAS and its retry
    /// on a lost race, the `generation` bump, `bind_slot_counters`, the
    /// `initialised` publish, and the R11-5 NUMA-cache invalidation — is
    /// identical between the two callers and lives here, run exactly once
    /// per returned pointer regardless of which hook ran.
    ///
    /// Ph4a: returns a [`HeapLease`] instead of a raw pointer. Operation
    /// order is unchanged (hint → bump → scan; `trim_for_recycle` on
    /// re-claim; NUMA invalidation; config-conflict hook). The lease is
    /// formed only AFTER the core is readable: first claim — after the
    /// `initialised` Release publish and the INITIALIZING→LIVE CAS; re-claim
    /// — the slot was already LIVE + initialised when the FREE→LIVE CAS won.
    fn claim_impl<M, R>(materialise: M, on_already_initialised: R) -> Option<HeapLease>
    where
        M: FnOnce(u32) -> Option<HeapCore>,
        R: FnOnce(*mut HeapCore),
    {
        let mut failed_chunk = None;
        loop {
            let reg = ensure();
            let candidate = match failed_chunk {
                Some(failed) => scan_claim_recovery_free(reg, failed),
                None => Self::pick_slot(),
            };
            let idx = candidate?;
            // A minted index is still below `count` after chunk OOM. Keep it
            // retryable, but first try an already-materialised FREE heap.
            let Some(slot) = reg.slot_or_none(idx) else {
                reg.reuse_hint.store(idx as u32, Ordering::Relaxed);
                reg.saturation.publish_claimable();
                failed_chunk = Some(idx);
                continue;
            };

            let observed = slot.state.load(Ordering::Acquire);
            let first_claim = match observed {
                STATE_EMPTY => {
                    if slot
                        .cas_state(
                            STATE_EMPTY,
                            STATE_INITIALIZING,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    true
                }
                STATE_FREE => {
                    // Claim ownership before reading the monotonic flag: a
                    // pre-CAS read could go stale across another claim/recycle.
                    if slot
                        .cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        continue;
                    }
                    if slot.initialised.load(Ordering::Acquire) {
                        false
                    } else {
                        // No core was ever published, so no producer can
                        // target this slot. Maintenance requires initialised.
                        if slot
                            .cas_state(
                                STATE_LIVE,
                                STATE_INITIALIZING,
                                Ordering::Release,
                                Ordering::Relaxed,
                            )
                            .is_err()
                        {
                            std::process::abort();
                        }
                        true
                    }
                }
                _ => continue,
            };
            let generation = slot.generation.fetch_add(1, Ordering::Release) + 1;
            if first_claim {
                let heap_ptr = slot.heap.get();
                match materialise(idx as u32) {
                    // SAFETY: sole writer, uninitialised slot, first claim.
                    Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                    None => {
                        // OOM on materialisation: release the slot to FREE
                        // so it is not leaked (M-5) — same shape as `recycle`.
                        push_back_after_oom(reg, slot, idx as u32);
                        return None;
                    }
                }
                // W3: plant this heap's stable handles to its slot-resident
                // diagnostic hit counters, now that the `HeapCore` is materialised
                // in the slot. See `bind_slot_counters`.
                // SAFETY: we just `write`(hc) into this slot's `UnsafeCell` and are
                // its sole writer (the FREE→LIVE CAS winner); no other thread holds
                // a reference to it yet (`initialised` not yet published).
                unsafe { bind_slot_counters(slot, heap_ptr.cast::<HeapCore>()) };
                // Publish readiness: Release-store `initialised = true` ONLY
                // now that `heap_ptr.write(hc)` has fully completed (task #133
                // hardening — see `HeapSlot::initialised`'s doc comment for the
                // UB window this closes: `count`/`generation` alone are bumped
                // BEFORE `HeapCore::new()` runs and are NOT safe gates for a
                // cross-thread reader to dereference `heap`). This Release
                // store is the publish half of the HB pair; diagnostic
                // aggregation readers (`tcache_hits_total`,
                // `large_cache_hits_total`) pair it with an Acquire load.
                slot.initialised.store(true, Ordering::Release);
                if slot
                    .cas_state(
                        STATE_INITIALIZING,
                        STATE_LIVE,
                        Ordering::Release,
                        Ordering::Relaxed,
                    )
                    .is_err()
                {
                    std::process::abort();
                }
            } else {
                // Re-claim of an already-materialised slot. SAFETY: slot is
                // LIVE and initialised; we are the sole writer (just won the
                // FREE→LIVE CAS) — `on_already_initialised` may soundly form
                // a shared `&HeapCore` over this pointer under that contract.
                // A reused FREE heap may have received terminal publications
                // since its previous owner exited. Reclaim once under the new
                // owner's lease before handing its legacy TLS pointer out.
                // SAFETY: the successful FREE -> LIVE CAS grants exclusive
                // access; remote producers touch only independent sidecars.
                unsafe { (*slot.heap.get().cast::<HeapCore>()).trim_for_recycle() };
                on_already_initialised(slot.heap.get().cast::<HeapCore>());
            }
            // R11-5: invalidate the per-AllocCore cached NUMA node before
            // handing the slot out, so the new owner's first
            // `current_node_cached()` re-queries rather than inheriting the
            // previous owner's stale value. A no-op on first materialisation
            // (the field starts at `None`); load-bearing on re-claim of a
            // recycled slot. Runs in BOTH the first-materialisation branch
            // above and the re-claim branch just navigated, uniformly, so
            // the invalidation is the single source of truth on (re-)claim.
            // SAFETY: we are the sole writer (FREE→LIVE CAS winner) and the
            // slot is LIVE + initialised at this point, so forming the
            // `&mut HeapCore` for the invalidator is the same shape as
            // `HeapLease::core` below.
            #[cfg(feature = "numa-aware")]
            unsafe {
                (*slot.heap.get().cast::<HeapCore>()).invalidate_numa_node_cache();
            }
            // The typed authority (Ph4a): slot is LIVE + initialised (see the
            // branch above), we are the sole writer. `generation` is a purely
            // numeric observer — no gate on the hot path.
            return Some(HeapLease {
                slot,
                index: idx as u32,
                generation,
                _not_send: PhantomData,
            });
        }
    }

    /// Pick from the hint, then capped high-water bump, then full scan at cap.
    /// An unmaterialised index below `count` remains cold-recoverable.
    pub(super) fn pick_slot() -> Option<usize> {
        let reg = ensure();
        pick_with_saturation(
            &reg.reuse_hint,
            &reg.count,
            &reg.saturation,
            MAX_HEAPS,
            || scan_claimable_slot(reg),
            || super::stack::bump_count(reg),
        )
    }
}

/// Non-copyable authority to mutate a FREE heap for one maintenance pass.
/// Its atomic sidecar is outside the `HeapCore` exclusive-borrow range.
#[doc(hidden)]
pub struct MaintenanceLease {
    index: usize,
    slot: &'static HeapSlot,
    _not_send: PhantomData<*mut ()>,
}

impl MaintenanceLease {
    /// Stable slot index while this lease is held.
    pub fn slot_index(&self) -> usize {
        self.index
    }
    /// Safe core access for one maintenance pass (Ph4c: no longer
    /// `pub unsafe`).
    ///
    /// Why safe without a caller obligation: the old `unsafe` contract had a
    /// single obligation — the callback must not touch producer-reachable
    /// bytes through `&mut`. Producers never touch this `HeapCore` at all:
    /// `publish_foreign` and every terminal publisher (`RouteDirectory::lookup`
    /// → `pin.publish_small`/`pin.publish_large`) write only into
    /// independently allocated sidecars (`SmallSidecar`/`LargeState` behind
    /// the route directory), global atomics, or the fallback heap — never
    /// into the MAINTENANCE slot's `HeapCore` or its reservations (the
    /// slot-resident atomic sidecar, `HeapSlotRemote`, sits outside the
    /// `HeapCore` exclusive-borrow range on its own aligned line). Combined
    /// with the FREE→MAINTENANCE CAS exclusivity (the core's only other
    /// owners are CAS winners whose authority ended at their own
    /// Release publication, paired with our Acquire CAS — S2 of addendum
    /// §2.1), the exclusive `&mut HeapCore` for the callback's duration is
    /// sound in safe code.
    ///
    /// Ph4c (ADR addendum §2.6): crate-only after the surface narrowing;
    /// external tests use the `internals`-gated [`Self::dbg_with_core`]
    /// alias.
    pub(crate) fn with_core<R>(&mut self, f: impl FnOnce(&mut HeapCore) -> R) -> R {
        // SAFETY: the FREE→MAINTENANCE CAS won and `initialised` was
        // Acquire-observed; no live alias survives (proof in the doc comment
        // — remote producers touch only independent sidecars).
        let core = unsafe { &mut *self.slot.heap.get().cast::<HeapCore>() };
        f(core)
    }

    /// Test-only alias of [`Self::with_core`] after the Ph4c narrowing made
    /// it crate-only (ADR addendum §2.6). Gated on `internals` like the
    /// other `dbg_*` observers.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_with_core<R>(&mut self, f: impl FnOnce(&mut HeapCore) -> R) -> R {
        self.with_core(f)
    }
}

impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
        if self
            .slot
            .cas_state(
                STATE_MAINTENANCE,
                STATE_FREE,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_err()
        {
            std::process::abort();
        }
        let reg = ensure();
        reg.reuse_hint.store(self.index as u32, Ordering::Relaxed);
        reg.saturation.publish_claimable();
    }
}

/// Typed, non-copyable owner authority for one heap slot (Ph4a, task #2091;
/// addendum §2.1). Created only after the core is readable — after the
/// `initialised == true` publish or in the re-claim branch — and always as
/// the sole-writer winner of the granting CAS (EMPTY→INITIALIZING→LIVE on
/// first claim, FREE→LIVE on re-claim).
///
/// `!Send + !Sync` (via `PhantomData<*mut ()>`), not `Copy`/`Clone`; no
/// `unsafe impl Send`/`Sync`. `Drop` returns the slot `LIVE → FREE`
/// (Release), aborting on a lost CAS (like `MaintenanceLease::drop`); after
/// the successful CAS it publishes the reuse hint and claimability. The
/// receiving side of any transfer is the winner of a fresh Acquire CAS
/// (claim or maintenance) — S2 of addendum §2.1.
// Ph4a (task #2091): `#[doc(hidden)] pub` like the neighbouring
// `HeapRegistry`/`MaintenanceLease`. (The alternative cfg_attr visibility
// switch does not compile: `cfg_attr` cannot carry a visibility.) The
// `registry` module itself is `pub` only under `internals` (lib.rs) and
// `pub(crate)` otherwise, so production builds without `internals` keep the
// crate-only semantics automatically.
#[doc(hidden)]
pub struct HeapLease {
    slot: &'static HeapSlot,
    index: u32,
    /// Snapshot taken on the successful claim (`fetch_add` result + 1).
    /// Numeric observer only — NEVER a coherence gate on the hot path
    /// (addendum: production reads of `HeapSlot::generation` are none).
    generation: u64,
    _not_send: PhantomData<*mut ()>,
}

impl HeapLease {
    /// Stable slot index while this lease is held (numeric observer).
    /// Ph4a (#2091): `#[doc(hidden)] pub` (same gating as the struct) so
    /// `tests/` probes can read it.
    #[allow(dead_code)] // test/diagnostic observer (Ph4c migrates tests onto it)
    #[doc(hidden)]
    pub fn slot_index(&self) -> u32 {
        self.index
    }

    /// Claim-time generation snapshot (numeric observer only, no gating).
    #[allow(dead_code)]
    #[doc(hidden)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Test/diagnostic (`internals`) access seam — the ONLY core-access seam
    /// (#2091: sole `unsafe` derivation of `&mut HeapCore` from the lease).
    /// Ph4c (#2107): `#[doc(hidden)] pub` under `internals` so `tests/` probes
    /// use this instead of the legacy raw-pointer API. Sound in safe code
    /// because the lease already carries the exclusive authority: the borrow
    /// is tied to `&mut self`, the lease is `!Send`/`!Sync`, and S3's only
    /// raw alias is same-thread TLS, dead before any Drop can overlap.
    /// Not a production API.
    ///
    /// # SAFETY (proof obligations of addendum §2.1)
    ///
    /// - **S1** exclusivity: the slot is `LIVE` and this lease exists only
    ///   because its constructor won the granting CAS (EMPTY→INITIALIZING
    ///   first-claim or FREE→LIVE re-claim); the `&mut HeapCore` owners are
    ///   exactly {the INITIALIZING winner, this lease, `MaintenanceLease`},
    ///   and the previous holder's authority ended at the CAS it lost.
    /// - **S2** happens-before: the previous owner published its writes with
    ///   a Release CAS (lease-Drop LIVE→FREE), which our Acquire
    ///   winning CAS pairs with.
    /// - **S3** no raw-pointer leak: the sole legacy derivative is the same
    ///   thread's TLS cache, poisoned to `TORN` before any Drop
    ///   — `core(&mut self)` borrows exclusively, so no alias survives the
    ///   borrow.
    /// - **S4** slot validity: `&'static HeapSlot` — the registry chunks are
    ///   never freed for the process lifetime, and the lease was formed only
    ///   after `initialised == true` was observed/published.
    #[doc(hidden)]
    pub fn core(&mut self) -> &mut HeapCore {
        // SAFETY: S1–S4 above — LIVE slot, sole writer by the won CAS,
        // happens-before from the previous owner's Release publication,
        // `'static` slot, `initialised` already true.
        unsafe { &mut *self.slot.heap.get().cast::<HeapCore>() }
    }
}

impl Drop for HeapLease {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
        // Release-publish all completed owner writes to the next winning
        // Acquire claim/maintenance CAS (S2). A lost CAS here means the
        // single-writer protocol is broken — abort, like `MaintenanceLease`.
        if self
            .slot
            .cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            std::process::abort();
        }
        // Same post-publication as any LIVE → FREE recycle: advisory hint +
        // claimability.
        let reg = ensure();
        reg.reuse_hint.store(self.index, Ordering::Relaxed);
        reg.saturation.publish_claimable();
    }
}

/// W3: plant a freshly-materialised heap's stable handles to its OWNING
/// slot's diagnostic hit counters (`HeapSlot::tcache_hits` /
/// `HeapSlot::large_cache_hits`). Called by `claim_lease` /
/// `claim_lease_with_config`
/// exactly once, at the slot's first claim (`new_gen == 1`), AFTER
/// `heap_ptr.write(hc)` and BEFORE the `initialised` Release publish.
///
/// This is the keystone of the W3 aliasing fix: the owner increments its hit
/// counters through these `&'static` handles into the SLOT (which is `Sync`,
/// designed to be shared), so the process-wide aggregators
/// (`tcache_hits_total` / `large_cache_hits_total`) can read the SAME
/// `AtomicU64`s directly off the `&HeapSlot` they already hold — WITHOUT ever
/// materialising a shared `&HeapCore`/`&AllocCore` over a struct the owner
/// concurrently holds a protected `&mut` into. The slot lives in the `'static`
/// registry array, so `&slot.<counter>` is a sound `&'static` for the process
/// lifetime.
///
/// # Safety
///
/// `heap` must point at the `HeapCore` just written into `slot`'s `UnsafeCell`
/// by the caller (the FREE→LIVE CAS winner, sole writer); no other thread may
/// hold a reference to it yet (the caller has not published `initialised`). We
/// form a single `&mut HeapCore` for the duration of the bind calls only.
#[cfg_attr(
    not(any(
        all(feature = "alloc-global", feature = "fastbin"),
        feature = "alloc-decommit"
    )),
    allow(unused_variables)
)]
unsafe fn bind_slot_counters(slot: &'static HeapSlot, heap: *mut HeapCore) {
    // SAFETY: caller's contract — `heap` is the just-written, sole-writer,
    // not-yet-published `HeapCore` in `slot`. This exclusive `&mut` is the only
    // live reference to it.
    let heap_ref: &mut HeapCore = unsafe { &mut *heap };
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    heap_ref.bind_tcache_hits(&slot.remote.tcache_hits);
    #[cfg(feature = "alloc-decommit")]
    heap_ref.bind_large_cache_hits(&slot.remote.large_cache_hits);
}

/// Construction failure: release INITIALIZING to FREE. The next claimant
/// checks `initialised` before choosing FREE -> INITIALIZING or LIVE.
pub(super) fn push_back_after_oom(reg: &Registry, slot: &HeapSlot, idx: u32) {
    if slot
        .cas_state(
            STATE_INITIALIZING,
            STATE_FREE,
            Ordering::Release,
            Ordering::Relaxed,
        )
        .is_err()
    {
        std::process::abort();
    }
    reg.reuse_hint.store(idx, Ordering::Relaxed);
    reg.saturation.publish_claimable();
}
