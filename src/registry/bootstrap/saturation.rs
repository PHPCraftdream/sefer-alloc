//! Versioned, non-authoritative full-registry hint.

use core::sync::atomic::{AtomicU64, Ordering};

const DISABLED: u64 = u64::MAX;
const LAST_VERSION: u64 = u64::MAX - 3;

/// Bit zero means a full scan found no claimable slot; the other bits are a
/// publication version. `u64::MAX` permanently disables this optimisation.
/// Only a slot-state CAS grants ownership.
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

    /// Construct a local model at a chosen boundary word; never touches the
    /// process-global registry.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_with_word_for_test(word: u64) -> Self {
        Self(AtomicU64::new(word))
    }

    pub fn snapshot(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn is_saturated(&self) -> bool {
        // An RMW cannot read a value older than a completed publisher's
        // RMW in this atomic's modification order. A plain load could give
        // a post-recycle claimant a stale negative verdict.
        let word = self.0.fetch_or(0, Ordering::Acquire);
        word != DISABLED && word & 1 != 0
    }

    /// Called after making a slot claimable, including maintenance release.
    #[allow(deprecated)] // `fetch_update` is deprecated for `try_update`, which needs a newer MSRV.
    pub fn publish_claimable(&self) {
        // One RMW both changes the version and clears saturation. Before the
        // next version would collide with the terminal word, disable the
        // optimisation forever instead of wrapping into an old snapshot.
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(if old >= LAST_VERSION {
                    DISABLED
                } else {
                    (old + 2) & !1
                })
            });
    }

    /// Commit a negative scan only if no availability publication raced it.
    pub fn try_mark_saturated(&self, snapshot: u64) {
        if snapshot != DISABLED && snapshot & 1 == 0 {
            let _ = self.0.compare_exchange(
                snapshot,
                snapshot | 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }
}
