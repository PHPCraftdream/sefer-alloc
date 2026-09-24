//! Link sentinel while a producer publishes the actual predecessor.

/// Claim state distinct from both free and tail; no aligned segment base can
/// equal this value. The owner leaves a head with this link queued for a
/// later drain, after the producer's Release store makes the link ready.
pub(crate) const DEFERRED_LARGE_PUBLISHING: u64 = u64::MAX - 2;
