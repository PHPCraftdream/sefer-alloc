//! Per-thread fast-concede memo cache for `push_with_overflow_retry`'s
//! retry patience (reorg step 4c): the stall-concession snapshot types, the
//! N-way cache arity constant, and the TLS `LAST_STALL_CONCESSIONS` memo,
//! moved VERBATIM out of `overflow.rs` for the repo's 1000-line file-size
//! cap. Pure code-movement sibling file; no behavior changed.

/// R6-REVIEW-F2: arity of the per-thread [`LAST_STALL_CONCESSIONS`]
/// fast-concede cache — how many DISTINCT stalled segments a thread can hold
/// concession snapshots for simultaneously. 4 was chosen as the smallest
/// power of two that covers the realistic paused-owner multi-segment shapes
/// this cache exists for (a paused owner's blocks interleaved across a
/// handful of 4 MiB segments — see
/// `tests/regression_paused_owner_multisegment.rs`): a producer alternating
/// frees across up to 4 stalled segments never evicts a snapshot it is about
/// to need, while the whole cache stays a tiny `Copy` array (no allocation,
/// no `Drop` — see `LAST_STALL_CONCESSIONS`'s TLS-safety note). More ways
/// would only matter for 5+ SIMULTANEOUSLY-stalled segments interleaved by
/// one producer — and even then the cost of an eviction is graceful (one
/// full-patience re-payment per evicted segment per rotation, not
/// unboundedly repeated per push, because round-robin eviction cycles
/// through slots rather than pathologically pinning one).
#[cfg(feature = "alloc-xthread")]
pub(super) const STALL_CONCESSION_WAYS: usize = 4;

/// R6-REVIEW-F2: one recorded concession snapshot — `(segment base, segment
/// ring drain head, heap-overflow drain head)` as captured at the moment a
/// live-owner retry conceded. See [`LAST_STALL_CONCESSIONS`].
#[cfg(feature = "alloc-xthread")]
pub(super) type StallSnapshot = (usize, u32, Option<usize>);

/// R6-REVIEW-F2: the per-thread cache payload — [`STALL_CONCESSION_WAYS`]
/// snapshot slots plus the round-robin write cursor. A plain `Copy` tuple so
/// the whole thing lives in a const-initialized `Cell` (no allocation, no
/// `Drop` — see [`LAST_STALL_CONCESSIONS`]'s TLS-safety note).
#[cfg(feature = "alloc-xthread")]
pub(super) type StallConcessionCache = ([Option<StallSnapshot>; STALL_CONCESSION_WAYS], usize);

#[cfg(feature = "alloc-xthread")]
std::thread_local! {
    /// R6-REGRESSION-2 (arity widened by R6-REVIEW-F2): per-thread
    /// fast-concede memo cache — up to [`STALL_CONCESSION_WAYS`] `(segment
    /// base, ring drain head, overflow drain head)` snapshots, each recorded
    /// at one of this thread's recent live-owner retry CONCESSIONS, plus a
    /// round-robin write cursor. Purely caller-side state (neither ring's
    /// protocol or layout is touched); read/written only by
    /// `push_with_overflow_retry`'s live-owner branch.
    ///
    /// **Why it exists.** The progress-detected stop condition deliberately
    /// waits a long time (up to [`RETRY_STALLED_ROUNDS_GIVE_UP`] ≈ ~0.3–2s of
    /// observed zero drain progress) before conceding — that generosity is what
    /// makes the #136 judge robust under host load. But in the sustained
    /// paused-owner shape (`owner=paused`: zero drains for an entire
    /// 100_000-push burst) EVERY push past the combined ring+overflow capacity
    /// must eventually concede, and re-paying up to ~2s per push would turn the
    /// paused benchmark cell into HOURS of (sleepy, but still
    /// pathological) stall — reintroducing the R6-REGRESSION wall-clock
    /// pathology in a politer form. This cache bounds that: once a thread has
    /// paid the FULL patience for a stall and conceded, it records the exact
    /// cursor snapshot it conceded against; a subsequent push into a segment
    /// whose `(base, ring head, overflow head)` triple exactly matches ANY
    /// cached snapshot is provably inside the same continuous zero-progress
    /// stall for THAT segment (both cursors are owner-advanced and monotonic —
    /// equality means literally nothing was drained since the concession), so
    /// it concedes after a single probe round instead of re-paying the full
    /// patience. The moment either cursor moves, that snapshot no longer
    /// matches (monotonic cursors never return to an old value short of a
    /// 2^32/2^64 wrap) and full patience is restored for that segment.
    ///
    /// **Why N ways, not one (R6-REVIEW-F2).** The original single-entry memo
    /// was overwritten on EVERY concession — a paused owner holding 2+
    /// saturated segments, with a producer whose frees interleave across them
    /// (A, B, A, B, …), replaced the memo with the OTHER segment's snapshot on
    /// every push, so the memo never matched and every push re-paid the full
    /// ~128-round patience: a linear-in-push-count wall-clock wall (in polite
    /// sleep form) that the memo exists to bound, reachable by any workload
    /// whose paused blocks span more than one 4 MiB segment (guarded by
    /// `tests/regression_paused_owner_multisegment.rs`). With
    /// [`STALL_CONCESSION_WAYS`] independent slots keyed by segment base, each
    /// concurrently-stalled segment keeps its own snapshot: a concession
    /// UPDATES the slot already holding its segment's base if one exists
    /// (so one segment never occupies two slots and repeated concessions
    /// against the same segment don't evict its neighbors), and otherwise
    /// fills the slot at the round-robin cursor, advancing the cursor.
    /// Round-robin (not true LRU) eviction: at 4 entries the difference is
    /// immaterial — the cache's job is "hold the handful of segments this
    /// thread is currently interleaving across", and any workload cycling
    /// through ≤ N stalled segments reaches a steady state where every
    /// segment keeps its slot regardless of replacement order; tracking
    /// recency would add bookkeeping to the allocator's dealloc path for
    /// no observable difference at this size.
    ///
    /// **Why this cannot affect the #136 judge (unchanged by the N-way
    /// widening).** The cache is written ONLY on a concession — for every
    /// slot, not just the first — and the judge asserts zero concessions: on
    /// any run where the judge's invariant holds, no slot is ever populated
    /// and the retry loop's behavior is byte-identical to the memo-less
    /// version. It changes only how CHEAPLY pushes AFTER a first concession
    /// give up — and any first concession already is the bounded-leak event
    /// the judge forbids. Likewise the premature-concession guard is
    /// per-push, not per-slot: a cache hit only lowers the INITIAL give-up
    /// threshold; the post-round progress check restores full patience
    /// (`give_up_after = RETRY_STALLED_ROUNDS_GIVE_UP`) the moment either
    /// cursor advances, so a stale hit against an owner that is actually
    /// draining costs at most one probe round of reduced patience before the
    /// first observed advance re-arms the full budget — for N entries exactly
    /// as it did for one, because at most ONE entry can match a given
    /// `(base, heads)` triple (slots are unique per base by the
    /// update-in-place rule) and the match only ever feeds that same single
    /// `give_up_after` initialization.
    ///
    /// **Why `thread_local!` is safe here.** Const-initialized `Cell` of a
    /// `Copy` type (a fixed-size array of `Option` tuples plus a `usize`
    /// cursor): no lazy initialization, no allocation (critical — this runs
    /// inside the global allocator's dealloc path), no `Drop` registration (so
    /// it is accessible even during another TLS destructor's cross-thread
    /// frees). Stale cross-workload state is self-correcting: a slot only
    /// matches while both cursors are EXACTLY at the recorded values, and a
    /// mismatch (the overwhelmingly common case for unrelated later traffic)
    /// falls back to full patience; a false match requires the same segment
    /// base AND both monotonic cursors at the recorded values, and its worst
    /// case is one cheap concession to the lossless intrusive spill rather
    /// than another futile retry round; it does not discard a legal free.
    pub(super) static LAST_STALL_CONCESSIONS: core::cell::Cell<StallConcessionCache> =
        const { core::cell::Cell::new(([None; STALL_CONCESSION_WAYS], 0)) };
}
