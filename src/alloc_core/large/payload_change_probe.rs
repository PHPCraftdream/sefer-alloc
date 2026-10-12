//! PRF-03 decision-data probe: observation-only Large-cache hit statistics.
//!
//! Counters (all `AtomicU64`, `Relaxed`, `fetch_add` — global statics shared
//! across heaps, so this is a MEASUREMENT aggregate, not an invariant; Relaxed
//! is sufficient because no counter is ever used for synchronization):
//!
//! - `requests`: cache-eligible Large requests — every entry into the
//!   `align < SEGMENT` branch of `AllocCore::alloc_large`.
//! - `hits`: best-fit scan found a compatible cached slot AND the reuse
//!   (`begin_large_reuse`) succeeded.
//! - `misses`: requests that fell through to `alloc_large_slow` inside the
//!   same branch — best-fit found nothing, reuse was refused, or the
//!   post-hit `register_payload` failed.
//! - `changed_payload_hits`: hits whose freshly computed payload offset
//!   (`hdr_aligned`) differs from the segment's PREVIOUS `payload_offset`
//!   header field — i.e. the r19 "retained route" would have had to drop and
//!   re-register (the R3 cost). This is the PRF-03 activation signal.
//!
//! INVARIANT expected by the PRF-03 harness: `requests == hits + misses`.
//!
//! The entire module is gated behind `bench-internals`: without the feature
//! zero code from it is compiled and the Large hot path is byte-identical.
//! Observation-only — no mutator touches allocator state. The public
//! snapshot accessor is deliberately NOT named `dbg_*` (measurement-only
//! surface, re-exported for the PRF-03 example/test under
//! `internals` + `bench-internals`).

/// Snapshot of the four PRF-03 probe counters (monotonic; compute deltas).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LargePayloadProbeSnapshot {
    /// Cache-eligible Large requests (`align < SEGMENT` branch entries).
    pub requests: u64,
    /// Best-fit large-cache hits with successful reuse.
    pub hits: u64,
    /// Fall-throughs to `alloc_large_slow` inside the same branch.
    pub misses: u64,
    /// Hits whose payload offset changed vs the segment's previous header.
    pub changed_payload_hits: u64,
}

static REQUESTS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static HITS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static MISSES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static CHANGED_PAYLOAD_HITS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Called on every entry into the `align < SEGMENT` branch of `alloc_large`.
pub(crate) fn record_request() {
    REQUESTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

/// Called when the best-fit scan found a slot AND reuse succeeded; `changed`
/// reports whether the new payload offset differs from the old header value.
pub(crate) fn record_hit(changed: bool) {
    HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if changed {
        CHANGED_PAYLOAD_HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }
}

/// Called at each fall-through to `alloc_large_slow` inside the
/// `align < SEGMENT` branch (no fit / refused reuse / failed registration).
pub(crate) fn record_miss() {
    MISSES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

/// Read-only snapshot of all four counters (Relaxed loads).
///
/// Measurement-only surface for the PRF-03 example/test — exposed via the
/// cfg-gated re-export in `alloc_core` (see that file). Read-only by design:
/// no reset, callers compute deltas.
pub fn r20_prf03_large_probe_snapshot() -> LargePayloadProbeSnapshot {
    use core::sync::atomic::Ordering::Relaxed;
    LargePayloadProbeSnapshot {
        requests: REQUESTS.load(Relaxed),
        hits: HITS.load(Relaxed),
        misses: MISSES.load(Relaxed),
        changed_payload_hits: CHANGED_PAYLOAD_HITS.load(Relaxed),
    }
}
