//! Coherent bounded updates of one request group's Q31 estimator pair.

use super::{AtomicU64, Ordering};
use crate::alloc_core::config::policy::large_thp::{
    LargeThpEstimator, LargeThpGradient, LargeThpShare,
};

/// Both estimator components live in one atomic word; no external payload is
/// published. Relaxed accesses suffice for the pair's indivisible value.
pub(crate) struct ThpLearningCell {
    pub(super) state: AtomicU64,
}

impl ThpLearningCell {
    pub(crate) const INITIAL: u64 =
        ((LargeThpShare::SCALE as u64) << 32) | LargeThpShare::SCALE as u64;
    const MAX_ATTEMPTS: usize = 8;

    // Process statics initialize directly; actual-source tests use new().
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicU64::new(Self::INITIAL),
        }
    }

    /// Convert one valid residency observation, never accumulated diagnostic
    /// loads. The widened product is below 2^95; the quotient is at most SCALE.
    pub(crate) fn observation(resident_pages: u64, sampled_pages: u64) -> Option<LargeThpShare> {
        if sampled_pages == 0 || resident_pages > sampled_pages {
            return None;
        }
        let raw = (resident_pages as u128 * LargeThpShare::SCALE as u128) / sampled_pages as u128;
        Some(LargeThpShare::from_raw(raw as u32))
    }

    /// One load obtains a coherent pair, not two independently sampled states.
    pub(crate) fn snapshot(&self) -> LargeThpEstimator {
        Self::decode(self.state.load(Ordering::Relaxed))
    }

    /// At most eight CAS attempts. False means the observation was not
    /// applied; the caller must account for that loss, never retry it later.
    /// Success linearizes exactly one update of both components together.
    pub(crate) fn try_observe(
        &self,
        gradient: &LargeThpGradient,
        observation: LargeThpShare,
    ) -> bool {
        let mut current = self.state.load(Ordering::Relaxed);
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            let mut next = Self::decode(current);
            next.observe(gradient, observation);
            let packed = Self::encode(next);
            match self
                .state
                .compare_exchange(current, packed, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
        false
    }

    fn encode(estimator: LargeThpEstimator) -> u64 {
        ((estimator.e_raw() as u64) << 32) | estimator.m_raw() as u64
    }

    fn decode(packed: u64) -> LargeThpEstimator {
        LargeThpEstimator::with_state((packed >> 32) as u32, packed as u32)
    }
}
