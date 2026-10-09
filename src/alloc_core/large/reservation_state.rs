//! Per-reservation Large phase transitions; no predecessor links or payload access.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::alloc_core::segment_header::{
    large_generation, large_phase, next_large_generation, pack_large_state, LargePhase,
};

/// Wrapper over ONE Large phase word, regardless of which storage backs
/// it. The bound atomic is EITHER a physical reservation's terminal word
/// (owner-only lifecycle transitions under the EXCLUSIVE owner's authority
/// — a registry heap lease or a standalone core's `&mut`, including its
/// Large cache's ownership of cached reservations; no access
/// after OS release) OR an independent route-descriptor word
/// (`LargeState`, System-backed), where the producer's terminal
/// publication also runs. Domain obligations attach to the bound word,
/// not to this type: the wrapper itself is neither a reservation credit
/// nor a descriptor pin.
pub struct LargeReservationState<'a> {
    word: &'a AtomicU64,
}

impl<'a> LargeReservationState<'a> {
    #[inline(always)]
    pub fn new(word: &'a AtomicU64) -> Self {
        Self { word }
    }

    /// Terminal LIVE(g) → PENDING(g) CAS. Reached (today, only) through the
    /// route-descriptor binding (`LargeState::publish_pending`). TWO
    /// distinct roles back this call: the ROUTE PIN keeps the INDEPENDENT
    /// descriptor word alive up to the CAS — a descriptor capability, not
    /// physical credit — while the caller's UNIQUE ALLOCATION CREDIT keeps
    /// the physical RESERVATION live until a successful CAS transfers that
    /// obligation to the owner. The successful CAS is the producer's LAST
    /// descriptor-state access and involves no reservation read; the caller
    /// must not touch the allocation, the descriptor, or the word
    /// afterwards.
    // Producer/claim protocol: only registry::segment_route (alloc-global) calls it.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
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
    // Producer/claim protocol: only registry::segment_route (alloc-global) calls it.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
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

    /// Owner-only claim of a LIVE word — the PHYSICAL-reservation reading
    /// of this method; every caller acts under the EXCLUSIVE owner's
    /// authority — a registry heap lease or a standalone core's `&mut`
    /// (issue-path adoption in `mem`, reclaim in `alloc_core_large`,
    /// teardown in `lifecycle`; cached Large reservations held unregistered
    /// by the running core are still owner-held). This constrains this
    /// method's physical invocation
    /// only: the same wrapper type is also bound to independent
    /// route-descriptor words (`LargeState`), where the producer's terminal
    /// PENDING CAS legitimately transitions that other word.
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

    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub fn release_cached(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Cached, LargePhase::Released)
    }

    /// Cache-hit rollback after CACHED -> INITIALIZING, before user issuance.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub fn release_initializing(&self, generation: u64) -> bool {
        self.transition(generation, LargePhase::Initializing, LargePhase::Released)
    }

    /// None leaves CACHED unchanged: the owner retires this reservation.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
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
