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
    /// Current depth per class.
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
