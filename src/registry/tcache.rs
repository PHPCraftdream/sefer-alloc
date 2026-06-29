//! [`Tcache`] -- per-thread, per-class magazine cache (Phase P2).
//!
//! A fixed array of per-class magazines, each an array of pointers (a
//! "magazine"/"stack"). Push/pop touch only the magazine array (hot,
//! sequential, cache-friendly); the block's own memory is not read until
//! the user uses it (no dependent load on the hit path).
//!
//! Owner-private: only the owning thread touches it. No atomics, no locks.
//! Cross-thread frees never touch it (they go to the per-segment ring).

use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

/// Bulk-mode bypass threshold (P7). When a class's consecutive-refill
/// streak reaches this value the magazine fast path is skipped for that
/// class — allocs go directly to `core.alloc` and frees to `core.dealloc`.
/// This avoids the per-free overflow flush cost on alloc-without-free
/// streaks (the bulk microbench pattern).
///
/// The streak counts consecutive magazine **misses** (refills), NOT
/// individual allocs. Each refill pulls `REFILL_N` (= `TCACHE_CAP` = 16)
/// blocks. So `BULK_THRESHOLD = 3` means 3 consecutive refills without an
/// intervening overflow (= 48 allocs). This keeps the magazine HIT path
/// (the churn hot path) completely streak-free — no read, no write.
///
/// Only the refill (miss) and dealloc overflow paths touch the streak,
/// and both are already slow paths.
pub(crate) const BULK_THRESHOLD: u8 = 3;

/// Bulk-mode re-entry hysteresis (P7). Documented design parameter;
/// currently unused in the implementation. In the current design,
/// bulk mode has no explicit exit: the streak stays high once set.
/// Under churn the magazine stays populated (alloc hits, dealloc
/// pushes), so the streak check (on miss or overflow) is never
/// reached — bulk mode is "dormant." If the workload shifts to
/// churn, the magazine path handles it without checking the streak.
#[allow(dead_code)]
pub(crate) const BULK_LOW_THRESHOLD: u8 = 0;

/// Magic constant for tcache-resident block marker (M2 double-free guard, P3).
///
/// Non-zero so an all-zero freshly-carved block does not collide. The actual
/// key written into a block's word1 is `TCACHE_KEY ^ (heap.id as usize)` so
/// different heaps have different keys (defence against confusion across
/// registry slots).
///
/// Value: ASCII bytes "SEFERCAC" packed into a `usize`. On 32-bit targets
/// only the low 4 bytes ("SEFE") are used, which is still non-zero and
/// distinctive.
pub(crate) const TCACHE_KEY: usize = 0x53_45_46_45_52_43_41_43;

/// Magazine capacity per size class. Start: 16. Tuned in P6.
pub(crate) const TCACHE_CAP: usize = 16;

/// Refill batch size (how many blocks refill_class pulls on a magazine miss).
pub(crate) const REFILL_N: usize = TCACHE_CAP;

/// Flush batch size on magazine overflow. Half-flush hysteresis: leave
/// `CAP - FLUSH_N` entries in the magazine after a flush, avoiding
/// flush/refill thrash when the working set hovers near CAP.
pub(crate) const FLUSH_N: usize = TCACHE_CAP / 2; // 8

/// Per-thread, per-class magazine cache.
///
/// `slots[c][0..count[c]]` are valid free-block pointers of class `c`.
/// The magazine is owner-private (single thread reads/writes it). No
/// atomics, no locks.
pub(crate) struct Tcache {
    /// Per-class pointer stacks. `slots[c][0..count[c]]` are valid.
    pub(crate) slots: [[*mut u8; TCACHE_CAP]; SMALL_CLASS_COUNT],
    /// Per-class magazine count + refill-streak pair. Laid out
    /// contiguously so `count[c]` and `alloc_streak[c]` share the same
    /// cache line for any class `c`, avoiding an extra cache-line touch
    /// on the dealloc path.
    ///
    /// **count:** current depth per class (0..=TCACHE_CAP).
    pub(crate) count: [u16; SMALL_CLASS_COUNT],
    /// **alloc_streak (P7):** consecutive-refill-miss counter per class.
    /// Incremented on each magazine miss (refill) for class `c`. Saturates
    /// at 255 (`u8::MAX`). NOT touched on the magazine HIT or PUSH paths
    /// (zero overhead on the churn hot path).
    ///
    /// When `alloc_streak[c] >= BULK_THRESHOLD`, allocs bypass the
    /// magazine (go directly to `core.alloc`). On the dealloc side,
    /// overflow with `streak >= BULK_THRESHOLD` flushes the full magazine
    /// and frees directly via `core.dealloc`.
    pub(crate) alloc_streak: [u8; SMALL_CLASS_COUNT],
}

impl Tcache {
    /// Construct an empty magazine (all counts zero, all slots null).
    /// `const fn` so it can be used in `HeapCore::new` with zero allocation
    /// at construction (M5-clean).
    pub(crate) const fn new() -> Self {
        Self {
            slots: [[core::ptr::null_mut(); TCACHE_CAP]; SMALL_CLASS_COUNT],
            count: [0u16; SMALL_CLASS_COUNT],
            alloc_streak: [0u8; SMALL_CLASS_COUNT],
        }
    }
}
