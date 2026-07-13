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
    AllocCore, LargeCacheDecayConfig, LargeCacheHitCounter, LARGE_CACHE_SLOTS,
};

impl AllocCore {
    // ── Phase 2 — lazy decay helpers ─────────────────────────────────────────

    /// Check whether enough wall-clock time has elapsed since the last decay
    /// tick; if so, run one decay step. Called at the top of both
    /// `alloc_large` and the large-dealloc branch so the "tax" on each large
    /// operation is a single `Instant::now()` comparison — nanosecond-range
    /// overhead, negligible against OS reservation costs.
    #[cfg(feature = "alloc-decommit")]
    pub(super) fn maybe_decay_large_cache(&mut self) {
        // FAST-PATH EARLY EXIT — avoid `Instant::now()` (a `QueryPerformanceCounter`
        // syscall on Windows, ~50-100 ns) when there is provably no work to do.
        // The decay can only ever release bytes when `cached > headroom`. If the
        // cache is at or below the headroom, `run_decay_step` would compute
        // `excess = 0` and bail anyway, so we skip the wall-clock read entirely.
        //
        // This covers the dominant benchmark workload (alloc+free cycle with one
        // cached span at ~4-16 MiB, far below the 256 MiB default headroom) and
        // restores the ~45 ns cache-hit timing that the unconditional clock read
        // had regressed to ~150 ns. See task #95.
        //
        // Correctness: a true decay opportunity (cached > headroom) only arises
        // *after* a `dealloc` deposit grows `large_cache_used_bytes` past
        // `headroom_bytes`; we then hit this path on the next op and do the
        // proper time-based decision.
        if self.large_cache_used_bytes <= self.decay_config.headroom_bytes {
            return;
        }
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
        self.last_decay_tick = Some(now);
        self.run_decay_step();
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
            let victim = self.large_cache[victim_idx].take().unwrap();
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
        self.maybe_decay_large_cache();
    }

    /// TEST-ONLY (Phase 2): override the decay configuration at runtime.
    /// Lets tests specify exact parameters without relying on env vars
    /// (which are process-global and therefore flaky in parallel runs).
    ///
    /// - `rate_bp`: decay rate in basis points (100 = 1%, 1000 = 10%).
    /// - `interval_ms`: minimum ms between ticks (0 = fire on every call).
    /// - `headroom`: target cache size in bytes.
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

    /// TEST-ONLY (Phase 2): return the current decay configuration as
    /// `(decay_rate_bp, decay_interval_ms, headroom_bytes)`.
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

    /// Find the occupied slot with the smallest `seq` — the true FIFO-oldest
    /// entry (task D1). Returns `None` if the cache is empty. `O(LARGE_CACHE_SLOTS)`;
    /// only called on the large-alloc/dealloc slow paths (never the small hot
    /// path), so the linear scan is not performance-sensitive even with 8
    /// slots.
    #[cfg(feature = "alloc-decommit")]
    fn oldest_occupied_slot(&self) -> Option<usize> {
        self.large_cache
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|c| (i, c.seq)))
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
        let victim = self.large_cache[victim_idx].take().unwrap();
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
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_used(&self) -> usize {
        self.large_cache_used_bytes
    }

    /// TEST/DIAGNOSTIC-ONLY (task D1 → #133): count of `alloc_large` calls
    /// served from `large_cache` (cache hits) for THIS `AllocCore` since it
    /// was constructed. Relaxed load of `large_cache_hits` — diagnostic
    /// only. Task #133 moved this from a process-wide `static` to a
    /// per-heap instance field (see its doc comment); callers that need the
    /// process-wide total should use
    /// `registry::heap_registry::large_cache_hits_total`, which sums this
    /// method's result across every live registry slot.
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
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_set_large_cache_budget(&mut self, budget: Option<usize>) {
        self.large_cache_budget_bytes = budget;
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
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_large_cache_mode(&self) -> LargeCacheMode {
        self.large_cache_mode
    }
}
