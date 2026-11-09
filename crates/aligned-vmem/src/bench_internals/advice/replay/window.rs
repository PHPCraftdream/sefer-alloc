//! One quiescent raw-position interval, with peaks seeded from incoming ownership.
#![forbid(unsafe_code)]
use super::super::snapshot::AdviceLifetimeSnapshot;
use super::snapshot::AdviceReplaySnapshot;

/// Before/after totals remain cumulative. Window peaks are recomputed over the
/// canonical interval, never cumulative-peak subtraction or summed group peaks.
/// This validates metadata, not quiescence, event authenticity or kernel maxima.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdviceWindowSnapshot {
    pub begin: u64,
    pub end: u64,
    pub before: AdviceReplaySnapshot,
    pub after: AdviceReplaySnapshot,
    pub peak_groups: [u64; 6],
    pub peak_process: u64,
}

impl AdviceWindowSnapshot {
    pub(crate) fn start(begin: u64, end: u64, before: AdviceReplaySnapshot) -> Self {
        Self {
            begin,
            end,
            before,
            after: before,
            peak_groups: before
                .advice
                .groups
                .map(|group| group[AdviceLifetimeSnapshot::NOW]),
            peak_process: before.advice.process[AdviceLifetimeSnapshot::NOW],
        }
    }

    pub(crate) fn observe(&mut self, after: AdviceReplaySnapshot) {
        for (peak, group) in self.peak_groups.iter_mut().zip(after.advice.groups) {
            *peak = (*peak).max(group[AdviceLifetimeSnapshot::NOW]);
        }
        self.peak_process = self
            .peak_process
            .max(after.advice.process[AdviceLifetimeSnapshot::NOW]);
        self.after = after;
    }
}
