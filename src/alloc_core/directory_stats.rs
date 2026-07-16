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
//! | `full_scan_slots_examined`    | `find_segment_with_free_impl` per-slot | YES |

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

/// Slots examined in the CURRENT linear scan (`find_segment_with_free_impl`).
/// Incremented once per slot visited (including null/skipped slots) so the
/// baseline already has the scan-cost counter live. This is the PRIMARY
/// observability counter for A0: it directly measures the O(S) cost the
/// directory is meant to eliminate.
pub(crate) static FULL_SCAN_SLOTS_EXAMINED: AtomicU64 = AtomicU64::new(0);
