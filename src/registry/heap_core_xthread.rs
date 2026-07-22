//! Cross-thread free routing for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10).
//!
//! This file holds the `impl HeapCore { .. }` block for the cross-thread
//! deferred-free drain and the foreign-thread dealloc routing protocol.
//! All methods are `#[cfg(feature = "alloc-xthread")]`.
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

use core::alloc::Layout;
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

use super::heap_core::HeapCore;
// Only used by the `#[cfg(feature = "alloc-xthread")]` push_with_overflow_retry
// method below — the items themselves are gated the same way in `heap_core.rs`,
// so this import must match or `alloc-global`-without-`alloc-xthread` fails
// with E0432 (no such item exists to import under that feature set).
#[cfg(feature = "alloc-xthread")]
use super::heap_core::{
    DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED, RING_PUSH_RETRY_SPINS,
};

// R6-REVIEW-F5: the R6-OPT-P0-4 flat scaled budget `RETRY_LOOP_ITERATIONS`
// (= `RING_PUSH_RETRY_SPINS` × 256 = 2,097,152 native) that once bounded the
// retry loop was DELETED — the loop's shape is now probe rounds of
// [`RETRY_ROUND_SPINS`] polls each, stopped by drain-progress detection
// ([`RETRY_STALLED_ROUNDS_GIVE_UP`]) under an absolute
// [`RETRY_ROUND_SAFETY_CAP`] round cap; no code read the historical constant
// anymore. Its calibration story (why the #136 scale-up existed and why the
// flat scaled budget became the R6-REGRESSION pathology) is preserved in
// `push_with_overflow_retry`'s doc comment below and in
// `RETRY_STALLED_ROUNDS_GIVE_UP`'s; the full A/B measurement notes remain in
// this constant's git history.

/// R6-REGRESSION (follow-up correction to R6-OPT-P0-4 / task #136): the
/// retry loop below is split into PROBE ROUNDS of this many pure
/// `core::hint::spin_loop()`-paced iterations each — see the
/// `push_with_overflow_retry` doc comment ("probe rounds, not one flat
/// spin") for the full rationale. `RING_PUSH_RETRY_SPINS` (8,192) is reused
/// as the round size: it is already the task #99 calibrated value for "how
/// many tight-spin polls give the owner a meaningful chance to drain
/// between checks", so the FIRST round alone reproduces the
/// PRE-R6-OPT-P0-4-scale-up spin shape exactly (same iteration count, same
/// no-sleep tight loop) — preserving the #136 high-contention judge's
/// calibration for the common, moderately-contended case, which resolves
/// within round 1 and never reaches a between-round sleep at all.
#[cfg(feature = "alloc-xthread")]
const RETRY_ROUND_SPINS: u32 = RING_PUSH_RETRY_SPINS;

/// R6-REGRESSION-2 (progress-detection stop condition — the follow-up
/// completing the R6-REGRESSION round/sleep reshaping): number of
/// CONSECUTIVE zero-drain-progress probe rounds after which
/// `push_with_overflow_retry`'s spin-retry tier concedes to the documented
/// bounded leak.
///
/// **Why progress detection, not a fixed round count.** The R6-REGRESSION
/// commit (`ba34fd5`) capped the retry at a FLAT 8 rounds. That fixed the
/// paused-owner CPU-burn pathology (see [`RETRY_ROUND_SLEEP`]'s history) but
/// reintroduced — under host CPU load — the exact throughput regression task
/// #136 exists to prevent: with a LIVE owner that is draining but CPU-starved
/// (descheduled between drain passes, or draining slower than 32 producers
/// can re-saturate the rings), a fixed ~8-round (~couple-ms) budget expires
/// while the owner is mid-recovery, and the push concedes even though waiting
/// WOULD have succeeded — measured as a flaky non-zero
/// `DBG_RING_PUSH_RETRY_EXHAUSTED` on `tests/remote_fanin.rs::
/// remote_fanin_high_contention_budget_is_sufficient` (exhausted_delta=821
/// observed during a host load spike; 0 when calm). No fixed round/iteration
/// budget can distinguish the two shapes this loop must treat oppositely:
///
/// - **paused owner (never drains):** any waiting is pure waste — give up
///   FAST (the R6-REGRESSION pathology was precisely waiting too long here);
/// - **live-but-slow owner under load:** the owner IS draining — stay
///   patient (conceding here is the #136 regression).
///
/// The distinguishing signal is whether the owner is making DRAIN PROGRESS,
/// and both rings expose it for free: each ring's `head` cursor is advanced
/// ONLY by the owner's drain (`RemoteFreeRing::head_relaxed` /
/// `HeapOverflow::head_relaxed` — cheap `Relaxed` loads of monotonic
/// cursors; see each accessor's doc comment for the soundness argument). The
/// loop snapshots both heads before round 1 and re-reads them after every
/// fully-failed round: if EITHER moved, the owner drained something in that
/// window — reset the stall counter and keep waiting; if NEITHER moved for
/// this many CONSECUTIVE rounds, the owner made zero progress across the
/// whole window — genuinely stalled/paused — concede.
///
/// **Why 128.** A stalled round's wall-clock is dominated by the
/// between-round sleep: [`RETRY_ROUND_SLEEP`] requests 200µs but the OS
/// timer's effective granularity on this project's dev host was MEASURED
/// anywhere from ~2ms to ~15ms per sleep (classic Windows timer
/// quantization, and it varies with whatever process currently holds the
/// system timer resolution — derived from
/// `tests/regression_paused_owner_wallclock.rs`'s per-concession cost
/// across runs), so 128 consecutive stalled rounds ≈ ~0.3–2s of
/// CONTINUOUSLY observed zero drain progress before the FIRST concession.
/// A small K is empirically too impatient: the owner's drains are BURSTY
/// (an entire ring is drained at once on the owner's alloc slow path, then
/// nothing until the next slow-path visit) and the owner thread itself can
/// be descheduled for tens of milliseconds under host load — K=4 measured
/// 6/10 failures on the #136 judge on an OTHERWISE IDLE host
/// (exhausted_delta 3..=696). K=128 measured 10/10 clean calm plus 8/8
/// clean under a deliberate 16-thread CPU-hog load (after the judge's own
/// harness-liveness race was separately fixed — see the R6-REGRESSION-2
/// note in `tests/remote_fanin.rs`). The generous first-concession patience
/// is affordable because it is paid at most ONCE per observed stall per
/// thread — see [`LAST_STALL_CONCESSIONS`] for the fast-concede memo cache
/// that keeps the paused-owner shapes
/// (`tests/regression_paused_owner_wallclock.rs`, the
/// `benches/heap_fanin_persistent.rs --reduced` paused cell) from re-paying
/// it on every subsequent push into the same unchanged stall.
///
/// Under `#[cfg(miri)]`: 1 (together with `RETRY_ROUND_SAFETY_CAP = 1` this
/// preserves the pre-existing exactly-one-round miri shape — miri's
/// interpreter gains nothing from sleeps or multi-round patience, and a
/// multi-round miri budget was independently measured impractically slow).
#[cfg(all(feature = "alloc-xthread", not(miri)))]
const RETRY_STALLED_ROUNDS_GIVE_UP: u32 = 128;
#[cfg(all(feature = "alloc-xthread", miri))]
const RETRY_STALLED_ROUNDS_GIVE_UP: u32 = 1;

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
const STALL_CONCESSION_WAYS: usize = 4;

/// R6-REVIEW-F2: one recorded concession snapshot — `(segment base, segment
/// ring drain head, heap-overflow drain head)` as captured at the moment a
/// live-owner retry conceded. See [`LAST_STALL_CONCESSIONS`].
#[cfg(feature = "alloc-xthread")]
type StallSnapshot = (usize, u32, Option<usize>);

/// R6-REVIEW-F2: the per-thread cache payload — [`STALL_CONCESSION_WAYS`]
/// snapshot slots plus the round-robin write cursor. A plain `Copy` tuple so
/// the whole thing lives in a const-initialized `Cell` (no allocation, no
/// `Drop` — see [`LAST_STALL_CONCESSIONS`]'s TLS-safety note).
#[cfg(feature = "alloc-xthread")]
type StallConcessionCache = ([Option<StallSnapshot>; STALL_CONCESSION_WAYS], usize);

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
    /// case is one cheap concession to the already-documented bounded leak.
    static LAST_STALL_CONCESSIONS: core::cell::Cell<StallConcessionCache> =
        const { core::cell::Cell::new(([None; STALL_CONCESSION_WAYS], 0)) };
}

/// R6-REGRESSION-2: absolute safety cap on TOTAL probe rounds (progressed or
/// not) per push — the backstop that keeps a single push's wall-clock
/// bounded even if the owner keeps making drain progress that this producer
/// somehow never converts into a successful push (e.g. every freed slot is
/// perpetually won by other producers). 4096 rounds (≈ tens of seconds at
/// the measured worst-case ~15ms effective sleep granularity) is far above
/// what the #136 judge needs even under heavy host load (its stuck pushes
/// resolve as soon as the starved owner gets a timeslice and drains —
/// observed well within tens of rounds), yet still a hard, finite bound: a
/// push can never wait unboundedly, preserving the loop's "mathematically
/// bounded" contract. In the paused-owner shape this cap is never reached —
/// zero progress trips [`RETRY_STALLED_ROUNDS_GIVE_UP`] (128 rounds) long
/// before it.
///
/// Under `#[cfg(miri)]`: 1 — see [`RETRY_STALLED_ROUNDS_GIVE_UP`]'s miri
/// note (exactly one pure-spin round, no sleep, as before).
#[cfg(all(feature = "alloc-xthread", not(miri)))]
const RETRY_ROUND_SAFETY_CAP: u32 = 4096;
#[cfg(all(feature = "alloc-xthread", miri))]
const RETRY_ROUND_SAFETY_CAP: u32 = 1;

/// R6-REGRESSION: the real OS-level sleep duration between probe rounds
/// (from round 2 onward — round 1 is a pure tight spin with no sleep before
/// it). 200 microseconds: long enough to be a genuine scheduler-visible
/// block (not a busy-loop in disguise — an earlier `yield_now()` attempt was
/// measured NOT to fix the paused-owner CPU-burn pathology, see
/// `push_with_overflow_retry`'s doc comment). NOTE the requested 200µs is a
/// floor, not the real cost: the effective granularity was measured at
/// ~15ms per sleep on this project's dev host (Windows timer quantization —
/// see [`RETRY_STALLED_ROUNDS_GIVE_UP`]'s doc comment for the measurement),
/// which is why the give-up cost budget is managed in ROUNDS with the
/// [`LAST_STALL_CONCESSIONS`] fast-concede memo cache rather than by shrinking
/// this Duration further (a sub-granularity request cannot get cheaper).
/// This sleep is load-bearing for the paused-owner fix: it is the OS-level
/// block that stopped the aggregate CPU burn (a genuinely idle wait, unlike
/// `spin_loop`/`yield_now`). Unused under `#[cfg(miri)]`
/// (`RETRY_ROUND_SAFETY_CAP == 1` there, so the loop never reaches a round
/// boundary).
#[cfg(feature = "alloc-xthread")]
const RETRY_ROUND_SLEEP: core::time::Duration = core::time::Duration::from_micros(200);

/// R7-A4: set the dirty bit for segment `base` in the owning HeapSlot's
/// `dirty_segments` bitmap. Called by `push_with_overflow_retry` AFTER a
/// successful `RemoteFreeRing::push` or `try_push_uncounted` — i.e. after
/// a ring entry has been published for this segment.
///
/// Resolves the owning HeapSlot via the segment's `owner_state` header stamp
/// (the SAME `unpack_owner_id` → `slot(idx)` path `resolve_heap_overflow`
/// uses). Reads the immutable `segment_id` from the segment header to compute
/// `(word, bit)` in the 16-word dirty bitmap.
///
/// **Ordering:** `fetch_or(bit, Release)` — the `Release` pairs with the
/// owner's `swap(0, Acquire)` in the dirty-drain loop, establishing
/// happens-before from the producer's ring publish (which completed before
/// this call) to the owner's dirty-drain iteration.
///
/// **Defensive:** if the owner id is out of range (should be unreachable for
/// a live, correctly-stamped segment — same argument as
/// `resolve_heap_overflow`'s `None` branch), the dirty bit is simply not set;
/// the linear-scan fallback eventually finds the ring entry anyway (P4
/// contract — see `remote_free_ring.rs` module doc).
///
/// R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): `packed` is the SAME
/// already-packed `(offset, class)` ring-entry word the caller just published
/// (see both call sites below) — `entry_class_idx(packed)` extracts the class
/// with no extra computation. When the feature is on, this ALSO sets the
/// corresponding bit in the per-(segment, class) sidecar
/// (`registry::dirty_by_class`), additively — the per-segment bit above is
/// still set unconditionally, so this cannot regress the non-class-aware
/// path.
///
/// R13-1 (task #271, P0 fix): sidecar materialisation failure (OOM) sets the
/// heap-wide, one-way [`HeapSlotRemote::sidecar_oom_latch`](super::heap_slot::HeapSlotRemote::sidecar_oom_latch)
/// PERMANENTLY, in addition to leaving the per-class bit unset for this push.
/// The per-segment bitmap (never affected by this) remains a correct routing
/// signal regardless — but the latch is what makes it the SOLE signal
/// `drain_dirty_segments` will ever consult for this heap again, closing a
/// visibility gap the old (pre-R13-1) "OOM = local no-op" behaviour left
/// open: without the latch, a LATER producer that successfully materialises
/// the sidecar (e.g. after the transient OOM condition clears) would flip the
/// consumer's `drain_dirty_segments` scan source over to the per-class path,
/// and THIS push's coarse-only entry — published while the sidecar was still
/// missing — would fall into the gap between the two signals: invisible to
/// the class-scoped scan (no per-class bit was ever set for it) until the
/// periodic full-scan fallback or an OOM-rescue scan eventually finds it.
/// See [`HeapSlotRemote::sidecar_oom_latch`](super::heap_slot::HeapSlotRemote::sidecar_oom_latch)'s
/// doc comment for the full design and [`AllocCore::drain_dirty_segments`](crate::alloc_core::AllocCore::drain_dirty_segments)'s
/// doc comment for the consumer-side read.
#[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
#[inline]
fn set_dirty_bit_for_segment(
    base: *mut u8,
    #[cfg_attr(not(feature = "class-aware-dirty"), allow(unused_variables))] packed: u32,
) {
    use crate::alloc_core::segment_header::{unpack_owner_id, SegmentHeader, SegmentMeta};

    let segment_id = SegmentHeader::segment_id_at(base) as usize;
    let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
    let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed)) as usize;
    let reg = super::bootstrap::ensure();
    if owner_id >= super::bootstrap::MAX_HEAPS {
        return; // Defensive: unstamped/garbled owner id.
    }
    let slot = reg.slot(owner_id);
    let word = segment_id / 64;
    let bit = 1u64 << (segment_id % 64);
    // The WORDS_PER_CLASS compile-time bound check: segment_id < MAX_SEGMENTS
    // is an invariant of the SegmentTable (register rejects overflow), so
    // word < DIRTY_BITMAP_WORDS by construction. debug_assert for defence.
    debug_assert!(
        word < super::heap_slot::DIRTY_BITMAP_WORDS,
        "segment_id {segment_id} out of dirty bitmap range"
    );
    if word < super::heap_slot::DIRTY_BITMAP_WORDS {
        slot.remote.dirty_segments[word].fetch_or(bit, Ordering::Release);
    }

    // R12-7 stage 2: additive per-class bit. See this function's doc comment.
    #[cfg(feature = "class-aware-dirty")]
    {
        use crate::alloc_core::dirty_by_class::{ensure_per_class_dirty, PER_CLASS_DIRTY_WORDS};
        use crate::alloc_core::remote_free_ring::entry_class_idx;
        use crate::alloc_core::segment_directory::WORDS_PER_CLASS;

        let class_idx = entry_class_idx(packed);
        let pc_word = class_idx * WORDS_PER_CLASS + word;
        // Defensive bounds guard (mirrors the `word < DIRTY_BITMAP_WORDS`
        // check above): `class_idx` is derived from `packed`, which THIS
        // caller just constructed from a real, in-range `SizeClasses::class_for`
        // result (never attacker-controlled or read back from a stale/garbled
        // source at this call site), so `pc_word` is in range by construction
        // — the `debug_assert!` documents that invariant loudly in debug
        // builds, while the runtime `if` keeps a release build's out-of-bounds
        // write impossible even if that invariant is ever violated by a
        // future change, rather than relying solely on the assert.
        debug_assert!(
            pc_word < PER_CLASS_DIRTY_WORDS,
            "class {class_idx} / segment word {word} out of per-class dirty bitmap range"
        );
        if pc_word < PER_CLASS_DIRTY_WORDS {
            match ensure_per_class_dirty(&slot.remote.dirty_by_class) {
                Some(pc) => {
                    // Release: pairs with the drain side's `swap(0, Acquire)`
                    // on this same word — identical ordering argument to the
                    // per-segment bit above, just projected onto the
                    // finer-grained sidecar.
                    pc.words[pc_word].fetch_or(bit, Ordering::Release);
                }
                None => {
                    // R13-1 (task #271): `ensure_per_class_dirty` returning
                    // `None` (sidecar OOM) trips the coarse-only latch
                    // PERMANENTLY for this heap — see this function's doc
                    // comment and `HeapSlotRemote::sidecar_oom_latch`'s doc
                    // comment for the full rationale. `Release`: pairs with
                    // `drain_dirty_segments`'s `Acquire` read of the latch,
                    // establishing happens-before from "this push's coarse-
                    // only publication" to "the consumer's decision to trust
                    // only the coarse bitmap". Idempotent plain store: every
                    // racing producer that ever observes sidecar OOM stores
                    // the same `true`, so no CAS is needed.
                    slot.remote.sidecar_oom_latch.store(true, Ordering::Release);
                }
            }
        }
    }
}

impl HeapCore {
    /// 0.3.0 (task A1); extracted for #132: push a Large/huge segment `base`
    /// onto the OWNING heap's deferred-free stack, given `head` — the
    /// owner's `thread_free_head()` (a `*const AtomicPtr<u8>`, obtained by a
    /// REMOTE freer from `owner_thread_free_at(segment_base)`). Called from
    /// [`dealloc_routing`](Self::dealloc_routing) in place of the old
    /// permanent-leak no-op.
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::push_large_deferred_free`] primitive
    /// (byte-for-byte the same push/CAS/double-push-guard logic this method
    /// used to inline — see that function's doc comment for the full
    /// mechanism and the double-push-guard hardening rationale). The
    /// primitive takes `&AtomicPtr<u8>` directly, so the pointer-to-reference
    /// deref of `head` stays HERE (via the `alloc_core::node` seam, same
    /// discipline as `deferred_next_atomic`/`owner_state_atomic`) rather
    /// than inside the shared (seam-free) module.
    #[cfg(feature = "alloc-xthread")]
    fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        // `heap_core.rs` is NOT an allowed `unsafe` seam (see `src/lib.rs`'s
        // seam whitelist), so the pointer-to-reference deref is delegated to
        // `Node::atomic_ptr_ref` (the `alloc_core::node` seam), same
        // discipline as `deferred_next_atomic`/`owner_state_atomic`.
        let head_ref: &AtomicPtr<u8> = Node::atomic_ptr_ref(head);
        crate::alloc_core::deferred_large::push_large_deferred_free(head_ref, base);
    }

    /// 0.3.0 (task A1); extracted for #132: drain this heap's deferred-free
    /// stack, reclaiming every queued Large/huge segment base via
    /// [`AllocCore::reclaim_large_segment`]. Called by the OWNER on its own
    /// `alloc_large` slow path, before reserving a fresh segment, so a
    /// cross-thread-freed large segment becomes available for reuse (via the
    /// `alloc-decommit` large-cache) or is released to the OS immediately
    /// (without `alloc-decommit`) — either way its `SegmentTable` slot is
    /// freed for reuse (the fix for the A1 permanent-leak bug).
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::drain_large_deferred_free`] primitive
    /// (byte-for-byte the same pop-loop/reclaim logic this method used to
    /// inline).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_large_deferred_free(&mut self) {
        // task H1: drain through the `&'static` slot handle, NOT an inline
        // field. `None` only in the pre-bind window — nothing could have been
        // pushed yet then (a remote push needs the stamp, which needs the
        // handle), so an empty-stack no-op is correct. Resolving the handle
        // BEFORE forming the `&mut self.core` split-borrow keeps the head
        // reference (a `&'static` into the slot, outside this `&mut HeapCore`)
        // disjoint from the core borrow.
        if let Some(head) = self.thread_free {
            crate::alloc_core::deferred_large::drain_large_deferred_free(head, &mut self.core);
        }
    }

    /// RAD-4b (task #72): drain THIS heap's slot-resident
    /// [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring — the
    /// second-chance queue [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// falls back to once a segment's own `RemoteFreeRing` AND its bounded
    /// retry are both exhausted (see that method's doc comment for the full
    /// design). Called by the OWNER on the SAME opportunistic schedule the
    /// per-segment rings are already drained (every magazine-miss slow path
    /// — see [`refill_magazine_slow`](Self::refill_magazine_slow) — and every
    /// `find_segment_with_free` scan), so overflow entries are reclaimed with
    /// the same liveness assumption every lazy-drain path in this allocator
    /// already relies on ("the owner drains on its own next `alloc()`").
    ///
    /// Each entry's `(base, packed)` pair is reclaimed via
    /// `AllocCore::reclaim_offset` (or, under `fastbin`, the
    /// magazine-checked `reclaim_offset_checked` — mirrors
    /// `dbg_drain_all_rings_impl`'s identical dual-path split) — the SAME
    /// defensively-guarded reclaim primitive the per-segment ring drain
    /// already uses, so a stale/garbled `base` (e.g. a segment recycled
    /// between push and drain) is rejected by its own magic/kind/bounds
    /// checks exactly as it would be for a per-segment ring entry, not
    /// specially trusted here.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn drain_heap_overflow(&mut self) {
        // RAD-4b: resolve through the pre-planted `&'static` handle (planted
        // by `bind_overflow` at claim time), NOT a fresh `bootstrap::ensure()`
        // + array index on every call — see the `overflow` field's doc
        // comment for the churn-gate cost this hoist recovers. `None` only in
        // the transient pre-bind window (never observed on any alloc/free
        // path — this drain runs only after a claimed heap's `alloc()`).
        let Some(overflow) = self.overflow else {
            return;
        };
        // RAD-4b iai churn-gate discipline: skip the full drain protocol
        // entirely on the overwhelmingly common "nothing has ever overflowed
        // into this ring" case — a single `Relaxed` load compared against our
        // own cached `tail`, mirroring the `last_stamped_segment` OPT-C cache
        // and `RemoteFreeRing`'s own documented `is_likely_empty` idiom. See
        // `HeapOverflow::is_likely_empty`'s doc comment for the full
        // soundness argument.
        if overflow.is_likely_empty(self.overflow_tail_cache) {
            return;
        }
        #[cfg(feature = "alloc-decommit")]
        let small_cur = self.core.small_cur();

        // R11-2 (Bug 2 — deferred pool/release finalization): collect segment
        // bases that go fully empty (`live_count` hits 0) during this drain
        // pass, to finalize via `release_or_pool_empty_segment` AFTER the
        // drain fully returns. This MUST be deferred, not done inline: a
        // single `overflow.drain(...)` call can process entries targeting
        // MANY different segment bases, and the SAME base can appear in
        // multiple entries. If base X goes empty at entry #2 of 5 targeting
        // base X, calling `release_or_pool_empty_segment(base X)` right there
        // would free/decommit/repurpose base X's metadata — and entries
        // #3–5 (still queued in this same drain pass) would then call
        // `reclaim_offset` against freed/decommitted memory.
        //
        // **Capacity.** `EMPTIED_BASES_CAP = 64`: going fully empty via this
        // SECOND-CHANCE overflow ring in one opportunistic drain call is a
        // rare tail event (requires ALL of a segment's live blocks to be
        // freed through the overflow ring, not through normal dealloc or the
        // per-segment ring). Under miri, `HEAP_OVERFLOW_CAP == 64`, so at
        // most 64 distinct bases can be drained in one pass — 64 covers
        // miri exactly. For native (`HEAP_OVERFLOW_CAP = 2048`), 64 gives
        // generous headroom for the realistic tail.
        //
        // **Capacity-exceeded case (R12-6: closed, was previously a silent
        // gap).** If a 65th distinct base goes empty, it is simply NOT
        // collected into `emptied_bases` — the segment stays as an ordinary
        // registered segment (its BinTable is populated with the freed
        // blocks, so `find_segment_with_free` will find and reuse it; it is
        // never leaked or unreachable). Before R12-6 it was ALSO never
        // finalized this pass except by chance on a future emptying through a
        // normal path — leaving native's realistic tail (`HEAP_OVERFLOW_CAP =
        // 2048` allows up to 2048 distinct bases in one drain, far past the
        // 64-slot dedup buffer) at inflated RSS/commit and outside the
        // pool-cap budget indefinitely, not merely "until next touched".
        // R12-6 closes this: `emptied_overflowed` below records whether the
        // buffer was actually exhausted, and if so a single post-drain sweep
        // (`AllocCore::finalize_orphaned_empty_segments`) scans every
        // registered segment for the ones the buffer had no room for and
        // finalizes them too. This is O(registered segments) instead of O(1),
        // but it runs ONLY on this genuinely rare tail (>64 DISTINCT bases
        // emptied by second-chance overflow reclaims alone, in one
        // opportunistic drain) — the common case (buffer never overflows)
        // still pays nothing beyond the existing dedup scan.
        //
        // **Dedup.** Under correctly-functioning ring data, `dec_live_and_
        // maybe_decommit` returns `true` at most once per base per drain
        // pass: `live_count` is monotonically non-increasing across this
        // pass (nothing here allocates, so nothing re-increments it), and
        // once it hits 0 any FURTHER entry for the same base necessarily
        // targets an already-freed block — rejected by `reclaim_offset`'s
        // `is_free` bitmap guard BEFORE `dec_live_and_maybe_decommit` is
        // ever called for it. So under normal operation the dedup scan is
        // defensive (belt-and-suspenders), not load-bearing.
        //
        // It is NOT purely theoretical, though: `SegmentMeta::dec_live` uses
        // `saturating_sub`, specifically so a live-count underflow "keeps
        // the counter sane rather than wrapping to `u32::MAX`" (see that
        // method's own doc comment) — which means if the `is_free` guard
        // were ever bypassed (a garbled/corrupt ring entry, or a future bug
        // elsewhere that lets an already-reclaimed offset re-enter the
        // ring), `dec_live` would clamp at 0 and `dec_live_and_maybe_
        // decommit` WOULD return `true` again for the same base in the same
        // pass. The dedup array is what keeps THAT scenario from calling
        // `release_or_pool_empty_segment` on the same base twice (a
        // double-pool/double-release, guarded again by that function's own
        // `debug_assert!` — see its doc comment). So this is real
        // defence-in-depth against the saturating-arithmetic edge case, not
        // dead code.
        const EMPTIED_BASES_CAP: usize = 64;
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_bases: [*mut u8; EMPTIED_BASES_CAP] =
            [core::ptr::null_mut(); EMPTIED_BASES_CAP];
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_count: usize = 0;
        // R12-6: set when a distinct empty-transition is observed but the
        // dedup buffer above is already full (the 65th+ distinct base case
        // documented above) — signals that the rare post-drain fallback
        // sweep is needed after this drain returns.
        #[cfg(feature = "alloc-decommit")]
        let mut emptied_overflowed = false;

        #[cfg(feature = "fastbin")]
        {
            // No "class `c` currently being refilled" context exists at this
            // call site (unlike `refill_class_bump_checked`'s closure in
            // `refill_magazine_slow`, which special-cases `k == c` because
            // `count[c] == 0` is a load-bearing invariant for THAT specific
            // refill) — this drain reclaims entries of ANY class, so the
            // predicate unconditionally checks the magazine-residency bitmap,
            // mirroring `dbg_drain_all_rings_impl`'s general-purpose pattern.
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                if AllocCore::reclaim_offset_checked(base, packed, &|ptr, _k| {
                    let pbase = os::segment_base_of_ptr(ptr);
                    let poff = (ptr as usize - pbase as usize) as u32;
                    SegmentMeta::new(pbase)
                        .magazine_bitmap()
                        .is_in_magazine(poff)
                }) {
                    // R11-2 (Bug 1): sync the segment directory inline per
                    // successful reclaim — mirrors the ESTABLISHED pattern
                    // in `drain_dirty_segments` / `find_segment_with_free_impl`'s
                    // per-segment ring drain, but with a per-entry immediate
                    // sync (1u64 << class_idx) instead of a batched bitmask,
                    // because `HeapOverflow` is a cross-segment MPSC ring
                    // (one drain call can touch many different bases).
                    let sid = SegmentHeader::segment_id_at(base) as usize;
                    let class_idx = crate::alloc_core::remote_free_ring::entry_class_idx(packed);
                    self.core
                        .sync_directory_for_segment_classes(base, sid, 1u64 << class_idx);
                    // R11-2 (Bug 2): collect the base for deferred
                    // pool/release if the segment just went empty.
                    #[cfg(feature = "alloc-decommit")]
                    {
                        if AllocCore::dec_live_and_maybe_decommit(base, small_cur) {
                            let already =
                                emptied_bases.iter().take(emptied_count).any(|&b| b == base);
                            if !already {
                                if emptied_count < EMPTIED_BASES_CAP {
                                    emptied_bases[emptied_count] = base;
                                    emptied_count += 1;
                                } else {
                                    // R12-6: a distinct 65th+ base emptied via
                                    // this drain's overflow-ring reclaims —
                                    // the dedup buffer has no room left.
                                    // Recorded here so the post-drain
                                    // fallback sweep below picks it (and any
                                    // sibling overflow bases) up.
                                    emptied_overflowed = true;
                                }
                            }
                        }
                    }
                }
            });
        }
        #[cfg(not(feature = "fastbin"))]
        {
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                if AllocCore::reclaim_offset(base, packed) {
                    let sid = SegmentHeader::segment_id_at(base) as usize;
                    let class_idx = crate::alloc_core::remote_free_ring::entry_class_idx(packed);
                    self.core
                        .sync_directory_for_segment_classes(base, sid, 1u64 << class_idx);
                    #[cfg(feature = "alloc-decommit")]
                    {
                        if AllocCore::dec_live_and_maybe_decommit(base, small_cur) {
                            let already =
                                emptied_bases.iter().take(emptied_count).any(|&b| b == base);
                            if !already {
                                if emptied_count < EMPTIED_BASES_CAP {
                                    emptied_bases[emptied_count] = base;
                                    emptied_count += 1;
                                } else {
                                    // R12-6: a distinct 65th+ base emptied via
                                    // this drain's overflow-ring reclaims —
                                    // the dedup buffer has no room left.
                                    // Recorded here so the post-drain
                                    // fallback sweep below picks it (and any
                                    // sibling overflow bases) up.
                                    emptied_overflowed = true;
                                }
                            }
                        }
                    }
                }
            });
        }

        // R11-2 (Bug 2): finalize each emptied base now that the drain has
        // fully returned. Safe: no more entries will be processed against
        // any base in this drain pass, so releasing/pooling cannot race
        // with an in-flight reclaim.
        #[cfg(feature = "alloc-decommit")]
        for &base in emptied_bases.iter().take(emptied_count) {
            self.core.release_or_pool_empty_segment(base);
        }

        // R12-6: the dedup buffer overflowed (more than `EMPTIED_BASES_CAP`
        // distinct bases emptied via this drain's overflow-ring reclaims
        // alone) — run the rare post-drain fallback sweep to finalize the
        // ones the buffer had no room for. Same "drain has fully returned"
        // safety argument as the loop above: every overflow entry has
        // already been reclaimed by this point, so no further reclaim can
        // race a release/pool of any base.
        #[cfg(feature = "alloc-decommit")]
        if emptied_overflowed {
            self.core.finalize_orphaned_empty_segments(small_cur);
        }
    }

    // -----------------------------------------------------------------------
    // Cross-thread free routing (only under `alloc-xthread`).
    //
    // This re-bases the Phase 10 `Heap::dealloc_small` /
    // `Heap::dealloc_any_thread` discipline on the registry-resident
    // `HeapCore`. The block at `ptr` may belong to:
    //   - a segment THIS heap owns (stamped with our head, or unstamped) →
    //     own-thread path via `AllocCore::dealloc`;
    //   - a segment owned by ANOTHER heap (stamped with its head) → push
    //     onto that heap's TFS via `ThreadFreeStack::push`;
    //   - a foreign (non-sefer) pointer → safe no-op.
    // -----------------------------------------------------------------------

    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(super) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        let base = os::segment_base_of_ptr(ptr);

        // Task #135 (Part 3, M2 hardening): check `self.core.contains_base(base)`
        // FIRST, before touching any segment memory. `contains_base` is an O(1)
        // lookup in OUR OWN `SegmentTable`'s open-addressing hash — it reads
        // only our own primordial-segment-resident table, never `base`'s
        // memory, so it is safe to call even if `base` is unmapped (a
        // released/decommitted segment).
        //
        // `contains_base(base) == true` if and only if `base` is currently
        // registered in OUR table — which happens exactly when we own a live
        // (mapped) segment there (`register_segment`/`alloc_large*` register
        // on creation; `unregister`/`recycle` remove on release — see
        // `segment_table.rs`). So TRUE implies "our segment, definitely
        // mapped" — equivalent to the old `owner_tf.is_null() || owner_tf ==
        // our_head` condition for every segment WE registered (an unstamped
        // own-segment has `owner_tf == null`; a stamped own-segment has
        // `owner_tf == our_head` — both cases are covered by "it's in our
        // table"), without reading `base`'s memory at all. Route it own-thread
        // immediately — no magic/kind read needed.
        if self.core.contains_base(base) {
            // Э9 (P7.1): `base` is already in hand from the `contains_base`
            // ownership check above; under fastbin, hand it to the own-thread
            // body directly so `segment_base_of_ptr` is not recomputed. Under
            // non-fastbin `dealloc_own_thread` just delegates to `core.dealloc`
            // (base unused there).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            self.dealloc_own_thread_with_base(ptr, layout, base);
            #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
            self.dealloc_own_thread(ptr, layout);
            return;
        }
        // `contains_base` is FALSE: not one of our segments. The entire cold
        // cross-thread tail (magic/kind checks, Large deferred-push, ring
        // push) is outlined below — see `dealloc_foreign_slow`'s doc comment
        // (PERF-PASS-2, G10/D2, task #50).
        self.dealloc_foreign_slow(ptr, base, layout);
    }

    /// PERF-PASS-2 (G10/D2, task #50): outlined cold cross-thread dealloc
    /// tail, split out of `dealloc_routing` (which is `#[inline(always)]` and
    /// sits directly behind the hot own-thread `contains_base` hit-check on
    /// EVERY free). Before this split, the entire body below — magic/kind
    /// header reads, the Large-segment deferred-free push, and the small-
    /// block ring push, each with hardened/non-hardened variants — was
    /// inlined into `dealloc_routing` itself, bloating the I-cache footprint
    /// of the hot own-thread free path with code that only ever executes on
    /// a genuine cross-thread free (`contains_base(base) == false`). Mirrors
    /// the existing `refill_magazine_slow` outlining pattern (`#[cold]
    /// #[inline(never)]`, called once behind a single cold branch).
    ///
    /// Thin wrapper (R6-OPT-P0-1): computes `our_head` (the ONE piece of this
    /// routing that genuinely needs `&self` — see
    /// [`dealloc_foreign_routing`]'s doc comment for why every other step is
    /// heap-instance-independent) and delegates to the shared, `&self`-free
    /// [`dealloc_foreign_routing`] with `Some(our_head)`, preserving this
    /// bound-thread caller's behavior byte-for-byte (the `owner_tf ==
    /// our_head` defensive no-op branch still fires exactly as before).
    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    fn dealloc_foreign_slow(&mut self, ptr: *mut u8, base: *mut u8, layout: Layout) {
        let our_head = self.thread_free_head();
        Self::dealloc_foreign_routing(ptr, base, layout, Some(our_head));
    }

    /// R6-OPT-P0-1: the heap-instance-independent core of the cross-thread
    /// dealloc routing tail, shared by two callers:
    ///
    /// - [`dealloc_foreign_slow`](Self::dealloc_foreign_slow) (a BOUND
    ///   thread's cold cross-thread-free path, via `dealloc_routing`) passes
    ///   `Some(our_head)` — `our_head` being ITS `thread_free_head()` — so the
    ///   `owner_tf == our_head` defensive no-op guard (see below) still
    ///   applies exactly as before this split.
    /// - a bind-less thread's dealloc resolver (`global::tls_heap`'s
    ///   `current_for_dealloc`, reached from `SeferAlloc::dealloc` WITHOUT
    ///   constructing or dereferencing any `*mut HeapCore`) passes `None` —
    ///   there is no "our_head" to compare against, because a bind-less
    ///   thread (TLS never bound, or `TORN` — its slot already recycled) has
    ///   no live heap of its own to compare the segment's owner stamp
    ///   against. Every valid pointer reaching `dealloc` on such a thread is
    ///   foreign BY CONSTRUCTION: `SeferAlloc::alloc` always binds a heap on
    ///   first use, so a thread whose TLS is null/TORN never allocated
    ///   anything itself under this allocator instance — the pointer, if
    ///   valid, was necessarily produced by (and stamped with the owner of)
    ///   some OTHER thread's heap.
    ///
    /// Byte-identical body to the pre-split `dealloc_foreign_slow` tail (same
    /// statements, same order, same `return` points) EXCEPT that the
    /// `owner_tf == our_head` half of the defensive check is skipped entirely
    /// when `our_head` is `None` — see the `match our_head` below. Every
    /// other call in this function is already an associated function taking
    /// no `&self`/`&mut self` (`Self::push_large_deferred_free`,
    /// `Self::push_with_overflow_retry`) — they operate purely on
    /// `base`/`packed`/`head` parameters and the process-global registry via
    /// `super::bootstrap::ensure()` — which is what makes this split
    /// possible at all: routing a foreign pointer never actually needed a
    /// live `&mut HeapCore`, only (for a bound thread) its own head for the
    /// self-check.
    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    pub(crate) fn dealloc_foreign_routing(
        ptr: *mut u8,
        base: *mut u8,
        layout: Layout,
        our_head: Option<*const AtomicPtr<u8>>,
    ) {
        // `base` is not one of OUR segments. Two possibilities:
        //   (a) a LIVE segment owned by ANOTHER heap — mapped, its owner's
        //       table contains it (just not ours) — reading its header is
        //       safe, and this cross-thread free must be routed to its owner.
        //   (b) a segment WE (or someone) already released — decommitted +
        //       unmapped, its table slot recycled — reading its header would
        //       fault.
        // We cannot O(1)-distinguish (a) from (b) without a global registry
        // (out of scope here); this is the same limitation every allocator
        // has for a double-free-after-full-release. A double-free of a
        // released, unmapped segment is fundamentally UB (as with any
        // allocator) and is NOT fixed by this change — only guarded for the
        // live/mapped case, which is what M2 promises. See the module-level
        // note referenced from task #135's report for the full argument.
        //
        // 0.3.0 (task #138): for the Large branch below, a further
        // POST-reuse mitigation (layout-vs-header size consistency check,
        // `large_layout_consistent`) narrows — but does not close — the
        // remaining window where `base` WAS released and has since been
        // reused for a new allocation before this stale free arrives. See
        // that function's doc comment for the residual limit.
        //
        // Field-specific reads (task #33 root-cause fix): read ONLY `magic`,
        // `kind`, `owner_thread_free` — the cross-thread-read fields written
        // once at init/stamp time and only read thereafter. A full-struct
        // `SegmentHeader::read_at` here raced with the Owner's `bump`-touching
        // `write_header` on `carve_block` (the §11 data race); reading each
        // field individually via its `offset_of!` offset touches bytes
        // disjoint from the owner-mutated `bump`, so there is no race.
        //
        // R4-2 (memory_safety_review, R4-MS-1/MS-2): the first field read is
        // `magic_at(base)`. For a garbage pointer like `1 as *mut u8`,
        // `segment_base_of_ptr` masks to `base == 0`, so `magic_at(0)` would
        // dereference address `offset_of!(SegmentHeader, magic)` with no guard
        // — an immediate read of a structurally-impossible "segment". Reject a
        // null base here as a safe no-op (the same outcome the magic mismatch
        // below produces for a non-segment base). This narrows, but does not
        // close, the cross-heap staleness window noted above (case (a) vs (b)):
        // a base that masks to null cannot be a real segment by construction.
        if base.is_null() {
            return;
        }
        if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
            return;
        }
        let owner_tf = SegmentHeader::owner_thread_free_at(base);
        // R6-OPT-P0-1: `Some(head)` preserves the exact bound-thread
        // defensive check (`owner_tf.is_null() || owner_tf == head`).
        // `None` (bind-less caller) skips the `== head` half entirely — there
        // is no "us" to compare against — but keeps the `is_null()` no-op
        // (an unstamped segment; defensive, should not happen for a live
        // block, same as today).
        let is_self_or_unstamped = match our_head {
            Some(head) => owner_tf.is_null() || owner_tf == head,
            None => owner_tf.is_null(),
        };
        if is_self_or_unstamped {
            // `contains_base` was false (for the bound-thread caller) — or
            // there simply is no `contains_base` table to consult (the
            // bind-less caller has no heap at all) — yet the header claims
            // this segment is unstamped, or (bound-thread only) stamped as
            // ours. For the bound-thread case this can only happen for a
            // segment that used to be ours and was released (case (b) above,
            // reading now-decommitted-but-still-committed metadata pages of a
            // NOT-YET-actually-unmapped segment is impossible in this
            // process — metadata pages are only unmapped by `os::release_segment`,
            // at which point this read would fault, not return a stale value).
            // Defensive no-op: do NOT route to ourselves via a table state we
            // just proved does not list this segment (or, for the bind-less
            // caller, do not touch an unstamped segment at all).
            return;
        }
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
            // 0.3.0 (task A1): used to be a bare `return` here — a PERMANENT
            // leak. The whole segment (4+ MiB, or more for an oversized
            // allocation) was never released and its `SegmentTable` slot was
            // never recycled, because no code path ever revisited a
            // cross-thread-freed Large segment. Fix: push `base` onto the
            // OWNING heap's deferred-free stack (`owner_tf`, already read
            // above — the owner's `thread_free_head()`); the owner reclaims
            // it lazily on its next `alloc_large` slow path (see
            // `drain_large_deferred_free`, called from `alloc`).
            //
            // 0.3.0 (task #138, A1 post-reuse mitigation): before queuing,
            // check that `layout`'s size matches the CURRENT occupant's
            // `large_size` in the header. A stale double-free whose segment
            // was ALREADY reclaimed+reused between the original free and
            // this call will, in the overwhelming majority of cases,
            // observe a header describing a DIFFERENT allocation — this is
            // NOT a full fix (a reuse that happens to request the
            // bit-identical size is not caught; double-free is UB by
            // contract) but narrows the post-reuse corruption window. See
            // `alloc_core::deferred_large::large_layout_consistent`'s doc
            // comment for the full rationale and residual limit.
            if crate::alloc_core::deferred_large::large_layout_consistent(base, layout.size()) {
                Self::push_large_deferred_free(owner_tf, base);
            }
            return;
        }
        // Variant-2: push (offset, class) to the per-segment ring (block bytes
        // untouched). The freer HAS the `Layout`, so it derives the size class
        // here and carries it in the ring entry — the owner's `page_map` is
        // unreliable for the mixed-class pages a shared bump cursor produces, so
        // `reclaim_offset` must NOT derive the class itself (RACE_DRAIN_RECLAIM
        // §13). `kind != Large` is already established above, so a small block's
        // class is always `Some`.
        let off = (ptr as usize - base as usize) as u32;
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let class_idx =
            match crate::alloc_core::size_classes::SizeClasses::class_for(size, layout.align()) {
                Some(c) => c as u32,
                None => return, // Large layout on a small segment: contract violation; drop.
            };
        // X7 Ф3 (task #191) touch (b): under `hardened`, stamp the block's
        // CURRENT generation (as observed by THIS freeing thread, Relaxed) into
        // the ring note via `pack_entry_hardened`. The owner's drain (touch (c))
        // compares this stamped gen against the block's gen-at-drain-time; a
        // mismatch means the block was re-issued since this note was stamped,
        // so honouring it would double-free/corrupt the CURRENT occupant — the
        // note is dropped. Non-hardened builds keep the untouched `pack_entry`
        // exactly as before (byte-identical, verified by construction — the
        // `cfg(not)` branch IS the pre-existing code, not a re-implementation).
        // Sibling-block discipline mirrors `Layout::small_meta_end()` (Ф1).
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset of a live block.
            #[allow(unsafe_code)]
            let gen = unsafe { crate::alloc_core::segment_header::gen_at(base, off as usize) };
            let packed =
                crate::alloc_core::remote_free_ring::pack_entry_hardened(gen, class_idx, off);
            let ring = SegmentMeta::new(base).remote_ring();
            Self::push_with_overflow_retry(&ring, base, packed);
        }
        #[cfg(not(feature = "hardened"))]
        {
            let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx);
            let ring = SegmentMeta::new(base).remote_ring();
            Self::push_with_overflow_retry(&ring, base, packed);
        }
    }

    /// RAD-4 (Phase 4, E3a); extended by RAD-4b (task #72); reordered by
    /// R6-OPT-P0-4: push `packed` (the block's segment-relative `(offset,
    /// class)` word, already packed by the caller) onto `ring`, falling back
    /// through a three-tier chain — segment ring → heap-level overflow ring →
    /// bounded spin-retry against both — before conceding to the original
    /// documented-sound bounded leak.
    ///
    /// **R6-OPT-P0-4 — overflow-first policy (current).** The PRE-R6-OPT-P0-4
    /// policy exhausted the WHOLE [`RING_PUSH_RETRY_SPINS`] (8,192) spin
    /// budget against the segment ring FIRST, and only tried the heap-level
    /// [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring after that
    /// budget was exhausted. Each failed `RemoteFreeRing::push` attempt inside
    /// that budget ticks TWO diagnostic counters (`overflow()` +
    /// `DBG_RING_OVERFLOW`, both locked RMWs) — so a single logical free
    /// landing on a saturated ring with a LIVE owner (the common, non-
    /// pathological case) paid up to 8,193 full ring-state checks and 16,386
    /// counter RMWs before ever trying the second-chance ring that was sitting
    /// right there the whole time with 8x the capacity
    /// (`HeapOverflow::HEAP_OVERFLOW_CAP` = 2048 vs. `RING_CAP` = 256). The
    /// policy is now:
    ///
    /// 1. One normal (counted) [`RemoteFreeRing::push`] attempt — the fast
    ///    path; the common case never proceeds past this.
    /// 2. On that push failing (ring full): IMMEDIATELY try
    ///    [`push_to_heap_overflow`](Self::push_to_heap_overflow) — BEFORE any
    ///    spinning. This is the actual inversion: try the cheap,
    ///    already-provisioned second-chance ring first, instead of last.
    /// 3. Only if BOTH the ring push AND the immediate overflow attempt
    ///    failed (both momentarily full — a rare double-saturation case) does
    ///    this fall into the bounded spin-retry loop below, still gated by
    ///    [`owner_slot_is_live`](Self::owner_slot_is_live) exactly as before
    ///    (see that method's doc comment — the gate exists to stop
    ///    pathological aggregate stall against a dead/exited owner; see
    ///    `tests/race_norecycle.rs` and the big comment block above
    ///    [`RING_PUSH_RETRY_SPINS`] in `heap_core.rs` explaining why flat spin
    ///    (not backoff) was chosen and why the budget is calibrated to
    ///    8,192 = 32×`RING_CAP`). The loop runs as probe rounds of
    ///    [`RETRY_ROUND_SPINS`] polls each, stopped by drain-progress
    ///    detection ([`RETRY_STALLED_ROUNDS_GIVE_UP`]) under the absolute
    ///    [`RETRY_ROUND_SAFETY_CAP`] — NOT one flat `RING_PUSH_RETRY_SPINS`
    ///    (or scaled) iteration budget; see the R6-REGRESSION section below
    ///    for how the loop's bound evolved to this shape.
    /// 4. Inside that retry loop, every poll retries BOTH tiers: the segment
    ///    ring via [`RemoteFreeRing::try_push_uncounted`] — NOT `push` — so a
    ///    failed ring poll does not re-tick either ring diagnostic counter
    ///    (both already ticked once, in step 1's single counted attempt) —
    ///    AND the heap-level overflow ring, via the SAME `&'static
    ///    HeapOverflow` reference [`resolve_heap_overflow`](
    ///    Self::resolve_heap_overflow) resolves ONCE before the loop starts
    ///    (not re-resolved on every poll — see that function's doc comment for
    ///    the measured per-poll resolution cost this hoist avoids). Retrying
    ///    BOTH on every poll (not just the ring) matters under sustained high
    ///    fan-in: the owner's opportunistic `drain_heap_overflow` runs only
    ///    between its own `alloc()` calls, so a transient overflow-ring-full
    ///    moment needs the SAME spin window's repeated chances the ring gets,
    ///    not a single try-once-and-never-again attempt — see race model (a)
    ///    in this method's correctness notes ("owner-drain racing producer-
    ///    reservation on EITHER tier"). Skipping this and only retrying the
    ///    ring measurably regressed
    ///    `tests/remote_fanin.rs::remote_fanin_high_contention_budget_is_sufficient`
    ///    (32-producer live-owner fan-in) during this policy's development.
    ///    On a successful retry (either tier), [`DBG_RING_PUSH_RETRIED`] is
    ///    bumped exactly once (a single, meaningful, low-frequency event —
    ///    not per-attempt). If the owner is NOT live, spinning on the ring is
    ///    skipped entirely (nothing will drain it — see
    ///    [`owner_slot_is_live`](Self::owner_slot_is_live)'s doc comment) but
    ///    ONE more `push_to_heap_overflow` attempt still runs (that ring is
    ///    drained by whichever thread next claims the slot, not necessarily
    ///    "this owner"). Only once every avenue above is exhausted does this
    ///    concede to the bounded leak and bump [`DBG_RING_PUSH_RETRY_EXHAUSTED`]
    ///    — which now means "truly nothing worked: initial ring attempt
    ///    failed, immediate overflow attempt failed, AND the bounded retry
    ///    against both never recovered".
    ///
    /// Net effect: the overwhelmingly common case (ring full, overflow has
    /// room) now costs 2 checks total (1 counted ring push + 1 overflow push)
    /// instead of up to 8,193. The genuinely rare double-saturation case still
    /// gets the full retry protection against both tiers, just without the
    /// ring's own counter-RMW tax on every failed ring poll.
    ///
    /// **R6-REGRESSION + R6-REGRESSION-2 (follow-up corrections to
    /// R6-OPT-P0-4) — progress-detected probe rounds with a real sleep, not
    /// one flat 2M-iteration spin and not a fixed round count.** Step 4's
    /// retry loop is not one flat 2,097,152-iteration busy-spin (the
    /// historical `RETRY_LOOP_ITERATIONS` budget, since deleted —
    /// R6-REVIEW-F5). It is split into
    /// [`RETRY_ROUND_SPINS`]-sized (8,192) PROBE ROUNDS with a real
    /// `std::thread::sleep(`[`RETRY_ROUND_SLEEP`]`)` OS-level block between
    /// rounds (from round 2 onward), and its STOP CONDITION is
    /// drain-progress detection, not a fixed round budget: before round 1
    /// and after every fully-failed round the loop reads both tiers' DRAIN
    /// cursors (`RemoteFreeRing::head_relaxed` /
    /// `HeapOverflow::head_relaxed` — each advanced ONLY by the owner's
    /// drain). If either cursor moved, the owner drained something in that
    /// window — the stall counter resets and the loop keeps waiting (the
    /// owner is live-but-slow; conceding here is exactly the #136
    /// regression). Only after [`RETRY_STALLED_ROUNDS_GIVE_UP`] (128 native /
    /// 1 miri) CONSECUTIVE rounds in which NEITHER cursor advanced — the
    /// owner made zero drain progress across the whole observed window,
    /// i.e. it is genuinely stalled/paused — does the push concede to the
    /// documented bounded leak. An absolute [`RETRY_ROUND_SAFETY_CAP`] (4096
    /// native / 1 miri) on total rounds backstops the pathological
    /// "owner keeps draining but this producer never wins a slot" shape so
    /// a single push stays hard-bounded in wall-clock regardless. And so
    /// that a SUSTAINED stall (the paused-owner burst shape, where every
    /// push past capacity must eventually concede) does not re-pay the full
    /// first-concession patience on every subsequent push, each concession
    /// memoizes its cursor snapshot per-thread into a small
    /// [`STALL_CONCESSION_WAYS`]-way cache keyed by segment base
    /// ([`LAST_STALL_CONCESSIONS`]): a following push that finds both cursors
    /// still exactly at a snapshot it previously conceded against (for ANY of
    /// the cached segments — the multi-way arity is what keeps frees
    /// interleaved across SEVERAL simultaneously-stalled segments from
    /// evicting each other's snapshots, R6-REVIEW-F2) is provably inside the
    /// same continuous stall and concedes after a single probe round. See
    /// [`RETRY_STALLED_ROUNDS_GIVE_UP`]'s and [`LAST_STALL_CONCESSIONS`]'s
    /// doc comments for the full two-shapes tension (paused owner must give
    /// up cheaply; slow-live owner under host load must be waited out) and
    /// why no fixed budget can resolve both.
    ///
    /// R6-OPT-P0-4 (task #136) scaled the then-current flat retry budget
    /// (`RETRY_LOOP_ITERATIONS`, historical — deleted by R6-REVIEW-F5) to
    /// 256× `RING_PUSH_RETRY_SPINS` (2,097,152) specifically so a MODERATELY
    /// contended double-saturation still resolves within the (now much
    /// cheaper, uncounted) spin budget. It did not anticipate — and
    /// `benches/heap_fanin_persistent.rs --reduced`'s `T=32, burst=100_000,
    /// owner=paused` matrix cell later measured — a SUSTAINED
    /// double-saturation: many producer threads, an owner that is live
    /// (`owner_slot_is_live` true, so the gate does not short-circuit) but
    /// genuinely never drains for the whole burst. In that shape nearly
    /// every one of the burst's pushes spins through most/all of its
    /// 2,097,152-iteration budget purely burning CPU on
    /// `core::hint::spin_loop()` — a CPU-level hint (e.g. `PAUSE`), not an
    /// OS-level yield, so it never gives the scheduler a chance to run
    /// anything else. Measured: this scaled cell burned thousands of
    /// CPU-seconds over minutes of wall-clock with zero throughput, on a
    /// 16-thread host, at BOTH T=32 (2x oversubscribed) and T=8 (not
    /// oversubscribed) — ruling out "just an oversubscription/scheduler-
    /// starvation artifact of T=32" as the sole mechanism; the dominant cost
    /// is the sheer aggregate iteration count (many threads × up to
    /// 2,097,152 iterations each) all CAS-contending the SAME two hot
    /// atomics (the ring's and the overflow ring's cursors), independent of
    /// whether the host is oversubscribed.
    ///
    /// The fix: keep the FIRST round (`RETRY_ROUND_SPINS` = the un-scaled
    /// `RING_PUSH_RETRY_SPINS` = 8,192) as a pure tight busy-spin, byte-for-
    /// byte the same shape task #99 originally calibrated — this alone is
    /// what the #136 high-contention judge
    /// (`tests/remote_fanin.rs::remote_fanin_high_contention_budget_is_sufficient`)
    /// needs in the CALM case; that judge's workload resolves within round 1
    /// essentially always when the host is idle and never reaches a sleep.
    /// Only once a full round fails outright does the loop sleep for
    /// [`RETRY_ROUND_SLEEP`] (200µs) before starting the next round —
    /// continuing for as long as the owner keeps making drain progress
    /// (R6-REGRESSION-2, see above), up to [`RETRY_ROUND_SAFETY_CAP`] rounds
    /// total.
    ///
    /// **An earlier version of this fix kept the SAME 2,097,152-iteration
    /// total budget and only inserted `std::thread::yield_now()` between
    /// rounds** (a re-shaping, not a reduction, of the R6-OPT-P0-4 budget).
    /// Measured against the same pathological workload, this did NOT fix
    /// it: a `T=32, N=6_000` repro of the pathology still failed to complete
    /// within a 60s hard timeout (vs. 87s unfixed — better, but still
    /// pathological). Root cause: `yield_now()` is a scheduling HINT with no
    /// other runnable work to hand the CPU to when EVERY thread in the
    /// contending set is itself spin-then-yield-looping — the OS scheduler
    /// round-robins the same spinning threads back onto the same cores
    /// almost immediately (confirmed via CPU-time sampling mid-run: ~9
    /// CPU-seconds burned per wall-clock second at 32 threads / 16 cores,
    /// i.e. governed by core count, not actually idling), AND it does
    /// nothing to shrink the total iteration count paid by a push that can
    /// never succeed (`owner=paused`'s defining property — once the fixed
    /// combined ring+overflow capacity, 256 + 2048 = 2304, is exhausted, no
    /// number of "another chance" rounds can succeed; the 2,097,152-iteration
    /// budget was only ever delaying the concession to the bounded leak, at
    /// real CPU cost, not buying additional chances). The fix that actually
    /// resolves the pathology needed BOTH a real OS-level block (`sleep`,
    /// not `yield_now`) AND a way to stop waiting quickly when nothing is
    /// draining — which R6-REGRESSION first approximated with a fixed 8-round
    /// cap, and R6-REGRESSION-2 replaced with the drain-progress stop
    /// condition (a fixed cap small enough to keep the paused case fast was
    /// measured too impatient for a live-but-CPU-starved owner under host
    /// load — see [`RETRY_STALLED_ROUNDS_GIVE_UP`]'s doc comment for the full
    /// comparison).
    ///
    /// A wall-clock deadline (real-time-bounded, not iteration/round-count-
    /// bounded) was also considered — it would give a tighter worst-case
    /// latency guarantee independent of host speed — but this crate has no
    /// existing cheap monotonic-tick primitive usable from inside this
    /// `unsafe`-free hot path without either a syscall-heavy timer read on
    /// every poll (far more expensive than the uncounted CAS attempts it
    /// would gate) or adding a new one, which the task's own guidance was to
    /// avoid absent a demonstrated need; the round-cap-plus-sleep reshaping
    /// closes the measured pathology (CPU burn with zero scheduling
    /// progress, unbounded-in-practice per-push wall-clock cost) directly, at
    /// the mechanism actually shown to be the cause, without a new
    /// primitive.
    ///
    /// Does NOT touch either ring's own push/drain/cursor PROTOCOL — this is
    /// a caller-side wrapper composing `RemoteFreeRing::push` /
    /// `try_push_uncounted` and `HeapOverflow::push` / `push_uncounted`. The
    /// two `_uncounted` siblings are new (added by this task, byte-identical
    /// to their counted namesakes except for the diagnostic-counter bump on
    /// the full-ring branch — see each one's own doc comment); the counted
    /// `push` methods themselves are unmodified.
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn push_with_overflow_retry(
        ring: &crate::alloc_core::remote_free_ring::RemoteFreeRing,
        base: *mut u8,
        packed: u32,
    ) {
        if ring.push(packed).is_ok() {
            // R7-A4 (P3): set the dirty bit for this segment after a
            // successful ring publish — the fast-path producer site.
            #[cfg(feature = "alloc-segment-directory")]
            set_dirty_bit_for_segment(base, packed);
            return; // Fast path: the common case never proceeds further.
        }
        // R6-OPT-P0-4: the segment ring is full. Try the heap-level
        // second-chance overflow ring IMMEDIATELY — before any spinning. This
        // is the policy inversion: the pre-R6-OPT-P0-4 code spent the WHOLE
        // spin budget against the segment ring first; `push_to_heap_overflow`
        // is a single cheap CAS-reserve attempt against an already-provisioned
        // ring with 8x the capacity, so trying it first resolves the
        // overwhelmingly common case (ring momentarily full, overflow has
        // room) in exactly 2 checks total.
        if Self::push_to_heap_overflow(base, packed) {
            return;
        }
        // Both the segment ring AND the immediate overflow attempt failed —
        // the rare double-saturation case. Fall into the bounded spin-retry,
        // gated by `owner_slot_is_live` exactly as before R6-OPT-P0-4 (see
        // that method's doc comment for the full "why gate" rationale,
        // repeated briefly here): the spin window exists to buy time for the
        // OWNER to drain the ring; it is pure waste when no owner CAN drain.
        // Under the Phase 12.5 shard model a segment's rings are drained only
        // by its slot's CURRENT claimant (lazily, on that thread's alloc
        // path); when the owning slot is FREE (its thread exited, nobody has
        // re-claimed it), no drain can happen until a future claim, so
        // spinning cannot succeed. Without this gate, EVERY free into a full
        // ring of an owner-less segment paid the whole spin-retry budget — a
        // send-then-exit producer pattern
        // (`tests/race_norecycle.rs`: producers exit while ~10⁵ of their
        // blocks are still in flight to a long-lived freeing consumer)
        // multiplied that into MINUTES of aggregate dealloc() stall, tripping
        // the test's 30 s watchdog (`process::abort` → 0xC0000409). A LIVE
        // owner keeps the designed behaviour (`tests/remote_fanin.rs` remains
        // the judge for that shape).
        if Self::owner_slot_is_live(base) {
            // R6-OPT-P0-4: resolve the target `HeapOverflow` ONCE before the
            // loop (not on every poll — see `resolve_heap_overflow`'s doc
            // comment for the measured cost of re-resolving thousands of
            // times: it thins this loop's effective poll rate enough to
            // matter under host CPU contention). `None` only for a
            // defensively-unstamped/garbled owner id (should be unreachable
            // for a live segment); the loop still polls the ring alone in
            // that case, matching `push_to_heap_overflow`'s own "returns
            // false" defensive behaviour.
            let overflow = Self::resolve_heap_overflow(base);
            // R6-REGRESSION-2: probe rounds of `RETRY_ROUND_SPINS` tight-spin
            // polls each, with a real `std::thread::sleep(RETRY_ROUND_SLEEP)`
            // OS-level block between rounds (from round 2 onward — the sleep
            // is load-bearing: it is what stopped the paused-owner aggregate
            // CPU burn), stopped by DRAIN-PROGRESS detection rather than a
            // fixed round count: snapshot both tiers' drain cursors before
            // round 1, re-read them after every fully-failed round, and give
            // up only after `RETRY_STALLED_ROUNDS_GIVE_UP` CONSECUTIVE rounds
            // in which NEITHER cursor advanced (the owner drained nothing
            // across the whole observed window — genuinely stalled/paused).
            // Any observed advance resets the stall counter: the owner is
            // draining, however slowly (e.g. CPU-starved under host load),
            // so waiting remains meaningful — see
            // `RETRY_STALLED_ROUNDS_GIVE_UP`'s doc comment for the measured
            // failure both fixed budgets (large AND small) exhibited.
            // `RETRY_ROUND_SAFETY_CAP` hard-bounds the total wait regardless
            // of progress. Under `#[cfg(miri)]` both constants are 1, so
            // this runs exactly one pure-spin round with no sleep reached at
            // all — miri's interpreter gains nothing from a real sleep and a
            // scaled-down multi-round miri budget was already independently
            // measured impractically slow.
            let mut prev_ring_head = ring.head_relaxed();
            let mut prev_overflow_head = overflow.map(|o| o.head_relaxed());
            // R6-REGRESSION-2 fast-concede (R6-REVIEW-F2: N-way): if THIS
            // thread already paid the full stall patience for THIS segment
            // (any of its cached concession snapshots matches) and neither
            // drain cursor has moved since that concession, this push is
            // inside the same continuous zero-progress stall — concede after
            // a single probe round instead of re-paying the full patience.
            // See `LAST_STALL_CONCESSIONS`'s doc comment for why this cannot
            // affect the zero-concession (#136 judge) case at all.
            let snapshot = Some((base as usize, prev_ring_head, prev_overflow_head));
            let mut give_up_after =
                if LAST_STALL_CONCESSIONS.with(|c| c.get().0.contains(&snapshot)) {
                    1
                } else {
                    RETRY_STALLED_ROUNDS_GIVE_UP
                };
            let mut stalled_rounds: u32 = 0;
            for round in 0..RETRY_ROUND_SAFETY_CAP {
                if round > 0 {
                    // Only BETWEEN rounds, never before the first: the first
                    // round is a pure tight busy-spin, byte-for-byte the
                    // original task #99-calibrated shape, so the common
                    // (moderately contended, actively-draining-owner) case
                    // that resolves within round 1 never pays a sleep at
                    // all — this is the #136 judge's exact workload.
                    #[cfg(not(miri))]
                    std::thread::sleep(RETRY_ROUND_SLEEP);
                }
                for _ in 0..RETRY_ROUND_SPINS {
                    core::hint::spin_loop();
                    // R6-OPT-P0-4: uncounted — the ring's own overflow
                    // diagnostics already ticked once (step 1's counted
                    // `push` attempt above); re-ticking them on every one of
                    // up to `RETRY_ROUND_SPINS` × `RETRY_ROUND_SAFETY_CAP`
                    // failed polls here would tax the diagnostic counters
                    // with a locked RMW per poll for no informational gain
                    // (see `try_push_uncounted`'s doc comment for the full
                    // argument).
                    if ring.try_push_uncounted(packed).is_ok() {
                        // R7-A4 (P3): set the dirty bit — the retry-path
                        // producer site (try_push_uncounted in the bounded
                        // spin-retry loop, the R6-REGRESSION-2 path).
                        #[cfg(feature = "alloc-segment-directory")]
                        set_dirty_bit_for_segment(base, packed);
                        DBG_RING_PUSH_RETRIED.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    // Also retry the heap-level overflow ring on every poll,
                    // not just once before/after this loop. Under sustained
                    // high-fan-in pressure (many producers racing the SAME
                    // segment ring), the immediate single overflow attempt
                    // above can itself land on a momentarily-full overflow
                    // ring — the owner's opportunistic `drain_heap_overflow`
                    // only runs between its own `alloc()` calls, so both
                    // tiers need the SAME spin window to give the owner
                    // repeated chances to drain EITHER one (race model (a) in
                    // the task spec: "owner-drain racing producer-reservation
                    // on either tier"). Without this, a transient
                    // overflow-ring-full moment graduates straight to burning
                    // the whole spin budget against the segment ring alone,
                    // which measurably regressed the high-contention judge
                    // (`remote_fanin_high_contention_budget_is_sufficient`)
                    // during development of this fix. A coarser cadence
                    // (checking overflow only every Nth poll) was also tried
                    // and measured WORSE — throttling the overflow retries at
                    // contention this high loses more than the ring-poll-rate
                    // dilution it was meant to avoid, so every-poll is the
                    // retained shape (made affordable by resolving `overflow`
                    // once above instead of per-poll).
                    // Zero-trust review finding: this MUST be
                    // `push_uncounted`, not `push` — `HeapOverflow::push`'s
                    // "ring full" branch bumps its OWN `overflow_count`
                    // diagnostic (a locked RMW on a cache line shared by
                    // every producer targeting this heap slot's overflow
                    // ring), so calling the counted `push` here reintroduces
                    // exactly the per-poll atomic-storm class this whole task
                    // exists to close — just relocated from the segment
                    // ring's counters to the overflow ring's counter, and now
                    // scaled by up to `RETRY_ROUND_SPINS` ×
                    // `RETRY_ROUND_SAFETY_CAP` polls instead of the old
                    // single-round `RING_PUSH_RETRY_SPINS` (8,192). The ONE
                    // counted
                    // overflow attempt already made (the immediate step-2
                    // attempt above this loop, or the single not-live-path
                    // attempt in the `else` branch below) remains the signal
                    // "this heap's overflow ring saturated at all"; every
                    // in-loop poll here is uncounted, mirroring the ring's
                    // own `try_push_uncounted` discipline exactly.
                    if let Some(overflow) = overflow {
                        if overflow.push_uncounted(base, packed) {
                            DBG_RING_PUSH_RETRIED.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
                // R6-REGRESSION-2: the whole round failed — did the owner
                // drain ANYTHING (either tier) since the last check? Both
                // heads are owner-advanced monotonic cursors, so inequality
                // is an exact "a drain happened in this window" signal (a
                // stale Relaxed read can only under-report progress by one
                // round, never fabricate it — see each accessor's doc
                // comment).
                let ring_head = ring.head_relaxed();
                let overflow_head = overflow.map(|o| o.head_relaxed());
                if ring_head != prev_ring_head || overflow_head != prev_overflow_head {
                    prev_ring_head = ring_head;
                    prev_overflow_head = overflow_head;
                    stalled_rounds = 0;
                    // The owner drained something: any fast-concede memo is
                    // out of date — restore the full patience for the rest
                    // of THIS push too (the memo comparison below records
                    // fresh cursors if this push still ends in concession).
                    give_up_after = RETRY_STALLED_ROUNDS_GIVE_UP;
                } else {
                    stalled_rounds += 1;
                    if stalled_rounds >= give_up_after {
                        break; // Zero drain progress for K consecutive rounds.
                    }
                }
            }
            // Conceding with a LIVE owner: memoize the exact cursor snapshot
            // this thread conceded against, so subsequent pushes into the
            // SAME still-unchanged stall give up cheaply instead of each
            // re-paying the full patience — see `LAST_STALL_CONCESSIONS`'s
            // doc comment. (`prev_*` are current here: the loop only exits
            // through consecutive stalled rounds — or the safety cap, where
            // an at-most-one-round-stale snapshot merely under-matches and
            // costs the next push nothing but full patience.)
            //
            // R6-REVIEW-F2 write policy: update-in-place if some slot already
            // holds THIS segment's base (a segment never occupies two slots,
            // and refreshing its own snapshot must not evict a neighboring
            // stalled segment's), else fill the round-robin cursor's slot and
            // advance the cursor — see `LAST_STALL_CONCESSIONS`'s doc comment
            // for why round-robin (not LRU) suffices at this arity.
            LAST_STALL_CONCESSIONS.with(|c| {
                let (mut slots, mut cursor) = c.get();
                let snap = Some((base as usize, prev_ring_head, prev_overflow_head));
                if let Some(slot) = slots
                    .iter_mut()
                    .find(|s| matches!(s, Some((b, _, _)) if *b == base as usize))
                {
                    *slot = snap;
                } else {
                    slots[cursor] = snap;
                    cursor = (cursor + 1) % STALL_CONCESSION_WAYS;
                }
                c.set((slots, cursor));
            });
        } else if Self::push_to_heap_overflow(base, packed) {
            // Owner not live: no point spinning on the segment ring (nothing
            // will drain it), but the heap-level overflow ring is drained by
            // whichever thread next CLAIMS this slot, not by "this specific
            // owner" — so one attempt here still has a chance (mirrors the
            // pre-loop immediate attempt; kept as a distinct branch so the
            // not-live path does not fall through to ANOTHER redundant
            // overflow attempt below when it already just tried and failed).
            return;
        }
        // Every tier exhausted (the owner was live but made zero drain
        // progress for `RETRY_STALLED_ROUNDS_GIVE_UP` consecutive probe
        // rounds — or kept trickling progress this producer never converted
        // into a push for `RETRY_ROUND_SAFETY_CAP` rounds — or the owner was
        // not live and the single not-live-path attempt above also failed):
        // the genuinely-unrecovered case. The segment ring's
        // own `DBG_RING_OVERFLOW` / per-segment `overflow_count` ticked ONCE
        // (step 1's single counted attempt, not on every retry poll); this
        // counter marks ONLY this fully-unrecovered case.
        DBG_RING_PUSH_RETRY_EXHAUSTED.fetch_add(1, Ordering::Relaxed);
    }

    /// RAD-4b (task #72): resolve `base`'s owning [`HeapSlot`](super::heap_slot::HeapSlot)
    /// from its `owner_state` header stamp and push `(base, packed)` onto
    /// that slot's [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring.
    /// Returns `false` if the owner id is out of range (defensive — should
    /// be unreachable for a live, correctly-stamped segment) or the
    /// second-chance ring is itself saturated.
    ///
    /// `owner_state` is read Relaxed: this is the SAME diagnostic-strength
    /// read `dbg_owner_id_for` already performs cross-thread (the id is
    /// written once per segment-lifetime by the owner's `stamp_segment_owner`
    /// and never concurrently mutated by a second writer — the single-writer
    /// invariant on `owner_state` that every other cross-thread reader of
    /// this field already relies on, e.g. `dealloc_foreign_slow`'s own
    /// `owner_thread_free_at` read a few lines above this call site's
    /// caller). A transient stale read (segment recycled and re-stamped
    /// between this load and the array index below) resolves to either the
    /// SAME heap (harmless) or a DIFFERENT live heap's slot (the pushed
    /// entry sits in the wrong heap's overflow ring, drained on ITS next
    /// opportunistic pass — not a correctness hazard: `HeapOverflow::drain`'s
    /// `reclaim_offset(_checked)` call independently re-validates `base`'s
    /// `magic`/`kind`/bounds before touching anything, exactly as the
    /// existing per-segment ring drain already does for the identical class
    /// of stale-entry hazard).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn push_to_heap_overflow(base: *mut u8, packed: u32) -> bool {
        match Self::resolve_heap_overflow(base) {
            Some(overflow) => overflow.push(base, packed),
            None => false, // Defensive: unstamped/garbled owner id.
        }
    }

    /// R6-OPT-P0-4: factored out of [`push_to_heap_overflow`](
    /// Self::push_to_heap_overflow) so the bounded spin-retry loop in
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry) can
    /// resolve `base`'s owning [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// ONCE before the loop and reuse the `&'static` reference across up to
    /// [`RETRY_ROUND_SPINS`] × [`RETRY_ROUND_SAFETY_CAP`] poll iterations,
    /// instead of
    /// re-reading the `owner_state` header atomic and re-indexing the
    /// registry's slot array on EVERY poll. The re-resolution cost (an extra
    /// atomic load plus an array index, repeated thousands of times) was
    /// measured to matter under contention: it slows this loop's effective
    /// poll rate enough to visibly increase `DBG_RING_PUSH_RETRY_EXHAUSTED`
    /// flakes on `tests/remote_fanin.rs::remote_fanin_high_contention_
    /// budget_is_sufficient` specifically when the host machine is ALSO under
    /// concurrent CPU load (multiple `cargo`/build processes contending for
    /// cores) — a same-machine, same-code A/B (10 runs each) measured 1/10
    /// baseline-shaped flakes vs. 8/10 with per-iteration re-resolution,
    /// dropping back to a baseline-comparable rate once resolved once here.
    ///
    /// Same staleness argument as [`push_to_heap_overflow`]'s own doc comment
    /// applies UNCHANGED, just amortised across the loop instead of repeated
    /// per iteration: a transient stale read (segment recycled and
    /// re-stamped between this resolution and a later poll inside the loop)
    /// still resolves to either the SAME heap (harmless) or a DIFFERENT live
    /// heap's slot (the pushed entry sits in the wrong heap's overflow ring,
    /// drained on ITS next opportunistic pass — not a correctness hazard, see
    /// that doc comment for the full argument). Returns `None` if the owner
    /// id is out of range (defensive — should be unreachable for a live,
    /// correctly-stamped segment).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn resolve_heap_overflow(base: *mut u8) -> Option<&'static super::heap_overflow::HeapOverflow> {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed));
        let reg = super::bootstrap::ensure();
        let idx = owner_id as usize;
        if idx >= super::bootstrap::MAX_HEAPS {
            return None; // Defensive: unstamped/garbled owner id.
        }
        // R6-OPT-P0-2: `idx < MAX_HEAPS` just checked; `slot()` resolves it
        // through the chunked slot array (materialising the owning chunk if
        // needed — sound here because this index was read off a LIVE
        // segment's owner stamp, i.e. some earlier `claim()` already
        // materialised this chunk; a fresh materialisation would still be
        // correct, just redundant with that earlier one).
        Some(&reg.slot(idx).overflow)
    }

    /// Advisory owner-liveness probe gating
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry)'s spin
    /// window: `true` iff `base`'s owning registry slot is currently
    /// `STATE_LIVE` — i.e. some thread exists that will (lazily, on its alloc
    /// path) drain this segment's ring, so waiting for that drain is
    /// meaningful. Resolution is the same `owner_state` → `unpack_owner_id` →
    /// `slots[idx]` walk [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// performs (see its doc comment for why the Relaxed `owner_state` read is
    /// sound cross-thread).
    ///
    /// **Advisory, not authoritative — both stale outcomes are benign.** The
    /// slot's `state` is read Relaxed with no generation check, so this can
    /// race claim/recycle in either direction:
    /// - stale `LIVE` (owner exited just after the load): ONE free wastes one
    ///   spin budget; the NEXT free re-probes and sees `FREE`. Bounded,
    ///   one-off — not the per-free multiplication this gate exists to stop.
    /// - stale `FREE` (slot re-claimed just after the load): the push skips
    ///   ahead to the `HeapOverflow` ring, whose entries the new claimant
    ///   drains on its own schedule — the same destination those entries had
    ///   anyway. No block is lost that the spin would have saved.
    ///
    /// An out-of-range id (`OWNER_ID_NONE` — an unstamped early segment, or
    /// the process-global fallback heap, whose `id = u32::MAX` masks to
    /// `OWNER_ID_NONE` under `pack_owner`'s 31-bit id field) has no slot to
    /// consult; report "live" to preserve RAD-4's original unconditional spin
    /// there (the fallback heap is process-lived and drains on its own
    /// allocs, so waiting for it is meaningful).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn owner_slot_is_live(base: *mut u8) -> bool {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let idx = unpack_owner_id(owner_atomic.load(Ordering::Relaxed)) as usize;
        if idx >= super::bootstrap::MAX_HEAPS {
            return true;
        }
        // R6-OPT-P0-2: `slot()` resolves through the chunked slot array —
        // see `resolve_heap_overflow`'s identical rationale above for why a
        // (redundant, in practice) chunk materialisation here is sound.
        super::bootstrap::ensure()
            .slot(idx)
            .state
            .load(Ordering::Relaxed)
            == super::heap_slot::STATE_LIVE
    }
}
