//! Registry diagnostics: the config-conflict counter, the process-wide
//! hit-total aggregators over slot-resident W3 counters, the minted-slot
//! high-water mark, and the UBFIX-5 test-only introspection hooks.
#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use super::claim::{push_back_after_oom, HeapRegistry};
use crate::registry::bootstrap::{ensure, MAX_HEAPS};
#[cfg(feature = "alloc-stats")]
use crate::registry::heap_slot::HeapSlot;
use crate::registry::heap_slot::{STATE_FREE, STATE_LIVE};

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
pub(super) static CONFIG_CONFLICTS: AtomicU64 = AtomicU64::new(0);

/// DIAGNOSTIC (task E1): the high-water mark of minted registry slots — the
/// number of distinct heap slots ever claimed (via `bump_count`) since
/// process start. This is a **high-water mark, not a live count**: a slot
/// that was claimed and later recycled is still counted here (recycled slots
/// are reused, not un-minted). Relaxed load of [`Registry::count`].
/// `#[doc(hidden)]` — diagnostic-only surface, not part of the crate's
/// supported public API; reached via `SeferAlloc::stats()`.
///
/// [`Registry::count`]: crate::registry::bootstrap::Registry::count
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
///
/// [`HeapSlot::tcache_hits`]: crate::registry::heap_slot::HeapSlot::tcache_hits
/// [`HeapSlot::initialised`]: crate::registry::heap_slot::HeapSlot::initialised
/// [`HeapSlot::heap`]: crate::registry::heap_slot::HeapSlot::heap
/// [`HeapCore`]: crate::registry::heap_core::HeapCore
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
        let mut total: u64 = 0;
        // #1994: shared walk/gate loop — see `walk_initialised_slots`'s doc
        // for the full initialised-gate soundness argument (identical here).
        walk_initialised_slots(|slot| {
            // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
            // `(*heap_ptr).…` deref, so NO shared `&HeapCore` is ever materialised
            // over a struct the owning thread concurrently holds a protected `&mut`
            // into. This closes the Stacked-Borrows aliasing gap the old
            // `(*heap_ptr).tcache_hits()` read had. Relaxed load of a shared
            // `Sync` atomic — sound from any thread; observes the owner's
            // monotonic single-writer increments.
            total = total.saturating_add(slot.remote.tcache_hits.load(Ordering::Relaxed));
        });
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
///
/// [`HeapSlot::initialised`]: crate::registry::heap_slot::HeapSlot::initialised
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
        let mut total: u64 = 0;
        // #1994: shared walk/gate loop — see `walk_initialised_slots`'s doc
        // for the full initialised-gate soundness argument (identical here).
        walk_initialised_slots(|slot| {
            // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
            // `(*heap_ptr).core.…` deref, so NO shared `&HeapCore`/`&AllocCore` is
            // ever materialised over a struct the owning thread concurrently holds
            // a protected `&mut` into. This closes the Stacked-Borrows aliasing gap
            // the old `(*heap_ptr).core.dbg_large_cache_hits()` read had. Relaxed
            // load of a shared `Sync` atomic — sound from any thread.
            total = total.saturating_add(slot.remote.large_cache_hits.load(Ordering::Relaxed));
        });
        total
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        0
    }
}

/// DIAGNOSTIC (#1986): fused single-pass variant of [`tcache_hits_total`] +
/// [`large_cache_hits_total`] for [`SeferAlloc::stats()`](crate::SeferAlloc::stats)'s
/// own use — one registry-slot walk instead of two when BOTH counters are
/// compiled in (`alloc-global + fastbin + alloc-decommit`, all present under
/// `production`). The two standalone accessors are UNCHANGED and still exist
/// for any caller that needs only one counter, or needs it independent of the
/// other's feature gate (both have callers outside `stats()` — see
/// `tests/regression_percounter_perheap_aggregation.rs` and friends).
///
/// Soundness and the `initialised`-gate rationale are IDENTICAL to
/// [`tcache_hits_total`] / [`large_cache_hits_total`] above (same walk, same
/// per-slot Acquire/Release pairing, same happens-before argument) — see
/// those functions' doc comments for the full writeup; not repeated here.
///
/// Like both standalone accessors, the WALK is additionally gated on
/// `alloc-stats` (R3-A, round3 finding N1): without it both slot-resident
/// counters are compile-time 0, so this returns `(0, 0)` with no loop, to
/// keep `stats()` O(1) on a metrics-scrape hot path as its doc promises.
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
#[doc(hidden)]
#[must_use]
pub fn tcache_and_large_cache_hits_total() -> (u64, u64) {
    #[cfg(feature = "alloc-stats")]
    {
        let mut tcache_total: u64 = 0;
        let mut large_cache_total: u64 = 0;
        walk_initialised_slots(|slot| {
            tcache_total =
                tcache_total.saturating_add(slot.remote.tcache_hits.load(Ordering::Relaxed));
            large_cache_total = large_cache_total
                .saturating_add(slot.remote.large_cache_hits.load(Ordering::Relaxed));
        });
        (tcache_total, large_cache_total)
    }
    #[cfg(not(feature = "alloc-stats"))]
    {
        (0, 0)
    }
}

/// #1994 (registry review P3-1): the shared "walk every minted slot, skip
/// un-initialised ones, visit the rest" loop that backed three separately
/// copy-pasted per-slot walks (`tcache_hits_total`, `large_cache_hits_total`,
/// `tcache_and_large_cache_hits_total` above). Each caller supplies its own
/// `visit` closure to accumulate whichever counter(s) it aggregates —
/// see `tcache_hits_total`'s doc comment for the full soundness argument
/// this loop's `initialised`-gate rests on (identical for every caller: the
/// Acquire load pairs with `claim`'s Release publish after
/// `heap_ptr.write(hc)` completes, so a mid-mint slot is never visited).
#[cfg(feature = "alloc-stats")]
fn walk_initialised_slots(mut visit: impl FnMut(&'static HeapSlot)) {
    let reg = ensure();
    let count = reg.count.load(Ordering::Acquire) as usize;
    for idx in 0..count.min(MAX_HEAPS) {
        // R6-OPT-P0-2: `idx < count <= MAX_HEAPS`. `slot()` transparently
        // materialises (or finds already-materialised) exactly the chunks
        // this `count`-bounded walk touches — every index in `0..count` was,
        // by construction, either freshly minted by `bump_count` or popped
        // off `free_slots`, both of which already call `slot()` on it, so
        // its owning chunk is already materialised by the time this walk
        // reaches it.
        let slot = reg.slot(idx);
        if !slot.initialised.load(Ordering::Acquire) {
            continue;
        }
        visit(slot);
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
