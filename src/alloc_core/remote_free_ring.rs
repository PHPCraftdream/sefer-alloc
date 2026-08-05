//! `RemoteFreeRing` — a per-segment, bounded, **non-intrusive** MPSC queue
//! of freed-block **offsets** (`u32`), carved from segment metadata.
//!
//! ## Why this exists — the cross-thread-free drain-reclaim UAF fix
//!
//! The Phase 12.5 inline `ThreadFreeStack` (an intrusive Treiber stack whose
//! "node" was the freed block's own first word) raced fatally across the slot
//! release→claim boundary (root-caused in `docs/RACE_DRAIN_RECLAIM.md` §8): a
//! cross-thread freer and the slot's new owner contended the SAME block word —
//! the freer wrote a `next` pointer into it while the owner had already popped
//! the block from the `BinTable` and handed it to the app (which wrote user
//! data). The drain then read user data as a free-list `next` pointer → UAF.
//!
//! **This queue removes the contended word entirely.** A cross-thread freer
//! never touches the block's bytes: it only pushes the block's
//! *segment-relative offset* (a plain `u32`) into this in-segment ring. The
//! owner drains the ring and reclaims each offset into the segment's `BinTable`
//! as the single writer. The block's first word is owned solely by whoever
//! currently holds it (free-list `next` while queued in the `BinTable`, or user
//! data while live) — there is no third "in-flight to a remote queue" role that
//! the intrusive TFS introduced. This restores the original `ShardedRegion` 7b
//! discipline (queues carry references/indices, never poison the object).
//!
//! ## What this module IS and is NOT
//!
//! - IS: pure safe data + arithmetic over the `node` (`super::node`) seam. Every
//!   atomic access goes through `Node::atomic_u32_at` (a confined-`unsafe`
//!   primitive identical in spirit to `atomic_u64_at`). There is NO `unsafe`
//!   here — the crate's structural promise ("`unsafe` lives ONLY in `os` +
//!   `node`") is upheld by the compiler.
//! - IS: an MPSC bounded queue. **Many producers** (cross-thread freers) push
//!   via `fetch_add`-free CAS-reserve; **one consumer** (the owning thread)
//!   drains. The single-consumer invariant is the slot's single-writer rule
//!   (the slot's owner is the sole `BinTable` writer, hence the sole drainer).
//! - IS NOT: a way to read or write the *payload* of a freed block. Only the
//!   offset (an integer) crosses the queue.
//!
//! ## Layout in a segment
//!
//! ```text
//!   ... bin_table_off + BinTable::FOOTPRINT (4-byte aligned)
//!   ┌──────────────────────────────────────────────────────────┐
//!   │ RemoteFreeRing                                           │
//!   │  offset 0..64  (own cache line — consumer-only writes):  │
//!   │  • head: AtomicU32  (4 B) — drain cursor (consumer)      │
//!   │  • [60 B reserved padding]                                │
//!   │  offset 64..128 (own cache line — producer-touched):     │
//!   │  • tail: AtomicU32  (4 B) — push reserve cursor (producers)
//!   │  • overflow: AtomicU32 (4 B) — count of discarded pushes  │
//!   │    (ring-full → bounded leak; sound, never corrupts)      │
//!   │  • cached_head: AtomicU32 (4 B) — F10 shadow-head hint,   │
//!   │    same line as tail/overflow (producer-only touched)     │
//!   │  • [52 B reserved padding]                                │
//!   │  offset 128.. (data, starts on its own cache line):       │
//!   │  • slots: [AtomicU32; RING_CAP]  (RING_CAP × 4 B)         │
//!   │    each slot holds a block offset or RING_SLOT_EMPTY      │
//!   └──────────────────────────────────────────────────────────┘
//! ```
//!
//! **PERF-PASS-4 (G8/ML4, task #52):** the cursor block widened from 16 to
//! 128 bytes so `head` (consumer-only), `tail`/`overflow` (producer-touched),
//! and the data slots each start on their OWN 64-byte cache line — the
//! pre-task packing put all three on one line (the ring's in-segment base is
//! 64-byte aligned), guaranteeing maximal ping-pong: a consumer's `head`
//! publish invalidated the producers' `tail` CAS line AND the first 12 data
//! slots. `FOOTPRINT = CURSOR_BLOCK (128) + RING_CAP * 4`. With `RING_CAP =
//! 256` that is 1152 bytes per segment (was 1040) — still under one page,
//! negligible vs. the 4 MiB segment.
//!
//! ## MPSC protocol (Vyukov-style bounded, CAS-reserved)
//!
//! Two monotonic cursors: `tail` (producers reserve push slots) and `head`
//! (the consumer advances past drained slots). `slots[i % CAP]` holds the
//! offset for the reservation `i`, or `RING_SLOT_EMPTY` if not-yet-written /
//! already-drained.
//!
//! **Push (multi-producer) — F10 shadow-head fast path.** The full check
//! needs to know whether the ring MIGHT be full; it does not need the
//! EXACT current `head` value unless it might be. `cached_head` is a
//! producer-line-resident replica of the last real `head` value any producer
//! observed. See "F10 — shadow/cached head" below for the full soundness
//! argument; summary of the steps:
//! 1. `t = tail.load(Relaxed)`, `ch = cached_head.load(Acquire)` — both same
//!    line, no cross-core traffic in the common case. The `Acquire` on
//!    `cached_head` (R34-6, finding F-1) restores the happens-before edge
//!    the pre-F10 `head.load(Acquire)` supplied — see the "F10 ordering
//!    supplement" below.
//! 2. If `t.wrapping_sub(ch) < CAP`, the shadow already proves the ring has
//!    room — skip straight to the CAS (step 4). This is the fast path.
//! 3. Otherwise (shadow suggests full, or never yet refreshed): fall through
//!    to the REAL check — `h = head.load(Acquire)`; refresh
//!    `cached_head.store(h, Release)` (R34-6: pairs with the fast path's
//!    `Acquire` load); if `t.wrapping_sub(h) >= CAP`, genuinely
//!    full → `Err(Overflow)` (the caller discards the block: bounded leak,
//!    sound). The `Acquire` here is exactly the one the module doc's original
//!    protocol required — sees the consumer's `Release` head advance, so a
//!    slot freed by the drain becomes observable before the overflow verdict
//!    is taken.
//! 4. CAS `tail: t → t+1` with `AcqRel` on success (the reservation is the
//!    linearization point — exactly one producer wins each `t`). `Relaxed` on
//!    failure (retry; no side-effect).
//! 5. Store `slots[t % CAP] = offset` with `Release` (publishes the offset to
//!    the consumer's `Acquire` slot read). Return `Ok(())`.
//!
//! ## F10 (task #502) — shadow/cached head: soundness argument
//!
//! **Claim: `head` is monotonic (only ever advances, never regresses) under
//! every REAL (non-test) call path.** Verified by enumerating every write
//! site to `head` — there are FOUR (pinned by a drift-detection test,
//! `tests/remote_free_ring_head_write_sites.rs`, so this list cannot silently
//! fall out of sync with the code):
//!
//! 1. [`drain`](RemoteFreeRing::drain)'s `head.store(h, Release)` — the
//!    ONLY production write. Since R34-17/task #536 (finding F-7) this store
//!    lives inside the `DrainHeadPublish` RAII guard's `Drop` (so a `reclaim`
//!    closure that unwinds mid-drain still publishes partial progress), but it
//!    is still the single logical write of the drain path: `h` is derived ONLY
//!    by `h = h.wrapping_add(1)` starting from the PREVIOUS stored `head`
//!    value (`self.head().load(Relaxed)` at the top of `drain`), so each
//!    drain call's stored `head` is `>=` the value it read (wrapping
//!    arithmetic is monotonic over one lap). This is the advance the
//!    monotonicity claim is about.
//! 2. [`init_in_place`](RemoteFreeRing::init_in_place)'s raw write of `0`
//!    to `HEAD_OFF` — zeroes BOTH `head` AND `cached_head` together at
//!    bootstrap (single-writer, exclusively-owned segment, before any
//!    `push`/`drain` can observe the ring). Benign: it cannot leave the
//!    two cursors inconsistent and is not reachable after init.
//! 3. [`dbg_set_cursors`](RemoteFreeRing::dbg_set_cursors) —
//!    `#[doc(hidden)]` test-only, `alloc-xthread`-gated. Documents a
//!    quiescent-ring precondition AND `tail.wrapping_sub(head) <=
//!    RING_CAP`; also resets `cached_head` to match. Reachable from
//!    neither `push` nor any production call path.
//! 4. [`dbg_advance_head_only`](RemoteFreeRing::dbg_advance_head_only) —
//!    `#[doc(hidden)]` test-only, `alloc-xthread`-gated. Stores an
//!    arbitrary `u32` into `head` and deliberately does NOT touch
//!    `cached_head`. Documents a quiescent-ring precondition AND a
//!    "must never regress `head`" precondition (storing a value BELOW
//!    the current `head` would leave `cached_head` above the regressed
//!    `head` — a STALE-HIGH shadow — which this argument declares
//!    impossible). Reachable from neither `push` nor any production
//!    call path; its only real caller advances by `wrapping_add(1)`.
//!
//! Only site (1) is reachable from a production call path; sites (2)–(4)
//! are bootstrap- or test-only with their own documented preconditions.
//! There is a SINGLE consumer per ring (the module's own MPSC contract),
//! so there is no cross-consumer race that could interleave two `drain`
//! calls' stores out of order.
//!
//! **Claim: `cached_head` can only be STALE-LOW relative to the true `head`,
//! never stale-high.** `cached_head` is written in exactly one place: the
//! refresh step above, `cached_head.store(h, Relaxed)` where `h` was JUST
//! read from the real `head` via `Acquire`. Because `head` only advances, any
//! value `cached_head` ever holds was a real, once-true value of `head` — and
//! by the time a LATER producer reads `cached_head`, the real `head` has only
//! moved forward (or stayed put) since that store. So at every read,
//! `cached_head <= head` (mod wrap — both are the same class of monotonic
//! `u32` wrapping counter as `tail`, so the ring's existing
//! `wrapping_sub`-based comparisons apply unchanged; see the wrap note below).
//!
//! **Consequence for each of the three failure modes named in the survey:**
//! - **Missed overflow (accepting a push the ring cannot hold):** cannot
//!   happen. The shadow's fast path (`t.wrapping_sub(ch) < CAP`) only ever
//!   makes the ring look MORE full than the real state (`ch <= head`, so
//!   `t.wrapping_sub(ch) >= t.wrapping_sub(head)`) — never less full. A push
//!   that the fast path accepts would ALSO be accepted by the real check
//!   (since the real occupancy is `<=` what the shadow computed). The
//!   converse — the fast path rejecting a push the real check would have
//!   accepted — is possible (a stale-low `ch` inflates apparent occupancy)
//!   but is exactly the case that falls through to step 3, which performs
//!   the REAL `Acquire` check before ever returning `Err`. So the fast path
//!   never itself decides "full" — it only ever decides "definitely NOT
//!   full, skip the real check", and that decision is proven safe by the
//!   inequality above. `Err(Overflow)` is returned ONLY from the code path
//!   that already re-derives `h` from a fresh `Acquire` load — byte-identical
//!   to the pre-F10 protocol on that branch.
//! - **Lost entry:** the push protocol's entry-publishing steps (CAS-reserve
//!   `tail`, `Release`-store the slot) are completely unmodified — F10 only
//!   changes HOW the full-check decides whether to attempt them, never what
//!   happens once a reservation is won. A push that reaches the CAS follows
//!   the exact same reserve/publish sequence as before; nothing about entry
//!   delivery changed.
//! - **Premature slot reuse before drain:** slot reuse is gated by the SAME
//!   invariant the pre-F10 code enforced — `tail.wrapping_sub(head) < CAP`
//!   before a NEW reservation of a slot index is allowed. F10 does not change
//!   what value gates the CAS attempt (the CAS itself, and its bound
//!   `t + 1`, are unmodified); it only changes which LOAD supplies the
//!   comparand on the common path, and that comparand is proven `<=` the
//!   real `head` above — so F10 can only be MORE conservative about
//!   permitting a reservation, never less.
//!
//! **Wrap correctness.** `cached_head` is refreshed only FROM a real `head`
//! value, so it inherits the exact same `u32` wrapping-counter semantics as
//! `head`/`tail` (see the compile-time `RING_CAP.is_power_of_two()` pin
//! above, which this shadow does not disturb — it adds no new modulus
//! arithmetic, only a `wrapping_sub` comparison identical in shape to the
//! ones `push`/`drain` already use). The fast-path comparison uses
//! `t.wrapping_sub(ch)`, exactly mirroring the real check's
//! `t.wrapping_sub(h)` — a naive `<`/`>` comparison would break at the
//! `u32::MAX → 0` wrap (the same hazard the module's existing wrap-note
//! documents for `head`/`tail`); this shadow reuses the SAME wrapping-safe
//! idiom, not a new one.
//!
//! **Wrap argument precondition — the staleness bound (ASSUMPTION, not a
//! theorem — see below).** The inequality `cached_head <= head` holds only
//! MODULO `2^32`, and only while the shadow's staleness lag stays strictly
//! below `2^32` REAL head-advances. Unlike the pre-F10 check — which
//! compared `t` against a `head` value read microseconds earlier (lag bounded
//! by cache-coherence latency) — the shadow's lag is bounded only by the
//! preemption window between the refresh's `Acquire` load of `head`
//! (`full_check`'s step 3) and its immediately-following `Relaxed` store of
//! that same value: two adjacent instructions. Were the true `head` to
//! advance by exactly `2^32 − k` during that window, the stored value would
//! be modularly `k` AHEAD of the true `head`, and `t.wrapping_sub(ch)` would
//! under-report occupancy by `k` — at `k = 1` with a genuinely full ring,
//! the fast path would admit a push it must not (premature slot reuse). This
//! requires a producer descheduled between those two adjacent instructions
//! while ~4.29 × 10⁹ drains complete on that one segment's ring — judged not
//! practically reachable, consistent with how this module treats its other
//! genuinely-reachable-but-astronomically-rare wrap hazard (the power-of-two
//! `RING_CAP` compile-time pin, which exists for exactly the "2^32
//! cross-thread frees on a single hot, long-lived segment" case this same
//! window would need). This is an **ASSUMPTION** about the scheduler /
//! preemption behaviour of the host, not a theorem of the abstract memory
//! model — stated explicitly as such per the second-independent-review
//! request (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`,
//! `RemoteFreeRing::cached_head` section). No code change is warranted for
//! a hazard this remote.
//!
//! **In one sentence, for anyone citing this module's proof status:** the
//! F10 shadow-head design's soundness rests on the Rust memory model
//! (§"F10 ordering supplement" above, closed by R34-6's Acquire/Release
//! promotion) **PLUS** this one bounded-staleness scheduler/time
//! assumption — it is NOT a proof that holds under the abstract memory
//! model alone, and any claim that this design is "formally verified"
//! without naming this residual assumption is incomplete. (Round-32/33
//! independent review, finding F7, and the Sol release readonly review,
//! finding F7, both raised exactly this precision point — see
//! `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` §11 for the closure
//! trail across both reviews.)
//!
//! **Worst case cost of a stale shadow:** at most ONE extra real
//! `head.load(Acquire)` per push that the shadow's fast path declines to
//! shortcut — never a correctness cost, only a fallback to the exact
//! pre-F10 behaviour on that call.
//!
//! **F10 ordering supplement (R34-6, task #525, finding F-1).** The
//! value-domain argument above (`cached_head <= head`) proves the fast
//! path cannot over-estimate room. It does NOT by itself prove the
//! *ordering* invariant that the pre-F10 `head.load(Acquire)` used to
//! supply: that when a producer publishes an offset into a recycled slot,
//! the consumer's `slot.store(EMPTY)` for that slot's previous occupant
//! is guaranteed to precede the producer's `slot.store(offset)` in that
//! slot's modification order. Pre-F10, every push's `head.load(Acquire)`
//! created a synchronizes-with edge with the consumer's
//! `head.store(h', Release)` — hence a happens-before chain through to
//! the consumer's clear — that supplied this guarantee. The F10 fast
//! path removed that load, and under the abstract memory model a
//! second producer P that reads only `cached_head` (never `head`)
//! carries no such chain: its `slot.store(offset)` and the consumer's
//! `slot.store(EMPTY)` are unordered by happens-before. (This is NOT a
//! data race — both are atomic on the same `AtomicU32`; it is a
//! potential lost-update/liveness defect. The gap was identified by the
//! release-stabilization audit (finding F-1) but confirmed NOT
//! realisable on any hardware Rust targets — x86-TSO, ARMv8, RISC-V
//! RVWMO, and POWER cumulativity all make the clear globally visible
//! before P's store is issued — so it is a *proof* gap, not a *bug*.)
//!
//! **Resolution: promote `cached_head`'s two accesses from `Relaxed` to
//! `Acquire`/`Release`** (R34-6). The fast path's load is now
//! `cached_head.load(Acquire)`, and the slow path's refresh store is
//! `cached_head.store(h, Release)`. This restores the exact edge the
//! removed `head.load(Acquire)` supplied, on the SAME producer-owned
//! cache line (no new cross-core traffic): a producer X whose slow path
//! refreshes the shadow does `head.load(Acquire)` (sees the consumer's
//! `head.store(Release)` → synchronizes-with → X's history now includes
//! the consumer's `slot.store(EMPTY)`), then `cached_head.store(Release)`
//! — and a later producer P that reads `cached_head.load(Acquire)`
//! synchronizes-with X's `Release` store, inheriting the edge. The cost
//! is fence *strength*, not a fence *instruction*: on x86-TSO both
//! `Acquire` loads and `Release` stores compile to the SAME `mov` as
//! `Relaxed` (verified byte-for-byte identical via disassembly — all x86
//! loads are acquire, all non-`SeqCst` stores are release); on aarch64
//! they are one `ldapr`/`stlr` instead of `ldr`/`str` (measured cost:
//! noise-level, see `benches/r34_6_remote_ring_cached_head_ordering_gate.rs`).
//!
//! **Drain (single consumer):**
//! 1. `t = tail.load(Acquire)` (sees every producer's `Release` reservation).
//! 2. While `h != t` (wrap-correct — both cursors are monotonic wrapping
//!    counters, so the undrained count is `t.wrapping_sub(h)`, NOT `t - h`):
//!    load `slots[h % CAP]` with `Acquire`. If `RING_SLOT_EMPTY`
//!    → the reservation was won but the publish store hasn't happened yet
//!    (producer is between steps 2 and 3); **stop draining** (we cannot skip
//!    it — order is preserved by the cursors; a later drain picks it up).
//!    Otherwise reclaim the offset, store `slots[h % CAP] = RING_SLOT_EMPTY`
//!    (`Relaxed` — only this consumer writes a non-empty value... no: producers
//!    also write here on their reserved slot; but a producer only writes to
//!    `slots[p % CAP]` for a `p` it reserved, and reservations are unique, so
//!    by the time we drain slot `h`, no producer will write it again until
//!    `tail` wraps past `h + CAP` — which the full-check prevents. `Relaxed` is
//!    safe because the next producer to touch this slot will `Release`-store
//!    its offset, and our drain reads with `Acquire`.), `h = h.wrapping_add(1)`.
//! 3. `head.store(h, Release)` (publishes the drain progress to producers'
//!    full-check `Acquire` head load).
//!
//! **Ordering summary (each justified above):**
//! - producer reservation CAS: `AcqRel` (success) / `Relaxed` (failure).
//! - producer publish store: `Release`.
//! - consumer tail load: `Acquire`.
//! - consumer slot load: `Acquire`.
//! - consumer slot clear: `Relaxed`.
//! - consumer head store: `Release`.
//! - producer full-check head load (slow path): `Acquire`.
//! - producer full-check cached_head load (fast path): `Acquire` (R34-6,
//!   finding F-1 — restores the happens-before edge the pre-F10
//!   `head.load(Acquire)` supplied; byte-identical `mov` to `Relaxed`
//!   on x86-TSO, one `ldapr` on aarch64).
//! - producer full-check cached_head refresh store (slow path): `Release`
//!   (R34-6, finding F-1 — pairs with the fast path's `Acquire` load).
//!
//! ## P4 — visibility contract change (R7-A4, dirty routing)
//!
//! With the A4 dirty-routing mechanism (`alloc-segment-directory` +
//! `alloc-xthread`), a cross-thread freer sets a per-slot dirty bit AFTER
//! a successful ring publish. A producer stalled between `push`/
//! `try_push_uncounted` (the ring entry is visible in the ring) and
//! `fetch_or` on the dirty bitmap (the owning slot's dirty word) is
//! INVISIBLE to the owner's dirty-routing drain until the bit lands.
//! This is bounded deferral of the same class as the existing "later
//! drain picks it up" contract (above): the entry is in the ring and
//! will be found by:
//!   (a) the next dirty-routing drain pass, once the producer's
//!       `fetch_or` completes and a subsequent owner `swap(0, Acquire)`
//!       observes it;
//!   (b) the guarded linear-scan fallback, which still drains every
//!       ring unconditionally (the scan body is unchanged and always
//!       reachable as the directory-miss path);
//!   (c) any drain triggered by a DIFFERENT cross-thread free to the
//!       SAME segment that DID complete its `fetch_or` — that drain
//!       reads the ring up to the current `tail`, which includes the
//!       stalled producer's entry.
//! A producer that crashes between `push` and `fetch_or` (e.g. a
//! process-level abort after the CAS but before the fetch_or) leaves
//! the ring entry orphaned from the dirty bitmap, but the linear-scan
//! fallback (path (b)) eventually finds it. No ring entry is ever lost
//! — only its discoverability via the fast dirty path is deferred.
//!
//! ## Overflow semantics (the honest remainder)
//!
//! When the ring is full (`tail - head == CAP`), a push returns
//! `Err(PushOverflow)` and the caller **discards** the block (it stays mapped,
//! unused — a bounded leak). This is SOUND (no UAF, no corruption) but costs
//! RSS: at most `(CAP - drained_count)` blocks per segment can be in flight,
//! and a sustained burst faster than the owner drains leaks one block per
//! overflow. In practice the owner drains on every alloc, so the ring rarely
//! fills under normal churn; the leak bound is the in-flight cross-thread-free
//! footprint per segment between drains. This is strictly better than the
//! Phase 12.5 discard (which leaked the ENTIRE cross-thread-free chain per slot
//! recycle) and, crucially, it is a *correctness-preserving* fallback, not a
//! correctness violation — the race is gone.

// Only reached by the ring's atomic push/drain methods, which are themselves
// only reachable on builds that exercise cross-thread free
// (`alloc-xthread`); unused under `--features alloc-core` alone.
#[cfg_attr(not(feature = "alloc-xthread"), allow(unused_imports))]
use core::sync::atomic::Ordering;

use super::node::Node;
use super::size_classes::SMALL_CLASS_COUNT;

/// TEST/DIAGNOSTIC-ONLY (task D2): process-wide count of ring-push overflows
/// (a cross-thread free that found its target segment's ring full and
/// discarded the block — a sound but observable bounded leak; see "Overflow
/// semantics" above). Bumped in [`RemoteFreeRing::push`] alongside the
/// existing per-segment `overflow` cursor-block counter. The per-segment
/// counter ([`RemoteFreeRing::overflow_count`]) is exact for one segment but
/// requires the caller to already hold a `RemoteFreeRing` handle (i.e. know
/// which segment to ask); this process-wide counter gives O(1) visibility
/// into "did overflow happen anywhere, ever" without walking the segment
/// table — the minimum bar for production observability (feeds Phase E
/// stats). Relaxed: diagnostic only, no synchronisation implied.
#[doc(hidden)]
pub static DBG_RING_OVERFLOW: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// F10 (task #502) path-activation oracle: process-wide count of
/// [`RemoteFreeRing::full_check`] calls that took the FAST (shadow-hit)
/// path — `cached_head` alone proved the ring had room, no real
/// `head.load(Acquire)` was issued. `bench-internals`-gated: this is a
/// measurement-only counter with no production caller, so it defaults to the
/// narrowest gate per CLAUDE.md's benchmark-hook rule (never widens a plain
/// `production` build's surface). Relaxed: diagnostic only.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
pub static DBG_RING_PUSH_SHADOW_FAST: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// F10 (task #502) path-activation oracle: process-wide count of
/// [`RemoteFreeRing::full_check`] calls that took the SLOW (real
/// `head.load(Acquire)`) path — the shadow suggested the ring might be full
/// (or had never been refreshed) and a genuine cross-core-visible load was
/// issued. `bench-internals`-gated, same rationale as
/// [`DBG_RING_PUSH_SHADOW_FAST`]. `DBG_RING_PUSH_SHADOW_FAST +
/// DBG_RING_PUSH_SHADOW_SLOW` is the total number of `full_check` calls
/// (i.e. of push attempts, counted or uncounted) since process start — a
/// gate's harness uses the SLOW/(FAST+SLOW) ratio as its regime oracle: near
/// 0 proves the "favorable" (rarely-full) regime; near 1 proves the
/// "adversarial" (often-full) regime.
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
pub static DBG_RING_PUSH_SHADOW_SLOW: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Sentinel slot value meaning "this slot carries no offset" (either
/// not-yet-published by a producer, or already drained by the consumer). A real
/// block offset is always `< SEGMENT` (`1 << 22`), so `u32::MAX` is unambiguous.
#[doc(hidden)]
pub const RING_SLOT_EMPTY: u32 = u32::MAX;

/// The number of offset slots in the ring. 256 → 1 KiB of slots per segment.
///
/// **Rationale:** a 4 MiB segment holds up to `SEGMENT / MIN_BLOCK` blocks
/// (≈ 256 K at `MIN_BLOCK = 16`). The ring need only absorb the *burst* of
/// cross-thread frees that arrive between the owner's drains (the owner drains
/// on every alloc and on the `find_segment_with_free` scan). 256 covers a
/// typical burst with headroom; overflow degrades to a bounded leak (sound).
/// Larger caps trade segment metadata footprint for rarer overflow; 256 is the
/// mimalloc-class default for per-page deferred-free queues.
#[doc(hidden)]
pub const RING_CAP: usize = 256;

// The ring's u32 `head`/`tail` cursors are monotonic WRAPPING counters:
// occupancy is `tail.wrapping_sub(head)` and the slot index is `i % RING_CAP`.
// For the slot sequence to stay CONTINUOUS across the `u32::MAX → 0` wrap, the
// index must not jump at the boundary: `(2^32 - 1) % CAP` must be followed by
// `0 % CAP`, i.e. `2^32 % CAP == 0`. That holds iff `CAP` is a power of two.
// A non-power-of-two CAP would jump the slot index at the wrap (…, (2^32-1) mod
// CAP, 0 mod CAP …) and corrupt the FIFO on the ONE genuinely reachable wrap
// hazard (2^32 cross-thread frees on a single hot, long-lived segment). This is
// an otherwise UNSTATED dependency; pin it at compile time.
const _: () = assert!(
    RING_CAP.is_power_of_two(),
    "RING_CAP must be a power of two so 2^32 % RING_CAP == 0 — the ring's u32 \
     head/tail cursors wrap continuously across u32::MAX only then; a \
     non-power-of-two CAP would jump the slot index at the wrap and corrupt the \
     FIFO"
);

/// The byte footprint of a `RemoteFreeRing` in segment metadata. Fixed so the
/// bootstrap can carve it deterministically alongside the bin table.
#[doc(hidden)]
pub const FOOTPRINT: usize = CURSOR_BLOCK + RING_CAP * core::mem::size_of::<u32>();

/// Bits of a ring entry reserved for the block's segment-relative offset.
/// `SEGMENT = 1 << 22`, so every offset is `< 2^22` and fits in the low 22 bits;
/// the high bits carry the size **class** the cross-thread freer stamped (it has
/// the `Layout`, unlike the owner, whose `page_map` is unreliable for the
/// mixed-class pages a shared bump cursor produces — see RACE_DRAIN_RECLAIM §13).
pub(crate) const ENTRY_OFF_BITS: u32 = 22;
/// Mask for the offset field of a packed ring entry.
pub(crate) const ENTRY_OFF_MASK: u32 = (1 << ENTRY_OFF_BITS) - 1;

// R6-OPT-P0-3a (correctness-surface item #6, "cross-thread free" / packed
// `(offset, class)` bit budget): the non-hardened packing above reserves 22
// bits for `off` and the remaining `32 - 22 = 10` bits (values 0..=1023) for
// `class_idx`. `medium-classes` (R6-OPT-P0-3a) grows `SMALL_CLASS_COUNT` from
// 49 to 55 — nowhere near the 10-bit ceiling, so this packing has ample
// headroom (the review's own §4 P0-3 correctness-surface list explicitly
// calls for verifying this, even when it "technically still fits" — see the
// task's final report). Two things must hold for every real
// `(off, class_idx)` pair: `class_idx` must fit in the 10 high bits
// (`SMALL_CLASS_COUNT <= 1024`), and the packed word must never equal
// `RING_SLOT_EMPTY` (`u32::MAX`) — which only happens when EVERY bit is 1,
// i.e. `off == ENTRY_OFF_MASK` (0x3FFFFF, a real reachable last-block offset)
// AND `class_idx == 1023` (0x3FF). The second conjunct is what this assert
// closes: as long as the maximum REAL class index (`SMALL_CLASS_COUNT - 1`)
// stays strictly below 1023, no real pair can produce the sentinel. (Compare
// the `hardened` packing's identical-shaped guard further down this file,
// which pins the SAME property for its own, much tighter 6-bit class field.)
const _: () = assert!(
    (SMALL_CLASS_COUNT as u32) < (1u32 << (32 - ENTRY_OFF_BITS)) - 1,
    "the non-hardened ring entry's class field is 32 - ENTRY_OFF_BITS bits wide; \
     SMALL_CLASS_COUNT must stay strictly below its all-ones value so a real \
     (offset, class) pair can never collide with RING_SLOT_EMPTY (u32::MAX)"
);

/// Pack a `(offset, class_idx)` pair into a single `u32` ring entry.
/// `off < 2^22` (a segment offset) and `class_idx < SMALL_CLASS_COUNT (= 49
/// without `medium-classes`, 55 with it)`, so the result is `< 2^32` and
/// never collides with `RING_SLOT_EMPTY` (`u32::MAX`) for any real block —
/// see the compile-time pin immediately above.
#[cfg_attr(
    any(not(feature = "alloc-xthread"), feature = "hardened"),
    allow(dead_code)
)]
#[inline(always)]
pub(crate) fn pack_entry(off: u32, class_idx: u32) -> u32 {
    debug_assert!(off <= ENTRY_OFF_MASK, "offset overflows ring-entry field");
    off | (class_idx << ENTRY_OFF_BITS)
}

/// Unpack a ring entry into `(offset, class_idx)`.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub(crate) fn unpack_entry(packed: u32) -> (u32, u32) {
    (packed & ENTRY_OFF_MASK, packed >> ENTRY_OFF_BITS)
}

// ---------------------------------------------------------------------------
// X7 Ф2 (task #190) — hardened ring-entry repack: `[gen:8|class:6|off16:18]`.
//
// The non-hardened `pack_entry`/`unpack_entry` ABOVE are byte-for-byte
// untouched (the production entry format, compiled whenever `alloc-xthread`
// is on — this is NOT a hardened-only surface). The block below adds a
// SEPARATE packing scheme compiled ONLY under `#[cfg(feature = "hardened")]`,
// threading the block's generation counter (X7 Ф1's gen-table byte) into the
// ring note so a drain can drop a note whose generation no longer matches the
// block's current life (X7 plan §2.4, §3-Ф2). Nothing here is wired into
// `push`/`drain` or any other ring method yet — that is Ф3. This phase is
// purely the pack/unpack pair + round-trip tests, mirroring Ф1's discipline.
//
// Bit layout (low bits → high bits), matching the plan's notation
// `[gen:8|class:6|off16:18]` read high-to-low (the same convention the
// non-hardened doc comment uses: `[class_idx: bits 22..32][off: bits 0..22]`
// lists the HIGH field first):
//
//   bits [ 0..18) : off16 = off >> MIN_BLOCK_SHIFT   (off in MIN_BLOCK units)
//   bits [18..24) : class_idx                         (size class, < 64)
//   bits [24..32) : gen                               (generation byte, 0..=255)
//
// `off16` is 18 bits because `SEGMENT / MIN_BLOCK = 2^22 / 2^4 = 2^18` — every
// `MIN_BLOCK`-aligned segment-relative offset divides to a value `< 2^18`.
// `class` is 6 bits because `SMALL_CLASS_COUNT = 49 < 64 = 2^6`. `gen` is 8
// bits — the `u8` generation counter established in Ф1 (wraps at 256, the
// accepted 1/256 residual; X7 §2.5). The three fields sum to exactly 32 — no
// wasted or overlapping bits. The external contract is symmetric with the
// non-hardened pair: callers pass and receive the FULL segment-relative byte
// offset (the `off16` internal representation never leaks — pack shifts down
// by `MIN_BLOCK_SHIFT`, unpack shifts back up).
//
// `RING_SLOT_EMPTY` (`u32::MAX`) non-collision: the packed word equals
// `u32::MAX` only when ALL three fields are simultaneously all-ones — i.e.
// `gen=0xFF`, `class=0x3F` (=63), `off16=0x3_FFFF`. `off16=0x3_FFFF` IS
// reachable (it is `SEGMENT - MIN_BLOCK` >> 4, a real last block start), and
// `gen=0xFF` is reachable (the u8 wrap). BUT `class=63` is NOT: the maximum
// real small class index is `SMALL_CLASS_COUNT - 1 = 48` (`0x30`) without
// `medium-classes`, or `54` (`0x36`) WITH it (R6-OPT-P0-3a: 49 -> 55
// classes), so the class field never reaches `0x3F` either way. The maximum
// packed word over real ranges is therefore `0xFFC3_FFFF < u32::MAX` without
// `medium-classes` (computed and pinned by the
// `entry_never_collides_with_ring_slot_empty` regression test) — WITH
// `medium-classes` the maximum real class value shifts from `0x30` to `0x36`
// but stays strictly below `0x3F`, so the same non-collision argument holds,
// just with a NARROWER margin. This safety HOLDS ONLY WHILE
// `SMALL_CLASS_COUNT <= 62` — the const-assert below pins that the class
// field's all-ones value (`2^ENTRY_CLASS_BITS - 1 = 63`) stays strictly above
// `SMALL_CLASS_COUNT - 1`, so a future bump of `SMALL_CLASS_COUNT` past 62
// cannot silently reintroduce a collision. Ф3's ring `push`/`drain` reuse is
// sound under that invariant.
//
// R6-OPT-P0-3a HONEST MARGIN NOTE (correctness-surface item #6, "cross-thread
// free" — the task's own instruction to flag a tight fit explicitly even when
// it technically still fits): `medium-classes` consumes 6 of the 6-bit
// field's 14 headroom values (49 -> 55 used, ceiling 62) — plenty of room for
// THIS experiment (55 << 62), but this field is measurably tighter than the
// non-hardened packing's 10-bit field (ceiling 1022, headroom in the
// hundreds). A THIRD source of classes stacked on top of `medium-classes`
// under `hardened` (e.g. a future page-run layer's own per-run classes, if
// P0-3b ever reuses this SAME packed-word scheme rather than a dedicated one)
// would need to re-check this 62-class ceiling explicitly — it is the first
// of this crate's two ring-entry encodings to feel `medium-classes`' growth
// at all.
// ---------------------------------------------------------------------------

/// X7 Ф2: bits of a hardened ring entry reserved for `off16` (the offset in
/// `MIN_BLOCK` units). `SEGMENT / MIN_BLOCK = 2^18`, so 18 bits suffice.
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_OFF16_BITS: u32 = 18;
/// X7 Ф2: bits reserved for the size class. `SMALL_CLASS_COUNT = 49 < 2^6`.
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_CLASS_BITS: u32 = 6;
/// X7 Ф2: bits reserved for the generation counter (the Ф1 `u8`, wraps at 256).
#[cfg(feature = "hardened")]
#[doc(hidden)]
pub const ENTRY_GEN_BITS: u32 = 8;

/// X7 Ф2: shift of the `class` field (starts where `off16` ends).
#[cfg(feature = "hardened")]
const ENTRY_CLASS_SHIFT: u32 = ENTRY_OFF16_BITS;
/// X7 Ф2: shift of the `gen` field (starts where `class` ends).
#[cfg(feature = "hardened")]
const ENTRY_GEN_SHIFT: u32 = ENTRY_OFF16_BITS + ENTRY_CLASS_BITS;

/// X7 Ф2: mask for the `off16` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_OFF16_MASK: u32 = (1u32 << ENTRY_OFF16_BITS) - 1;
/// X7 Ф2: mask for the `class` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_CLASS_MASK: u32 = (1u32 << ENTRY_CLASS_BITS) - 1;
/// X7 Ф2: mask for the `gen` field of a hardened ring entry.
#[cfg(feature = "hardened")]
pub(crate) const ENTRY_GEN_MASK: u32 = (1u32 << ENTRY_GEN_BITS) - 1;

// X7 Ф2: compile-time pin of the bit layout (W7-style const-asserts, mirroring
// the existing `RING_CAP.is_power_of_two()` assert above). Each field's value
// range is provably covered, and the three fields sum to exactly 32 — the
// plan's layout, not a looser one. `SEGMENT / MIN_BLOCK` and `SMALL_CLASS_COUNT`
// are referenced via `super::` (this file otherwise imports only `Node`); the
// hardened-only `use` is colocated with the asserts so it is invisible to a
// non-hardened compile.
#[cfg(feature = "hardened")]
const _: () = {
    use super::os::SEGMENT;
    use super::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT, SMALL_CLASS_COUNT};
    assert!(
        ENTRY_GEN_BITS + ENTRY_CLASS_BITS + ENTRY_OFF16_BITS == 32,
        "hardened ring entry fields must sum to exactly 32 bits (X7 §2.4 layout)"
    );
    assert!(
        ENTRY_GEN_BITS == 8,
        "gen field must be exactly 8 bits (the Ф1 u8 generation counter)"
    );
    assert!(
        (SMALL_CLASS_COUNT as u64) <= (1u64 << ENTRY_CLASS_BITS),
        "class field must cover SMALL_CLASS_COUNT"
    );
    assert!(
        MIN_BLOCK.is_power_of_two() && SEGMENT.is_power_of_two(),
        "MIN_BLOCK and SEGMENT must be powers of two for the exact off16 division"
    );
    assert!(
        MIN_BLOCK_SHIFT == MIN_BLOCK.trailing_zeros(),
        "MIN_BLOCK_SHIFT must equal log2(MIN_BLOCK) (kept in sync by size_classes)"
    );
    assert!(
        (SEGMENT as u64) >> MIN_BLOCK_SHIFT <= (1u64 << ENTRY_OFF16_BITS),
        "off16 field must cover SEGMENT/MIN_BLOCK (the largest MIN_BLOCK-aligned offset)"
    );
    // RING_SLOT_EMPTY non-collision pin (see the block doc above): the packed
    // word is `u32::MAX` only when gen=0xFF AND class=0x3F AND off16=0x3_FFFF.
    // gen and off16 maxima ARE reachable, so safety rests on class=0x3F being
    // UNreachable — i.e. the max real class (`SMALL_CLASS_COUNT - 1`) staying
    // strictly below the class field's all-ones value (`2^BITS - 1`). Pin it so
    // a future bump of SMALL_CLASS_COUNT into the all-ones value fails to
    // compile here instead of silently reintroducing a sentinel collision.
    assert!(
        (SMALL_CLASS_COUNT as u64) < (1u64 << ENTRY_CLASS_BITS) - 1,
        "SMALL_CLASS_COUNT must stay strictly below the class field's all-ones value \
         so a hardened ring entry can never equal RING_SLOT_EMPTY (u32::MAX)"
    );
};

/// X7 Ф2: pack `(gen, class_idx, off)` into a single `u32` hardened ring entry
/// with the layout `[gen:8|class:6|off16:18]` (gen in the HIGH bits, class in
/// the middle, `off16 = off >> MIN_BLOCK_SHIFT` in the LOW bits — see the block
/// doc above). `off` is the FULL segment-relative byte offset (same units as
/// the non-hardened [`pack_entry`]); the `off16` internal representation never
/// leaks to callers. Returns a value that never collides with
/// [`RING_SLOT_EMPTY`] for any real `(gen, class, off)` triple (verified by the
/// `entry_never_collides_with_ring_slot_empty` regression test).
///
/// Compiled ONLY under `#[cfg(feature = "hardened")]`; not wired into
/// `push`/`drain` yet (that is Ф3).
#[cfg(feature = "hardened")]
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub fn pack_entry_hardened(gen: u8, class_idx: u32, off: u32) -> u32 {
    debug_assert!(
        off >> super::size_classes::MIN_BLOCK_SHIFT <= ENTRY_OFF16_MASK,
        "offset overflows hardened ring-entry off16 field"
    );
    debug_assert!(
        off.is_multiple_of(super::size_classes::MIN_BLOCK as u32),
        "hardened ring-entry offset must be MIN_BLOCK-aligned (off16 = off >> MIN_BLOCK_SHIFT)"
    );
    debug_assert!(
        class_idx <= ENTRY_CLASS_MASK,
        "class_idx overflows hardened ring-entry class field"
    );
    let off16 = off >> super::size_classes::MIN_BLOCK_SHIFT;
    let packed = (off16 & ENTRY_OFF16_MASK)
        | ((class_idx & ENTRY_CLASS_MASK) << ENTRY_CLASS_SHIFT)
        | ((gen as u32 & ENTRY_GEN_MASK) << ENTRY_GEN_SHIFT);
    debug_assert_ne!(
        packed, RING_SLOT_EMPTY,
        "hardened pack_entry must never produce the ring-slot sentinel"
    );
    packed
}

/// X7 Ф2: unpack a hardened ring entry into `(gen, class_idx, off)`, where
/// `off` is the FULL segment-relative byte offset (the `off16` internal field
/// is shifted back up by `MIN_BLOCK_SHIFT` so the external contract is symmetric
/// with the non-hardened [`unpack_entry`]).
///
/// Compiled ONLY under `#[cfg(feature = "hardened")]`; not wired into
/// `push`/`drain` yet (that is Ф3).
#[cfg(feature = "hardened")]
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[inline(always)]
pub fn unpack_entry_hardened(packed: u32) -> (u8, u32, u32) {
    let off16 = packed & ENTRY_OFF16_MASK;
    let class_idx = (packed >> ENTRY_CLASS_SHIFT) & ENTRY_CLASS_MASK;
    let gen = ((packed >> ENTRY_GEN_SHIFT) & ENTRY_GEN_MASK) as u8;
    let off = off16 << super::size_classes::MIN_BLOCK_SHIFT;
    (gen, class_idx, off)
}

/// R8-1 (task #214): extract ONLY the class index from a packed ring entry,
/// without paying for the full unpack (offset/generation are not needed by the
/// ring-drain call sites that just want to know WHICH classes a drain pass
/// touched, so they can drive an incremental directory sync instead of
/// re-sweeping all `SMALL_CLASS_COUNT` classes — see
/// `AllocCore::sync_directory_for_segment_classes`).
///
/// Dispatches to the existing [`unpack_entry`] / [`unpack_entry_hardened`]
/// (matching the build's packing scheme) rather than re-deriving the bit
/// layout, so this stays correct by construction if either packing changes.
///
/// Compiled under `alloc-xthread` (the only build where ring drains run); the
/// `hardened`/non-hardened split lives in the BODY (not on the function's own
/// cfg), so the function is reachable in both hardened and non-hardened
/// `alloc-xthread` builds. `hardened` implies `fastbin` implies
/// `alloc-xthread`, so [`unpack_entry_hardened`] is always in scope when this
/// function's hardened arm compiles.
#[cfg(feature = "alloc-xthread")]
#[inline(always)]
pub(crate) fn entry_class_idx(packed: u32) -> usize {
    #[cfg(feature = "hardened")]
    {
        let (_gen, class_idx, _off) = unpack_entry_hardened(packed);
        class_idx as usize
    }
    #[cfg(not(feature = "hardened"))]
    {
        let (_off, class_idx) = unpack_entry(packed);
        class_idx as usize
    }
}

// R8-1 (task #214): the incremental directory sync driven by
// `entry_class_idx` packs the classes a drain pass touched into a `u64`
// bitmask (one bit per class). That design is valid only while
// `SMALL_CLASS_COUNT <= 64` (a wider class space would need a wider mask).
// Pin it at compile time so a future bump past 64 fails HERE instead of
// silently truncating the bitmask at runtime. `SMALL_CLASS_COUNT` is already
// imported at the top of this file (`use super::size_classes::...`), matching
// the sibling const-assert above that references the same constant.
#[cfg(feature = "alloc-xthread")]
const _: () = assert!(
    SMALL_CLASS_COUNT <= 64,
    "entry_class_idx-based incremental directory sync packs touched classes \
     into a u64 bitmask; SMALL_CLASS_COUNT must stay <= 64"
);

/// The cursor block: `head`, `tail`, `overflow`, padded up to 128 bytes — two
/// full cache lines, so `head` (consumer-only) and `tail`/`overflow`
/// (producer-touched) each start their OWN 64-byte-aligned line.
///
/// **PERF-PASS-4 (G8/ML4, task #52) — was 16 bytes.** At `CURSOR_BLOCK = 16`,
/// `head`@0 + `tail`@4 + `overflow`@8 + a 4-byte pad + `slots[0..12]` all
/// shared ONE 64-byte cache line (the ring's in-segment base is 64-byte
/// aligned, so this was exact, not approximate). Producers CAS `tail` and
/// Acquire-load `head` on every push; the consumer Release-stores `head`,
/// Acquire-loads `tail`, and reads/clears slots — all landing on that SAME
/// line. Widening to 128 bytes puts `head` (offset 0, consumer-only writes)
/// on its own line and `tail`/`overflow` (offset 64, producer-touched) on a
/// SECOND line, disjoint from both `head` and the first data slots
/// (`SLOTS_OFF` moves from 16 to 128). Costs 112 extra bytes per segment's
/// ring metadata (4 MiB segment; negligible). `FOOTPRINT` and every
/// downstream segment-metadata offset (`Layout::small_meta_end`, etc.)
/// derive FROM this constant, so the layout re-composes automatically — see
/// the compile-time layout asserts at the bottom of `segment_header.rs`,
/// which re-verify unchanged.
const CURSOR_BLOCK: usize = 128;

/// Offset of the `head` cursor within the ring metadata. Own cache line
/// (bytes 0..64) — consumer-only writes (`drain`'s `head.store`), producer
/// reads (`push`'s full-check `head.load(Acquire)`).
///
/// Only read by the ring's push/drain methods, which are only reachable on
/// builds that exercise cross-thread free (`alloc-xthread`); unused under
/// `--features alloc-core` alone.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const HEAD_OFF: usize = 0;
/// Offset of the `tail` cursor within the ring metadata. PERF-PASS-4: moved
/// from 4 to 64 — its own cache line, separate from `head`'s line and from
/// the first data slots. Producer-CASed on every push; consumer
/// Acquire-loads it once per drain.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const TAIL_OFF: usize = 64;
/// Offset of the `overflow` counter within the ring metadata. PERF-PASS-4:
/// moved from 8 to 68 — shares `tail`'s line (both are producer-touched;
/// `overflow` is only written on the rare full-ring path, so co-locating it
/// with `tail` costs nothing on the common push path and avoids spending a
/// THIRD cache line on one counter).
const OVERFLOW_OFF: usize = 68;
/// F10 (task #502): offset of the `cached_head` shadow within the ring
/// metadata — 72, immediately after `overflow` (68) on the SAME producer
/// line as `tail`/`overflow`. Was unused reserved padding (bytes 72..128 of
/// the cursor block were entirely unclaimed before this task — confirmed by
/// grepping every other `_OFF` constant in this file: none references any
/// offset in `[72, 128)`). A producer's full-check reads this field on the
/// SAME cache line as its own `tail` load, instead of the CONSUMER's `head`
/// line — see the module doc's "F10 — shadow/cached head" section for the
/// full soundness argument. `CURSOR_BLOCK` (128) is unchanged, so
/// `FOOTPRINT`/`SLOTS_OFF` and every downstream segment-metadata offset are
/// byte-identical to before this task.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const CACHED_HEAD_OFF: usize = 72;
/// Offset of the first slot within the ring metadata. PERF-PASS-4: moved
/// from 16 to 128 (`CURSOR_BLOCK`) — the data slots now start on a line past
/// BOTH cursor lines, so neither producer's `tail` CAS nor the consumer's
/// `head` store dirties a line the other side is scanning for data.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
const SLOTS_OFF: usize = CURSOR_BLOCK;

// F10 (task #502): pin that `cached_head` fits strictly within the existing
// `CURSOR_BLOCK` padding without colliding with `SLOTS_OFF` — a future
// CURSOR_BLOCK shrink (unlikely, but this is exactly the kind of silent
// layout hazard the module's other compile-time pins exist to catch) would
// fail HERE instead of corrupting ring data at runtime.
const _: () = assert!(
    CACHED_HEAD_OFF + core::mem::size_of::<u32>() <= CURSOR_BLOCK,
    "F10's cached_head field (CACHED_HEAD_OFF..+4) must fit inside CURSOR_BLOCK, \
     strictly before SLOTS_OFF"
);

/// The per-segment non-intrusive cross-thread-free MPSC ring.
///
/// A thin view over in-segment metadata (no allocation — the bootstrap carves
/// the bytes at [`super::segment_header::Layout::remote_ring_off`]). Producers
/// push block offsets; the single consumer ([`drain`](Self::drain)) reclaims
/// them. See the module docs for the protocol and orderings.
///
/// The struct + `FOOTPRINT` are compiled unconditionally (the segment `Layout`
/// always reserves the ring's bytes); the `push`/`drain`/`at`/`init_in_place`
/// methods exist only under `alloc-xthread` (the cross-thread feature).
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[doc(hidden)]
pub struct RemoteFreeRing {
    base: *mut u8,
}

/// A push failed because the ring is full. The caller MUST discard the block
/// (bounded leak) — see "Overflow semantics" in the module docs.
#[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
#[doc(hidden)]
pub struct PushOverflow;

/// F-7 (R34-17/task #536) — RAII guard that publishes [`RemoteFreeRing::drain`]'s
/// `head` cursor on drop, so a `reclaim` closure that unwinds mid-drain still
/// commits the progress made before the panic. Mirrors the `LockGuard`
/// (`global::fallback`, task L4) / `ConflictRollback` (`registry::heap_registry`,
/// R6-CQ-3) panic-safety pattern already in this crate. The guard is the SOLE
/// writer of `head` on the drain path: the pre-F-7 explicit
/// `head.store(h, Release)` after the loop was removed in favour of this `Drop`,
/// so there is exactly one publish whether the drain completes normally or
/// unwinds.
///
/// **Exact contract (Sol-F5, task #567 — release-readiness review finding
/// F5, `docs/reviews/2026-08-05-sol-release-readonly-review.md`).** This
/// guard is unwind-safe against **losing already-fully-processed prior
/// elements**: every offset whose `reclaim` call returned normally, and
/// whose slot was cleared, before the panicking iteration is still
/// published (not re-drained, not silently dropped). It does **NOT**
/// provide exactly-once semantics for the **specific element being
/// processed when the panic occurs**. `drain`'s loop body
/// (`RemoteFreeRing::drain`, below) calls `reclaim(off)` BEFORE clearing
/// the slot and BEFORE advancing/publishing `h` — so if `reclaim(off)`
/// mutates allocator/external state and THEN panics, the slot is left
/// non-empty and `h` is left one short: a `catch_unwind`ing caller that
/// resumes draining will re-pass that SAME `off` to `reclaim` on the next
/// call, i.e. `reclaim` may run twice (once mutating state, then again
/// after resume) for the element that was in flight at panic time. This is
/// a property of the drain loop's `reclaim → clear → advance` ORDER
/// (`RemoteFreeRing::drain`'s loop body), not something this guard's
/// publish-on-drop can fix by itself — the guard only ever publishes `h`
/// values that were fully advanced past a cleared slot.
///
/// Production `reclaim` closures (`AllocCore::reclaim_offset` /
/// `AllocCore::reclaim_offset_checked`, `src/alloc_core/alloc_core_small_reclaim.rs`,
/// called from the three drain sites in `src/alloc_core/alloc_core_small.rs`)
/// are, by inspection of their current bodies, not currently known to panic
/// after mutating state — the reclaim functions themselves are ordinary
/// `pub(crate) fn`s returning `bool` with no `unwrap`/`expect`/`panic!`/
/// unchecked indexing on their mutation-bearing paths, and the calling
/// closures only perform a decrement/OR-accumulate afterward. But this is
/// an observation about the code AS WRITTEN, not a structural guarantee —
/// nothing in the type system prevents a future `reclaim` closure (or a
/// direct/internal `catch_unwind` caller of `drain`) from panicking after a
/// mutation. Achieving true exactly-once-under-unwind would need a
/// two-phase/idempotent reclaim protocol (or an explicit poison/skip
/// policy), which is out of scope for this guard — see the review finding
/// for the fuller discussion. In practice this residual is reachable only
/// through a direct/internal `catch_unwind` around `drain`: an unwind that
/// escapes through the `GlobalAlloc` entry points still aborts the process
/// (see `src/global/sefer_alloc.rs`'s panic-tripwire docs), so the replay
/// window described here cannot be observed through ordinary allocator
/// usage. Tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 22 (task #575/H5).
#[cfg(feature = "alloc-xthread")]
struct DrainHeadPublish {
    head: &'static core::sync::atomic::AtomicU32,
    h: u32,
}

#[cfg(feature = "alloc-xthread")]
impl Drop for DrainHeadPublish {
    fn drop(&mut self) {
        // Publish the new head so producers' full-check sees the freed space.
        // Release: pairs with their Acquire head load in `push`/`full_check`.
        // `self.h` is the most-recently-advanced value (updated inside the loop
        // after each successful reclaim+clear), so on the unwind path only the
        // offsets that were FULLY processed (reclaimed + cleared + advanced) are
        // committed — the panicking iteration's offset is NOT advanced past,
        // matching the pre-F-7 invariant that `head` only marks fully-drained
        // slots.
        self.head.store(self.h, Ordering::Release);
    }
}

impl RemoteFreeRing {
    /// Construct the view over ring metadata at `base + off`. The caller (the
    /// bootstrap / `SegmentMeta::remote_ring`) guarantees the byte range
    /// `[base + off, base + off + FOOTPRINT)` is carved, 4-byte-aligned, and
    /// inside a live segment.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn at(base: *mut u8, off: usize) -> Self {
        Self {
            base: Node::offset(base, off),
        }
    }

    /// **Test surface** (`#[doc(hidden)] pub`): construct a ring view over an
    /// arbitrary aligned byte buffer at offset 0. Used ONLY by the isolated
    /// ring unit test (`tests/remote_ring_unit.rs`), which builds a ring over a
    /// plain `Box<[u8]>` (NOT a segment, NOT an allocator) to prove the ring's
    /// MPSC correctness in isolation from the allocator / ABA concerns.
    ///
    /// Production code MUST use [`at`](Self::at) with a segment-relative offset
    /// from [`Layout::remote_ring_off`](super::segment_header::Layout::remote_ring_off).
    ///
    /// R2-3: the null + 4-byte-alignment preconditions are checked by a
    /// RELEASE-surviving `assert!` (not `debug_assert!`), so a null/misaligned
    /// base panics in every build.
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee that `base` points to at least `FOOTPRINT`
    /// writable, 4-byte-aligned bytes that are exclusively owned by the caller
    /// and live for the ring's use (e.g. an `alloc::vec![0u8; FOOTPRINT]` boxed
    /// slice). The `FOOTPRINT`-writability / liveness / exclusivity half of the
    /// contract cannot be checked at runtime — the only documented use is an
    /// owned boxed buffer. Passing a too-short, dangling, shared, or
    /// non-`FOOTPRINT`-valid base is undefined behaviour.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary — the
                          // validity/size/alignment/lifetime/exclusivity of the caller-supplied
                          // pointer is unverifiable by the callee, so the contract MUST live in the
                          // signature, not in prose. The body is safe (delegates to `Self::at`).
    pub unsafe fn over_test_buffer(base: *mut u8) -> Self {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "over_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::at(base, 0)
    }

    /// **Test surface**: initialise a fresh ring at `base` (offset 0). Same as
    /// [`init_in_place`](Self::init_in_place) but for a standalone buffer (no
    /// segment-relative offset). See [`over_test_buffer`](Self::over_test_buffer).
    ///
    /// R2-3: carries the same release-surviving null + 4-byte-alignment `assert!`
    /// as [`over_test_buffer`](Self::over_test_buffer).
    ///
    /// # Safety
    ///
    /// Same contract as [`over_test_buffer`](Self::over_test_buffer#safety):
    /// `base` MUST point to at least `FOOTPRINT` writable, 4-byte-aligned,
    /// exclusively-owned bytes that are live for the ring's use. The callee
    /// writes cursors and all slots starting at `base`, so a too-short, dangling
    /// or shared buffer is undefined behaviour.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn init_test_buffer(base: *mut u8) {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "init_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::init_in_place(base, 0)
    }

    /// **Test surface**: the overflow counter's current value (diagnostic). Used
    /// by the isolated ring test to assert `reclaimed + overflowed == pushed`.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn overflow_count(&self) -> u32 {
        self.overflow().load(Ordering::Acquire)
    }

    /// **Test surface** (task: long-run u32 wrap): preset the `head` and `tail`
    /// cursors directly so a test can drive the ring across the `u32::MAX → 0`
    /// boundary without first pushing 2^32 entries. Writes the atomics with
    /// `Release` (mirrors the production drain's `head` publish / push's `tail`
    /// reservation visibility) so a subsequently spawned producer/consumer sees
    /// the preset. MUST be called on a quiescent ring (no concurrent push/drain)
    /// and MUST leave `tail.wrapping_sub(head) <= RING_CAP` (the ring's full
    /// invariant) — the caller is responsible for a consistent preset.
    ///
    /// F10 (task #502): also resets `cached_head` to the new `head` value.
    /// Without this, a preset that MOVES `head` (e.g. from its `init_in_place`
    /// zero to a wrap-boundary value) would leave a STALE `cached_head` behind
    /// — harmless by the shadow's own soundness argument (a stale-low shadow
    /// only ever forces the conservative slow path, never an unsound fast-path
    /// accept — see the module doc), but needlessly forces every subsequent
    /// push in the test to pay the slow path, which is not representative of
    /// what a real preset-then-drive scenario should measure. Resetting here
    /// keeps `dbg_set_cursors` an honest "quiescent ring, consistent state"
    /// preset rather than relying on the shadow's stale-low safety margin to
    /// paper over an inconsistency this seam itself introduced.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_set_cursors(&self, head: u32, tail: u32) {
        self.head().store(head, Ordering::Release);
        self.tail().store(tail, Ordering::Release);
        self.cached_head().store(head, Ordering::Relaxed);
    }

    /// F10 (task #502) **test surface**: advance ONLY the real `head` cursor
    /// (`Release`, mirroring the production drain's own store), deliberately
    /// NOT touching `cached_head` — the inverse of `dbg_set_cursors`'s
    /// consistency-preserving reset. Lets a test simulate "the owner drained
    /// but no producer has refreshed its shadow yet", i.e. deliberately
    /// STALE the shadow relative to the real head, to drive the shadow's
    /// slow path on demand and prove it still re-derives correctly (see
    /// `tests/remote_ring_shadow_head.rs`'s adversarial-regime path-
    /// activation coverage). MUST be called on a quiescent ring (no
    /// concurrent push/drain), same precondition as `dbg_set_cursors`,
    /// and MUST NOT regress `head` below its current value — storing a
    /// value lower than the current `head` would leave `cached_head`
    /// above the regressed `head` (a STALE-HIGH shadow), which the module
    /// doc's F10 monotonicity argument declares impossible and which
    /// could let the fast path admit a push into a full ring. The hook's
    /// only real caller (`tests/remote_ring_shadow_head.rs`) uses
    /// `wrapping_add(1)` — an advance, never a regression.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_advance_head_only(&self, head: u32) {
        self.head().store(head, Ordering::Release);
    }

    /// **Test surface** (task: long-run u32 wrap): read the current `(head,
    /// tail)` cursor pair. Lets a test assert occupancy (`tail.wrapping_sub(
    /// head)`) across the wrap. `Acquire` loads (uniform with the drain/push).
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_cursors(&self) -> (u32, u32) {
        (
            self.head().load(Ordering::Acquire),
            self.tail().load(Ordering::Acquire),
        )
    }

    /// Initialise a fresh ring at `base + off`: zero the cursors and mark every
    /// slot `RING_SLOT_EMPTY`. Called by the bootstrap when a small/primordial
    /// segment is reserved. The segment is exclusively owned at init time
    /// (single-writer), so plain writes suffice — no atomics needed here.
    ///
    /// `base + off` MUST point to `FOOTPRINT` writable bytes.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn init_in_place(base: *mut u8, off: usize) {
        let ring = Self::at(base, off);
        // Cursors: zero (empty ring). Plain writes — bootstrap is single-writer.
        Node::write_u32(Node::offset(ring.base, HEAD_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, TAIL_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, OVERFLOW_OFF) as *mut u32, 0);
        // F10: cached_head starts at 0, matching the real head's initial value
        // (the shadow's own invariant — it only ever holds a value that was
        // once really `head` — holds trivially at init since both start at 0).
        Node::write_u32(Node::offset(ring.base, CACHED_HEAD_OFF) as *mut u32, 0);
        // Every slot empty.
        for i in 0..RING_CAP {
            let slot =
                Node::offset(ring.base, SLOTS_OFF + i * core::mem::size_of::<u32>()) as *mut u32;
            Node::write_u32(slot, RING_SLOT_EMPTY);
        }
    }

    /// The `&AtomicU32` head cursor (consumer drain position).
    #[cfg(feature = "alloc-xthread")]
    fn head(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, HEAD_OFF)
    }
    /// The `&AtomicU32` tail cursor (producer reserve position).
    #[cfg(feature = "alloc-xthread")]
    fn tail(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, TAIL_OFF)
    }
    /// The `&AtomicU32` overflow counter (diagnostic; number of discarded
    /// pushes due to a full ring).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    fn overflow(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, OVERFLOW_OFF)
    }
    /// F10 (task #502): the `&AtomicU32` producer-line shadow replica of
    /// `head`. Same cache line as `tail`/`overflow` — reading it costs no
    /// cross-core coherence traffic beyond what `push`'s own `tail` load
    /// already pays. See the module doc's "F10 — shadow/cached head" section
    /// for the full soundness argument for why a stale value here is always
    /// safe.
    #[cfg(feature = "alloc-xthread")]
    fn cached_head(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, CACHED_HEAD_OFF)
    }
    /// The `&AtomicU32` slot at reservation index `i` (`i % RING_CAP`).
    #[cfg(feature = "alloc-xthread")]
    fn slot(&self, i: usize) -> &'static core::sync::atomic::AtomicU32 {
        let idx = i % RING_CAP;
        Node::atomic_u32_at(self.base, SLOTS_OFF + idx * core::mem::size_of::<u32>())
    }

    /// F10 (task #502): the shared full-check used by both [`push`](Self::push)
    /// and [`try_push_uncounted`](Self::try_push_uncounted). Returns `Ok(())`
    /// if reservation `t` is provably within capacity; `Err(())` if the ring
    /// is (really, `Acquire`-confirmed) full.
    ///
    /// **Fast path (shadow):** `ch = cached_head.load(Acquire)` — same
    /// producer cache line as `tail`, no cross-core traffic. If
    /// `t.wrapping_sub(ch) < RING_CAP`, the ring provably has room (the
    /// module doc's "F10" soundness section proves `cached_head <= head`
    /// always, so this can only UNDER-estimate available room, never
    /// over-estimate it) — return `Ok(())` immediately without touching the
    /// consumer's `head` line at all. The `Acquire` (R34-6, task #525,
    /// finding F-1) restores the happens-before edge the pre-F10
    /// `head.load(Acquire)` supplied: a producer whose slow path refreshed
    /// `cached_head` with a `Release` store (below) carries the consumer's
    /// `slot.store(EMPTY)` in its history, and THIS `Acquire` load
    /// synchronizes-with that store — so a later producer that wins the
    /// tail CAS into a recycled slot is guaranteed to observe the clear
    /// before it publishes. On x86-TSO this `Acquire` load compiles to the
    /// SAME `mov` as the old `Relaxed` (all x86 loads are acquire); the
    /// cost is fence *strength*, not a fence instruction.
    ///
    /// **Slow path (real check + shadow refresh):** only reached when the
    /// shadow suggests the ring MIGHT be full. Performs the exact pre-F10
    /// `head.load(Acquire)`, refreshes `cached_head` from it (`Release` —
    /// the refresh now carries the synchronisation edge that the fast
    /// path's `Acquire` load pairs with; see the ordering note above),
    /// and re-checks against the REAL value before returning `Err(())`.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    fn full_check(&self, t: u32) -> Result<(), ()> {
        // R34-6 (task #525, finding F-1): Acquire — restores the happens-
        // before edge that the pre-F10 `head.load(Acquire)` supplied (see
        // the module doc's F10 ordering supplement). On x86-TSO this is a
        // plain `mov` (identical to the old `Relaxed`); on aarch64 it is
        // one `ldapr` instead of `ldr`.
        let ch = self.cached_head().load(Ordering::Acquire);
        if t.wrapping_sub(ch) < RING_CAP as u32 {
            // Shadow proves room exists (stale-low cached_head only makes
            // this branch LESS likely to fire, never falsely fire — see the
            // module doc soundness section). Skip the real Acquire load.
            #[cfg(feature = "bench-internals")]
            DBG_RING_PUSH_SHADOW_FAST.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        // Shadow suggests full (or has never been refreshed since init, both
        // starting at 0): fall through to the real, Acquire-ordered check —
        // byte-identical to the pre-F10 protocol on this branch.
        #[cfg(feature = "bench-internals")]
        DBG_RING_PUSH_SHADOW_SLOW.fetch_add(1, Ordering::Relaxed);
        let h = self.head().load(Ordering::Acquire);
        // R34-6 (task #525, finding F-1): Release — pairs with the fast
        // path's `Acquire` load so a later producer that reads this
        // refreshed value carries the consumer's `slot.store(EMPTY)` in
        // its happens-before past. On x86-TSO this is a plain `mov`
        // (identical to the old `Relaxed`); on aarch64 it is one `stlr`.
        self.cached_head().store(h, Ordering::Release);
        if t.wrapping_sub(h) >= RING_CAP as u32 {
            return Err(());
        }
        Ok(())
    }

    /// Push a freed block's segment-relative `offset` into the ring. Called by
    /// a NON-OWNER thread (a cross-thread freer). Returns `Err(PushOverflow)`
    /// if the ring is full — the caller MUST then discard the block (bounded
    /// leak, sound).
    ///
    /// `offset` MUST be `< SEGMENT` (a real block offset, not the sentinel).
    #[cfg(feature = "alloc-xthread")]
    pub fn push(&self, offset: u32) -> Result<(), PushOverflow> {
        debug_assert_ne!(offset, RING_SLOT_EMPTY, "offset must not be the sentinel");
        loop {
            let t = self.tail().load(Ordering::Relaxed);
            // F10: shadow-checked full-check (see `full_check`'s doc for the
            // fast/slow path split and the module doc for the soundness
            // argument). Semantically identical to the pre-F10
            // `t.wrapping_sub(head.load(Acquire)) >= RING_CAP` check.
            if self.full_check(t).is_err() {
                // Ring full: bounded leak. Count it (diagnostic, both the
                // per-segment cursor-block counter AND the process-wide D2
                // counter) and bail.
                let _ = self.overflow().fetch_add(1, Ordering::Relaxed);
                DBG_RING_OVERFLOW.fetch_add(1, Ordering::Relaxed);
                return Err(PushOverflow);
            }
            // Reserve slot `t`: CAS tail t → t+1. AcqRel on success — the
            // reservation is the linearization point; Acquire pairs with a
            // prior producer's Release publish (harmless here, but uniform with
            // the drain's view). Relaxed on failure: retry, no side-effect.
            match self.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Publish: write the offset into the reserved slot. Release
                    // so the consumer's Acquire slot load sees this write.
                    self.slot(t as usize).store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue, // Another producer reserved `t`; retry.
            }
        }
    }

    /// R6-OPT-P0-4: byte-identical push/CAS/publish protocol to [`push`](
    /// Self::push), EXCEPT the "ring full" branch does NOT bump either
    /// diagnostic counter (`self.overflow()` / [`DBG_RING_OVERFLOW`]).
    ///
    /// Exists ONLY for `HeapCore::push_with_overflow_retry`'s bounded
    /// spin-retry loop, which (under the R6-OPT-P0-4 "overflow-first"
    /// policy) is now reached only in the genuinely rare case where BOTH the
    /// segment ring's one counted attempt AND an immediate
    /// `push_to_heap_overflow` attempt have already failed — i.e. every
    /// failed poll inside that loop is a re-check of an already-known-full
    /// ring, not a new diagnostic event. Counting each of up to
    /// `RING_PUSH_RETRY_SPINS` (8,192) re-polls would tax the diagnostic
    /// counters with a locked RMW per poll for no informational gain: the ONE
    /// counted [`push`](Self::push) attempt the caller already made is the
    /// signal "this ring overflowed at all"; the retry loop's OWN outcome is
    /// separately, meaningfully counted by the caller via
    /// `DBG_RING_PUSH_RETRIED` (single bump, on eventual success) and
    /// `DBG_RING_PUSH_RETRY_EXHAUSTED` (single bump, if the whole budget is
    /// exhausted) — see that caller's doc comment for the full accounting.
    ///
    /// `offset` MUST be `< SEGMENT` (a real block offset, not the sentinel) —
    /// same contract as [`push`](Self::push).
    #[cfg(feature = "alloc-xthread")]
    pub fn try_push_uncounted(&self, offset: u32) -> Result<(), PushOverflow> {
        debug_assert_ne!(offset, RING_SLOT_EMPTY, "offset must not be the sentinel");
        loop {
            let t = self.tail().load(Ordering::Relaxed);
            // F10: identical shadow-checked full-check as `push` (see
            // `full_check`'s doc + the module doc's soundness section).
            if self.full_check(t).is_err() {
                // Ring full: bounded leak, SAME as `push` — but deliberately
                // uncounted (see doc comment above for why).
                return Err(PushOverflow);
            }
            // Reserve slot `t`: identical CAS/publish protocol to `push`.
            match self.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slot(t as usize).store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue, // Another producer reserved `t`; retry.
            }
        }
    }

    /// Drain all published offsets from the ring, passing each to `reclaim`.
    /// Called ONLY by the owning thread (single consumer). `reclaim` receives
    /// the block's segment-relative offset; the caller turns it back into a
    /// pointer and routes it to the segment's `BinTable`.
    ///
    /// Stops at the first not-yet-published reserved slot (a producer won the
    /// reservation CAS but hasn't stored the offset yet) — order is preserved by
    /// the cursors, so a later drain picks it up.
    ///
    /// Returns the final `head` value written (i.e. the drain cursor after
    /// this call). PERF-PASS-4 (G9/C2, task #52): callers that maintain an
    /// owner-private cached copy of `head` (to skip future empty drains — see
    /// [`RemoteFreeRing::is_likely_empty`]) use this to refresh their cache
    /// without a second atomic load; callers that don't care simply ignore it
    /// (existing call sites are source- and behaviour-compatible).
    ///
    /// **Unwind contract if `reclaim` panics:** see [`DrainHeadPublish`]'s
    /// doc comment for the exact guarantee (no loss of already-fully-processed
    /// prior elements) and the exact non-guarantee (no exactly-once for the
    /// element `reclaim` was processing when it panicked — that offset may be
    /// re-passed to `reclaim` on a subsequent `drain` call after a
    /// `catch_unwind`). The loop body below calls `reclaim(off)` BEFORE
    /// clearing the slot and BEFORE advancing `h`, which is the reason the
    /// non-guarantee exists.
    #[cfg(feature = "alloc-xthread")]
    pub fn drain<F: FnMut(u32)>(&self, mut reclaim: F) -> u32 {
        // Acquire: see every producer's Release reservation (tail CAS) and
        // their Release publish (slot store).
        let t = self.tail().load(Ordering::Acquire);
        // Relaxed is sound here despite `head` being written (below) with a
        // Release store and read here without an Acquire: the ring has a SINGLE
        // consumer, but consumer IDENTITY moves with slot ownership. A ring
        // belongs to a segment; when that segment is recycled and re-claimed by
        // a new owner thread, the registry recycle→claim handshake is itself a
        // Release/Acquire pair that establishes happens-before between the
        // previous owner's LAST `head` Release store and the new owner's first
        // drain. So the new owner-consumer is guaranteed to observe the prior
        // owner's final `head` value; no per-load Acquire on `head` is needed
        // because there is never a concurrent writer to `head` — only a prior
        // one, already fenced by the ownership transfer (review B, Finding 4).
        let mut h = self.head().load(Ordering::Relaxed);
        // F-7 (R34-17/task #536): RAII-publish the drain cursor so a `reclaim`
        // closure that unwinds mid-drain still publishes the progress actually
        // made. WITHOUT this guard, a panic propagating out of `reclaim(off)`
        // would skip the `head.store(h, Release)` below entirely (it sits AFTER
        // the loop) — so the next `drain` re-reads the stale `head` and, since
        // the slots of any fully-processed offsets are now `EMPTY`, breaks
        // immediately at the first cleared slot, leaking every offset from the
        // panicking iteration onward (a stuck "false-empty" until the segment is
        // recycled and the ring reset). The guard publishes EXACTLY ONCE: on the
        // happy path its `Drop` runs at scope end; on the unwind path its `Drop`
        // runs during unwind — either way `h` holds the most-recently-advanced
        // value, so only real progress is published.
        let mut publish = DrainHeadPublish {
            head: self.head(),
            h,
        };
        // Wrap-correct drain: both cursors are monotonic wrapping counters
        // (incremented by `wrapping_add(1)`), so the undrained count is
        // `t.wrapping_sub(h)` — NOT `t - h`, which overflows on cursor wrap.
        // `while h < t` would silently stop draining once `tail` wraps past
        // `u32::MAX` while `head` has not, leaking every subsequent offset
        // (and, worse, a later drain could re-process a slot whose offset was
        // already reclaimed before the wrap if `head` were ever advanced past
        // `tail` — impossible while `head <= tail` by the full-check, but the
        // `<` comparison is still wrong and must be `!=`). The full-check in
        // `push` guarantees `t.wrapping_sub(h) < RING_CAP` at all times, so
        // `h == t` is exactly the empty condition and `h != t` the non-empty
        // one — order is preserved by the cursors, never by the comparison.
        while h != t {
            let slot = self.slot(h as usize);
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                // Reserved but not yet published. Cannot skip (cursor order);
                // a later drain will pick it up once the producer publishes.
                break;
            }
            // Reclaim the offset. Done BEFORE clearing the slot so a concurrent
            // producer cannot reuse this slot before we've consumed it (the
            // full-check prevents reuse while undrained, and clearing marks it
            // drained for the next wrap).
            reclaim(off);
            // Clear the slot for the next wrap. Relaxed: the next producer to
            // touch this slot will Release-store its offset; our drain reads
            // Acquire. No cross-thread dependency on this clear's ordering.
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
            publish.h = h;
        }
        // The guard's `Drop` publishes `h` with Release — the sole head store,
        // covering both the happy path (scope-end drop) and the unwind path
        // (drop during unwind). No explicit store is needed here.
        h
    }

    /// PERF-PASS-4 (G9/C2, task #52) — pre-drain empty-guard primitive: a
    /// cheap Relaxed load of `tail` ONLY (no `head` load at all — the caller
    /// already holds its own owner-private cached copy of `head`, refreshed
    /// from [`drain`](Self::drain)'s return value).
    ///
    /// **Why `Relaxed` is sound here (extends the existing single-consumer
    /// argument at [`drain`](Self::drain)'s doc comment):** the sole purpose
    /// of this load is to decide "has ANY producer reserved a slot since we
    /// last drained". A push's `tail` CAS is `AcqRel`; a Relaxed load here may
    /// observe an OLDER value of `tail` than the most recent CAS (no
    /// synchronizes-with edge), but it can NEVER observe a value that skips a
    /// real advance: `tail` is monotonic (only ever `wrapping_add(1)`-ed by a
    /// winning CAS), so ANY Relaxed load of it returns either the cached
    /// value or a LATER one — never a value that hides a genuine push. Three
    /// outcomes:
    ///   - `tail_relaxed() == cached_head` → the ring is PROVABLY unchanged
    ///     since the cache was taken (no push can have landed without moving
    ///     `tail` off `cached_head`, and `cached_head` was itself set FROM a
    ///     real `head` value that only advances up to a real `tail`) — safe
    ///     to skip the drain entirely.
    ///   - `tail_relaxed() != cached_head` but a push landed AFTER this load
    ///     returns → exactly the same as today's drain missing a push that
    ///     lands after `drain`'s own `tail.load(Acquire)` returns: the
    ///     "later drain picks it up" contract (module docs) already covers
    ///     this window, unconditionally, regardless of whether THIS call
    ///     skipped or ran a real drain.
    ///   - A push landed and is visible: `tail_relaxed() != cached_head`, the
    ///     caller falls through to a real `drain()`, which re-establishes
    ///     ordering via its own `Acquire` tail load — this Relaxed load is
    ///     ONLY a pre-filter, never the operation that reads the pushed data.
    ///
    /// The slot re-claim boundary (a segment's ring surviving a `HeapSlot`
    /// recycle→claim, per the whole-slot-reuse discipline — see
    /// `crate::registry::heap_registry`'s module doc and
    /// `AbandonGuard::drop`'s "Phase 12.5 (architectural turn)" note) needs NO
    /// extra fence
    /// here: the cache lives in the segment's OWN header
    /// (`SegmentHeader::ring_drain_head`), which is reset to `0` only when a
    /// segment is freshly reserved (`SegmentHeader::small`), exactly mirroring
    /// the ring's own `head`/`tail` reset in `RemoteFreeRing::init_in_place`
    /// at the SAME call site (`reserve_small_segment`). A recycled `HeapSlot`
    /// re-claimed by a new owner thread reuses the SAME `HeapCore` (and hence
    /// the SAME live segments/rings) whole — there is no "new owner, old
    /// ring" combination in this codebase's shard-reuse model, so there is no
    /// window where a stale cached head from a different logical owner could
    /// leak across a re-claim.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn tail_relaxed(&self) -> u32 {
        self.tail().load(Ordering::Relaxed)
    }

    /// R6-REGRESSION-2 (progress-detection stop condition in
    /// `HeapCore::push_with_overflow_retry`): the ring's current DRAIN cursor
    /// (`head`) as a single `Relaxed` load — the production (non-`dbg_*`)
    /// sibling of the test-only [`dbg_cursors`](Self::dbg_cursors) hook,
    /// exposing ONLY the consumer-advanced half of the cursor pair.
    ///
    /// **Purpose.** A producer stuck in the bounded retry loop needs to
    /// distinguish "the owner is draining, however slowly" (keep waiting)
    /// from "the owner is making zero drain progress" (concede to the
    /// documented bounded leak). `head` is advanced ONLY by the owner's
    /// [`drain`](Self::drain) — producers never write it — so observing it
    /// move between probe rounds is an exact "the owner drained something"
    /// signal, and observing it NOT move is an exact "the owner drained
    /// nothing in that window" signal.
    ///
    /// **Why `Relaxed` is sound (same monotonicity argument as
    /// [`tail_relaxed`](Self::tail_relaxed), applied to `head`):** `head` is
    /// monotonic (only ever advanced by the single consumer's `Release`
    /// store), so a `Relaxed` load returns either the latest value or an
    /// older one — never a fabricated future value. The caller compares two
    /// such loads taken hundreds of microseconds apart purely to detect
    /// MOVEMENT: a stale read can only UNDER-report progress (delaying the
    /// "progressed" verdict to the next probe round — one extra cheap round,
    /// never a correctness hazard), and can never fabricate progress that did
    /// not happen. No payload is read through this value, so no
    /// Acquire-ordered visibility is needed here — the retry loop's own
    /// `try_push_uncounted` re-establishes ordering via its `Acquire` head
    /// load when it actually attempts the push.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn head_relaxed(&self) -> u32 {
        self.head().load(Ordering::Relaxed)
    }
}
