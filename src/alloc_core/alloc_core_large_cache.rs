//! Large-cache decay/eviction cluster of [`AllocCore`] (mechanical split of
//! `alloc_core.rs`).
//!
//! This file holds an additional `impl AllocCore { .. }` block carrying the
//! large-cache lazy-decay, eviction, and diagnostic methods. It is a pure
//! code-movement sibling of `alloc_core.rs`; no behavior changed. The whole
//! module is `alloc-decommit`-gated because every method here is.

use super::os;

use super::large_cache_mode::LargeCacheMode;

use super::alloc_core::{
    AllocCore, CachedLarge, LargeCacheDecayConfig, LargeCacheHitCounter, LARGE_CACHE_SLOTS,
};
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
use super::alloc_core::{FORCE_DECAY_CLOCK_READ, MAYBE_DECAY_GUARD_PASSED};
#[cfg(feature = "large-cache-extended")]
use super::large_cache_extended::{self, LARGE_CACHE_EXTENDED_SLOTS};

/// R32-8 (task #499, F9): once `maybe_decay_large_cache`'s headroom fast-exit
/// no longer applies (`used > headroom`), only actually consult the clock
/// every this-many calls — see that function's own doc for the full
/// rationale and the exact granularity trade this buys. 64 chosen as a round
/// power-of-two stride: at the default 1000 ms `decay_interval`, a workload
/// sustaining even a modest few hundred large ops/second keeps the maximum
/// possible tick-lateness (`stride - 1` ops) to a small fraction of the
/// interval, while cutting clock reads on the "guard fails" side by ~64x.
#[cfg(feature = "alloc-decommit")]
const DECAY_CLOCK_CHECK_STRIDE: u32 = 64;

/// R34-11 (task #530): cap on how many decay steps a single clock read may
/// fire in catch-up mode. Once the stride throttle
/// ([`DECAY_CLOCK_CHECK_STRIDE`]) lets the clock go unread for many
/// intervals (sparse-traffic regime — see R34-10,
/// `docs/perf/R34_10_SPARSE_DECAY_GATE.md`), a single read can find many
/// intervals due. Without a catch-up loop, only one eviction step fires per
/// read and the retention gap accumulates (R34-10 measured a 4-segment peak
/// persisting for 95% of the run). This cap bounds the work done in one call
/// while comfortably exceeding the worst observed gap: 4 segments of excess
/// drain to headroom in exactly 4 geometric-decay steps (10% per step, each
/// evicting at least one 4 MiB segment — see `run_decay_step`), so 8 gives
/// a 2× margin. Worst case: 8 FIFO eviction scans + at most 8 OS
/// `release_segment` calls per clock read — negligible against the stride's
/// ~64-op amortization. Does NOT change when the clock is read (the R32-8
/// stride benefit is preserved — see the gate), only how many decay steps
/// fire once it is.
#[cfg(feature = "alloc-decommit")]
const DECAY_CATCHUP_MAX_STEPS: u32 = 8;

/// R13-7 (task #277): a slot index into the COMBINED base+extension
/// large-cache index space. `0..LARGE_CACHE_SLOTS` addresses `self
/// .large_cache`; `LARGE_CACHE_SLOTS..LARGE_CACHE_SLOTS +
/// LARGE_CACHE_EXTENDED_SLOTS` addresses `self.large_cache_extension`'s
/// `slots` array (materialising it on first write if needed). With
/// `large-cache-extended` OFF, only the base range exists — every method
/// below degrades to exactly the pre-R13-7 base-8-slots-only behaviour.
type CombinedSlot = usize;

impl AllocCore {
    /// Total addressable slots in the combined base+extension index space:
    /// `LARGE_CACHE_SLOTS` (8) when the extension is not materialised (or
    /// the feature is off), `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS`
    /// (40) once it is. Read-only — does NOT materialise the extension (a
    /// scan that finds nothing to do should not pay a reservation cost; see
    /// `large_cache_extended`'s module doc, same "OOM is not allocator OOM,
    /// stay off until genuinely needed" posture `directory_sidecar` uses).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(super) fn large_cache_scan_bound(&self) -> CombinedSlot {
        #[cfg(feature = "large-cache-extended")]
        {
            if self.large_cache_extension.is_null() {
                LARGE_CACHE_SLOTS
            } else {
                LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS
            }
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            LARGE_CACHE_SLOTS
        }
    }

    /// Read-only slot access at a COMBINED index (see [`CombinedSlot`]).
    /// `None` if `idx` is out of the currently-materialised range (base
    /// slots are always in range; an extension index is out of range only
    /// if the sidecar has never been materialised, in which case it holds
    /// no entries by construction).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
                          // boundary (see its doc) when `large-cache-extended` is on. Sound here:
                          // `self.large_cache_extension` is just proven non-null (produced only
                          // by `reserve_large_cache_extension`, which typed-initialises it), and
                          // `AllocCore`'s owner-only discipline (neither `Send` nor `Sync`) rules
                          // out a concurrent writer. The returned `&LargeCacheExtension` does not
                          // outlive this call.
    pub(super) fn large_cache_slot_get(&self, idx: CombinedSlot) -> Option<&CachedLarge> {
        if idx < LARGE_CACHE_SLOTS {
            return self.large_cache[idx].as_ref();
        }
        #[cfg(feature = "large-cache-extended")]
        {
            if self.large_cache_extension.is_null() {
                return None;
            }
            // SAFETY: see the `#[allow(unsafe_code)]` justification above this
            // function.
            let ext = unsafe {
                large_cache_extended::deref_large_cache_extension(self.large_cache_extension)
            };
            ext.slots[idx - LARGE_CACHE_SLOTS].as_ref()
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            None
        }
    }

    /// Take (remove) the entry at a COMBINED index, leaving that slot empty.
    /// Panics if `idx` addresses an empty slot or an unmaterialised
    /// extension range — callers only ever call this on an index just
    /// proven occupied by [`large_cache_scan_bound`]/[`large_cache_slot_get`],
    /// mirroring the pre-existing `self.large_cache[i].take().unwrap()`
    /// call sites this replaces.
    ///
    /// R32-12 (task #503, F8 sub-change (2)): one of the two sites (see
    /// `large_cache_occupied`'s own doc) that maintain the occupancy
    /// bitmask in lockstep — clears bit `idx` on every successful take
    /// (i.e. only once the `.expect()`/extension path proves an entry
    /// really was there, so the bitmask is never cleared for a slot that
    /// was already empty).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
                          // boundary when `large-cache-extended` is on. Sound: `self
                          // .large_cache_extension` was produced only by
                          // `reserve_large_cache_extension` (typed-initialised), `AllocCore`'s
                          // owner-only discipline (neither `Send` nor `Sync`) rules out a
                          // concurrent reader/writer, and no other reference to the sidecar is
                          // live across this call.
    pub(super) fn large_cache_slot_take(&mut self, idx: CombinedSlot) -> CachedLarge {
        if idx < LARGE_CACHE_SLOTS {
            let taken = self.large_cache[idx]
                .take()
                .expect("large_cache_slot_take: empty base slot");
            self.large_cache_occupied &= !(1u64 << idx);
            return taken;
        }
        #[cfg(feature = "large-cache-extended")]
        {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above this
            // function.
            let ext = unsafe {
                large_cache_extended::deref_large_cache_extension_mut(self.large_cache_extension)
            };
            let taken = ext.slots[idx - LARGE_CACHE_SLOTS]
                .take()
                .expect("large_cache_slot_take: empty extension slot");
            self.large_cache_occupied &= !(1u64 << idx);
            taken
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            unreachable!("large_cache_slot_take: idx out of base range with extension disabled")
        }
    }

    /// R14-5 (task #290, review finding @fm P3): can a deposit of
    /// `usable_size` bytes EVER be admitted under the current byte-budget,
    /// no matter how much the cache is evicted first? Returns `true` only
    /// when the budget is `Some(b)` and `usable_size > b` — i.e. even an
    /// entirely empty cache (`large_cache_used_bytes == 0`, the best case
    /// eviction could ever reach) could not fit this one deposit alone.
    /// `false` whenever the budget is `None` (unbounded) or `usable_size` is
    /// small enough to fit after enough eviction.
    ///
    /// This is a CHEAP, PURELY ARITHMETIC pre-check (no eviction, no sidecar
    /// touch) that both large-dealloc admission call sites
    /// (`alloc_core.rs`'s Large `dealloc` branch,
    /// `alloc_core_large.rs::reclaim_large_segment`) now run BEFORE calling
    /// [`large_cache_find_free_slot`](Self::large_cache_find_free_slot) at
    /// all. Rationale: without this pre-check, a deposit the budget will
    /// unconditionally reject (e.g. `budget_bytes(0)`, or a single span
    /// larger than the whole configured budget) still walked into
    /// `large_cache_find_free_slot`'s base-8 scan and — once the base was
    /// full — MATERIALISED the extension sidecar (a real
    /// `leak_zeroed_pages` OS reservation, paying for a whole page) purely
    /// to discover a free slot for a deposit that was never going to be
    /// admitted regardless of slot availability. Under a workload that
    /// hammers this admission-reject path repeatedly (e.g. a persistently
    /// tight/zero budget with churn well above `LARGE_CACHE_SLOTS`), the old
    /// order paid that reservation cost on every such rejected deposit —
    /// see [`large_cache_find_free_slot`](Self::large_cache_find_free_slot)'s
    /// own doc for why materialisation is not free even beyond the first
    /// paid call, if the reservation had actually failed (a persistent OOM
    /// on the sidecar page).
    ///
    /// Not a FULL feasibility check — it only proves the single-deposit
    /// unconditional-rejection case (budget smaller than one span). It does
    /// NOT attempt to predict the eviction loop's eventual outcome when the
    /// budget is merely tight but not impossible (that still runs the
    /// existing evict-and-retry loop, unchanged).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(super) fn large_cache_deposit_budget_infeasible(&self, usable_size: usize) -> bool {
        matches!(self.large_cache_budget_bytes, Some(budget) if usable_size > budget)
    }

    /// Find a free slot to admit a new deposit into, in the COMBINED index
    /// space: scans the base 8 slots first (no materialisation cost), then —
    /// only if `large-cache-extended` is on and the base is full — lazily
    /// materialises the extension sidecar and scans it. Returns `None` if
    /// every slot in the currently-available space (base, or base+extension
    /// once materialised) is occupied, OR if extension materialisation
    /// itself hit OOM (sidecar OOM is not allocator OOM — the caller's
    /// existing eviction-and-retry loop simply keeps operating within the
    /// base 8 slots, exactly as if this feature did not exist).
    ///
    /// CORRECTNESS NOTE (R14-5, task #290, fixing a stale claim from an
    /// earlier revision of this doc comment): materialisation here is **NOT**
    /// idempotently free after the first call. `self.large_cache_extension`
    /// is only set non-null on a *successful* reservation
    /// (`reserve_large_cache_extension()?`); under a PERSISTENT OOM (the
    /// sidecar's one-page `leak_zeroed_pages` reservation keeps failing —
    /// e.g. the process is genuinely address-space-starved) the pointer
    /// stays null forever, so **every** call that reaches this branch with
    /// the base 8 full pays a full OS reservation ATTEMPT again, not a cheap
    /// no-op null check. Callers on a hot retry loop (the admission loop in
    /// `alloc_core.rs`'s Large-dealloc branch / `alloc_core_large.rs`'s
    /// `reclaim_large_segment`) must not call this when the deposit is
    /// already known to be budget-rejected regardless of slot availability —
    /// see [`AllocCore::large_cache_deposit_budget_infeasible`], which those
    /// callers consult FIRST specifically to avoid paying this reservation
    /// attempt for a deposit that can never be admitted.
    ///
    /// R32-12 (task #503, F8 sub-change (2)): the base-8 scan
    /// (`self.large_cache.iter().position(|s| s.is_none())`, one `Option`
    /// read per slot = up to 7 cache lines at the measured 56 B/slot stride)
    /// is replaced by `large_cache_occupied.trailing_ones()` — the index of
    /// the lowest CLEAR bit in the occupancy bitmask, found without touching
    /// the `large_cache` array at all. `trailing_ones() as usize` is only
    /// used as a slot index when it is `< LARGE_CACHE_SLOTS` (checked
    /// below); a fully-occupied base (all 8 low bits set) yields
    /// `trailing_ones() == 8`, correctly falling through to the extension
    /// arm exactly as the old `.position()` returning `None` did.
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
                          // boundary when `large-cache-extended` is on, right after the sidecar is
                          // proven materialised (either already non-null, or just reserved and
                          // typed-initialised by `reserve_large_cache_extension` on this same
                          // line). `AllocCore`'s owner-only discipline (neither `Send` nor `Sync`)
                          // rules out a concurrent writer; the returned `&LargeCacheExtension`
                          // does not outlive this call.
    pub(super) fn large_cache_find_free_slot(&mut self) -> Option<CombinedSlot> {
        let base_free = self.large_cache_occupied.trailing_ones() as usize;
        if base_free < LARGE_CACHE_SLOTS {
            return Some(base_free);
        }
        #[cfg(feature = "large-cache-extended")]
        {
            if self.large_cache_extension.is_null() {
                let ptr = large_cache_extended::reserve_large_cache_extension()?;
                self.large_cache_extension = ptr;
            }
            // SAFETY: see the `#[allow(unsafe_code)]` justification above this
            // function.
            let ext = unsafe {
                large_cache_extended::deref_large_cache_extension(self.large_cache_extension)
            };
            ext.slots
                .iter()
                .position(|s| s.is_none())
                .map(|i| LARGE_CACHE_SLOTS + i)
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            None
        }
    }

    /// Write `entry` into the COMBINED slot `idx` (must currently be empty —
    /// mirrors the pre-existing `self.large_cache[slot_idx] = Some(..)`
    /// assignment this replaces).
    ///
    /// R32-12 (task #503, F8 sub-change (2)): the other of the two sites
    /// (see `large_cache_occupied`'s own doc) that maintain the occupancy
    /// bitmask in lockstep — sets bit `idx` unconditionally, in both the
    /// base and extension arms (the slot transitions None → Some in both
    /// cases; callers only ever call this on a slot just proven empty by
    /// `large_cache_find_free_slot`).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension_mut`
                          // boundary when `large-cache-extended` is on. Sound: `self
                          // .large_cache_extension` was produced only by
                          // `reserve_large_cache_extension` (typed-initialised), `AllocCore`'s
                          // owner-only discipline (neither `Send` nor `Sync`) rules out a
                          // concurrent reader/writer, and no other reference to the sidecar is
                          // live across this call.
    pub(super) fn large_cache_slot_set(&mut self, idx: CombinedSlot, entry: CachedLarge) {
        if idx < LARGE_CACHE_SLOTS {
            self.large_cache[idx] = Some(entry);
            self.large_cache_occupied |= 1u64 << idx;
            return;
        }
        #[cfg(feature = "large-cache-extended")]
        {
            // SAFETY: see the `#[allow(unsafe_code)]` justification above this
            // function.
            let ext = unsafe {
                large_cache_extended::deref_large_cache_extension_mut(self.large_cache_extension)
            };
            ext.slots[idx - LARGE_CACHE_SLOTS] = Some(entry);
            self.large_cache_occupied |= 1u64 << idx;
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            unreachable!("large_cache_slot_set: idx out of base range with extension disabled")
        }
    }

    /// TEST-ONLY (R13-7, task #277): the `usable_size` of each slot in the
    /// EXTENSION sidecar only (base slots stay covered by the pre-existing
    /// [`dbg_large_cache_slot_sizes`](Self::dbg_large_cache_slot_sizes)).
    /// Returns all-`None` if the extension has never been materialised
    /// (never overflowed the base 8 slots, or the feature is off).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "large-cache-extended")]
    #[allow(unsafe_code)] // R14-1 (task #286): calls the `unsafe fn deref_large_cache_extension`
                          // boundary, right after `self.large_cache_extension` is proven non-null.
                          // Sound: the pointer was produced only by
                          // `reserve_large_cache_extension` (typed-initialised), `AllocCore`'s
                          // owner-only discipline (neither `Send` nor `Sync`) rules out a
                          // concurrent writer, and the returned `&LargeCacheExtension` does not
                          // outlive this call.
    pub fn dbg_large_cache_extended_slot_sizes(
        &self,
    ) -> [Option<usize>; LARGE_CACHE_EXTENDED_SLOTS] {
        let mut out = [None; LARGE_CACHE_EXTENDED_SLOTS];
        if self.large_cache_extension.is_null() {
            return out;
        }
        // SAFETY: see the `#[allow(unsafe_code)]` justification above this
        // function.
        let ext = unsafe {
            large_cache_extended::deref_large_cache_extension(self.large_cache_extension)
        };
        for (i, slot) in ext.slots.iter().enumerate() {
            out[i] = slot.as_ref().map(|c| c.usable_size);
        }
        out
    }

    /// TEST-ONLY (R13-7, task #277): whether the large-cache extension
    /// sidecar has been materialised for this `AllocCore`. Always `false`
    /// when `large-cache-extended` is off.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "large-cache-extended")]
    pub fn dbg_large_cache_extension_materialised(&self) -> bool {
        !self.large_cache_extension.is_null()
    }

    /// TEST-ONLY (R13-7, task #277): total addressable slot count in the
    /// combined base+extension space right now (8 if the extension has not
    /// materialised, 40 once it has). Always 8 when `large-cache-extended`
    /// is off.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_total_slots(&self) -> usize {
        self.large_cache_scan_bound()
    }

    // ── Phase 2 — lazy decay helpers ─────────────────────────────────────────

    /// Check whether enough wall-clock time has elapsed since the last decay
    /// tick; if so, run one decay step. Called at the top of both
    /// `alloc_large` and the large-dealloc branch so the "tax" on each large
    /// operation is, in the common case, a cheap counter compare —
    /// nanosecond-range overhead, negligible against OS reservation costs.
    ///
    /// R32-8 (task #499, F9): the ORIGINAL guard here was "is
    /// `large_cache_used_bytes <= headroom_bytes`, skip the clock read
    /// entirely; otherwise ALWAYS read the clock." That shape is a cliff: two
    /// shipped non-default profiles (`LargeCachePolicy::LowHeadroom` /
    /// `::Trimmed64MiB`, `src/alloc_core/profile.rs`) exist SPECIFICALLY to
    /// keep a heap's working set ABOVE `headroom_bytes` during normal
    /// operation — i.e. by design, on the wrong side of that cliff for their
    /// whole intended use case, paying an UNCONDITIONAL
    /// `std::time::Instant::now()` (a `QueryPerformanceCounter` syscall on
    /// Windows) on every large alloc/free. Measured, confound-free
    /// (`docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`, fixed
    /// headroom across arms, `FORCE_DECAY_CLOCK_READ` isolates the clock-read
    /// cost from any headroom-driven hit-rate effect): a real, reproducible
    /// **~74-138 ns per call across 5 independent runs** (~150 ns per
    /// steady-state alloc+free cycle, two calls, in the tightest-clustered
    /// runs) — consistent with task #95's own historical ~105 ns/call
    /// anchor.
    ///
    /// **Fix: a cheap monotonic op-counter throttles how OFTEN the clock is
    /// even consulted, once past headroom.** `large_cache_decay_op_count` is
    /// incremented on every call that passes the headroom check; the clock is
    /// only actually read every [`DECAY_CLOCK_CHECK_STRIDE`]-th such call.
    /// Between clock reads, this function assumes the interval has NOT yet
    /// elapsed and returns without decaying.
    ///
    /// **Semantic trade, stated explicitly (per the survey's own
    /// requirement):** this trades DECAY-TICK GRANULARITY for fewer clock
    /// reads. A decay tick that becomes due can now fire up to
    /// `DECAY_CLOCK_CHECK_STRIDE - 1` large ops LATE (i.e. up to that many
    /// alloc/dealloc calls after the `decay_interval` wall-clock deadline
    /// technically passed), instead of firing on the very next call as
    /// before. It can NEVER fire EARLY (the stride only delays a clock read,
    /// never fabricates elapsed time), so a decay tick is never more
    /// aggressive than the un-throttled behavior — only, at most, slightly
    /// less prompt. On a workload with plenty of large-op traffic (the exact
    /// regime `LowHeadroom`/`Trimmed64MiB` are chosen for), `stride - 1`
    /// large ops is a small sliver of the 1-second default `decay_interval`;
    /// on a workload with sparse large-op traffic the guard was already
    /// mostly idle-triggered (see the module's event-driven-only design
    /// note), so a modest additional delay changes little in practice. This
    /// throttle applies ONLY on the organic call path — [`dbg_force_decay_tick`]
    /// (used by tests and R29-13's forced-convergence measurement) explicitly
    /// BYPASSES the stride so a forced tick still fires deterministically on
    /// every call, exactly as before this change (see its own doc for how).
    ///
    /// **R34-11 (task #530) catch-up loop:** when the stride throttle does let
    /// many intervals elapse between clock reads (sparse-traffic regime), a
    /// single read now fires as many decay steps as intervals are due, bounded
    /// by [`DECAY_CATCHUP_MAX_STEPS`] — not just one. R34-10 found that the
    /// old single-step-per-read shape let the retention gap accumulate to 4
    /// segments and persist for 95% of the run; the catch-up loop closes that
    /// persistence (the gap drops to 0 on the first clock read instead of
    /// staying at 3–4 segments). The timer advances by `due * decay_interval`
    /// (not to `now`) so the sub-interval remainder carries over honestly.
    ///
    /// [`dbg_force_decay_tick`]: Self::dbg_force_decay_tick
    #[cfg(feature = "alloc-decommit")]
    pub(super) fn maybe_decay_large_cache(&mut self) {
        // FAST-PATH EARLY EXIT — avoid touching the clock-read throttle at
        // all when there is provably no work to do. The decay can only ever
        // release bytes when `cached > headroom`. If the cache is at or
        // below the headroom, `run_decay_step` would compute `excess = 0`
        // and bail anyway, so we skip everything past this point entirely.
        //
        // This covers the dominant benchmark workload (alloc+free cycle with
        // one cached span at ~4-16 MiB, far below the 256 MiB default
        // headroom) and restores the ~45 ns cache-hit timing that an
        // unconditional clock read had regressed to ~150 ns. See task #95.
        //
        // Correctness: a true decay opportunity (cached > headroom) only
        // arises *after* a `dealloc` deposit grows `large_cache_used_bytes`
        // past `headroom_bytes`; we then hit this path on the next op and do
        // the proper time-based decision.
        //
        // R32-8/F9 measurement seam: `bench-internals` only, never compiled
        // into a plain `production` build. `FORCE_DECAY_CLOCK_READ` lets the
        // A/B probe force this guard to behave as if it always failed
        // (i.e. skip straight past the fast exit) WITHOUT changing
        // `headroom_bytes` — isolating the clock-read cost from any
        // headroom-driven hit-rate confound (see this function's own doc
        // above).
        #[cfg(feature = "bench-internals")]
        let forced = FORCE_DECAY_CLOCK_READ.load(core::sync::atomic::Ordering::Relaxed);
        #[cfg(not(feature = "bench-internals"))]
        let forced = false;
        if !forced && self.large_cache_used_bytes <= self.decay_config.headroom_bytes {
            return;
        }

        // R32-8/F9 STRIDE THROTTLE — past the headroom fast-exit, only
        // actually consult the clock every DECAY_CLOCK_CHECK_STRIDE-th call.
        // The counter is reset to 0 whenever the clock IS read below, so the
        // stride always counts from the most recent real check. Wrapping add
        // is fine — `% DECAY_CLOCK_CHECK_STRIDE` on a wrapped value is still
        // a valid stride position, just not the "true" call count; only the
        // periodicity matters, not the absolute count.
        self.large_cache_decay_op_count = self.large_cache_decay_op_count.wrapping_add(1);
        if !forced
            && !self
                .large_cache_decay_op_count
                .is_multiple_of(DECAY_CLOCK_CHECK_STRIDE)
            && self.last_decay_tick.is_some()
        {
            // Not yet due for a clock check this stride, AND the timer is
            // already primed (a `None` timer always falls through to prime
            // immediately below — delaying the FIRST-EVER prime would let an
            // unbounded number of large ops pass before decay logic engages
            // at all on a fresh heap, which is a materially different
            // semantic than "decay ticks may fire a little late").
            return;
        }

        #[cfg(feature = "bench-internals")]
        MAYBE_DECAY_GUARD_PASSED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.large_cache_decay_op_count = 0;
        let now = std::time::Instant::now();
        let elapsed = match self.last_decay_tick {
            Some(t) => now.duration_since(t),
            None => {
                // First call ever: prime the timer but do not decay yet.
                // Without this guard the first alloc_large after a cold start
                // would decay with an arbitrarily large "elapsed" (since the
                // epoch), potentially flushing the cache unnecessarily.
                self.last_decay_tick = Some(now);
                return;
            }
        };
        if elapsed < self.decay_config.decay_interval {
            return;
        }
        // R34-11 (task #530): CATCH-UP LOOP. R34-10
        // (`docs/perf/R34_10_SPARSE_DECAY_GATE.md`) found that the stride
        // throttle lets many decay intervals elapse between clock reads in
        // sparse-traffic regimes, but `run_decay_step` fired only ONE step
        // per read — so the throttled arm accumulated a multi-segment
        // retention gap (peak 4 segments) and NEVER caught up (persisted at
        // ≥3 segments for 95% of the run). Fix: once the clock IS read and
        // the interval has elapsed, fire as many steps as intervals are due,
        // bounded by `DECAY_CATCHUP_MAX_STEPS`. The timer advances by
        // `due * decay_interval` (NOT to `now`) so the sub-interval remainder
        // carries over honestly to the next check. This does not change WHEN
        // the clock is read (the R32-8 stride benefit is preserved — see
        // `docs/perf/R34_11_CATCHUP_DECAY_GATE.md`), only HOW MANY decay
        // steps fire once it is.
        let interval = self.decay_config.decay_interval;
        let intervals_due: u32 = if interval.is_zero() {
            // Zero interval (test-only via dbg_set_decay_config): fire one
            // step per call, advancing the timer to now (pre-R34-11 shape).
            self.last_decay_tick = Some(now);
            1
        } else {
            let ratio =
                (elapsed.as_nanos() / interval.as_nanos()).min(DECAY_CATCHUP_MAX_STEPS as u128);
            let due = u32::try_from(ratio).unwrap_or(DECAY_CATCHUP_MAX_STEPS);
            if let Some(t) = self.last_decay_tick {
                self.last_decay_tick = Some(t + interval * due);
            }
            due
        };
        for _ in 0..intervals_due {
            self.run_decay_step();
        }
    }

    /// Compute the excess over `headroom_bytes` and release `decay_rate_bp /
    /// 10 000` of it back to the OS via FIFO eviction.
    ///
    /// Phase 2 simplification: `live_bytes = 0` (we do not track outstanding
    /// large allocations explicitly). The target is therefore simply
    /// `headroom_bytes`. A future phase can add live-count tracking to tighten
    /// the target when many large blocks are outstanding.
    #[cfg(feature = "alloc-decommit")]
    fn run_decay_step(&mut self) {
        let target = self.decay_config.headroom_bytes; // live = 0 in Phase 2
        let excess = self.large_cache_used_bytes.saturating_sub(target);
        if excess == 0 {
            return; // Cache is at or below target — nothing to release.
        }
        // release = excess * rate_bp / 10_000.  We use saturating_mul to
        // guard against an absurdly large excess (> usize::MAX / 10_000 on
        // 32-bit — pathological but safe).
        let release = excess.saturating_mul(self.decay_config.decay_rate_bp as usize) / 10_000;
        if release == 0 {
            return;
        }
        self.evict_at_least(release);
    }

    /// FIFO-evict cached spans until at least `min_bytes` of cache have been
    /// released to the OS, or the cache is empty. Each iteration evicts the
    /// occupied slot with the smallest `seq` (task D1: true insertion-order
    /// FIFO, not array-index order — see the `CachedLarge::seq` doc comment
    /// for why index order stopped being a valid proxy once
    /// `LARGE_CACHE_SLOTS > 2`). The OS reservation of each evicted span is
    /// released immediately.
    #[cfg(feature = "alloc-decommit")]
    fn evict_at_least(&mut self, min_bytes: usize) {
        let mut released = 0usize;
        while released < min_bytes {
            // Find the occupied slot with the smallest seq (true FIFO-oldest).
            let Some(victim_idx) = self.oldest_occupied_slot() else {
                break; // Cache is empty.
            };
            let victim = self.large_cache_slot_take(victim_idx);
            self.large_cache_used_bytes = self
                .large_cache_used_bytes
                .saturating_sub(victim.usable_size);
            // Release the OS reservation. The slot was unregistered from the
            // table on deposit (same as `try_evict_to_fit`), so we release
            // directly without touching the table.
            os::release_segment(victim.reservation, victim.reservation_len);
            released += victim.usable_size;
        }
    }

    /// Evict the **entire** large cache — release every cached span's OS
    /// reservation until the cache is empty. Called from the teardown-trim
    /// path (`HeapCore::trim_for_recycle`, task #95/N1) to return retained
    /// large segments to the OS on thread exit rather than leaving them
    /// mapped on a recycled slot. Each eviction releases the FIFO-oldest
    /// entry via [`evict_one_oldest`](Self::evict_one_oldest); the loop
    /// terminates when the cache is empty (`evict_one_oldest` returns
    /// `false`). Cost: O(LARGE_CACHE_SLOTS) — thread exit is cold.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn evict_all(&mut self) {
        while self.evict_one_oldest() {}
    }

    // ── Phase 2 test seams ────────────────────────────────────────────────────

    /// TEST-ONLY (Phase 2): force a decay tick by rewinding `last_decay_tick`
    /// to be exactly `decay_interval` in the past, then calling
    /// `maybe_decay_large_cache`. This causes the interval check to pass
    /// unconditionally on the very next call, without sleeping. Safe to call
    /// multiple times — each call produces exactly one decay step.
    ///
    /// Concretely: for a test with `decay_interval = 10s` this makes it
    /// appear as if 10 s have elapsed since the last tick, so the subsequent
    /// `maybe_decay_large_cache` fires immediately.
    ///
    /// R32-8 (task #499, F9): `maybe_decay_large_cache` now throttles how
    /// often it consults the clock at all once past the headroom fast-exit
    /// (see [`DECAY_CLOCK_CHECK_STRIDE`] and that function's own doc). This
    /// forcing seam BYPASSES that throttle — it primes
    /// `large_cache_decay_op_count` to exactly one call short of the stride
    /// boundary, so the immediately-following `maybe_decay_large_cache` call
    /// is GUARANTEED to land on a real clock read regardless of how many (or
    /// how few) organic calls happened before it. This preserves the
    /// documented "safe to call multiple times — each call produces exactly
    /// one decay step" contract exactly as it held before this task —
    /// `tests/large_cache_decay.rs` and R29-13's forced-convergence loop
    /// (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §1.6) both depend on
    /// every single call reliably firing a real decay tick, never on
    /// whichever call happens to land on a stride boundary by chance.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_force_decay_tick(&mut self) {
        // Rewind last_decay_tick by the full interval so the elapsed check
        // passes.  `checked_sub` returns None if the duration is longer than
        // the time since the epoch (impossible in practice); in that edge case
        // we fall back to `now` which will prime the timer without decaying.
        let interval = self.decay_config.decay_interval;
        self.last_decay_tick = Some(
            std::time::Instant::now()
                .checked_sub(interval)
                .unwrap_or_else(std::time::Instant::now),
        );
        // Bypass the stride throttle deterministically: prime the op-count to
        // one short of the boundary so the `wrapping_add(1)` inside
        // `maybe_decay_large_cache` lands exactly on a multiple of
        // `DECAY_CLOCK_CHECK_STRIDE`, guaranteeing this call reads the clock.
        self.large_cache_decay_op_count = DECAY_CLOCK_CHECK_STRIDE - 1;
        self.maybe_decay_large_cache();
    }

    /// TEST-ONLY (Phase 2): override the decay configuration at runtime.
    /// Lets tests specify exact parameters without relying on env vars
    /// (which are process-global and therefore flaky in parallel runs).
    ///
    /// - `rate_bp`: decay rate in basis points (100 = 1%, 1000 = 10%).
    /// - `interval_ms`: minimum ms between ticks (0 = fire on every call).
    /// - `headroom`: target cache size in bytes.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_set_decay_config(&mut self, rate_bp: u32, interval_ms: u64, headroom: usize) {
        self.decay_config = LargeCacheDecayConfig {
            decay_rate_bp: rate_bp,
            decay_interval: core::time::Duration::from_millis(interval_ms),
            headroom_bytes: headroom,
        };
        // Reset the tick timer so the new interval is observed from this
        // moment forward (avoids a stale timer confusing the first post-config
        // call).
        self.last_decay_tick = None;
    }

    /// TEST-ONLY (R32-8, task #499, F9): process-wide count of
    /// `maybe_decay_large_cache` calls that passed the fast-path guard and
    /// therefore reached the `Instant::now()` read (path-activation oracle,
    /// R30-8 rule). `bench-internals`-gated: always 0 without it. See
    /// `MAYBE_DECAY_GUARD_PASSED`'s own doc in `alloc_core.rs`.
    #[doc(hidden)]
    #[cfg(feature = "internals")]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_maybe_decay_guard_passed_count() -> u64 {
        MAYBE_DECAY_GUARD_PASSED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (R32-8, task #499, F9): set the process-wide
    /// `FORCE_DECAY_CLOCK_READ` override used by the F9 clock-read-cost A/B
    /// probe. See `FORCE_DECAY_CLOCK_READ`'s own doc in `alloc_core.rs` for
    /// exactly what this does and why it isolates the clock-read cost from
    /// any headroom-driven hit-rate confound. `bench-internals`-gated.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_set_force_decay_clock_read(forced: bool) {
        FORCE_DECAY_CLOCK_READ.store(forced, core::sync::atomic::Ordering::Relaxed);
    }

    /// TEST-ONLY (Phase 2): return the current decay configuration as
    /// `(decay_rate_bp, decay_interval_ms, headroom_bytes)`.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_decay_config(&self) -> (u32, u64, usize) {
        (
            self.decay_config.decay_rate_bp,
            self.decay_config.decay_interval.as_millis() as u64,
            self.decay_config.headroom_bytes,
        )
    }

    // ── end Phase 2 ──────────────────────────────────────────────────────────

    /// Find the occupied COMBINED slot (see [`CombinedSlot`]) with the
    /// smallest `seq` — the true FIFO-oldest entry (task D1). Returns `None`
    /// if the cache is empty. `O(large_cache_scan_bound())` — 8 with
    /// `large-cache-extended` off or not-yet-materialised, up to 40 once
    /// materialised; only called on the large-alloc/dealloc slow paths
    /// (never the small hot path), so the linear scan is not
    /// performance-sensitive at either size.
    #[cfg(feature = "alloc-decommit")]
    fn oldest_occupied_slot(&self) -> Option<CombinedSlot> {
        (0..self.large_cache_scan_bound())
            .filter_map(|i| self.large_cache_slot_get(i).map(|c| (i, c.seq)))
            .min_by_key(|&(_, seq)| seq)
            .map(|(i, _)| i)
    }

    /// Evict the FIFO-oldest cached entry (smallest `seq`, task D1 — see
    /// [`oldest_occupied_slot`](Self::oldest_occupied_slot)) and release its
    /// OS reservation. Returns `true` if an entry was evicted, `false` if the
    /// cache was already empty.
    ///
    /// Used by the admission policy when either the byte-budget would
    /// overflow or all slots are occupied (the loop in the large-`dealloc`
    /// branch evicts-and-retries until both constraints hold or the cache is
    /// empty). The victim was unregistered from the segment table on
    /// deposit, so this function only releases the OS reservation and
    /// updates the byte-budget counter.
    #[cfg(feature = "alloc-decommit")]
    pub(super) fn evict_one_oldest(&mut self) -> bool {
        let Some(victim_idx) = self.oldest_occupied_slot() else {
            return false;
        };
        let victim = self.large_cache_slot_take(victim_idx);
        self.large_cache_used_bytes = self
            .large_cache_used_bytes
            .saturating_sub(victim.usable_size);
        os::release_segment(victim.reservation, victim.reservation_len);
        true
    }

    /// TEST-ONLY (Phase 1 large-cache budget): return the current running sum
    /// of `usable_size` across all occupied large-cache slots. The test
    /// `large_cache_used_bytes_invariant` compares this against the manual sum
    /// to verify the invariant is maintained.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_used(&self) -> usize {
        self.large_cache_used_bytes
    }

    /// TEST-ONLY (R32-12, task #503, F8 sub-change (2)): return the raw
    /// `large_cache_occupied` bitmask. Lets a test verify the falsification-
    /// first invariant "bit `i` set ⟺ combined slot `i` is `Some`" directly,
    /// independent of `large_cache_slot_set`/`large_cache_slot_take`'s own
    /// internals — see `tests/large_cache_occupancy_bitmask_invariant.rs`.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_large_cache_occupied_bits(&self) -> u64 {
        self.large_cache_occupied
    }

    /// TEST/DIAGNOSTIC-ONLY (task D1 → #133): count of `alloc_large` calls
    /// served from `large_cache` (cache hits) for THIS `AllocCore` since it
    /// was constructed. Relaxed load of `large_cache_hits` — diagnostic
    /// only. Task #133 moved this from a process-wide `static` to a
    /// per-heap instance field (see its doc comment); callers that need the
    /// process-wide total should use
    /// `registry::heap_registry::large_cache_hits_total`, which sums this
    /// method's result across every live registry slot.
    ///
    /// R31-14b (task #484, closing P2-11 filed in
    /// `docs/CORRECTNESS_OPEN_ITEMS.md` item 9): stays gated `alloc-decommit`
    /// alone — NOT tightened to `all(alloc-decommit, bench-internals)` like
    /// its `HeapCore`-level sibling ([`crate::registry::HeapCore::dbg_large_cache_hits`],
    /// tightened by R31-4/task #467). The two are not the same case:
    /// `HeapCore::dbg_large_cache_hits` had zero callers outside
    /// `bench-internals`-gated examples, so CLAUDE.md's benchmark-hook rule
    /// 2 ("no production caller ⇒ MUST default to `bench-internals`-gating")
    /// applied cleanly. THIS method has real regression-test callers that
    /// run in a plain `production` test build without `bench-internals` —
    /// `tests/alloc_zeroed_fresh_large_skip.rs` and
    /// `tests/regression_large_cache_span_usable_stable.rs` both gate only
    /// on `#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]`
    /// and assert on this method's return value — so it does have a
    /// production-build (test-time) caller, and tightening it would break
    /// those tests without a compensating rewrite. It is also a zero-argument
    /// `&self` read of an already-relaxed atomic counter (no pointer, no
    /// mutation), allowlisted as a `PURE_OBSERVERS` entry in
    /// `tests/dbg_hook_safety_tripwire.rs`, matching the same sanctioned
    /// read-only shape as its `large_cache_used`/`large_cache_budget`/
    /// `large_cache_mode` siblings in this same file, none of which are
    /// `bench-internals`-gated either.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_large_cache_hits(&self) -> u64 {
        // W3: read the SLOT's counter when bound (the SAME `AtomicU64` the
        // aggregator reads, so per-heap and process-wide agree), else the owned
        // fallback (standalone `AllocCore`). Safe references throughout.
        self.large_cache_hits_sink
            .unwrap_or(&self.large_cache_hits)
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// W3: plant the stable `&'static` handle to THIS heap's SLOT-resident
    /// large-cache hit counter. Called (via `HeapCore::bind_large_cache_hits`)
    /// by `HeapRegistry::claim` right after the slot binds, before any alloc on
    /// this heap. Redirects all subsequent increments and diagnostic reads to
    /// the slot's `AtomicU64`, closing the aliasing gap (see
    /// [`LargeCacheHitCounter`]). Idempotent — the slot counter is `'static`,
    /// so re-planting on a re-claim is a harmless no-op.
    ///
    /// Only reachable via the registry (`HeapRegistry::claim`, `alloc-global`);
    /// unused in an `alloc-decommit`-without-`alloc-global` build.
    #[cfg(feature = "alloc-decommit")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn bind_large_cache_hits(&mut self, counter: &'static LargeCacheHitCounter) {
        self.large_cache_hits_sink = Some(counter);
    }

    /// TEST-ONLY (Phase 1 large-cache budget): return the `usable_size` of
    /// each large-cache slot as an array of `Option<usize>` (None = empty slot,
    /// Some(sz) = occupied with that many bytes). Lets tests verify the
    /// invariant `sum(Some values) == dbg_large_cache_used()` without exposing
    /// the private `CachedLarge` type.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_slot_sizes(&self) -> [Option<usize>; LARGE_CACHE_SLOTS] {
        let mut out = [None; LARGE_CACHE_SLOTS];
        for (i, slot) in self.large_cache.iter().enumerate() {
            out[i] = slot.as_ref().map(|c| c.usable_size);
        }
        out
    }

    /// TEST-ONLY (Phase 1 large-cache budget): override the byte-budget at
    /// runtime. Allows a test to set a different budget after calling
    /// `AllocCore::new_with_config`, without constructing a new instance.
    /// Pass `None` for unbounded.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_set_large_cache_budget(&mut self, budget: Option<usize>) {
        self.large_cache_budget_bytes = budget;
    }

    /// TEST-ONLY (R14-5, task #290): read back the CURRENTLY RESOLVED
    /// byte-budget (`None` = unbounded). Lets a test verify what
    /// `AllocCore::new()`/`new_with_config` actually resolved a config to —
    /// in particular, the `large-cache-extended` feature's own finite
    /// default (`DEFAULT_EXTENDED_BUDGET_BYTES`,
    /// `large_cache_config.rs`) — without needing to reconstruct the
    /// resolution logic in the test itself.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_budget(&self) -> Option<usize> {
        self.large_cache_budget_bytes
    }

    // ── Phase 3 test seams ────────────────────────────────────────────────────

    /// TEST-ONLY: return the `LargeCacheMode` set at construction time via
    /// [`LargeCacheConfig::mode`]. Lets tests verify the mode stored in the
    /// shard without relying on implementation internals.
    ///
    /// Returns `LargeCacheMode::Lazy` when `LargeCacheConfig::DEFAULT` was
    /// used (or no `.mode()` call was made on the config).
    ///
    /// [`LargeCacheConfig::mode`]: super::large_cache_config::LargeCacheConfig::mode
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_mode(&self) -> LargeCacheMode {
        self.large_cache_mode
    }
}
