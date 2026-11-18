//! Stable identities for diagnostic counters originating in segment/route.

#[cfg(feature = "bench-internals")]
use core::sync::atomic::AtomicU64;

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static B4_DIRECTORY_LOOKUPS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static B4_SHARD_LOCK_SPINS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static B4_SHARD_LOCK_YIELDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static B4_U31_SCAN_HOLDERS_BASE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static B4_U31_SCAN_SEGMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static CONTAINS_BASE_TIER1_HITS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub static CONTAINS_BASE_TIER1_MISSES: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
#[inline(always)]
pub fn tier1_bump(counter: &AtomicU64) {
    counter.store(
        counter
            .load(core::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1),
        core::sync::atomic::Ordering::Relaxed,
    );
}
