//! Compatibility re-export for the canonical reservation-state implementation.
#[doc(hidden)]
pub use sefer_segment::support::reservation_state_impl::LargeReservationState;
//
// P2.1 structure-limits split: this compat module also hosts the R6-CR-3/9
// Large debug-injection hooks and their statics, moved verbatim out of
// `alloc_core_large.rs` (the `large/` directory was at its 7-entry limit, so
// no new sibling file was possible). Pure code movement, no behavior change;
// the statics stay `pub(crate)` so `alloc_core_large.rs` can load them.

#[cfg(feature = "internals")]
use core::sync::atomic::{AtomicBool, Ordering};

use crate::alloc_core::alloc_core::AllocCore;

// R6-CR-3 test hook: forces `alloc_large` to fail (null) without touching
// allocator metadata, so a test can drive the realloc move-leg OOM path.
#[cfg(feature = "internals")]
pub(crate) static DBG_INJECT_LARGE_ALLOC_OOM: AtomicBool = AtomicBool::new(false);
// R6-CR-9 test hook: forces the FIRST non-NUMA biased Large reserve attempt
// to fail; the post-drain retry stays real.
#[cfg(feature = "internals")]
pub(crate) static DBG_INJECT_BIASED_RESERVE_FAIL: AtomicBool = AtomicBool::new(false);

// Forces the fresh-path `register_payload` failure branch (advice already reserved).
#[cfg(feature = "internals")]
pub(crate) static DBG_INJECT_LARGE_REGISTER_FAIL: AtomicBool = AtomicBool::new(false);

impl AllocCore {
    /// `#[doc(hidden)]` test hook (R6-CR-3) — not public API. Forces
    /// `alloc_large` to return null while set. A test MUST clear it (Drop guard).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_set_inject_large_alloc_oom(on: bool) {
        DBG_INJECT_LARGE_ALLOC_OOM.store(on, Ordering::Relaxed);
    }

    /// `#[doc(hidden)]` test hook (R6-CR-9) — not public API. Forces the first
    /// non-NUMA biased Large reserve attempt to return None while set.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_set_inject_biased_reserve_fail(on: bool) {
        DBG_INJECT_BIASED_RESERVE_FAIL.store(on, Ordering::Relaxed);
    }

    /// `#[doc(hidden)]` test hook — not public API. Forces the fresh-path
    /// Large registration to fail while set, after any advice was reserved.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_set_inject_large_register_fail(on: bool) {
        DBG_INJECT_LARGE_REGISTER_FAIL.store(on, Ordering::Relaxed);
    }

    /// `#[doc(hidden)]` test forwarder (R6-CR-4) — not public API. Reaches
    /// the unsafe `alloc_large` directly with a raw size a `Layout` cannot
    /// express (`from_size_align` rejects sizes above `isize::MAX`). Same
    /// caller contract as `alloc_large` itself — no wrapper guard.
    ///
    /// # Safety
    ///
    /// - `align` MUST be a non-zero power of two (the same `alloc_large`
    ///   caller contract).
    /// - `size` may be ANY `usize`, including overflow-inducing values:
    ///   `alloc_large`'s checked arithmetic surfaces them as a null return,
    ///   not UB.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // P2.5: item-scoped; same caller contract as `alloc_large` (# Safety above).
    pub unsafe fn dbg_alloc_large_for_test(&mut self, size: usize, align: usize) -> *mut u8 {
        // SAFETY (`alloc_large` # Safety): upheld by THIS function's caller —
        // `align` is a non-zero power of two; `size` may be any `usize` and
        // `alloc_large`'s header-offset `checked_add` plus every downstream
        // rounding/reservation step are checked, so an inexpressible size
        // yields a null return rather than wrapped arithmetic.
        unsafe { self.alloc_large(size, align) }.0
    }
}
