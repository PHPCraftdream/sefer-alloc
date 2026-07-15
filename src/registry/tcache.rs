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

// TCACHE_KEY — REMOVED in P6.1 (Э6). The magazine double-free guard no longer
// stamps a per-heap key into a block's word1 (block body). The two exact
// oracles (in-magazine scan + BinTable `is_free` bitmap) are now consulted on
// every magazine free with no block-body filter gating them (see
// `HeapCore::dealloc_own_thread`), so the free path never reads or writes the
// block body — cheaper on cold working sets AND stronger M2 for the own-thread
// double-free (the old key filter was skipped once the user overwrote word1,
// missing a flushed double-free; the bitmap oracle now catches it regardless of
// block-body contents). The two oracles are EXACT for the two own-thread
// resting places (this class's magazine + the BinTable free list).
//
// RESIDUAL M2 LIMIT (task #164): they do NOT cover a block whose cross-thread
// free is still in-flight (undrained) in its segment's `RemoteFreeRing` — the
// ring push sets neither oracle. A cross-thread double-free (own-thread free of
// a block already queued in the ring) therefore slips past both. Pre-existing
// since fastbin; Э6 neither opened nor closed it. See the RESIDUAL M2 LIMIT
// note in `HeapCore::dealloc_own_thread`.

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
///
/// **R6-OPT-P0-3a (`medium-classes`, correctness-surface item #2 — "a refill
/// batch size... assumption a 3-4-block segment would violate"):** this byte
/// budget already generalises correctly to the new 256 KiB..1 MiB classes
/// with NO code change needed — it was designed (task D3) for exactly this
/// shape of problem (a large `block_size` shrinking the per-refill block
/// count), just never previously exercised past ~253 KiB. For the new 1 MiB
/// class, `REFILL_BYTE_BUDGET / block_size = 65536 / 1048576 = 0`, which
/// [`refill_n_for_class`] clamps to `1` (its documented "never 0" floor): a
/// magazine miss for the 1 MiB class refills exactly ONE block, not 16 — so a
/// 4 MiB segment holding only ~4 such blocks (see `alloc_core_small.rs`'s
/// carve/refill machinery) is never asked to refill more blocks than it can
/// hold. Verified by the `refill_n_for_medium_classes_is_bounded_by_budget`
/// test (`medium_classes` feature).
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

/// PERF-PASS-5 (G7/FP2, task #53): one size class's magazine — `count` and
/// `slots` bundled together so a magazine push/pop touches ONE cache line
/// instead of two.
///
/// Before this change, `Tcache` stored `slots: [[*mut u8; CAP]; N]` and
/// `count: [u16; N]` as two SEPARATE top-level arrays (`count` ~6 KiB away
/// from `slots` in the enclosing `HeapCore`/`HeapSlot`), so every magazine
/// hit/push/pop touched `count[c]` (one line) AND `slots[c]` (a different,
/// far-away line) — two dependent cache lines for what is architecturally a
/// single "check depth, touch top-of-stack" operation. Grouping `count` and
/// `slots` into one `PerClass` struct and using `[PerClass; N]` puts a
/// class's depth counter directly adjacent to (in front of) its own pointer
/// stack, so both live in the same 8-byte-aligned region and — for the
/// common case where a hit/push touches only the top few slots — the SAME
/// 64-byte cache line.
///
/// `count` is `u8`, not `u16`: `TCACHE_CAP` (16) fits comfortably in a `u8`
/// (max 255), and every accumulation of `count` is compared against
/// `TCACHE_CAP` (16) or `FLUSH_N` (8) before use — see the call sites in
/// `heap_core.rs` (`cnt + 1`, `remaining + 1` after a half-flush) — so no
/// arithmetic on this path ever approaches the `u8` range limit. Shrinking
/// `count` from `u16` to `u8` also shrinks `PerClass` by 1 byte per class
/// (49 classes × 1 byte saved), a minor bonus on top of the locality win.
#[derive(Clone, Copy)]
pub(crate) struct PerClass {
    /// Current magazine depth for this class (0..=`TCACHE_CAP`). `u8`: see
    /// the [`PerClass`] doc for why `TCACHE_CAP` (16) safely fits.
    pub(crate) count: u8,
    /// This class's pointer stack. `slots[0..count as usize]` are valid.
    pub(crate) slots: [*mut u8; TCACHE_CAP],
}

impl PerClass {
    /// Construct an empty per-class magazine (count zero, all slots null).
    const fn new() -> Self {
        Self {
            count: 0,
            slots: [core::ptr::null_mut(); TCACHE_CAP],
        }
    }
}

/// Per-thread, per-class magazine cache.
///
/// `classes[c].slots[0..classes[c].count as usize]` are valid free-block
/// pointers of class `c`. The magazine is owner-private (single thread
/// reads/writes it). No atomics, no locks.
pub(crate) struct Tcache {
    /// Per-class magazines: each entry bundles that class's depth counter
    /// with its pointer stack (see [`PerClass`] for the cache-locality
    /// rationale). (P3: the former `alloc_streak` companion array was
    /// removed with the P7 bulk-mode bypass — see the module-level note.)
    pub(crate) classes: [PerClass; SMALL_CLASS_COUNT],
}

impl Tcache {
    /// Construct an empty magazine (all counts zero, all slots null).
    /// `const fn` so it can be used in `HeapCore::new` with zero allocation
    /// at construction (M5-clean).
    pub(crate) const fn new() -> Self {
        Self {
            classes: [PerClass::new(); SMALL_CLASS_COUNT],
        }
    }
}
