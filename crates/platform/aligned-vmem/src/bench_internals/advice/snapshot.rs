//! Accepted-advice accounting quantities, never RSS or physical huge backing.
#![forbid(unsafe_code)]

/// Pure benchmark bookkeeping snapshot. Group order is supplied by the caller;
/// this record does not authenticate a producer or observe any OS operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdviceLifetimeSnapshot {
    /// Six requested-size groups; columns follow the associated protocol constants.
    pub groups: [[u64; 4]; 6],
    /// Process accounting peak is not the sum of the individual group peaks.
    pub process: [u64; 4],
}

impl AdviceLifetimeSnapshot {
    pub const TOTAL: usize = 0;
    pub const NOW: usize = 1;
    /// Maximum outstanding accepted advice in the observed accounting event order.
    /// Not an exact simultaneous-kernel-VMA maximum or a residency measurement.
    pub const PEAK: usize = 2;
    pub const RELEASED: usize = 3;

    pub(crate) const EMPTY: Self = Self {
        groups: [[0; 4]; 6],
        process: [0; 4],
    };
}
