//! Bounded, non-materialising maintenance of registry heaps.
//!
//! Ph4c: `MaintenanceLease::with_core` is safe (proof at its doc comment);
//! this file no longer contains any `unsafe`.

use core::sync::atomic::Ordering;

use super::HeapRegistry;
use crate::registry::bootstrap::{ensure, MAX_HEAPS};
use crate::registry::HeapCore;

impl HeapRegistry {
    /// Visit at most `budget` indices in one round-robin pass. Discovery is
    /// O(indices visited), not a full high-water scan for each absent heap.
    /// Each acquired heap receives one bounded ingress step. Contended,
    /// incomplete and absent slots are skipped.
    pub(crate) fn maintenance_pass(cursor: &mut usize, budget: usize) -> usize {
        let reg = ensure();
        let count = (reg.count.load(Ordering::Acquire) as usize).min(MAX_HEAPS);
        if count == 0 {
            *cursor = 0;
            return 0;
        }
        let visits = budget.min(count);
        let start = *cursor % count;
        let mut maintained = 0;
        for offset in 0..visits {
            let index = (start + offset) % count;
            let Some(mut lease) = Self::try_maintenance_at(index) else {
                continue;
            };
            lease.with_core(|core| {
                core.background_maintenance_step(HeapCore::BACKGROUND_INGRESS_BUDGET)
            });
            drop(lease);
            maintained += 1;
        }
        *cursor = (start + visits) % count;
        maintained
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn maintenance_pass_for_test(cursor: &mut usize, budget: usize) -> usize {
        Self::maintenance_pass(cursor, budget)
    }
}
