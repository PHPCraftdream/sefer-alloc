//! `ThpAdvice` — pure geometry and classification for range-limited
//! transparent-huge-page advice on a fresh Large mapping. No atomics, no OS.

use crate::alloc_core::config::policy::large_thp::{
    LargeThpMode, LargeThpRequestGroup, LargeThpShare,
};
use crate::alloc_core::config::policy::large_thp_runtime::thp_advice_outcome::ThpAdviceOutcome;
use crate::alloc_core::config::policy::large_thp_runtime::ThpTuple;

/// Namespace for the pure advice helpers; carries no state.
pub struct ThpAdvice;

// Sole production consumer is the mechanism-gated fresh-path advice
// (advise_fresh_large calls eligible/share_for/extent/classify); the attribute
// keeps rows without that gate warning-clean.
#[cfg_attr(
    not(all(feature = "alloc-decommit", not(feature = "numa-aware"))),
    allow(dead_code)
)]
impl ThpAdvice {
    /// Advice granule: one 2 MiB huge page.
    pub const GRANULE: usize = 2 * 1024 * 1024;

    /// Bytes of the 2 MiB-aligned PREFIX of the mapping to advise:
    /// `floor(min(span, share * span, budget_left) / GRANULE) * GRANULE`.
    /// `budget_left == None` is unlimited. Returns 0 when the runtime page
    /// size is 0 or does not divide the granule. Never exceeds `span`, never
    /// rounds up.
    #[must_use]
    pub const fn extent(
        span: usize,
        share: LargeThpShare,
        budget_left: Option<usize>,
        page_size: usize,
    ) -> usize {
        if page_size == 0 || Self::GRANULE % page_size != 0 {
            return 0;
        }
        let scaled = ((span as u128 * share.raw() as u128) >> 31) as usize;
        let mut cap = if scaled < span { scaled } else { span };
        if let Some(left) = budget_left {
            if left < cap {
                cap = left;
            }
        }
        cap / Self::GRANULE * Self::GRANULE
    }

    /// Whether a request is eligible: never under `Off`; otherwise the
    /// REQUESTED size (not the span) must reach `min_bytes`.
    #[must_use]
    pub const fn eligible(tuple_mode: LargeThpMode, min_bytes: usize, request_size: usize) -> bool {
        match tuple_mode {
            LargeThpMode::Off => false,
            LargeThpMode::Always | LargeThpMode::Auto => request_size >= min_bytes,
        }
    }

    /// Pure mode baseline without estimator state: Off/Auto zero, Always one.
    /// The fresh Auto path instead reads its process-group learned share.
    #[must_use]
    pub fn share_for(tuple: &ThpTuple, request_size: usize) -> LargeThpShare {
        match tuple.mode {
            LargeThpMode::Always => LargeThpShare::ONE,
            LargeThpMode::Auto => {
                Self::auto_share(LargeThpRequestGroup::of_bytes(request_size as u64))
            }
            LargeThpMode::Off => LargeThpShare::ZERO,
        }
    }

    /// Zero fallback for pure geometry/model callers without process state.
    /// This is not the runtime learner; fresh Auto advice bypasses this helper.
    #[must_use]
    pub fn auto_share(group: LargeThpRequestGroup) -> LargeThpShare {
        let _ = group;
        LargeThpShare::ZERO
    }

    /// Classify an advice result per the budget rules: validation failures
    /// and errors without an OS code (injected, unsupported, poisoned page
    /// size) are assumed to have touched nothing; an error with an OS code
    /// may have applied partially.
    #[must_use]
    pub fn classify(r: Result<usize, aligned_vmem::VmemError>) -> ThpAdviceOutcome {
        match r {
            Ok(n) => ThpAdviceOutcome::Accepted(n),
            Err(e) if e.is_invalid_argument() => ThpAdviceOutcome::PreSyscall,
            Err(e) => match e.os_code() {
                None => ThpAdviceOutcome::PreSyscall,
                Some(_) => ThpAdviceOutcome::UnknownPartial,
            },
        }
    }
}
