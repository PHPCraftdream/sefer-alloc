//! Owning word for the existing LargeReservationState transition protocol.

use super::{AtomicU64, Ordering};

use crate::alloc_core::reservation_state::LargeReservationState;
use crate::alloc_core::segment_header::{large_generation, pack_large_state, LargePhase};

pub struct LargeState {
    word: AtomicU64,
}

impl LargeState {
    /// Fresh Large reservation, before first user-visible issue.
    pub fn new() -> Self {
        Self {
            word: AtomicU64::new(pack_large_state(LargePhase::Live, 1)),
        }
    }

    fn state(&self) -> LargeReservationState<'_> {
        LargeReservationState::new(&self.word)
    }

    /// Producer terminal LIVE(g) -> PENDING(g) CAS.
    pub fn publish_pending(&self) -> bool {
        let generation = self.generation();
        self.state().publish_pending(generation)
    }

    pub fn claim_pending(&self) -> Option<u64> {
        self.state().claim_pending()
    }

    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn cache_consumed(&self, generation: u64) -> bool {
        self.state().cache_consumed(generation)
    }

    /// CACHED(g) -> INITIALIZING(g+1), without wrap.
    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn begin_reuse(&self) -> Option<u64> {
        self.state().begin_reuse()
    }

    /// Owner must finish layout/table reset before INITIALIZING -> LIVE.
    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn finish_reuse(&self, generation: u64) -> bool {
        self.state().finish_reuse(generation)
    }

    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn release_cached(&self, generation: u64) -> bool {
        self.state().release_cached(generation)
    }

    pub fn generation(&self) -> u64 {
        large_generation(self.word.load(Ordering::Acquire))
    }
    #[cfg(feature = "internals")]
    pub fn word_for_test(&self) -> u64 {
        self.word.load(Ordering::Acquire)
    }
    #[cfg(feature = "internals")]
    pub(super) fn pending_for_test(&self) -> bool {
        crate::alloc_core::segment_header::large_phase(self.word.load(Ordering::Acquire))
            == Some(LargePhase::Pending)
    }
}

impl Default for LargeState {
    fn default() -> Self {
        Self::new()
    }
}
