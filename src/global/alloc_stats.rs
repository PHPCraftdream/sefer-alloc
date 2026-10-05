//! [`AllocStats`] — a cheap, process-wide diagnostic snapshot of
//! [`SeferAlloc`](super::SeferAlloc)'s internal relaxed counters (task E1).
//!
//! ## Why this exists
//!
//! Before this file, every one of these counters was `#[doc(hidden)] pub
//! dbg_*` — reachable only by this crate's own tests, not by a downstream
//! consumer. A production process running `SeferAlloc` as its
//! `#[global_allocator]` had no way to see how many segments were live, how
//! often the large-object cache hit, how many frees could not be routed or
//! published, or how many heap slots the registry has minted — all of that was
//! invisible until something went wrong badly enough to abort (e.g. OOM).
//! [`SeferAlloc::stats`](super::SeferAlloc::stats) closes
//! that gap: one cheap, lock-free snapshot a consumer (e.g. a metrics
//! exporter) can poll on a timer.
//!
//! ## Cost
//!
//! Without `alloc-stats` (the default — not part of `production`), `stats()`
//! is a fixed handful of relaxed atomic loads: no locks, no segment/heap
//! walk, no allocation. Safe to call from a metrics-scrape hot path. WITH
//! `alloc-stats`, the two hit-rate counters (`tcache_hits`/`large_cache_hits`)
//! are aggregated by a walk over the initialized registry slots (still no
//! locks, no segment payload touched — only per-slot counter metadata); every
//! other field remains a single relaxed load either way. See
//! [`SeferAlloc::stats`](super::SeferAlloc::stats)'s doc for the full
//! per-mode cost breakdown (R3-A, round3 finding N1).
//!
//! ## Stability across feature combinations
//!
//! `AllocStats` has a **fixed set of fields regardless of which optional
//! features are enabled**. A counter that lives behind a feature not
//! compiled into this build simply reads back `0` — the struct's shape never
//! changes between feature combinations, so downstream code that matches on
//! `AllocStats` fields compiles and behaves predictably no matter which
//! `sefer-alloc` feature set the binary was built with.
//!
//! ## Diagnostic, not accounting-grade
//!
//! Counter reads use relaxed atomics; the optional hit-counter walk also uses
//! an Acquire load to check each slot's initialization before reading it.
//! There is no cross-field synchronisation: fields may reflect different
//! points in concurrent activity. Fine for monitoring and alerting; do not
//! treat the snapshot (or a computed delta) as an exact, linearizable count.

/// A process-wide snapshot of `SeferAlloc`'s diagnostic counters, returned by
/// [`SeferAlloc::stats`](super::SeferAlloc::stats).
///
/// All fields are cumulative (since process start) unless documented
/// otherwise, and are relaxed-atomic snapshots — see the module docs for the
/// consistency and feature-availability caveats. A field backed by a counter
/// that is not compiled into this build (its feature is off) always reads
/// `0`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AllocStats {
    /// Number of `alloc_large` calls served directly from the per-heap
    /// large-object cache (a cache hit) since process start, summed as
    /// observed process-wide. Requires the `alloc-decommit` feature; `0`
    /// otherwise.
    ///
    /// **Also requires the `alloc-stats` feature (task W3).** The per-hit
    /// increment is gated behind `alloc-stats` (default OFF, and NOT part of
    /// `production`) so the large-cache hit fast path carries no counter
    /// bookkeeping by default. Without `alloc-stats` this field reads `0` even
    /// when large-cache hits are occurring; build with
    /// `--features "production alloc-stats"` (or add `alloc-stats` to your
    /// feature set) to get the real count.
    pub large_cache_hits: u64,

    /// Number of logical entries into the small-segment decommit helper (the
    /// M6 "decommit an emptied segment" decision point) since process start,
    /// across both decommit variants. Requires the `alloc-decommit` feature;
    /// `0` otherwise.
    ///
    /// **This is NOT an OS decommit-syscall count.** Callers whose segment is
    /// immediately recycled take the `release_follows` fast path: the whole
    /// reservation goes back to the OS via the release call
    /// (`MEM_RELEASE` / `munmap`) instead, so the helper returns after only a
    /// bump-cursor reset and never issues a payload `decommit_pages` syscall
    /// at all. Those entries are still counted here (deliberately — see the
    /// counting note in `src/alloc_core/small/alloc_core_small_pool/decommit.rs`),
    /// so this field is an upper bound on real page-decommit syscalls, not an
    /// equality.
    pub decommit_calls: u64,

    /// Completed owner-side Large remote retirements since process start,
    /// whether the retired reservation enters the cache or is released to the
    /// OS. A terminal publication alone does not increment this counter;
    /// the owner must claim and reclaim it. Always live under `alloc-global`;
    /// local direct frees do not increment it.
    pub large_xthread_reclaimed: u64,

    /// Number of small allocations served from a thread's per-class magazine
    /// cache (`fastbin` tcache hit) since process start. Requires the
    /// `fastbin` feature (which implies `alloc-global` + `alloc-xthread`);
    /// `0` otherwise.
    ///
    /// **Also requires the `alloc-stats` feature (task W3).** The per-hit
    /// increment is gated behind `alloc-stats` (default OFF, and NOT part of
    /// `production`) so the magazine (churn) hot path carries no counter
    /// bookkeeping by default — the measured saving is a few instructions per
    /// hit on the hottest path in the allocator. Without `alloc-stats` this
    /// field reads `0` even when magazine hits are occurring; build with
    /// `--features "production alloc-stats"` to get the real count.
    pub tcache_hits: u64,

    /// Cumulative count of successful OS segment reservations since process
    /// start, across every heap in the process (small-heap segments,
    /// large-object segments, NUMA-pinned segments). Monotonic — always
    /// available (not feature-gated); every build reserves segments through
    /// the same OS seam.
    pub segments_reserved_total: u64,

    /// Cumulative count of successful OS segment releases since process
    /// start, across every heap in the process. Monotonic — always
    /// available.
    ///
    /// `segments_reserved_total - segments_released_total` is the
    /// approximate process-wide **live segment count** (subject to the
    /// relaxed-ordering skew documented on the struct) — the single most
    /// useful field for spotting a segment leak (classes A1/D2) in
    /// production before it escalates to an OOM abort.
    pub segments_released_total: u64,

    /// High-water mark of registry heap slots ever claimed (minted) since
    /// process start — i.e. the largest number of distinct heap slots the
    /// process has needed simultaneously-or-sequentially. This is **not** a
    /// live count: a claimed-then-recycled slot is still counted (recycled
    /// slots are reused for new threads, never un-minted). Always available.
    ///
    /// `u64` for consistency with every other field on this struct (the
    /// underlying registry counter is a `u32`; widened here via `as u64` —
    /// see `SeferAlloc::stats()`).
    pub heaps_claimed_high_water: u64,

    /// Cumulative, process-wide count of frees dropped when address-only
    /// route lookup finds no matching live route or a pinned route's terminal
    /// sidecar rejects publication. This is **not** a ring-overflow count:
    /// there is no ring on the current foreign-free path. It is diagnostic,
    /// not an exhaustive audit of invalid `GlobalAlloc::dealloc` calls;
    /// callers still must supply a live allocation and its exact `Layout`.
    ///
    /// **Always available — no `alloc-stats` required.** `alloc-global`
    /// always implies `alloc-xthread` (R5-01), and the cross-thread routing
    /// drop branches (`HeapCore::publish_foreign`) increment this counter
    /// unconditionally, not gated behind `alloc-stats`. A standalone
    /// `AllocCore::dealloc` also counts its rejected foreign branch when
    /// `alloc-stats` is enabled. A healthy program keeps this at `0`;
    /// investigate a non-zero, growing value as a dropped free.
    pub foreign_or_unroutable_frees: u64,

    /// Number of times `claim_lease_with_config` found an already-materialised
    /// registry slot whose live (resolved) large-cache/pool policy differs
    /// from the requested config. Each such event means the slot's
    /// pre-existing config silently overrides the caller's request
    /// (first-materialisation-wins semantics — see
    /// [`SeferAlloc::with_config`](super::SeferAlloc::with_config)'s doc for
    /// the full binding-semantics explanation).
    ///
    /// **This is the field to monitor when running multiple `SeferAlloc`
    /// instances with different configs in one process** (unusual and
    /// effectively unsupported — see `with_config`'s doc). In the normal
    /// single-global-allocator case this stays at `0`.
    ///
    /// Unlike the hit counters (`tcache_hits`, `large_cache_hits`), this
    /// counter's increment is NOT gated
    /// behind `alloc-stats`: it lives on the cold claim/bind path (at most
    /// one increment per thread bind, never on the alloc/dealloc fast path),
    /// so there is no perf cost to always compiling it in. Requires the
    /// `alloc-decommit` feature (where `claim_lease_with_config` exists); `0`
    /// otherwise.
    pub config_conflicts: u64,
}
