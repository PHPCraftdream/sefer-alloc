//! Per-class segment directory diagnostic counters (task R7-A0).
//!
//! Process-wide `AtomicU64` counters for observing the segment-scan and
//! directory-lookup behaviour. Storage is ALWAYS compiled under
//! `alloc-core` so that the `dbg_*` read accessors have a stable definition
//! regardless of the feature set (reads return 0 when no increment was
//! compiled in). The per-event INCREMENTS are gated behind `alloc-stats`
//! (matching the opt-in `tcache_hits` and `large_cache_hits` diagnostics).
//! Foreign-route rejection counts have separate, unconditional increments.
//!
//! ## Current counter inventory
//!
//! All increments require `alloc-stats` and the applicable discovery path.
//!
//! | Counter | Incremented by |
//! |---------|----------------|
//! | `directory_hits` | Validated directory lookup hit |
//! | `directory_stale_hits` | Stale-positive rejection and clear |
//! | `directory_fallback_scans` | Directory fallback scan entry |
//! | `directory_words_examined` | Per-class bitmap word inspected, including zero words |
//! | `full_scan_slots_examined` | Small/Primordial fallback candidate probe |
//! | `directory_authoritative_miss` | Trusted negative skipping fallback; not routed production |
//! | `directory_miss_self_heal` | Negative lookup scan found and repaired a missed segment |
//! | `directory_rescue_oom_avoided` | Forced rescue found a missed free block before OOM |
//! | `routed_miss_scans` | Routed negative-directory scan entry, excluding rescue |
//! | `routed_miss_scan_drain_created_free` | Hit bin empty before sidecar drain, nonempty after |
//! | `routed_miss_scan_bin_already_nonempty` | Hit bin already nonempty before drain |
//! | `routed_miss_scan_nothing` | Routed miss scan found no block |

use core::sync::atomic::AtomicU64;

/// Segment directory lookup hits: a candidate passed validation.
/// Reads 0 unless the applicable `alloc-stats` increment path is enabled.
pub(crate) static DIRECTORY_HITS: AtomicU64 = AtomicU64::new(0);

/// Stale directory hits: a candidate failed validation and its bit was
/// cleared. Reads 0 unless the applicable `alloc-stats` path is enabled.
pub(crate) static DIRECTORY_STALE_HITS: AtomicU64 = AtomicU64::new(0);

/// Directory fallback scans entered after a negative query.
/// Reads 0 unless the applicable `alloc-stats` increment path is enabled.
pub(crate) static DIRECTORY_FALLBACK_SCANS: AtomicU64 = AtomicU64::new(0);

/// Directory bitmap words examined: EVERY u64 word inspected during a
/// per-class bitmap scan, including all-zero words (R13-01) — one increment
/// per scanned word per scanned node bucket, before the zero-word skip.
/// Reads 0 unless `alloc-stats` is on (the increment site is gated).
pub(crate) static DIRECTORY_WORDS_EXAMINED: AtomicU64 = AtomicU64::new(0);

/// Current Small/Primordial candidates probed by the fallback in
/// `find_segment_with_free_impl`, incremented before each `base_at`.
/// Historical A0 counted NULL/skipped high-water slots too; the active-kind
/// index now excludes those slots before a probe.
pub(crate) static FULL_SCAN_SLOTS_EXAMINED: AtomicU64 = AtomicU64::new(0);

/// In `production` (with `alloc-global` + `alloc-xthread`), routed tables do
/// not trust a negative directory result: discovery falls back to a full scan,
/// so this authoritative-miss counter is not incremented there. It records
/// trusted negatives only in configurations where that shortcut is enabled.
pub(crate) static DIRECTORY_AUTHORITATIVE_MISS: AtomicU64 = AtomicU64::new(0);

/// A negative directory lookup followed by a full scan found a segment the
/// directory had missed, and repaired its bit in-place. Routed lookups scan
/// every negative; standalone lookups do so only during periodic re-validation
/// (R8-2, task #215). Expected to stay at 0 in normal operation — a nonzero
/// value is a canary for a directory-tracking bug and warrants investigation,
/// not a normal event. See perf open item 82.
pub(crate) static DIRECTORY_MISS_SELF_HEAL: AtomicU64 = AtomicU64::new(0);

/// R9-8 (task #230): a forced O(S) "rescue scan", run as a last resort right
/// before the small-allocation path would surface an OOM (segment-table full
/// or OS reservation failure) to the user, found a real free block the
/// directory had hidden — i.e. the rescue scan AVOIDED a spurious OOM that a
/// directory-invariant violation would otherwise have caused. Distinguished
/// from `DIRECTORY_MISS_SELF_HEAL` (the periodic re-validation's routine
/// self-heals) so this genuinely-rare OOM-backstop path is independently
/// observable in production diagnostics. Like `DIRECTORY_MISS_SELF_HEAL`, a
/// nonzero value here indicates a directory-tracking bug and warrants
/// investigation, NOT a normal/expected event. Reads 0 unless `alloc-stats` is
/// on (the increment site is gated) and `alloc-segment-directory` is active
/// with a materialised sidecar (the rescue is gated on the directory feature).
pub(crate) static DIRECTORY_RESCUE_OOM_AVOIDED: AtomicU64 = AtomicU64::new(0);

/// Routed negative-directory scans (round 12 O-4, data only): a routed core's
/// directory lookup found no candidate and the full fallback scan was entered
/// (rescue scans excluded). One increment per lookup, not per slot. Invariant:
/// equals the sum of the three outcome counters below. Reads 0 unless
/// `alloc-stats` is on.
pub(crate) static ROUTED_MISS_SCANS: AtomicU64 = AtomicU64::new(0);

/// Outcome of a routed miss scan: it hit a segment whose class bin was EMPTY
/// before that segment's sidecar drain and non-empty after — a terminal
/// publication hidden behind the negative directory, the case that makes
/// trusting the negative unsafe. Reads 0 unless `alloc-stats` is on.
pub(crate) static ROUTED_MISS_SCAN_DRAIN_CREATED_FREE: AtomicU64 = AtomicU64::new(0);

/// Outcome of a routed miss scan: it hit a segment whose class bin was already
/// non-empty before the drain (the directory lagged; the scan self-healed it).
/// Reads 0 unless `alloc-stats` is on.
pub(crate) static ROUTED_MISS_SCAN_BIN_ALREADY_NONEMPTY: AtomicU64 = AtomicU64::new(0);

/// Outcome of a routed miss scan: no segment had a block (the directory was
/// right). Reads 0 unless `alloc-stats` is on.
pub(crate) static ROUTED_MISS_SCAN_NOTHING: AtomicU64 = AtomicU64::new(0);
