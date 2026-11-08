//! `ThpBudgetLedger` — lock-free ledger of ADVISED bytes (not RSS) charged
//! against a budget. Core only, no unsafe, no allocation.

use crate::alloc_core::config::policy::large_thp_runtime::thp_advice::ThpAdvice;
// Atomics through `super` so the actual-source concurrency model can bind.
use super::{AtomicU64, Ordering};

/// Outstanding charged bytes; invariant `used <= budget` of every successful
/// reservation (a stale read can only make the CAS fail).
pub struct ThpBudgetLedger {
    pub(crate) used: AtomicU64,
}

// Sole production consumers are the mechanism-gated fresh-path advice
// (advise_fresh_large: try_reserve) and the terminal-release funnel
// (release_large_mapping: give_back); the attribute keeps other rows clean.
#[cfg_attr(
    not(all(feature = "alloc-decommit", not(feature = "numa-aware"))),
    allow(dead_code)
)]
impl ThpBudgetLedger {
    const MAX_ATTEMPTS: u32 = 32;

    /// An empty ledger.
    // Not `const`: the modelable atomic alias has no const constructor, so the
    // process statics in `thp_runtime.rs` write their initializer directly.
    // In-crate statics in `thp_runtime.rs` initialize directly (see there);
    // `new` is consumed by the root test binaries, which mount/import this
    // type and are invisible to lib-target dead-code analysis.
    #[allow(dead_code)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            used: AtomicU64::new(0),
        }
    }

    /// Currently charged bytes. Production readers are only the
    /// bench-internals dbg ledger hook; the attribute covers rows without it.
    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    #[must_use]
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed) as usize
    }

    /// Reserve up to `want` bytes against `budget`, floored to the advice
    /// granule; returns the bytes taken (0 = none, no advice). Budget 0 means
    /// unlimited and never touches the ledger, so it returns 0 here. Gives up
    /// with 0 after a bounded number of lost races.
    pub fn try_reserve(&self, want: usize, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }
        let mut attempts = 0;
        while attempts < Self::MAX_ATTEMPTS {
            attempts += 1;
            let cur = self.used.load(Ordering::Relaxed);
            let left = (budget as u64).saturating_sub(cur);
            let cap = if (want as u64) < left {
                want as u64
            } else {
                left
            };
            let take = cap / ThpAdvice::GRANULE as u64 * ThpAdvice::GRANULE as u64;
            if take == 0 {
                return 0;
            }
            let Some(next) = cur.checked_add(take) else {
                return 0;
            };
            if self
                .used
                .compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return take as usize;
            }
        }
        0
    }

    /// Return `bytes` previously reserved; 0 is a no-op.
    pub fn give_back(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let prev = self.used.fetch_sub(bytes as u64, Ordering::Relaxed);
        debug_assert!(prev >= bytes as u64);
    }
}

impl Default for ThpBudgetLedger {
    fn default() -> Self {
        Self::new()
    }
}
