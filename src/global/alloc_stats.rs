//! [`AllocStats`] — a cheap, process-wide diagnostic snapshot of
//! [`SeferAlloc`](super::SeferAlloc)'s internal relaxed counters (task E1).
//!
//! ## Why this exists
//!
//! Before this file, every one of these counters was `#[doc(hidden)] pub
//! dbg_*` — reachable only by this crate's own tests, not by a downstream
//! consumer. A production process running `SeferAlloc` as its
//! `#[global_allocator]` had no way to see how many segments were live, how
//! often the large-object cache hit, how many cross-thread frees the ring
//! dropped, or how many heap slots the registry has minted — all of that was
//! invisible until something went wrong badly enough to abort (OOM, ring
//! saturation, ...). [`SeferAlloc::stats`](super::SeferAlloc::stats) closes
//! that gap: one cheap, lock-free snapshot a consumer (e.g. a metrics
//! exporter) can poll on a timer.
//!
//! ## Cost
//!
//! `stats()` is a fixed handful of relaxed atomic loads — no locks, no
//! segment/heap walk, no allocation. Safe to call from a metrics-scrape hot
//! path.
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
//! Every field is a `Relaxed`-ordered `AtomicU64`/`AtomicU32` load. There is
//! no cross-field synchronisation: two fields read a few nanoseconds apart
//! may reflect slightly different points in concurrent activity on other
//! threads. Fine for monitoring and alerting; do not treat any field (or a
//! computed delta) as an exact, linearizable count.

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

    /// Number of M6 "decommit an emptied segment's payload pages back to the
    /// OS" invocations since process start. Requires the `alloc-decommit`
    /// feature; `0` otherwise.
    pub decommit_calls: u64,

    /// Number of large allocations reclaimed from another thread's heap via
    /// the cross-thread large-object reclaim path (task A1) since process
    /// start. Requires the `alloc-xthread` feature; `0` otherwise.
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

    /// Number of times a cross-thread free could not be pushed onto a
    /// segment's remote-free ring because the ring was full. On overflow the
    /// freed block is **discarded** (it stays mapped and unused — a bounded
    /// leak; see "Overflow semantics" in
    /// [`remote_free_ring`](crate::alloc_core::remote_free_ring)'s module
    /// docs). This is sound (no UAF, no corruption) but is NOT free — a
    /// sustained high rate here means blocks are actually being leaked and
    /// indicates ring-capacity pressure worth tuning (e.g. a larger ring, or
    /// more frequent owner-side drains). Requires the `alloc-xthread`
    /// feature; `0` otherwise.
    pub ring_overflows: u64,

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
    /// process-wide **live segment count** at snapshot time (modulo the
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
}
