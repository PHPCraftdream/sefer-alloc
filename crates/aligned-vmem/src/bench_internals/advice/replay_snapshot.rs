//! Closed event-stream accounting, not authenticated native or residency evidence.
#![forbid(unsafe_code)]
use super::snapshot::AdviceLifetimeSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdviceReplaySnapshot {
    pub advice: AdviceLifetimeSnapshot,
    /// Fresh Large requests, threshold-eligible requests, policy-eligible requests.
    pub requests: [[u64; 3]; 6],
    pub requested_bytes: [u64; 6],
    pub usable_span_bytes: [u64; 6],
    pub reservation_bytes: [u64; 6],
    /// Includes unadvised fresh Large mappings until confirmed terminal release.
    pub outstanding_fresh: usize,
    pub events: u64,
    /// Confirmed releases placed before proven native address reuse. Peak follows
    /// this canonical accounting order, not a simultaneous kernel-time maximum.
    pub canonical_releases: u64,
}

impl AdviceReplaySnapshot {
    pub const FRESH: usize = 0;
    pub const THRESHOLD_ELIGIBLE: usize = 1;
    pub const POLICY_ELIGIBLE: usize = 2;
}
