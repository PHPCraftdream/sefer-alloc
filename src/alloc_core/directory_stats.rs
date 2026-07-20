//! Per-class segment directory diagnostic counters (task R7-A0).
//!
//! Process-wide `AtomicU64` counters for observing the segment-scan and
//! (future) directory-lookup behaviour. Storage is ALWAYS compiled under
//! `alloc-core` so that the `dbg_*` read accessors have a stable definition
//! regardless of the feature set (reads return 0 when no increment was
//! compiled in). The per-event INCREMENTS are gated behind `alloc-stats`
//! (matching the crate's established pattern for `FOREIGN_OR_UNROUTABLE_FREES`,
//! `tcache_hits`, `large_cache_hits` -- the hot path carries no bookkeeping
//! unless the caller explicitly opts in).
//!
//! ## Counter inventory
//!
//! | Counter                       | Incremented by          | Live in A0? |
//! |-------------------------------|-------------------------|-------------|
//! | `directory_hits`              | A3 directory lookup hit | storage only|
//! | `directory_stale_hits`        | A3 stale-positive clear | storage only|
//! | `directory_fallback_scans`    | A3 fallback scan entry  | storage only|
//! | `directory_words_examined`    | A3 bitmap word scan     | storage only|
//! | `dirty_segments_drained`      | A4 dirty-drain loop     | storage only|
//! | `wasted_dirty_drains`         | R9-6 dirty-drain loop (drain produced zero sought-class blocks) | storage only|
//! | `full_scan_slots_examined`    | `find_segment_with_free_impl` per-slot | YES |
//! | `directory_authoritative_miss`| R8-2 authoritative-miss fast path (O(S) scan SKIPPED) | storage only|
//! | `directory_miss_self_heal`    | R8-2 periodic re-validation found a directory-missed segment | storage only|

use core::sync::atomic::AtomicU64;

/// Segment directory lookup hits (A3: a directory query found a non-empty
/// segment and the validation succeeded). Reads 0 until A3 wires the
/// increment.
pub(crate) static DIRECTORY_HITS: AtomicU64 = AtomicU64::new(0);

/// Stale directory hits (A3: a directory query found a set bit whose segment's
/// BinTable head was actually empty -- the bit was cleared and the scan
/// continued). Reads 0 until A3 wires the increment.
pub(crate) static DIRECTORY_STALE_HITS: AtomicU64 = AtomicU64::new(0);

/// Directory fallback scans (A3: the directory query found nothing and the
/// guarded linear-scan fallback was entered). Reads 0 until A3 wires the
/// increment.
pub(crate) static DIRECTORY_FALLBACK_SCANS: AtomicU64 = AtomicU64::new(0);

/// Directory bitmap words examined (A3: each u64 word inspected during a
/// per-class bitmap scan). Reads 0 until A3 wires the increment.
pub(crate) static DIRECTORY_WORDS_EXAMINED: AtomicU64 = AtomicU64::new(0);

/// Dirty segments drained (A4: each segment whose dirty bit was set and whose
/// ring was drained by the directory-driven lookup). Reads 0 until A4 wires
/// the increment.
pub(crate) static DIRTY_SEGMENTS_DRAINED: AtomicU64 = AtomicU64::new(0);

/// R9-6 (class-aware dirty routing judge): counts the subset of
/// `DIRTY_SEGMENTS_DRAINED` events where the segment's ring, once drained in
/// response to a `find_segment_with_free_impl(class_idx)` call, produced ZERO
/// reclaimed blocks of the sought `class_idx` — i.e. from THAT caller's
/// perspective the drain was wasted work that class-aware dirty routing (a
/// per-(segment,class) dirty bitmap) would have avoided entirely. Diagnostic
/// only; does not influence the drain algorithm. Reads 0 unless `alloc-stats`
/// is on (the increment site is gated) and `alloc-xthread` + not-`numa-aware`
/// (the drain itself is gated).
pub(crate) static WASTED_DIRTY_DRAINS: AtomicU64 = AtomicU64::new(0);

/// Slots examined in the CURRENT linear scan (`find_segment_with_free_impl`).
/// Incremented once per slot visited (including null/skipped slots) so the
/// baseline already has the scan-cost counter live. This is the PRIMARY
/// observability counter for A0: it directly measures the O(S) cost the
/// directory is meant to eliminate.
pub(crate) static FULL_SCAN_SLOTS_EXAMINED: AtomicU64 = AtomicU64::new(0);

/// Genuine directory misses where the directory was TRUSTED (the full
/// linear-scan fallback was SKIPPED) — R8-2 (task #215)'s authoritative-miss
/// fast path. This is the primary observability counter for the fix: it
/// directly measures how often the O(S) scan is now avoided.
pub(crate) static DIRECTORY_AUTHORITATIVE_MISS: AtomicU64 = AtomicU64::new(0);

/// A periodic re-validation full scan (R8-2, task #215) found a segment the
/// directory's own lookup had missed, and the directory bit was repaired
/// in-place. Expected to stay at 0 in normal operation — the incrementally-
/// maintained directory is proven correct in every scenario task #214's test
/// suite covers. A nonzero value here in real testing/CI is a canary
/// indicating a genuine directory-tracking bug and warrants investigation,
/// NOT a normal/expected event to silence.
pub(crate) static DIRECTORY_MISS_SELF_HEAL: AtomicU64 = AtomicU64::new(0);
