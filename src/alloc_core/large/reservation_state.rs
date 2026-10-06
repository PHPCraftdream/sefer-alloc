//! Per-reservation Large phase transitions; no predecessor links or payload access.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::alloc_core::segment_header::{
    large_generation, large_phase, next_large_generation, pack_large_state, LargePhase,
};

/// The word remains mapped through every call. Owner transitions require the
/// heap lease; producer publication requires the unique valid free of LIVE(g).
pub struct LargeReservationState<'a> {
    word: &'a AtomicU64,
}

impl<'a> LargeReservationState<'a> {
    #[inline(always)]
    pub fn new(word: &'a AtomicU64) -> Self {
        Self { word }
    }

    /// The successful CAS is the producer's last reservation access. The
    /// caller must not touch the header, payload or this word afterwards.
    #[inline(always)]
    pub fn publish_pending(&self, generation: u64) -> bool {
        self.word
            .compare_exchange(
                pack_large_state(LargePhase::Live, generation),
                pack_large_state(LargePhase::Pending, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Only a table-scan owner with the heap lease may claim this obligation.
    /// The caller derives the canonical base from the table, never a producer.
    #[inline(always)]
    pub fn claim_pending(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Acquire);
        if large_phase(observed) != Some(LargePhase::Pending) {
            return None;
        }
        let generation = large_generation(observed);
        self.word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Consuming, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| generation)
    }

    /// Owner-only claim of the allocation still held by its local caller.
    /// Also used by the legacy deferred stack until ingress integration.
    #[inline(always)]
    pub fn claim_live(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Acquire);
        if large_phase(observed) != Some(LargePhase::Live) {
            return None;
        }
        let generation = large_generation(observed);
        self.word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Consuming, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| generation)
    }

    #[inline(always)]
    pub fn cache_consumed(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Consuming, LargePhase::Cached)
    }

    /// Must happen while mapped, after table removal and before OS release.
    #[inline(always)]
    pub fn release_consumed(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Consuming, LargePhase::Released)
    }

    /// Owner-only cache eviction, while still mapped and before OS release.
    #[inline(always)]
    pub fn release_cached(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Cached, LargePhase::Released)
    }

    /// Cache-hit rollback after CACHED -> INITIALIZING, before user issuance.
    #[inline(always)]
    pub fn release_initializing(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Initializing, LargePhase::Released)
    }

    /// None leaves CACHED unchanged: the owner retires this reservation.
    #[inline(always)]
    pub fn begin_reuse(&self) -> Option<u64> {
        let observed = self.word.load(Ordering::Acquire);
        if large_phase(observed) != Some(LargePhase::Cached) {
            return None;
        }
        let next = next_large_generation(large_generation(observed))?;
        self.word
            .compare_exchange(
                observed,
                pack_large_state(LargePhase::Initializing, next),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()
            .map(|_| next)
    }

    /// Caller completes layout/owner/table preparation before this release.
    #[inline(always)]
    pub fn finish_reuse(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Initializing, LargePhase::Live)
    }

    #[inline(always)]
    fn transition(&self, generation: u64, from: LargePhase, to: LargePhase) -> bool {
        self.word
            .compare_exchange(
                pack_large_state(from, generation),
                pack_large_state(to, generation),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}
