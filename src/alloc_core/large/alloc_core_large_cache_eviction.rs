//! Cold FIFO eviction and diagnostic methods for the Large cache.
//! Sibling of `alloc_core_large_cache`; both are `alloc-decommit`-gated.

use crate::alloc_core::alloc_core::{AllocCore, LargeCacheHitCounter};
#[cfg(feature = "internals")]
use crate::alloc_core::alloc_core::{LargeCacheDecayConfig, LARGE_CACHE_SLOTS};
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
use crate::alloc_core::alloc_core::{FORCE_DECAY_CLOCK_READ, MAYBE_DECAY_GUARD_PASSED};
use crate::alloc_core::large::reservation_state::LargeReservationState;
#[cfg(feature = "internals")]
use crate::alloc_core::large_cache_mode::LargeCacheMode;
use crate::alloc_core::os;
use crate::alloc_core::segment_header::{large_generation, SegmentMeta};

use super::alloc_core_large_cache::{CombinedSlot, DECAY_CLOCK_CHECK_STRIDE};

impl AllocCore {
    /// Test-only snapshot of the oldest cached Large reservation.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn terminal_cached_large_state_for_test(&self) -> Option<u64> {
        let idx = self.oldest_occupied_slot()?;
        let slot = self.large_cache_slot_get(idx)?;
        Some(SegmentMeta::new(slot.base).terminal_snapshot().large_state)
    }
    /// FIFO-evict cached spans until at least `min_bytes` of cache have been
    /// released to the OS, or the cache is empty. Each iteration evicts the
    /// occupied slot with the smallest `seq` (task D1: true insertion-order
    /// FIFO, not array-index order — see the `CachedLarge::seq` doc comment
    /// for why index order stopped being a valid proxy once
    /// `LARGE_CACHE_SLOTS > 2`). The OS reservation of each evicted span is
    /// released immediately.
    #[cfg(feature = "alloc-decommit")]
    pub(super) fn evict_at_least(&mut self, min_bytes: usize) {
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
            let meta = SegmentMeta::new(victim.base);
            let generation = large_generation(meta.terminal_snapshot().large_state);
            if !LargeReservationState::new(meta.large_state_atomic()).release_cached(generation) {
                std::process::abort();
            }
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
    ///
    /// R1-08: also gated on `alloc-global` — the only caller,
    /// `HeapCore::trim_for_recycle`, lives in `registry`.
    #[cfg(all(feature = "alloc-decommit", feature = "alloc-global"))]
    pub(crate) fn evict_all(&mut self) {
        while self.evict_one_oldest() {}
    }

    // ── Phase 2 test seams ────────────────────────────────────────────────────

    /// TEST-ONLY (Phase 2): force a decay tick by rewinding `last_decay_tick`
    /// to be exactly `decay_interval` in the past, then calling
    /// `maybe_decay_large_cache`. This primes the next call for a clock read
    /// without sleeping. A zero interval processes one step; a nonzero
    /// interval processes up to eight due steps if additional time elapses
    /// between rewinding the timer and reading the clock. Calls at or below
    /// headroom remain no-ops unless the diagnostic clock override is enabled.
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
    /// passes the stride throttle regardless of how many (or how few)
    /// organic calls happened before it. This guarantees a clock
    /// read when past the headroom guard, not exactly one decay step or an
    /// eviction: bounded catch-up still applies, and a step can be a no-op.
    /// See `docs/perf/R34_11_CATCHUP_DECAY_GATE.md` §1 for the accepted policy;
    /// `tests/large_cache_decay.rs` and R29-13's forced-convergence loop
    /// (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §1.6) use this seam.
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
    /// - `interval_ms`: elapsed-time period for counting due steps, not minimum spacing between invocations. Nonzero intervals use bounded catch-up (up to eight steps, timer advanced by `due * interval`, remaining debt retained); 0 runs one step on every eligible call and bypasses the R32-8 stride throttle (R2-18).
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
        // Clear the tick timer: the first eligible nonzero-interval check
        // primes it without decay; a zero interval primes and decays.
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
    /// if the cache is empty. P1-3 (#1985): iterates only the set bits of
    /// [`large_cache_occupied_within_bound`](Self::large_cache_occupied_within_bound)
    /// — O(popcount) slot reads, not one `Option` discriminant load per slot
    /// over `large_cache_scan_bound()` (8 with `large-cache-extended` off or
    /// not-yet-materialised, up to 40 once materialised). Still only called
    /// on the large-alloc/dealloc slow paths (never the small hot path).
    /// Short-circuits on `seq == 0`: that is the initial `large_cache_seq`
    /// value, so no occupied slot can beat it (the counter is monotonic) —
    /// and on u64 wraparound the old `min_by_key` picked the LOWEST-INDEX
    /// slot holding the minimum seq, which is exactly what this loop still
    /// returns, so the short-circuit is behavior-preserving even there.
    #[cfg(feature = "alloc-decommit")]
    fn oldest_occupied_slot(&self) -> Option<CombinedSlot> {
        let mut occupied = self.large_cache_occupied_within_bound();
        let mut best_idx: Option<CombinedSlot> = None;
        let mut best_seq = u64::MAX;
        while occupied != 0 {
            let i = occupied.trailing_zeros() as usize;
            occupied &= occupied - 1; // clear the lowest set bit
            if let Some(c) = self.large_cache_slot_get(i) {
                if c.seq < best_seq {
                    best_idx = Some(i);
                    best_seq = c.seq;
                    if c.seq == 0 {
                        // Initial counter value — cannot be beaten.
                        break;
                    }
                }
            }
        }
        best_idx
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
    pub(in crate::alloc_core) fn evict_one_oldest(&mut self) -> bool {
        let Some(victim_idx) = self.oldest_occupied_slot() else {
            return false;
        };
        let victim = self.large_cache_slot_take(victim_idx);
        self.large_cache_used_bytes = self
            .large_cache_used_bytes
            .saturating_sub(victim.usable_size);
        let meta = SegmentMeta::new(victim.base);
        let generation = large_generation(meta.terminal_snapshot().large_state);
        if !LargeReservationState::new(meta.large_state_atomic()).release_cached(generation) {
            std::process::abort();
        }
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
    /// `docs/CORRECTNESS_OPEN_ITEMS.md` item 9): retains `alloc-decommit`
    /// plus `internals` gating — NOT tightened to `bench-internals` like
    /// its `HeapCore`-level sibling ([`crate::registry::HeapCore::dbg_large_cache_hits`],
    /// tightened by R31-4/task #467). The two are not the same case:
    /// `HeapCore::dbg_large_cache_hits` had zero callers outside
    /// `bench-internals`-gated examples, so CLAUDE.md's benchmark-hook rule
    /// 2 ("no production caller ⇒ MUST default to `bench-internals`-gating")
    /// applied cleanly. THIS method has real regression-test callers —
    /// `tests/alloc_zeroed_fresh_large_skip.rs` and
    /// `tests/regression_large_cache_span_usable_stable.rs` both require
    /// `alloc-core`, `alloc-decommit` and `internals`, but not
    /// `bench-internals`, and assert on this method's return value.
    /// Tightening it would break those tests without a compensating rewrite.
    /// It is also a zero-argument `&self` read of an already-relaxed atomic
    /// counter (no pointer, no mutation) — the hook's read-only safety argument
    /// stands on that shape itself, not merely on inventory membership.
    /// Its historical `PURE_OBSERVERS` entry in the removed
    /// `tests/dbg_hook_safety_tripwire.rs` now lives in
    /// `scripts/verify-dbg-hook-safety.mjs`, alongside its `large_cache_used`/
    /// `large_cache_budget`/`large_cache_mode` siblings. The live scanner checks
    /// the inventory and gates; it does not prove the observer's invariant.
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
    /// during lease binding, before any alloc on
    /// this heap. Redirects all subsequent increments and diagnostic reads to
    /// the slot's `AtomicU64`, closing the aliasing gap (see
    /// [`LargeCacheHitCounter`]). Idempotent — the slot counter is `'static`,
    /// so re-planting on a re-claim is a harmless no-op.
    ///
    /// Only reachable through an active registry lease (`alloc-global`);
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
    /// [`LargeCacheConfig::mode`]: crate::alloc_core::large_cache_config::LargeCacheConfig::mode
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_mode(&self) -> LargeCacheMode {
        self.large_cache_mode
    }
}
