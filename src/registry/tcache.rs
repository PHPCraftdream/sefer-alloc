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

// P7 bulk-mode bypass — RETIRED in P3 (task #147). The former
// `BULK_THRESHOLD` / `BULK_LOW_THRESHOLD` / `alloc_streak` machinery skipped
// the magazine on alloc-without-free streaks. With Э1 (bump-direct batched
// carve, see `AllocCore::refill_class_bump`) a magazine miss now carves
// straight into the magazine at near-`memcpy` cost, so bulk mode buys
// nothing and both the alloc-side and dealloc-side bypasses were removed
// (retiring one without the other would leave a permanently-dead branch).

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
///
/// This is the physical size of the `slots[c]` array — a COMPILE-TIME bound,
/// shared by every class regardless of `block_size`. It is NOT the per-class
/// refill amount (see [`refill_n_for_class`], task D3): a class may use fewer
/// than `TCACHE_CAP` slots on a given refill, but the array itself must be
/// sized for the largest per-class refill (`TCACHE_CAP` itself, for the
/// smallest classes) plus headroom for `push`-side accumulation up to a full
/// magazine before an overflow flush.
pub(crate) const TCACHE_CAP: usize = 16;

/// Per-class byte budget for a magazine refill (task D3). mimalloc-style:
/// cap the bytes a single refill parks in one thread's magazine, not just the
/// block COUNT. With `SMALL_MAX` around 253 KiB, a large small-class refilled
/// at a fixed `TCACHE_CAP` (= 16) block count would park up to
/// `16 * ~253 KiB` ≈ 4 MiB in a single thread's magazine for ONE size class —
/// real RSS parked in a per-thread cache that may sit idle. 64 KiB keeps a
/// refill's footprint bounded while leaving small classes (whose blocks are
/// tiny) at their old fixed-`TCACHE_CAP` behaviour (the byte budget for them
/// comfortably exceeds `TCACHE_CAP * block_size`).
///
/// Task D3 replaced the former unconditional `REFILL_N = TCACHE_CAP` constant
/// (used for every class regardless of `block_size`) with this budget, read
/// through [`refill_n_for_class`].
pub(crate) const REFILL_BYTE_BUDGET: usize = 64 * 1024;

/// Compute the refill amount (number of blocks) for a class with the given
/// `block_size`, honouring both `REFILL_BYTE_BUDGET` and the physical
/// `TCACHE_CAP` array-size ceiling (task D3).
///
/// `refill_n = clamp(REFILL_BYTE_BUDGET / block_size, 1, TCACHE_CAP)`
///
/// - Never 0: even a class whose single block exceeds the byte budget still
///   gets 1 block per refill (a magazine miss must make progress).
/// - Never more than `TCACHE_CAP`: the physical `slots[c]` array bound.
/// - Small classes (`block_size` small relative to the budget) get the full
///   `TCACHE_CAP`, matching the pre-D3 behaviour exactly (no regression for
///   the common small-object case).
/// - Large small-classes (block_size approaching `SMALL_MAX`) get fewer
///   blocks per refill, bounding the RSS a single magazine miss parks in one
///   thread's cache.
#[inline]
pub(crate) const fn refill_n_for_class(block_size: usize) -> usize {
    if block_size == 0 {
        return TCACHE_CAP; // defensive; block_size is always > 0 in practice.
    }
    let by_budget = REFILL_BYTE_BUDGET / block_size;
    if by_budget == 0 {
        1
    } else if by_budget > TCACHE_CAP {
        TCACHE_CAP
    } else {
        by_budget
    }
}

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
    /// Per-class magazine depth. **count:** current depth per class
    /// (0..=TCACHE_CAP). (P3: the former `alloc_streak` companion array was
    /// removed with the P7 bulk-mode bypass — see the module-level note.)
    pub(crate) count: [u16; SMALL_CLASS_COUNT],
}

impl Tcache {
    /// Construct an empty magazine (all counts zero, all slots null).
    /// `const fn` so it can be used in `HeapCore::new` with zero allocation
    /// at construction (M5-clean).
    pub(crate) const fn new() -> Self {
        Self {
            slots: [[core::ptr::null_mut(); TCACHE_CAP]; SMALL_CLASS_COUNT],
            count: [0u16; SMALL_CLASS_COUNT],
        }
    }
}
