//! Overflow-retry tier of the former flat `heap_core_xthread.rs` (reorg
//! step 4): the probe-round/stall-concession retry constants, the G1
//! resolve/apply dirty-bit pipeline, and the `impl HeapCore` block for
//! `push_with_overflow_retry` plus its `HeapOverflow` resolution and
//! owner-liveness helpers. The TLS fast-concede memo cache lives in the
//! sibling `stall.rs` (reorg step 4c, moved for the file-size cap). Pure
//! code-movement sibling files; no behavior changed.

// Both imports below serve only `dbg_resolve_dirty_notification`; gate them
// with its exact cfg so feature sets without `internals`/`bench-internals`
// (e.g. `production`) do not warn them unused.
#[cfg(all(
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "internals",
    feature = "bench-internals"
))]
use core::alloc::Layout;
use core::sync::atomic::Ordering;

#[cfg(all(
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "internals",
    feature = "bench-internals"
))]
use crate::alloc_core::os;

use crate::registry::heap_core::HeapCore;
// Only used by the `#[cfg(feature = "alloc-xthread")]` push_with_overflow_retry
// method below — the items themselves are gated the same way in
// `heap_core/core.rs` (re-exported by `heap_core/mod.rs`), so this import must
// match or `alloc-global`-without-`alloc-xthread` fails with E0432 (no such
// item exists to import under that feature set).
#[cfg(feature = "alloc-xthread")]
use crate::registry::heap_core::{DBG_RING_PUSH_RETRIED, RING_PUSH_RETRY_SPINS};
// Reorg step 4c: the stall-concession memo cluster moved VERBATIM to the
// sibling `stall.rs` for the repo's 1000-line file-size cap. Cfg-gated to
// match those items (they do not exist without `alloc-xthread`).
#[cfg(feature = "alloc-xthread")]
use super::stall::{LAST_STALL_CONCESSIONS, STALL_CONCESSION_WAYS};

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
/// `push_with_overflow_retry`'s spin-retry tier selects the intrusive spill.
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

/// G1 (review `docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md`,
/// finding G1, P1): the dirty-bit pipeline is split into a RESOLVE phase and
/// an APPLY phase so that NO segment memory is ever touched after the free
/// record is published on the ring.
///
/// `ResolvedDirtyTarget` is a G1 resolution snapshot: it contains ONLY a
/// process-lifetime `&'static HeapSlot` plus bitmap arithmetic (`word`, `bit`)
/// and the packed ring-entry word — deliberately NO segment pointer, so it
/// stays valid across the publish even if the owner drains the record and
/// RELEASES the segment in the publish-to-apply window.
#[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
#[derive(Clone, Copy)]
struct ResolvedDirtyTarget {
    slot: &'static crate::registry::heap_slot::HeapSlot,
    word: usize,
    bit: u64,
    #[cfg_attr(not(feature = "class-aware-dirty"), allow(dead_code))]
    packed: u32,
}

/// G1 RESOLVE phase: resolve the owning HeapSlot and the dirty-bitmap
/// `(word, bit)` for segment `base`. MUST be called BEFORE the `ring.push` /
/// `try_push_uncounted` that publishes this block's free record.
///
/// Call-ordering contract: before the publish, this block is still counted in
/// the owner's `live_count` (the owner cannot have observed THIS producer's
/// record yet), so the segment cannot have been released — reading
/// `segment_id_at(base)` and `owner_state_atomic()` here is safe. After the
/// publish it is NOT: the owner may drain the record, drop `live_count` to
/// zero, and unmap the segment before this thread runs again, which is the
/// use-after-free finding G1 fixes.
///
/// Resolves the owning HeapSlot via the segment's `owner_state` header stamp
/// (the SAME `unpack_owner_id` → `slot(idx)` path `resolve_heap_overflow`
/// uses). Reads the immutable `segment_id` from the segment header to compute
/// `(word, bit)` in the 16-word dirty bitmap.
///
/// **Defensive:** if the owner id is out of range (should be unreachable for
/// a live, correctly-stamped segment — same argument as
/// `resolve_heap_overflow`'s `None` branch), returns `None`; the dirty bit is
/// simply not set and the linear-scan fallback eventually finds the ring entry
/// anyway (P4 contract — see `remote_free_ring.rs` module doc).
///
/// R34-15/task #534: free-path slot resolution. `slot_or_none` returns `None`
/// on chunk-materialisation OOM instead of aborting, folding into the same
/// defensive bail as the garbled-id check above. F-3 context: `owner_id` is
/// read from FOREIGN segment memory with only the `idx < MAX_HEAPS` range
/// check above; a garbled-but-in-range id can trigger a FRESH OS reservation
/// here. For a legitimate cross-thread free this is harmless — and under G1
/// this invariant now holds UNCONDITIONALLY, because the resolve runs BEFORE this
/// producer's publish: the block holds `live_count >= 1` until the owner's
/// drain of a record that does not exist yet, so the segment cannot be
/// released under the freer at this point (see the review's G1 section). The
/// residual risk is the same caller-contract-violation surface (double free /
/// stale pointer) every allocator has.
#[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
#[inline]
fn resolve_dirty_bit_target(
    base: *mut u8,
    #[cfg_attr(not(feature = "class-aware-dirty"), allow(unused_variables))] packed: u32,
) -> Option<ResolvedDirtyTarget> {
    use crate::alloc_core::segment_header::{unpack_owner_id, SegmentHeader, SegmentMeta};

    let segment_id = SegmentHeader::segment_id_at(base) as usize;
    let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
    let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed)) as usize;
    let reg = crate::registry::bootstrap::ensure();
    if owner_id >= crate::registry::bootstrap::MAX_HEAPS {
        return None; // Defensive: unstamped/garbled owner id.
    }
    // R34-15/task #534: free-path slot resolution. `slot_or_none` returns
    // `None` on chunk-materialisation OOM instead of aborting, folding into
    // the same defensive bail as the garbled-id check above. F-3 context:
    // `owner_id` is read from FOREIGN segment memory with only the
    // `idx < MAX_HEAPS` range check above; a garbled-but-in-range id can
    // trigger a FRESH OS reservation here. For a legitimate cross-thread
    // free this is harmless — and under G1 this invariant is UNCONDITIONAL here,
    // because this resolve runs BEFORE this producer's publish: the block
    // holds `live_count >= 1` until the owner drains a record that does not
    // exist yet, so the segment cannot be released under the freer at this
    // point (see the review's G1 section). The residual risk is the same
    // caller-contract-violation surface (double free / stale pointer) every
    // allocator has.
    let Some(slot) = reg.slot_or_none(owner_id) else {
        return None; // Chunk-materialisation OOM — defensive return (R34-15).
    };
    let word = segment_id / 64;
    let bit = 1u64 << (segment_id % 64);
    // The WORDS_PER_CLASS compile-time bound check: segment_id < MAX_SEGMENTS
    // is an invariant of the SegmentTable (register rejects overflow), so
    // word < DIRTY_BITMAP_WORDS by construction. debug_assert for defence;
    // the runtime guard below (G1) moves the old post-publish `if word < ...`
    // gating decision into resolve so behaviour is identical — no bit is set
    // when out of range.
    debug_assert!(
        word < crate::registry::heap_slot::DIRTY_BITMAP_WORDS,
        "segment_id {segment_id} out of dirty bitmap range"
    );
    if word >= crate::registry::heap_slot::DIRTY_BITMAP_WORDS {
        return None;
    }
    Some(ResolvedDirtyTarget {
        slot,
        word,
        bit,
        packed,
    })
}

/// G1 APPLY phase: perform the dirty-bit writes from a pre-publish
/// [`ResolvedDirtyTarget`] snapshot. MUST be called ONLY AFTER a successful
/// ring publish (the fast-path `push` or in-loop `try_push_uncounted`).
///
/// G1 contract: this function takes NO segment pointer and NEVER touches
/// segment memory — by the time it runs, the owner may legally have drained
/// this producer's record AND RELEASED the segment (unmap/decommit); every
/// input here is either a process-lifetime `&'static HeapSlot` or immutable
/// bitmap arithmetic snapshotted before the publish.
///
/// **Ordering:** `fetch_or(bit, Release)` — the `Release` pairs with the
/// owner's `swap(0, Acquire)` in the dirty-drain loop, establishing
/// happens-before from the producer's ring publish (which completed before
/// this call) to the owner's dirty-drain iteration. The G1 split does not
/// change that chain: the publish still precedes this call.
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
/// heap-wide, one-way [`HeapSlotRemote::sidecar_oom_latch`](crate::registry::heap_slot::HeapSlotRemote::sidecar_oom_latch)
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
/// See [`HeapSlotRemote::sidecar_oom_latch`](crate::registry::heap_slot::HeapSlotRemote::sidecar_oom_latch)'s
/// doc comment for the full design and [`AllocCore::drain_dirty_segments`](crate::alloc_core::AllocCore::drain_dirty_segments)'s
/// doc comment for the consumer-side read.
#[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
#[inline]
fn apply_resolved_dirty_bit(target: ResolvedDirtyTarget) {
    #[cfg_attr(not(feature = "class-aware-dirty"), allow(unused_variables))]
    let ResolvedDirtyTarget {
        slot,
        word,
        bit,
        packed,
    } = target;
    if word < crate::registry::heap_slot::DIRTY_BITMAP_WORDS {
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
                    //
                    // R14-2 (task #287): this comment's "pairs with the
                    // Acquire read" claim used to be false — the production
                    // consumer read this latch `Relaxed` (three independent
                    // Round 13 reviews found the divergence against this
                    // comment, `HeapSlotRemote::sidecar_oom_latch`'s doc, and
                    // the loom model, which had always used `Acquire`).
                    // `drain_dirty_segments` was promoted to `Acquire` so the
                    // comment is now literally true, not aspirational.
                    slot.remote.sidecar_oom_latch.store(true, Ordering::Release);
                }
            }
        }
    }
}

impl HeapCore {
    /// Prepare a delayed notification for the post-release regression test.
    ///
    /// Test support only (`internals` + `bench-internals`), not stable API.
    /// The closure owns only a process-lifetime snapshot and may run on another
    /// thread after release. It reports whether a previously clear coarse bit
    /// was set; the test must exclude competing notifications/drains of that bit.
    ///
    /// # Safety
    /// `ptr` must be a live Small block of a registry heap, allocated with `layout`.
    /// It must remain allocated until this function returns.
    #[cfg(all(
        feature = "alloc-xthread",
        feature = "alloc-segment-directory",
        feature = "internals",
        feature = "bench-internals"
    ))]
    #[doc(hidden)]
    #[allow(unsafe_code)] // The caller establishes header lifetime before resolve.
    pub unsafe fn dbg_resolve_dirty_notification(
        ptr: *mut u8,
        layout: Layout,
    ) -> Option<impl FnOnce() -> bool + Send + 'static> {
        let class =
            crate::alloc_core::size_classes::SizeClasses::class_for(layout.size(), layout.align())?
                as u32;
        #[cfg(not(feature = "hardened"))]
        let packed = crate::alloc_core::remote_free_ring::pack_entry(0, class);
        #[cfg(feature = "hardened")]
        let packed = crate::alloc_core::remote_free_ring::pack_entry_hardened(0, class, 0);
        let target = resolve_dirty_bit_target(os::segment_base_of_ptr(ptr), packed)?;
        Some(move || {
            let before = target.slot.remote.dirty_segments[target.word].load(Ordering::Acquire);
            apply_resolved_dirty_bit(target);
            let after = target.slot.remote.dirty_segments[target.word].load(Ordering::Acquire);
            before & target.bit == 0 && after & target.bit != 0
        })
    }

    /// RAD-4 (Phase 4, E3a); extended by RAD-4b (task #72); reordered by
    /// R6-OPT-P0-4: push `packed` (the block's segment-relative `(offset,
    /// class)` word, already packed by the caller) onto `ring`, falling back
    /// through a three-tier chain — segment ring → heap-level overflow ring →
    /// bounded spin-retry against both — then the lossless intrusive spill.
    ///
    /// **R6-OPT-P0-4 — overflow-first policy (current).** The PRE-R6-OPT-P0-4
    /// policy exhausted the WHOLE [`RING_PUSH_RETRY_SPINS`] (8,192) spin
    /// budget against the segment ring FIRST, and only tried the heap-level
    /// [`HeapOverflow`](crate::registry::heap_overflow::HeapOverflow) ring after that
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
    ///    [`RING_PUSH_RETRY_SPINS`] in `heap_core/core.rs` explaining why flat spin
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
    ///    publish an intrusive note in the freed block. Legal frees no longer
    ///    reach the old `DBG_RING_PUSH_RETRY_EXHAUSTED` drop counter.
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
    /// intrusive spill. An absolute [`RETRY_ROUND_SAFETY_CAP`] (4096
    /// native / 1 miri) on total rounds backstops the pathological
    /// "owner keeps draining but this producer never wins a slot" shape so
    /// a single push has a bounded probe-round budget; scheduler and OS-call
    /// latency are not bounded by that count. And so
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
    /// budget was only ever delaying the lossless spill, at
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
    // #1994: this is a ~270-line cold retry-loop with a `sleep` — `#[inline]`
    // cannot be honoured on a function this shape and contradicted its
    // callers' `#[cold]` discipline. Plain (no hint) lets the compiler's own
    // cold-path heuristics apply.
    #[cfg(feature = "alloc-xthread")]
    pub(super) fn push_with_overflow_retry(
        ring: &crate::alloc_core::remote_free_ring::RemoteFreeRing,
        block: *mut u8,
        base: *mut u8,
        packed: u32,
    ) {
        // G1 (docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md):
        // resolve BEFORE publish — after the push succeeds the owner may drain
        // the record and release the segment before this thread runs again; the
        // old post-publish dirty-bit helper read the segment header in exactly
        // that window (use-after-free).
        #[cfg(feature = "alloc-segment-directory")]
        let dirty_target = resolve_dirty_bit_target(base, packed);
        if ring.push(packed).is_ok() {
            // R7-A4 (P3): set the dirty bit for this segment after a
            // successful ring publish — the fast-path producer site.
            // G1: applied from the pre-publish resolution snapshot; no
            // segment-memory access happens after the publish.
            #[cfg(feature = "alloc-segment-directory")]
            if let Some(target) = dirty_target {
                apply_resolved_dirty_bit(target);
            }
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
            // G1: resolve ONCE before the loop — same rationale as the
            // fast path, same once-not-per-poll discipline as
            // `resolve_heap_overflow` above. This block's free record is
            // not published until a push inside the loop succeeds, so the
            // segment cannot be released across the retry window, and the
            // resolved target ('static slot + immutable segment_id
            // arithmetic) stays valid regardless.
            #[cfg(feature = "alloc-segment-directory")]
            let dirty_target = resolve_dirty_bit_target(base, packed);
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
                        // G1: applied from the pre-loop resolution
                        // snapshot; no segment-memory access after publish.
                        #[cfg(feature = "alloc-segment-directory")]
                        if let Some(target) = dirty_target {
                            apply_resolved_dirty_bit(target);
                        }
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
        // Both rings exhausted (the owner was live but made zero drain
        // progress for `RETRY_STALLED_ROUNDS_GIVE_UP` consecutive probe
        // rounds — or kept trickling progress this producer never converted
        // into a push for `RETRY_ROUND_SAFETY_CAP` rounds — or the owner was
        // not live and the single not-live-path attempt above also failed):
        // the third-tier case. The segment ring's `DBG_RING_OVERFLOW`
        // ticked once at the first full-ring attempt, not on each retry.
        // Both bounded rings are saturated. `block` still owns its storage:
        // no reclaim can release the segment before this note is published.
        // The slot-resident intrusive tier consumes no allocator/OS memory,
        // and persists across owner exit/recycle. A legal segment always has
        // a stamped, in-range owner slot; absence is an invariant failure,
        // not an OOM/full result that may silently discard this free.
        let Some(overflow) = Self::resolve_heap_overflow(base) else {
            std::process::abort();
        };
        overflow.spill_push(block, packed);
    }
}
