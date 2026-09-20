//! `HeapRegistry`'s claim/recycle API: slot picking + the `FREE → LIVE`
//! claim (plain and config-plumbed), OOM push-back, and the
//! config-conflict rollback guard.
#![allow(unsafe_code)]

use core::sync::atomic::Ordering;

#[cfg(feature = "alloc-decommit")]
use super::counters::CONFIG_CONFLICTS;
use super::stack::{bump_count, pop_free_slot, push_free_slot};
use crate::registry::bootstrap::{ensure, Registry, MAX_HEAPS};
use crate::registry::heap_core::HeapCore;
use crate::registry::heap_slot::{HeapSlot, STATE_FREE, STATE_LIVE};

/// The global heap slot table. All methods operate on the process-global
/// [`Registry`] returned by [`ensure`]; the type itself carries no state (it
/// is a name-space for the API, mirroring how `Node` / `Layout` are organised
/// elsewhere in the crate).
#[doc(hidden)]
pub struct HeapRegistry;

impl HeapRegistry {
    /// Claim a free slot and return a `*mut HeapCore` into it.
    ///
    /// Tries the `free_slots` stack first (a recycled slot); on empty, mints
    /// a fresh slot by bumping `count`. Then CASes the slot `FREE → LIVE`,
    /// bumps its `generation`, and (lazily) materialises the `HeapCore` in
    /// the slot's `UnsafeCell` if this slot has never been materialised
    /// before. Returns `null` if `count` has reached `MAX_HEAPS` AND the free
    /// stack is empty (registry exhaustion — the caller, 12.3, falls back to
    /// the primordial heap), OR if materialisation itself fails (OOM on the
    /// slot's first claim — see the M-5 note below).
    ///
    /// **M-5 (UBFIX-5):** the materialisation gate is
    /// `!slot.initialised.load(Acquire)`, NOT `new_gen == 1`. `generation` is
    /// bumped unconditionally by every successful `FREE → LIVE` CAS,
    /// including a claim that hits this exact slot again after a PRIOR claim
    /// materialised-then-OOM'd on it (see below) — in that scenario `new_gen`
    /// would already be `> 1` on the retry even though the slot's `HeapCore`
    /// was never actually written, and the old `new_gen == 1` gate would skip
    /// materialisation entirely and hand out a pointer to
    /// `MaybeUninit::uninit()` bytes. `initialised` is the correct gate
    /// because it is FALSE for exactly "this slot's `HeapCore` has never been
    /// written", independent of how many times `generation` has been bumped
    /// (see `HeapSlot::initialised`'s doc comment for the full publish
    /// argument).
    ///
    /// **OOM-on-materialisation push-back:** if `HeapCore::new` returns
    /// `None` (the OS refused the segment reservation), the slot has already
    /// been popped off `free_slots` (or freshly minted by `bump_count`) and
    /// CASed to `LIVE` — without pushing it back onto `free_slots`, it would
    /// be LIVE forever, never materialised and never claimable again (a
    /// leaked slot; `MAX_HEAPS` reachable prematurely). The OOM branch CASes
    /// the slot back `LIVE → FREE` and pushes it onto `free_slots` (the exact
    /// shape of a normal [`recycle`](Self::recycle)) before returning `null`,
    /// so a later claim can retry the same slot index once memory pressure
    /// eases.
    #[must_use]
    pub fn claim() -> *mut HeapCore {
        loop {
            let idx = match Self::pick_slot() {
                Some(i) => i,
                None => return core::ptr::null_mut(),
            };
            let reg = ensure();
            // R6-OPT-P0-2: `slot()` resolves the index through the chunked
            // slot array, lazily materialising the owning chunk if needed.
            let slot = reg.slot(idx);

            if slot.cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
                == Err(STATE_LIVE)
            {
                continue; // lost the slot race — retry
            }
            slot.generation.fetch_add(1, Ordering::Release);
            if !slot.initialised.load(Ordering::Acquire) {
                let heap_ptr = slot.heap.get();
                match HeapCore::new(idx as u32) {
                    // SAFETY: sole writer, uninitialised slot, first claim.
                    Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                    None => {
                        // OOM on materialisation: push the slot back to FREE
                        // so it is not leaked (M-5) — same shape as `recycle`.
                        push_back_after_oom(reg, slot, idx as u32);
                        return core::ptr::null_mut();
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
            }
            // R11-5: invalidate the per-AllocCore cached NUMA node before
            // handing the slot out, so the new owner's first
            // `current_node_cached()` re-queries rather than inheriting the
            // previous owner's stale value. A no-op on first materialisation
            // (the field starts at `None`); load-bearing on re-claim of a
            // recycled slot. SAFETY: we are the sole writer (FREE→LIVE CAS
            // winner) and the slot is LIVE + initialised at this point, so
            // forming the `&mut HeapCore` for the invalidator is the same
            // shape as the `return ... .cast::<HeapCore>()` below.
            #[cfg(feature = "numa-aware")]
            unsafe {
                (*slot.heap.get().cast::<HeapCore>()).invalidate_numa_node_cache();
            }
            // SAFETY: slot is LIVE and initialised; we are sole writer.
            return slot.heap.get().cast::<HeapCore>();
        }
    }

    /// Like [`claim`](Self::claim) but plumbs `config` into the newly
    /// materialised `HeapCore` (first materialisation only — gated on
    /// [`HeapSlot::initialised`], see the M-5 note on `claim` for why NOT
    /// `generation == 1`). On re-claim the existing `HeapCore` is reused
    /// as-is; its large-cache config was set at first materialisation and
    /// persists.
    ///
    /// **Config-conflict detection (task #95 / N2):** when a re-claim hits
    /// an already-initialised slot whose live (resolved) policy differs from
    /// `config`, the mismatch is counted in [`CONFIG_CONFLICTS`] (visible via
    /// [`SeferAlloc::stats`](crate::SeferAlloc::stats)'s `config_conflicts`
    /// field) and surfaced with a `debug_assert!` in debug builds. The slot's
    /// existing config silently wins — this is a detect-and-signal fix, not a
    /// reconfigure (reconfigure-with-trim needs old-owner quiescence that
    /// does not cleanly exist for the general case). The counter is the
    /// release-safe signal; the `debug_assert!` is the development-time loud
    /// signal.
    ///
    /// **OOM-on-materialisation push-back:** identical to `claim`'s — see
    /// that method's doc comment for the full rationale. On `HeapCore::new_with_config`
    /// returning `None`, the slot is CASed back to `FREE` and pushed onto
    /// `free_slots` before returning `null`, so it is not leaked.
    ///
    /// Only present under `alloc-decommit`.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn claim_with_config(config: crate::alloc_core::LargeCacheConfig) -> *mut HeapCore {
        loop {
            let idx = match Self::pick_slot() {
                Some(i) => i,
                None => return core::ptr::null_mut(),
            };
            let reg = ensure();
            // R6-OPT-P0-2: `slot()` resolves the index through the chunked
            // slot array, lazily materialising the owning chunk if needed.
            let slot = reg.slot(idx);

            if slot.cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
                == Err(STATE_LIVE)
            {
                continue; // lost the slot race — retry
            }
            slot.generation.fetch_add(1, Ordering::Release);
            if !slot.initialised.load(Ordering::Acquire) {
                let heap_ptr = slot.heap.get();
                // First materialisation: use the caller's config.
                match HeapCore::new_with_config(idx as u32, config) {
                    // SAFETY: sole writer, uninitialised slot, first claim.
                    Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                    None => {
                        // OOM on materialisation: push the slot back to FREE
                        // so it is not leaked (M-5) — same shape as `recycle`.
                        push_back_after_oom(reg, slot, idx as u32);
                        return core::ptr::null_mut();
                    }
                }
                // W3: plant slot-counter handles — see `claim` above and
                // `bind_slot_counters`.
                // SAFETY: identical to `claim` — sole writer, just materialised,
                // not yet published.
                unsafe { bind_slot_counters(slot, heap_ptr.cast::<HeapCore>()) };
                // Publish readiness — see the identical store in `claim` above
                // for the full rationale (task #133 hardening).
                slot.initialised.store(true, Ordering::Release);
            } else {
                // N2 (task #95): re-claim of an already-materialised slot.
                // The slot's existing config (set at first materialisation)
                // silently wins. Compare the requested config against the
                // slot's live policy; on mismatch, count + signal.
                //
                // SAFETY: slot is LIVE and initialised; we are the sole
                // writer (just won the FREE→LIVE CAS). The comparison is a
                // read-only `&self` method on `HeapCore` — no mutation, no
                // hazard.
                let heap_ptr = slot.heap.get().cast::<HeapCore>();
                let matches = unsafe { (*heap_ptr).live_config_matches(&config) };
                if !matches {
                    // Count FIRST (always compiled in — this is a cold path,
                    // one increment per mismatched bind, not a hot-path RMW
                    // worth gating behind `alloc-stats`).
                    CONFIG_CONFLICTS.fetch_add(1, Ordering::Relaxed);
                    // R6-CQ-3 (panic-safety): arm a rollback guard BEFORE the
                    // debug_assert! below. The FREE→LIVE CAS at the top of
                    // this loop iteration already popped this slot off
                    // `free_slots` (or minted it via `count`); the
                    // `debug_assert!` panics in debug builds, and without this
                    // guard the panic would propagate before the `return`
                    // below — leaking the slot as LIVE-but-caller-never-
                    // received-the-pointer (stuck LIVE forever, never
                    // reclaimable). The guard's `Drop` restores the slot to
                    // FREE + `free_slots` (identical to `recycle` /
                    // `push_back_after_oom`) DURING the unwind, BEFORE the
                    // panic crosses this function's frame.
                    let guard = ConflictRollback {
                        reg,
                        slot,
                        idx: idx as u32,
                    };
                    // Development-time loud signal. In release this is
                    // compiled out, leaving the counter as the silent signal.
                    // The counter was already incremented above, so even if
                    // this fires the diagnostic is observable via `stats()`.
                    debug_assert!(
                        matches,
                        "sefer-alloc: config conflict on recycled heap slot {} — \
                         the slot's existing config silently overrides the \
                         requested one (check SeferAlloc::stats().config_conflicts)",
                        idx
                    );
                    // Reached only in release (the assert is compiled out) —
                    // forget the guard so its `Drop` does NOT restore the
                    // slot, leaving it LIVE for the `return` below as
                    // intended. On the debug-build panic path this line is
                    // never reached, so the guard drops during unwind and
                    // performs the rollback.
                    core::mem::forget(guard);
                }
            }
            // R11-5: same NUMA-cache invalidation as `claim` — runs in BOTH
            // the first-materialisation branch above AND the re-claim branch
            // just navigated, uniformly, so the invalidation is the single
            // source of truth on (re-)claim. (On first materialisation the
            // field is already `None`, making this a no-op there.) SAFETY:
            // sole writer (FREE→LIVE CAS winner), slot LIVE + initialised.
            #[cfg(feature = "numa-aware")]
            unsafe {
                (*slot.heap.get().cast::<HeapCore>()).invalidate_numa_node_cache();
            }
            // SAFETY: slot is LIVE and initialised; we are sole writer.
            return slot.heap.get().cast::<HeapCore>();
        }
    }

    /// Pick a candidate slot index: pop from `free_slots` (recycled slot)
    /// or mint a fresh one by bumping `count`. Returns `None` on registry
    /// exhaustion (`count >= MAX_HEAPS` AND free stack empty).
    pub(super) fn pick_slot() -> Option<usize> {
        let reg = ensure();
        pop_free_slot(reg).or_else(|| bump_count(reg))
    }

    /// Recycle a live slot back to the free pool. Called by the owning
    /// thread (the LIVE-state holder) when it no longer needs the heap
    /// (typically on thread exit — Phase 12.5 whole-heap reuse: the `HeapCore`
    /// stays whole in the slot for the next claimer; nothing is abandoned).
    ///
    /// `heap` MUST be a pointer previously returned by [`claim`](Self::claim)
    /// and not yet recycled. Double-recycle is a no-op (defensive): the CAS
    /// LIVE→FREE fails on an already-FREE slot and we return without pushing.
    ///
    /// # Safety
    ///
    /// `heap` must be either null (treated as a no-op) or a pointer
    /// previously returned by [`claim`](Self::claim) and not yet passed to
    /// `recycle` (the slot must still be `LIVE`). Passing any other pointer
    /// is undefined behaviour (the registry reads `heap.id()` to find the
    /// owning slot, and an out-of-range id would index the slot array
    /// unsafely — the registry guards against this with a range check, but
    /// a dangling pointer may still fault on the read).
    pub unsafe fn recycle(heap: *mut HeapCore) {
        if heap.is_null() {
            return;
        }
        let reg = ensure();
        // SAFETY: caller guarantees `heap` was returned by `claim`, which
        // derived it from a slot at index `heap.id()` in the `'static` slot
        // array. The slot index is in range by construction (we re-check
        // below before indexing).
        let idx = unsafe { (*heap).id() } as usize;
        if idx >= MAX_HEAPS {
            return;
        }
        // R6-OPT-P0-2: `idx < MAX_HEAPS`, checked above; `slot()` resolves it
        // through the chunked slot array (the chunk is already materialised —
        // this index was returned by a prior `claim`, which touched it).
        let slot = reg.slot(idx);

        // CAS LIVE → FREE. This is a state transition only: its Release
        // orders this owner's writes sequenced-before it (the heap contents
        // from the slot's LIVE lifetime) so a later Acquire observation of
        // `state` as FREE sees them — it cannot publish the `next_free`
        // link, which is stored only afterwards, inside `push_free_slot`.
        // Relaxed on failure: the slot was not LIVE (double-recycle or
        // raced); we no-op.
        if slot.cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed)
            == Err(STATE_FREE)
        {
            // Already FREE — defensive no-op (do not push a free slot twice,
            // which would corrupt the stack).
            return;
        }

        // Push the slot onto the free_slots stack (tagged-Treiber). The push
        // stores the `next_free` link and then Release-CASes the stack head,
        // which is what publishes the link; a later claim's Acquire pop of
        // that head observes it, making this slot available for that claim.
        push_free_slot(reg, idx as u32);
    }
}

/// W3: plant a freshly-materialised heap's stable handles to its OWNING
/// slot's diagnostic hit counters (`HeapSlot::tcache_hits` /
/// `HeapSlot::large_cache_hits`). Called by `claim` / `claim_with_config`
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
        feature = "alloc-decommit",
        feature = "alloc-xthread"
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
    // task H1: plant the stable `&'static` handle to this slot's cross-thread
    // free-stack head (moved out of `HeapCore` into the `Sync` slot — see
    // `HeapSlotRemote::thread_free` / `HeapCore::thread_free`). This is what
    // makes the remote CAS target the slot word (outside every `&mut
    // HeapCore` retag range) instead of an inline `HeapCore` field.
    // PERF-PASS-4 (G8/ML2, task #52): the field moved into the
    // `remote: HeapSlotRemote` sub-struct; the address handed out here is
    // unaffected (a field reference's address is stable regardless of
    // nesting) — same stable `'static` address, just now on its own
    // 64-byte-aligned cache line.
    #[cfg(feature = "alloc-xthread")]
    heap_ref.bind_thread_free(&slot.remote.thread_free);
    // RAD-4b (task #72): plant the stable `&'static` handle to this slot's
    // second-chance overflow ring. `overflow` (unlike `remote`'s grouped
    // fields) lives directly on `HeapSlot`, not inside `HeapSlotRemote` — see
    // that field's doc comment in `heap_slot.rs`. Same claim-time-binding
    // discipline as `bind_thread_free`/`bind_tcache_hits` above.
    #[cfg(feature = "alloc-xthread")]
    heap_ref.bind_overflow(&slot.overflow);
    // R7-A4: plant the stable `&'static` handle to this slot's per-slot
    // dirty-segment bitmap. Same claim-time-binding discipline as
    // `bind_overflow` above. The dirty bitmap lives in `HeapSlotRemote`
    // (cross-thread-reachable, process-`'static`).
    #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
    heap_ref.bind_dirty_segments(&slot.remote.dirty_segments);
    // R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): plant the stable
    // `&'static` handle to this slot's per-(segment, class) dirty-bit
    // sidecar cell. Same claim-time-binding discipline as
    // `bind_dirty_segments` above.
    #[cfg(feature = "class-aware-dirty")]
    heap_ref.bind_dirty_by_class(&slot.remote.dirty_by_class);
    // R13-1 (task #271, P0 fix): plant the stable `&'static` handle to this
    // slot's coarse-only latch. Same claim-time-binding discipline as
    // `bind_dirty_by_class` above.
    #[cfg(feature = "class-aware-dirty")]
    heap_ref.bind_sidecar_oom_latch(&slot.remote.sidecar_oom_latch);
}

/// M-5 (UBFIX-5): push a slot back onto `free_slots` after its `HeapCore`
/// materialisation failed (OOM). Called from `claim`/`claim_with_config`
/// ONLY on the `HeapCore::new`/`new_with_config` `None` branch — at that
/// point the slot is `LIVE` (the caller already won the `FREE → LIVE` CAS)
/// but `heap` was never written and `initialised` was never published, so
/// this is NOT the general [`HeapRegistry::recycle`](HeapRegistry::recycle)
/// path (which requires a valid `*mut HeapCore` derived from a completed
/// claim) — it is the OOM-specific mirror of it, working directly off the
/// slot reference and index the caller already has in hand.
///
/// CASes the slot `LIVE → FREE` (mirrors `recycle`'s CAS — Release on
/// success, ordering this caller's writes for a later Acquire read of
/// `state`; the CAS is a state transition only and does NOT publish the
/// `next_free` link) then pushes it onto `free_slots`, exactly as `recycle`
/// does — the link is published by the push's own Release CAS on the
/// `free_slots` stack head, which a later claim's Acquire pop observes.
/// Without this push-back the slot
/// would stay `LIVE` forever: never materialised (so every future `claim`
/// hitting the initialisation branch on the SAME index would see
/// `initialised == false` and retry `HeapCore::new`, which is a correctness
/// non-issue) but also never reachable via `pick_slot` again (`free_slots`
/// never gets it back and `count` already counted it) — a genuine slot leak
/// under sustained memory pressure, tightening the effective `MAX_HEAPS`
/// cap with every transient OOM.
///
/// The CAS is expected to always succeed: the caller is the slot's sole
/// writer since winning the `FREE → LIVE` CAS in `claim`/`claim_with_config`,
/// and no other path can observe this slot as `LIVE` and race a state
/// transition on it before the caller itself either finishes materialising
/// or calls this function. We still use a CAS (not a plain store) to mirror
/// `recycle`'s defensive shape and keep `state`'s only mutator discipline
/// uniform across the module.
pub(super) fn push_back_after_oom(reg: &Registry, slot: &HeapSlot, idx: u32) {
    // Run-5 audit P4-3: the "expected to always succeed" contract documented
    // above is a debug-asserted invariant, not a discarded value — a future
    // caller reaching this function with an already-FREE slot would
    // otherwise silently double-push `idx` onto `free_slots` (compare
    // `recycle`'s explicit already-FREE no-op). Release behaviour is
    // unchanged: the push stays unconditional.
    let cas = slot.cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed);
    debug_assert!(
        cas.is_ok(),
        "push_back_after_oom: slot was not LIVE; pushing it again would double-push"
    );
    push_free_slot(reg, idx);
}

/// R6-CQ-3 (panic-safety): RAII rollback guard armed in the config-conflict
/// branch of [`HeapRegistry::claim_with_config`]. When dropped, it performs
/// the SAME `LIVE → FREE` CAS + `free_slots` push as
/// [`push_back_after_oom`] / [`HeapRegistry::recycle`], restoring the slot
/// to the free pool.
///
/// **Why this guard exists:** by the time `claim_with_config` reaches the
/// config-mismatch branch it has already won the `FREE → LIVE` CAS (popping
/// the slot off `free_slots`, or minting it via `count`). The mismatch is
/// signalled with a `debug_assert!`, which PANICS in debug builds. Without a
/// rollback that panic propagates before the function's `return`, so the
/// caller never receives the `*mut HeapCore`, can never call `recycle` on
/// it, and the slot is stuck `LIVE` forever — a genuine leak that shrinks
/// the reachable `MAX_HEAPS` pool by one per conflict. Arming this guard
/// around the `debug_assert!` means the slot is restored to `FREE` +
/// `free_slots` DURING the unwind, BEFORE the panic crosses the function
/// boundary, so the leak cannot occur regardless of whether the caller
/// catches the panic.
///
/// **Disarm (non-panic path):** the owning code calls [`core::mem::forget`]
/// on the guard once the `debug_assert!` has returned without panicking
/// (release builds, where the assert is compiled out). `forget` suppresses
/// `Drop`, so a normal `return` leaves the slot `LIVE` for the caller as
/// intended. On the panic path `forget` is never reached and `Drop` runs
/// during the unwind — the guard is therefore "armed iff not yet forgotten".
///
/// **Panic-safety of `Drop` itself:** `Drop` only runs a CAS and
/// `push_free_slot` (both atomic; neither panics in release — the P4-3
/// `debug_assert!` inside `push_back_after_oom` can panic in debug builds,
/// but only on the already-violated sole-writer invariant, where aborting is
/// the acceptable outcome), and the `&'static`
/// slot/registry references it holds remain valid for the entire unwind, so
/// there is no double-panic/abort risk and no `catch_unwind` /
/// `AssertUnwindSafe` is needed at the source level — the guard drops during
/// natural unwinding. (Under `panic = "abort"` the `debug_assert!` aborts the
/// process and the guard never runs, but then there is no leak either: the
/// process is gone.)
// Constructed only inside `claim_with_config`'s config-mismatch branch,
// itself `#[cfg(feature = "alloc-decommit")]`-gated (that method is "only
// present under `alloc-decommit`" — see its own doc comment above). A build
// without `alloc-decommit` (e.g. `hardened medium-classes`) would otherwise
// leave this struct never constructed (R23-5, task #374).
#[cfg(feature = "alloc-decommit")]
struct ConflictRollback {
    reg: &'static Registry,
    slot: &'static HeapSlot,
    idx: u32,
}

#[cfg(feature = "alloc-decommit")]
impl Drop for ConflictRollback {
    fn drop(&mut self) {
        push_back_after_oom(self.reg, self.slot, self.idx);
    }
}
