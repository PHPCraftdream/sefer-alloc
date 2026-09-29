//! Versioned, non-authoritative full-registry hint.

use core::sync::atomic::{AtomicU64, Ordering};

/// Bit zero means a full scan found no claimable slot; the other bits are a
/// publication version. Only a slot-state CAS grants ownership.
#[doc(hidden)]
pub struct SaturationHint(AtomicU64);

impl Default for SaturationHint {
    fn default() -> Self {
        Self::new()
    }
}

impl SaturationHint {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn snapshot(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn is_saturated(&self) -> bool {
        // An RMW cannot read a value older than a completed publisher's
        // RMW in this atomic's modification order. A plain load could give
        // a post-recycle claimant a stale negative verdict.
        self.0.fetch_or(0, Ordering::Acquire) & 1 != 0
    }

    /// Called after making a slot claimable, including maintenance release.
    pub fn publish_claimable(&self) {
        // One RMW both changes the version and clears saturation: a scanner
        // racing this publication cannot overwrite it with an old verdict.
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(old.wrapping_add(2) & !1)
            });
    }

    /// Commit a negative scan only if no availability publication raced it.
    pub fn try_mark_saturated(&self, snapshot: u64) {
        if snapshot & 1 == 0 {
            let _ = self.0.compare_exchange(
                snapshot,
                snapshot | 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}
