//! [`HeapRegistry`] — the global self-hosting heap slot table (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): claim/recycle over the process-global
//! [`Registry`](super::bootstrap::Registry) slot array.
//!
//! This is the lock-free fundament of Phase 12: every thread's heap is a SLOT
//! in this registry, not a TLS-owned `Box`. A thread claims a slot on first
//! use, caches a raw `*mut HeapCore` to it in TLS (12.3), and on thread exit
//! recycles the slot (whole-heap reuse — Phase 12.5: the `HeapCore` and its
//! segments stay with the slot, and the next claimer reuses them in full).
//! (The abandoned-segments / adoption substrate that previously lived here was
//! removed — task #97 / R4-5; see the "ABA defence" note below.)
//!
//! ## Phase 12.2 scope
//!
//! This file ships the structure + the claim/recycle API, exercised
//! single-threaded by `tests/registry_basic.rs`. The orderings are written
//! CORRECT for the lock-free concurrent case from day one (loom verification
//! is Phase 12.4); each atomic op carries a `// why:` comment.
//!
//! ## ABA defence
//!
//! The [`Registry::free_slots`] Treiber stack carries a monotonic tag in the
//! high bits of its `AtomicU64` head (48 bits — the low 16 hold the slot
//! index, task W7a), bumped on every push. This defeats the classic ABA
//! (pop-X, re-push-X while a racer is parked with head=X): the re-push bumps
//! the tag, so the racer's CAS on `(X, old_tag)` fails. See
//! `super::tagged_ptr::TaggedPtr` for the tag-width-vs-churn analysis.
//!
//! (The abandoned-segments intrusive Treiber stack that previously also lived
//! here was removed — task #97 / R4-5. It was unreachable on the production
//! whole-slot-reuse path and internally inconsistent; git history preserves
//! it. The `deferred_next` header field it shared with the
//! `deferred_large` cross-thread-free stack REMAINS — that stack is a
//! separate, live feature and is untouched by this removal.)

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented registry seam (the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`). R6-OPT-P0-2 (round 1): the
// former `get_unchecked` on a `'static` inline slot array is gone — every
// slot-array access now goes through `Registry::slot(idx)`
// (`bootstrap.rs`), the single chunk-resolving accessor, which is safe
// (range-checked via `debug_assert!` and array-index, not `get_unchecked`).
// `allow` lifts the crate-level `deny` for this file only — `unsafe`
// anywhere else in the crate is a hard error. Every remaining `unsafe` block
// carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use super::bootstrap::{ensure, Registry, MAX_HEAPS};
use super::heap_core::HeapCore;
use super::heap_slot::{HeapSlot, NEXT_FREE_TAIL, STATE_FREE, STATE_LIVE};
use super::tagged_ptr::TaggedPtr;

/// DIAGNOSTIC (task #95 / N2): process-wide count of config-conflict events
/// — times `claim_with_config` found an already-materialised slot whose live
/// (resolved) cache/pool policy differs from the requested config. Each such
/// event means the slot's pre-existing config silently overrides the caller's
/// request (first-materialisation-wins semantics; see
/// `SeferAlloc::with_config`'s doc).
///
/// This is a **cold-path counter** (incremented at most once per thread bind,
/// never on the alloc/dealloc hot path), so — unlike the `alloc-stats`-gated
/// hot-path counters — the increment is ALWAYS compiled in (not gated behind
/// `alloc-stats`). Reads `0` in a build without `alloc-decommit` (where
/// `claim_with_config` does not exist).
///
/// Relaxed ordering — diagnostic only, no synchronization obligation.
static CONFIG_CONFLICTS: AtomicU64 = AtomicU64::new(0);

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
            // SAFETY: slot is LIVE and initialised; we are sole writer.
            return slot.heap.get().cast::<HeapCore>();
        }
    }

    /// Pick a candidate slot index: pop from `free_slots` (recycled slot)
    /// or mint a fresh one by bumping `count`. Returns `None` on registry
    /// exhaustion (`count >= MAX_HEAPS` AND free stack empty).
    fn pick_slot() -> Option<usize> {
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

        // CAS LIVE → FREE. Release on success: a later claim's Acquire load
        // of `state` (via its CAS) sees the slot's recycled state and the
        // `next_free` link we are about to push. Relaxed on failure: the slot
        // was not LIVE (double-recycle or raced); we no-op.
        if slot.cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed)
            == Err(STATE_FREE)
        {
            // Already FREE — defensive no-op (do not push a free slot twice,
            // which would corrupt the stack).
            return;
        }

        // Push the slot onto the free_slots stack (tagged-Treiber). The push
        // establishes this slot as available for a future claim.
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
/// success so a later claim's Acquire load of `state` sees the slot's freed
/// state and the `next_free` link this function pushes) then pushes it onto
/// `free_slots`, exactly as `recycle` does. Without this push-back the slot
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
fn push_back_after_oom(reg: &Registry, slot: &HeapSlot, idx: u32) {
    let _ = slot.cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed);
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
/// `push_free_slot` (both atomic; neither panics), and the `&'static`
/// slot/registry references it holds remain valid for the entire unwind, so
/// there is no double-panic/abort risk and no `catch_unwind` /
/// `AssertUnwindSafe` is needed at the source level — the guard drops during
/// natural unwinding. (Under `panic = "abort"` the `debug_assert!` aborts the
/// process and the guard never runs, but then there is no leak either: the
/// process is gone.)
struct ConflictRollback {
    reg: &'static Registry,
    slot: &'static HeapSlot,
    idx: u32,
}

impl Drop for ConflictRollback {
    fn drop(&mut self) {
        push_back_after_oom(self.reg, self.slot, self.idx);
    }
}

// ---------------------------------------------------------------------------
// Treiber-stack primitives on the `free_slots` stack. Module-private.
// ---------------------------------------------------------------------------

/// Pop a free slot index off the `free_slots` stack (classic Treiber pop),
/// or `None` if empty.
///
/// This is the textbook Treiber pop: load the (tagged) head, read its
/// `next_free` link, then CAS the head to that next link. The tag in the high
/// 48 bits defeats the ABA problem: if between our load and our CAS another
/// thread pops the head and re-pushes the SAME slot index, the tag will have
/// advanced, our CAS fails, and we retry — never observing a stale chain.
///
/// **Ordering:** `Acquire` on the success CAS so we see the `next_free` link
/// the pusher wrote under `Release`; `Relaxed` on failure (retry, no
/// side-effect).
fn pop_free_slot(reg: &Registry) -> Option<usize> {
    let mut head = reg.free_slots.load(Ordering::Acquire);
    loop {
        if TaggedPtr::is_empty(head) {
            return None;
        }
        let (idx_v, _tag) = TaggedPtr::unpack(head);
        let idx = idx_v as u32;
        if idx as usize >= MAX_HEAPS {
            // Defensive: cannot happen by construction (push only stores
            // valid indices).
            return None;
        }
        // R6-OPT-P0-2: `idx < MAX_HEAPS`, checked above; `slot()` resolves it
        // through the chunked slot array (materialising the owning chunk if
        // this is the first touch of an index in that range — see `slot()`'s
        // doc comment: every slot-array access in the crate funnels through
        // it).
        let slot: &HeapSlot = reg.slot(idx as usize);
        // Read the next link BEFORE the CAS (the push stored it under
        // Release; our Acquire load of `head` + this Acquire read see it).
        let next = slot.next_free.load(Ordering::Acquire);
        // The new head is `next` (or the empty sentinel if
        // `next == NEXT_FREE_TAIL`) with the SAME tag we observed — the tag
        // is bumped only on PUSH, so a pop preserves it. A concurrent
        // re-push of `idx` will bump the tag and fail our CAS (the ABA fix).
        //
        // H-2 FIX: on the empty transition we must NOT reset the tag to 0
        // (`TaggedPtr::empty()` hardcodes tag=0). If we did, a parked racer
        // holding a stale `(idx, tag)` snapshot from BEFORE the stack emptied
        // could see the tag sequence restart from 0 on the next push and
        // spuriously match its stale tag, letting a stale CAS succeed onto a
        // freshly-repushed-but-different chain — the exact ABA corruption the
        // tag exists to prevent. Instead we pack the empty sentinel's index
        // half (`INDEX_MASK`) with the RUNNING tag we just read (`_tag`, the
        // tag observed on the head we are popping off) — `TaggedPtr::is_empty`
        // only inspects the index half, so this is still unambiguously empty,
        // but the tag keeps counting forward across the empty transition
        // exactly as it would across any other pop. `push_free_slot` already
        // reads the tag out of the current head (empty or not) and bumps it,
        // so this composes correctly with no other change needed.
        let new_head = if next == NEXT_FREE_TAIL {
            TaggedPtr::pack(TaggedPtr::empty_index(), _tag)
        } else {
            TaggedPtr::pack(next as u64, _tag)
        };
        // CAS the head to `new_head`. Acquire on success (see the push's
        // Release store of `next_free`); Relaxed on failure (retry).
        match reg
            .free_slots
            .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => return Some(idx as usize),
            Err(actual) => head = actual, // retry with the new head
        }
    }
}

/// Push a slot index onto the `free_slots` stack. Sets the slot's `next_free`
/// link first (so a later pop can restore the chain), then CASes the head.
fn push_free_slot(reg: &Registry, idx: u32) {
    // R6-OPT-P0-2: `idx < MAX_HEAPS` (the caller — recycle — derived it from
    // a valid heap pointer); `slot()` resolves it through the chunked slot
    // array (the chunk is already materialised at this point in every call
    // path — see the call sites of `push_free_slot`).
    let slot: &HeapSlot = reg.slot(idx as usize);
    let mut head = reg.free_slots.load(Ordering::Acquire);
    loop {
        // The next link this slot will chain to: the current head's index,
        // or NEXT_FREE_TAIL if the stack is empty (so a later pop sees the
        // tail sentinel and knows the chain ends here). Note: the empty
        // sentinel packs `INDEX_MASK` in the low bits, which numerically
        // equals `NEXT_FREE_TAIL` (`u32::MAX`), but we spell the empty→tail
        // mapping out explicitly so the invariant does not rest on the
        // accidental value coincidence.
        let next_link = if TaggedPtr::is_empty(head) {
            NEXT_FREE_TAIL
        } else {
            let (cur_idx, _tag) = TaggedPtr::unpack(head);
            cur_idx as u32
        };
        // Write the link under Release so a concurrent pop's Acquire read of
        // `next_free` (after observing this slot as the head) sees it.
        slot.next_free.store(next_link, Ordering::Release);
        // Advance the tag (the ABA fix) and CAS the head to this slot.
        let (_cur_idx, tag) = TaggedPtr::unpack(head);
        let new_tag = tag.wrapping_add(1);
        let new_head = TaggedPtr::pack(idx as u64, new_tag);
        // CAS: Release on success so a pop's Acquire sees the `next_free`
        // link we just wrote. Relaxed on failure (retry).
        match reg
            .free_slots
            .compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(actual) => head = actual,
        }
    }
}

/// Mint a fresh slot by bumping `count`. Returns the new slot's index, or
/// `None` if `count` has reached `MAX_HEAPS`. The new slot is already in its
/// bootstrap state (`FREE`, generation 0, heap uninitialised) thanks to the
/// `const` initialiser; no extra init is needed.
fn bump_count(reg: &Registry) -> Option<usize> {
    // fetch_add is RMW: AcqRel so we see any prior slot writes (none needed
    // here, but conservative) and later claimers see our bump.
    let idx = reg.count.fetch_add(1, Ordering::AcqRel);
    if idx as usize >= MAX_HEAPS {
        // Roll back the bump (best-effort) and report exhaustion. Under
        // concurrency a rollback race is benign (the cap is a soft bound; an
        // over-bump just wastes an index slot).
        reg.count.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    Some(idx as usize)
}

/// DIAGNOSTIC (task E1): the high-water mark of minted registry slots — the
/// number of distinct heap slots ever claimed (via `bump_count`) since
/// process start. This is a **high-water mark, not a live count**: a slot
/// that was claimed and later recycled is still counted here (recycled slots
/// are reused, not un-minted). Relaxed load of [`Registry::count`].
/// `#[doc(hidden)]` — diagnostic-only surface, not part of the crate's
/// supported public API; reached via `SeferAlloc::stats()`.
#[doc(hidden)]
#[must_use]
pub fn heaps_claimed_high_water() -> u32 {
    ensure().count.load(Ordering::Relaxed)
}

/// DIAGNOSTIC (task #95 / N2): process-wide count of config-conflict events
/// — times `claim_with_config` found an already-materialised slot whose live
/// (resolved) cache/pool policy differs from the requested config. Backs
/// [`AllocStats::config_conflicts`](crate::AllocStats::config_conflicts).
/// See [`CONFIG_CONFLICTS`] for the full rationale. A plain relaxed atomic
/// load — diagnostic only, no ordering obligation. Reads `0` in a build
/// without `alloc-decommit` (where `claim_with_config` does not exist).
#[doc(hidden)]
#[must_use]
pub fn config_conflicts_total() -> u64 {
    CONFIG_CONFLICTS.load(Ordering::Relaxed)
}

/// DIAGNOSTIC (task #133 → W3): process-wide magazine (tcache) hit total —
/// aggregated across every slot ever minted, summing each slot's own
/// [`HeapSlot::tcache_hits`] (moved there from `HeapCore` in W3 to close a
/// Stacked-Borrows aliasing gap — the aggregator no longer materialises any
/// `&HeapCore`). Replaces the pre-#133 single global `static`
/// counter (`DBG_TCACHE_HITS`), which was bumped by every thread's alloc
/// fast path and therefore a contended `lock xadd` on an otherwise
/// per-thread hot path (the regression this function's introduction fixes —
/// see the doc comment on [`HeapCore`]'s `tcache_hits` field).
///
/// ## Soundness of reading a foreign slot's counter
///
/// This walks slot indices `0..count` (the high-water mark of minted
/// slots — [`heaps_claimed_high_water`]) and, for each, performs a Relaxed
/// load of that slot's `HeapCore::tcache_hits` — but ONLY after first
/// checking [`HeapSlot::initialised`] with an `Acquire` load.
///
/// **This gate is load-bearing, not defensive.** `count` (bumped by
/// `bump_count`, called from `pick_slot` BEFORE the slot's `FREE → LIVE`
/// CAS) and `generation` (bumped to 1 by `claim` BEFORE `HeapCore::new()`
/// runs, which reserves an OS segment — not fast) are BOTH insufficient:
/// a slot index can be `< count` — and even have `generation == 1` — while
/// `HeapCore::new()` is still executing on the claiming thread and
/// `heap_ptr.write(hc)` has not yet run. `heap`'s storage is still
/// `MaybeUninit::uninit()` bytes at that point. Reading it from THIS
/// function (a different thread, e.g. via `SeferAlloc::stats()` called
/// concurrently with another thread's first-ever `claim`) would be a read
/// of uninitialised memory racing a concurrent non-atomic
/// `MaybeUninit::write` — undefined behaviour, not merely a stale value.
/// (This was a real defect caught in zero-trust review of the initial
/// #133 patch — see `HeapSlot::initialised`'s doc comment for the full
/// writeup, and `tests/regression_registry_initialised_gate.rs` for the
/// regression coverage.)
///
/// The fix: [`HeapRegistry::claim`] (and `claim_with_config`) Release-store
/// `true` into `HeapSlot::initialised` ONLY after `heap_ptr.write(hc)` has
/// fully completed. This function's Acquire load of `initialised`, when it
/// observes `true`, is guaranteed by the C++/Rust memory model to
/// happens-after that Release store — which is itself sequenced-after the
/// `write(hc)` on the same (claiming) thread — so observing `true` here
/// establishes happens-before from the write of `hc` into the
/// `UnsafeCell` to this function's subsequent dereference of `heap_ptr`.
/// That is the standard "publish a fully-constructed value via a
/// Release-store flag" pattern, and it is what makes the dereference below
/// sound. A slot observed with `initialised == false` is skipped entirely:
/// it has never been claimed (or is mid-claim), so it has never
/// incremented `tcache_hits` either — contributing 0 to the sum is correct,
/// not merely safe.
///
/// Once `initialised` is `true` it stays `true` for the process lifetime of
/// the slot (per the slot-reuse discipline documented on [`HeapSlot::heap`]
/// — a minted `HeapCore` is reused as-is across `recycle`/re-`claim`
/// cycles, never dropped or reset), so a slot that was `true` on a prior
/// observation can only still be `true` (or `true` with a newer,
/// larger-or-equal counter value) on a later one — no ABA hazard on this
/// flag itself.
///
/// No `unsafe` beyond what this module's header comment already documents
/// as its seam: `MaybeUninit::assume_init_ref` would be new `unsafe`, but
/// we avoid it entirely by going through the same raw-pointer path `claim`
/// already uses (`heap.get().cast::<HeapCore>()`).
///
/// Only present under `alloc-global + fastbin` (mirrors
/// `HeapCore::tcache_hits`'s cfg-gate). The per-slot WALK it performs is
/// additionally gated on `alloc-stats` (R3-A, round3 finding N1): the
/// counter it sums is only ever incremented under `alloc-stats`, which is
/// NOT part of `production`, so without `alloc-stats` the walk would sum
/// compile-time zeros — it is compiled out (returns 0 with no loop) to keep
/// `stats()` O(1) on a metrics-scrape hot path as its doc promises.
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
#[must_use]
pub fn tcache_hits_total() -> u64 {
    // R3-A (round3 N1): the per-slot counter this aggregates is only ever
    // incremented under `alloc-stats` (`heap_core.rs`'s W3 bump), which is NOT
    // part of `production`. Without it every slot's counter is compile-time 0,
    // so the walk below would sum zeros — yet it still read up to `count` slots
    // on every `stats()` call, contradicting `stats()`'s "no heap walk" doc.
    // Gate the WALK on `alloc-stats` to match the gate on the increment: with
    // it off, return 0 at compile time with NO loop; with it on, the existing
    // per-slot walk is UNCHANGED (W3's per-slot-counter design is deliberate —
    // it avoids a hot-path-contended global counter; see the W3 comment in
    // `heap_core.rs`).
    #[cfg(feature = "alloc-stats")]
    {
        let reg = ensure();
        let count = reg.count.load(Ordering::Acquire) as usize;
        let mut total: u64 = 0;
        for idx in 0..count.min(MAX_HEAPS) {
            // R6-OPT-P0-2: `idx < count <= MAX_HEAPS`. `slot()` transparently
            // materialises (or finds already-materialised) exactly the
            // chunks this `count`-bounded walk touches — no special-casing
            // needed: every index in `0..count` was, by construction, either
            // freshly minted by `bump_count` or popped off `free_slots`, both
            // of which already call `slot()` on it, so its owning chunk is
            // ALREADY materialised by the time this walk reaches it. The call
            // here is therefore always a fast-path hit (one Acquire load, two
            // comparisons), never a fresh chunk materialisation — but it is
            // still correct even in the hypothetical where it wasn't, because
            // `slot()` unconditionally guarantees the chunk exists before
            // returning.
            let slot = reg.slot(idx);
            // The `initialised` gate (task #133): keep it for the documented
            // ordering. With the W3 move it is no longer load-bearing for SAFETY
            // (the counter lives in the slot itself — an un-bound slot's
            // `tcache_hits` is a zero `AtomicU64`, sound to read and contributing
            // 0), but a mid-mint slot must still not be summed, and the Acquire
            // here pairs with `claim`'s Release publish for that ordering.
            if !slot.initialised.load(Ordering::Acquire) {
                continue;
            }
            // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
            // `(*heap_ptr).…` deref, so NO shared `&HeapCore` is ever materialised
            // over a struct the owning thread concurrently holds a protected `&mut`
            // into. This closes the Stacked-Borrows aliasing gap the old
            // `(*heap_ptr).tcache_hits()` read had. Relaxed load of a shared
            // `Sync` atomic — sound from any thread; observes the owner's
            // monotonic single-writer increments.
            total = total.saturating_add(slot.remote.tcache_hits.load(Ordering::Relaxed));
        }
        total
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        0
    }
}

/// DIAGNOSTIC (task #133): process-wide large-cache hit total — aggregated
/// across every slot ever minted, summing each slot's
/// `AllocCore::dbg_large_cache_hits()`. Replaces the pre-#133 single global
/// `static` counter (`LARGE_CACHE_HITS` in `alloc_core.rs`), which was
/// bumped by every heap's `alloc_large` cache-hit path and therefore a
/// contended `lock xadd` on an otherwise per-heap hot path.
///
/// Soundness of walking foreign slots and reading their `AllocCore` field
/// cross-thread: identical argument to [`tcache_hits_total`] above,
/// including the SAME load-bearing gate — `idx < count` alone does NOT
/// imply the slot's `HeapCore` is materialised (see that function's doc
/// comment for the exact UB window this closes: `count` is bumped by
/// `bump_count` before the claiming thread even starts `HeapCore::new()`).
/// This function gates every slot on an `Acquire` load of
/// [`HeapSlot::initialised`] before dereferencing `heap`, pairing with the
/// `Release` store `claim`/`claim_with_config` perform immediately after
/// `heap_ptr.write(hc)` completes — establishing happens-before to the
/// write. A slot observed `initialised == false` is skipped (never
/// claimed, or mid-claim — either way it has never incremented
/// `large_cache_hits`, so contributing 0 is correct).
///
/// Only present under `alloc-decommit` (mirrors
/// `AllocCore::dbg_large_cache_hits`'s cfg-gate). Like
/// [`tcache_hits_total`], the per-slot WALK is additionally gated on
/// `alloc-stats` (R3-A, round3 finding N1): the counter it sums is only ever
/// incremented under `alloc-stats`, which is NOT part of `production`, so
/// without `alloc-stats` the walk is compiled out (returns 0 with no loop).
#[cfg(feature = "alloc-decommit")]
#[doc(hidden)]
#[must_use]
pub fn large_cache_hits_total() -> u64 {
    // R3-A (round3 N1): see `tcache_hits_total` for the full rationale — the
    // same increment/walk gate mismatch applies here. Without `alloc-stats` the
    // per-slot counter is compile-time 0, so the walk is compiled out to keep
    // `stats()` O(1). With `alloc-stats` the existing per-slot walk runs
    // UNCHANGED.
    #[cfg(feature = "alloc-stats")]
    {
        let reg = ensure();
        let count = reg.count.load(Ordering::Acquire) as usize;
        let mut total: u64 = 0;
        for idx in 0..count.min(MAX_HEAPS) {
            // R6-OPT-P0-2: `idx < count <= MAX_HEAPS`. See the identical
            // reasoning in `tcache_hits_total` above — every index in
            // `0..count` was minted/popped through `slot()` already, so its
            // chunk is already materialised; `slot()` is still the correct
            // (and only sanctioned) way to resolve it.
            let slot = reg.slot(idx);
            // The `initialised` gate — see `tcache_hits_total`'s (identical
            // rationale): kept for the documented ordering, no longer load-bearing
            // for safety after the W3 move (the counter is in the slot itself).
            if !slot.initialised.load(Ordering::Acquire) {
                continue;
            }
            // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
            // `(*heap_ptr).core.…` deref, so NO shared `&HeapCore`/`&AllocCore` is
            // ever materialised over a struct the owning thread concurrently holds
            // a protected `&mut` into. This closes the Stacked-Borrows aliasing gap
            // the old `(*heap_ptr).core.dbg_large_cache_hits()` read had. Relaxed
            // load of a shared `Sync` atomic — sound from any thread.
            total = total.saturating_add(slot.remote.large_cache_hits.load(Ordering::Relaxed));
        }
        total
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        0
    }
}

// ---------------------------------------------------------------------------
// UBFIX-5 test-only hooks (M-5 / L-9a regression coverage).
//
// There is no test-only way to force `HeapCore::new`/`new_with_config` to
// return `None` (a real OS reservation refusal) without touching
// `alloc_core.rs` (out of this task's scope — see the task's isolation
// note). These hooks instead reproduce the EXACT slot-level state the OOM
// branch leaves behind, by driving the identical `FREE → LIVE` CAS +
// `generation` bump + push-back-to-FREE sequence `claim` performs, WITHOUT
// running `HeapCore::new` — i.e. "claim a slot, then simulate materialisation
// failing" — so a caller-side test can verify the state `push_back_after_oom`
// produces (LIVE→FREE, back on `free_slots`, `generation` bumped but
// `initialised` still false) and that a SUBSEQUENT real `claim()` recovers it
// correctly (the M-5 fix under test: the gate is `initialised`, not
// `generation == 1`).
// ---------------------------------------------------------------------------

/// Test-only hook (UBFIX-5 / M-5): claim a slot via the exact `pick_slot` +
/// `FREE → LIVE` CAS + `generation` bump prelude `claim` uses, then — instead
/// of materialising a `HeapCore` — immediately push it back to `FREE` via
/// [`push_back_after_oom`], exactly as `claim`'s OOM branch does. Returns the
/// slot index touched, or `None` on registry exhaustion (mirrors `claim`'s
/// own `None` case, vanishingly unlikely in a test).
///
/// This reproduces, deterministically and without touching `alloc_core.rs`,
/// the exact post-OOM slot state `claim`'s `HeapCore::new() == None` branch
/// leaves behind: `state == FREE`, the slot back on `free_slots`,
/// `generation` bumped by exactly one, and `initialised` still `false` (the
/// slot's `HeapCore` was never written). A caller can use the returned index
/// with [`dbg_slot_generation`] / a subsequent `claim()` to verify (a) the
/// slot is not leaked (a following `claim()` can reach it again) and (b) a
/// following `claim()` on this exact slot — which will observe
/// `generation >= 2` (already bumped once here) — still correctly
/// materialises the `HeapCore` rather than skipping materialisation (the
/// defect the old `new_gen == 1` gate had: it would treat any
/// `generation > 1` slot as "already materialised" regardless of
/// `initialised`, handing out a pointer to `MaybeUninit::uninit()` bytes).
#[doc(hidden)]
#[must_use]
pub fn dbg_claim_then_simulate_oom() -> Option<u32> {
    let idx = HeapRegistry::pick_slot()?;
    let reg = ensure();
    // R6-OPT-P0-2: `idx < MAX_HEAPS` by `pick_slot`; `slot()` resolves it
    // through the chunked slot array.
    let slot = reg.slot(idx);
    if slot.cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
        == Err(STATE_LIVE)
    {
        // Lost the slot race to a concurrent real claim (should not happen
        // under the crate's `tests/` serial-guard discipline, but stay
        // total rather than panic): report exhaustion-shaped `None` rather
        // than corrupting a slot we do not own.
        return None;
    }
    slot.generation.fetch_add(1, Ordering::Release);
    // Do NOT call `HeapCore::new` / write `heap` / publish `initialised` —
    // this is the simulated OOM: materialisation never happened.
    push_back_after_oom(reg, slot, idx as u32);
    Some(idx as u32)
}

/// Test-only introspection (UBFIX-5): read a slot's `initialised` flag
/// directly. `HeapSlot::initialised` is `pub(crate)` (not reachable from
/// `tests/` through the normal `#[doc(hidden)] pub` surface, unlike `state`/
/// `generation`), so this hook exists purely to let integration tests assert
/// the M-5 postcondition: a slot returned by [`dbg_claim_then_simulate_oom`]
/// must read `initialised == false` (materialisation never ran), and after a
/// following successful `claim()` on the same index it must read `true`.
#[doc(hidden)]
#[must_use]
pub fn dbg_slot_initialised(idx: u32) -> bool {
    let reg = ensure();
    if idx as usize >= MAX_HEAPS {
        return false;
    }
    // R6-OPT-P0-2: range-checked above; `slot()` resolves it through the
    // chunked slot array.
    let slot = reg.slot(idx as usize);
    slot.initialised.load(Ordering::Acquire)
}
