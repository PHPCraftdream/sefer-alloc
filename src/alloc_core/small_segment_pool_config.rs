//! [`SmallSegmentPoolConfig`] — a `const`-buildable configuration type for the
//! **empty-small-segment hysteresis pool** (Mechanism 2, feature =
//! `alloc-decommit`).
//!
//! ## What the pool is
//!
//! Without this pool, the instant a small segment's `live_count` reaches zero
//! (own-thread free, cross-thread ring drain, or a batched flush) its payload
//! is decommitted, its OS reservation released (`MEM_RELEASE` / `munmap`), and
//! its `SegmentTable` slot recycled. A workload that churns a working set
//! across a segment boundary (allocate N blocks, free them, reallocate N
//! blocks, …) therefore pays a full OS reserve → carve → release → re-reserve
//! cycle every oscillation — the exact shape the `working_set_cycle` bench
//! reproduces at 1024 B.
//!
//! The pool interposes a bounded, committed hysteresis buffer: when a small
//! segment empties, instead of releasing it, the allocator MAY retain it —
//! still registered in the `SegmentTable`, its pages still COMMITTED, its
//! per-class free lists still populated with the blocks that were just freed.
//! The very next allocation that would otherwise reserve a fresh segment
//! (`reserve_small_segment`) pops a pooled segment first: no OS syscall, no
//! metadata re-init, no page fault — the blocks are already on the free list,
//! so `pop_free` / `find_segment_with_free` serve straight out of it.
//!
//! Pages stay committed the ENTIRE time a segment is pooled — there is NO
//! `os::decommit_pages` / `os::recommit_pages` round-trip for a pooled segment
//! (that "decommit-then-pool" variant was rejected: the whole point is to
//! avoid the syscall/fault cost, and a committed 4 MiB span held briefly is a
//! bounded RSS cost the byte-cap governs).
//!
//! ## Bounded — no permanent pin
//!
//! The pool is a strict, small cap (`pool_segments`, default 4) governed
//! additionally by a byte ceiling (`pool_byte_cap`, default 16 MiB). When a
//! segment empties and the pool is already full, the emptying segment is
//! released immediately (today's behaviour) — the pool never holds MORE than
//! its cap at any instant, mid-scan or otherwise. This bounded retention is
//! what keeps the `regression_c3_unbounded_recycle` guarantee ("no unbounded /
//! permanent pinning of table slots") intact: at most `pool_segments` slots are
//! ever retained, and every retained slot is reusable (popped on the next
//! reserve) or drainable (evicted + recycled). See that test for the explicit
//! bounded-retention + eventual-drain proof.
//!
//! ## Default values
//!
//! `SmallSegmentPoolConfig::DEFAULT` (= `SmallSegmentPoolConfig::new()`) enables
//! the pool with the orchestrator's chosen production defaults:
//!
//! | Knob | Default | Meaning |
//! |---|---|---|
//! | `pool_segments` | `4` | retain at most 4 empty small segments |
//! | `pool_byte_cap` | `16 MiB` | …and at most 16 MiB of committed pool RSS |
//!
//! Setting EITHER knob to `0` disables the pool entirely — every empty small
//! segment is released immediately, byte-for-byte the pre-Mechanism-2 (task
//! #51) behaviour.
//!
//! ## Wiring
//!
//! `SmallSegmentPoolConfig` is carried as a field of
//! [`LargeCacheConfig`](super::large_cache_config::LargeCacheConfig) — both are
//! `alloc-decommit`-gated construction-time knobs threaded through the SAME
//! single-config plumbing (`SeferAlloc::with_config` →
//! `HeapRegistry::claim_with_config` → `HeapCore::new_with_config` →
//! `AllocCore::new_with_config`). Embedding it there (rather than adding a
//! second config parameter through four layers) keeps every existing
//! `with_config(cfg)` caller working unchanged while still honouring the
//! one-file-one-export rule: this file exports exactly
//! `SmallSegmentPoolConfig`; `LargeCacheConfig` merely holds one of them.

/// Compile-time-buildable configuration for the empty-small-segment hysteresis
/// pool (Mechanism 2).
///
/// Construct with [`new`](Self::new) (or the [`DEFAULT`](Self::DEFAULT)
/// constant) and chain the setter methods:
///
/// ```rust
/// # #[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
/// # {
/// use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};
///
/// const POOL: SmallSegmentPoolConfig = SmallSegmentPoolConfig::new()
///     .pool_segments(8)
///     .pool_byte_cap(32 * 1024 * 1024);
///
/// const CONFIG: LargeCacheConfig = LargeCacheConfig::new().pool(POOL);
/// # let _ = CONFIG;
/// # }
/// ```
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, Debug)]
pub struct SmallSegmentPoolConfig {
    /// Maximum number of empty small segments retained in the pool. `None`
    /// uses the default (4). `Some(0)` disables the pool (immediate release —
    /// the pre-Mechanism-2 behaviour). `Some(n > 0)` caps retention at `n`
    /// segments (subject also to `pool_byte_cap`).
    pub(crate) pool_segments: Option<usize>,

    /// Maximum committed bytes held by the pool. `None` uses the default
    /// (16 MiB). `Some(0)` disables the pool. `Some(n > 0)` caps the pool's
    /// committed RSS; since every small segment is exactly `SEGMENT` bytes, the
    /// effective segment cap is `min(pool_segments, pool_byte_cap / SEGMENT)`.
    pub(crate) pool_byte_cap: Option<usize>,
}

#[cfg(feature = "alloc-decommit")]
impl Default for SmallSegmentPoolConfig {
    /// Returns [`SmallSegmentPoolConfig::DEFAULT`].
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(feature = "alloc-decommit")]
impl SmallSegmentPoolConfig {
    /// Default number of pooled small segments: 4.
    pub(crate) const DEFAULT_POOL_SEGMENTS: usize = 4;

    /// Default pool byte ceiling: 16 MiB (= 4 × the 4 MiB `SEGMENT`).
    pub(crate) const DEFAULT_POOL_BYTE_CAP: usize = 16 * 1024 * 1024;

    /// The default configuration — pool ENABLED at the production defaults
    /// (4 segments / 16 MiB). Equivalent to `SmallSegmentPoolConfig::new()`.
    ///
    /// This is what `production` gets with no explicit opt-in: `SeferAlloc::new`
    /// → `LargeCacheConfig::DEFAULT` → this `DEFAULT`.
    pub const DEFAULT: Self = Self::new();

    /// Construct a config with all knobs at their defaults (pool enabled,
    /// 4 segments / 16 MiB).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool_segments: None,
            pool_byte_cap: None,
        }
    }

    /// Set the maximum number of empty small segments retained in the pool.
    ///
    /// `0` disables the pool (every empty small segment is released
    /// immediately — the pre-Mechanism-2 behaviour). Any `n > 0` is honoured
    /// exactly (RAD-3/E2, task #56: the pool's storage is an intrusive list
    /// threaded through the pooled segments' own headers, not a fixed-size
    /// array, so there is no compile-time upper bound to silently clamp
    /// against) — subject only to [`pool_byte_cap`](Self::pool_byte_cap): the
    /// resolved cap is `min(pool_segments, pool_byte_cap / SEGMENT)`. A very
    /// large `pool_segments` with the default (or a small) byte cap is
    /// therefore still bounded by the byte budget, not by this knob alone.
    ///
    /// Default: 4.
    #[must_use]
    pub const fn pool_segments(mut self, n: usize) -> Self {
        self.pool_segments = Some(n);
        self
    }

    /// Set the maximum committed bytes held by the pool.
    ///
    /// `0` disables the pool. Since every small segment is exactly `SEGMENT`
    /// (4 MiB), the effective segment cap is
    /// `min(pool_segments, pool_byte_cap / SEGMENT)`.
    ///
    /// Default: 16 MiB.
    #[must_use]
    pub const fn pool_byte_cap(mut self, bytes: usize) -> Self {
        self.pool_byte_cap = Some(bytes);
        self
    }

    // ── Resolution helpers (pub(crate)) ──────────────────────────────────────

    /// Resolve the segment cap. Falls back to the default (4) when unset.
    #[must_use]
    pub(crate) const fn resolved_pool_segments(&self) -> usize {
        match self.pool_segments {
            Some(v) => v,
            None => Self::DEFAULT_POOL_SEGMENTS,
        }
    }

    /// Resolve the byte cap. Falls back to the default (16 MiB) when unset.
    #[must_use]
    pub(crate) const fn resolved_pool_byte_cap(&self) -> usize {
        match self.pool_byte_cap {
            Some(v) => v,
            None => Self::DEFAULT_POOL_BYTE_CAP,
        }
    }
}
