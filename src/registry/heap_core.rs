//! [`HeapCore`] — the thin heap value that lives inside a registry slot.
//!
//! This is the type the Phase 12.3 raw-pointer TLS caches as
//! `*mut HeapCore`. Per §2.0 of `ALLOC_PLAN_PHASE12-13.md` the heap is now
//! thin (segment-centric free state lives in each segment's `BinTable`, not
//! in a heap-local array), so the per-slot heap needs to carry only:
//!
//! - its **id** (its slot index + the slot's `generation`), used by the 12.3
//!   ownership stamping on segment headers (`owner = heap id + generation`)
//!   and the M8/M9 coherence checks, and
//! - the **segment substrate** ([`AllocCore`]) that owns this heap's segments
//!   and performs all per-segment `BinTable` arithmetic.
//!
//! ## Phase 12.3 — allocation routes through `HeapCore`
//!
//! 12.3 wires `HeapCore::alloc`/`dealloc`/`realloc`/`alloc_zeroed` as the
//! entry points the raw-pointer TLS binding hands to the alloc face. They
//! delegate to the [`AllocCore`] (own-thread path). Under `alloc-xthread`,
//! [`HeapCore`] also carries the cross-thread `ThreadFreeStack` handle and
//! stamps `owner_thread_free` on segment headers so remote threads can route
//! cross-thread frees here (the §2.2 "owner stamping — 12.3" rule).
//!
//! ## M5-clean bootstrap invariant
//!
//! `HeapCore::new` bootstraps via [`AllocCore::new`] (OS aperture only —
//! `mmap`/`VirtualAlloc`, **never** `std::alloc`) and allocates NOTHING
//! else. In particular the cross-thread free-stack head is **not** an inline
//! `HeapCore` field and is **not** `Box`-allocated: task H1 hoisted it OUT of
//! `HeapCore` into the OWNING [`HeapSlot::thread_free`] (a `Sync`,
//! process-`'static` slot field, null-initialised in the registry bootstrap's
//! zeroed pages) — see the [`thread_free`](HeapCore::thread_free) field doc for
//! the full aliasing-gap rationale (a remote CAS onto an inline head would land
//! inside the owner's protected `&mut HeapCore` retag range). `HeapCore::new`
//! therefore leaves its [`thread_free`](HeapCore::thread_free) handle `None`;
//! the stable `&'static` handle to that slot word is planted right after the
//! slot binds by [`bind_thread_free`](HeapCore::bind_thread_free) — called from
//! `HeapRegistry::claim`, or from `fallback::heap_ptr` with `&FALLBACK_TFS` for
//! the standalone fallback heap. Because resolving that handle allocates
//! nothing, the bootstrap stays M5-clean (no `std::alloc` reach) and the
//! `#[global_allocator]` bind-path recursion an eager `Box::new` would have
//! caused remains impossible. In the transient pre-bind window
//! (`thread_free == None`, never observed on any alloc/free path) the heap
//! serves only own-thread allocations — cross-thread frees to its segments are
//! a safe no-op, matching the existing unstamped-segment behaviour in
//! `dealloc_small`.
//!
//! [`HeapSlot::thread_free`]: super::heap_slot::HeapSlot::thread_free
//!
//! ## `HeapCore` — the sole allocator face
//!
//! `HeapCore` is the slot-resident value the registry stores and the raw-
//! pointer TLS caches. The malloc face (`SeferAlloc`) routes through `HeapCore`
//! via the registry. (An earlier `Heap`/`with_heap` public face existed in
//! 0.3.0–0.3.x as a thin `AllocCore` wrapper without the magazine; it was
//! removed in 0.3.x — never used by the production fast path, ~9–12x slower
//! than mimalloc per `docs/HEAP_BENCH.md`. `HeapCore` is the magazine-backed
//! successor that supersedes it entirely.)

use crate::alloc_core::AllocCore;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;

// TEST-ONLY (0.3.0, task C1 → 0.4.x task #133): magazine (tcache) HIT
// counter. Originally a single process-wide `static AtomicU64`, bumped by
// EVERY thread's alloc fast path — a contended `lock xadd` on an otherwise
// fully per-thread hot path (the "churn hot path": pop from the magazine).
// Under MT this counter's cache line ping-pongs across cores on every
// magazine hit, adding cross-core traffic to a path that is architecturally
// per-thread (each `HeapCore` lives on one thread's registry slot — see the
// module doc). Perf regression #133.
//
// Fix: the counter is now a PER-HEAP field (`HeapCore::tcache_hits`, see
// below), incremented by its own owning thread only. Two threads' counters
// never share a cache line (each lives inside its own slot in the
// `'static` registry array), so the increment is a plain (uncontended)
// atomic RMW on ST and has NO cross-core traffic on MT — the contention is
// eliminated, not just made cheaper.
//
// It stays an `AtomicU64` (not a plain `u64`) because the process-global
// VIEW (`tcache_hits()` below, and `SeferAlloc::stats().tcache_hits`) reads
// EVERY live heap's counter from whatever thread calls `stats()` — a
// different thread than the owner in general. A plain `u64` written by one
// thread and read by another without synchronisation is a data race (UB,
// caught by TSan); `Relaxed` on both sides keeps this sound (no ordering
// requirement on a diagnostic counter — see the crate's existing
// `DBG_LARGE_XTHREAD_RECLAIMED` for the same relaxed-diagnostic pattern)
// while remaining `#![forbid(unsafe_code)]`-clean (no seam module needed —
// `AtomicU64` is safe-Rust top to bottom).
//
// TASK W3 (0.3.0) — the counter STORAGE moved out of `HeapCore` and into the
// owning `HeapSlot` (`HeapSlot::tcache_hits`), closing a formal aliasing gap.
// The old design put the `AtomicU64` INSIDE `HeapCore`; the process-wide
// aggregator (`tcache_hits_total`) then materialised a shared `&HeapCore`
// (`(*heap_ptr).tcache_hits()`) over a struct the OWNING thread concurrently
// holds a protected `&mut` into (the `alloc(&mut self, …)` protector) — a
// foreign-read of a protected `Unique`, UB under Stacked Borrows. Storing the
// counter in the `HeapSlot` (which is `Sync`, designed to be shared, and
// already read by the aggregator via `&HeapSlot` for `initialised`) lets the
// aggregator read it WITHOUT any `&HeapCore`. The owner reaches its slot's
// counter through the stable `*const AtomicU64` in the field below, planted by
// `HeapRegistry::claim` right after the slot is bound. See
// `HeapSlot::tcache_hits`.
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub(crate) type TcacheHitCounter = core::sync::atomic::AtomicU64;

// RAD-4 (Phase 4, E3a) — overflow-safe cross-thread small-block free.
//
// `RemoteFreeRing::push` (a per-segment, bounded MPSC queue — see that
// module's docs) returns `Err(PushOverflow)` when its `RING_CAP = 256` slots
// are all reserved-but-undrained. Before this task, BOTH push call sites in
// `dealloc_foreign_slow` below discarded that error (`let _ = ring.push(..)`)
// — a single overflow is a documented, sound, BOUNDED leak (the block stays
// mapped but unused), but a SUSTAINED producer→consumer fan-in (many remote
// threads freeing into one owner faster than the owner drains) turns that
// into an UNBOUNDED cumulative logical leak: every subsequent overflow drops
// another block, permanently, and `live_count` never returns to zero, which
// blocks `alloc-decommit` from ever releasing the segment.
//
// The ring's own push/drain/cursor protocol is NOT touched by this fix (out
// of scope per the RAD-4 task boundary, and the ring itself is fully sound —
// see `docs/RACE_DRAIN_RECLAIM.md`). Closing the leak at the SMALLEST
// protocol delta instead means changing what the CALLER does with
// `Err(PushOverflow)`: retry, rather than drop.
//
// ## Why retry (not a new Treiber stack of block-payload nodes)
//
// The plan's own precedent for a heap-level MPSC fallback is the A1
// deferred-large-free stack (`HeapCore::thread_free`, reused as a
// Treiber-stack head over segment BASES, chained through each segment's own
// `deferred_next` header field — see that field's doc comment). That idiom
// works because A1's payload IS a segment identity (the whole segment is the
// thing being queued) — no separate node storage is needed beyond the
// segment's own pre-reserved header bytes.
//
// This case is different: the lost item is one FREED BLOCK's `(offset,
// class)` pair, not a segment. Durably queuing that payload elsewhere would
// need one of:
//   - writing into the block's own bytes — reopens EXACTLY the H1-class UAF
//     this ring was built to close (see the module doc on
//     `RemoteFreeRing`: "a cross-thread freer never touches the block's
//     bytes" is the ring's core soundness argument);
//   - a new slot-resident (`HeapSlot`) head field, wired through
//     `HeapRegistry::claim`'s `bind_slot_counters` — out of this task's file
//     scope (`heap_registry.rs` is owned by a parallel task in this
//     session), and, independently, still needs per-BLOCK node storage
//     behind that head, which in turn needs either `Box::new` (the exact
//     `#[global_allocator]` reentrancy hazard `HeapCore`'s own module doc
//     warns against — "M5-clean bootstrap invariant") or a second in-segment
//     array (a `RemoteFreeRing`-shaped structure in a NEW location — a
//     segment-metadata layout change, explicitly out of scope for E3a: see
//     the implementation plan's Phase 4 vs. the gated, higher-risk Phase 7
//     dirty-segment queue);
//   - reusing `deferred_next` for LIVE Small segments too (it is unused
//     while a Small segment is live — the deferred-large stack chains only
//     Large segments) — rejected: making a field that is exclusively the
//     deferred-large (Large-segment) link also carry small-segment overflow
//     data introduces a field-sharing collision the UB audit already flags
//     as a latent, documented reactivation hazard (Finding 5 / M-7 in
//     `docs/reviews/2026-07-10-ub-audit-registry.md`); adding a SECOND
//     consumer of the same link field widens that hazard's blast radius
//     instead of leaving `deferred_next` with its single current owner.
//
// Retrying the push needs none of that: it adds no new struct, no new
// header field, no new slot wiring, and never touches block bytes or the
// ring's own cursor arithmetic. It is sound-by-construction (the ring stays
// exactly as correct as it already is) and closes the leak as long as the
// owner keeps draining — which it does on every `alloc()` call via
// `find_segment_with_free`'s lazy per-segment ring drain, the SAME liveness
// assumption every lazy-drain path in this allocator already relies on.
//
// ## Bound, not infinite spin
//
// An unconditionally infinite retry would make `dealloc()` able to block
// forever if the owner thread stops allocating entirely while producers are
// still freeing into it — a much bigger behavioural change for a
// `GlobalAlloc` face than this task's "smallest protocol delta" mandate
// accepts. `RING_PUSH_RETRY_SPINS` bounds the retry to a finite number of
// `core::hint::spin_loop()`-paced attempts (the same spin-hint idiom already
// used by `global::fallback::LockGuard`/`bootstrap`'s init-state spin). Within
// that bound, ANY owner drain (which empties up to the full `RING_CAP = 256`
// slots in one pass) reopens enough room for the retry to succeed — so the
// bound only matters for a truly pathological, sustained-overflow workload;
// the `remote_fanin` harness (`tests/remote_fanin.rs`) is the empirical judge
// of whether this bound is generous enough in practice.
//
// ## R6-OPT-P0-4 — overflow-first, spin last (current ordering)
//
// The paragraph above describes the SPIN window's own bound; the ORDER in
// which `push_with_overflow_retry` reaches for the three tiers (segment ring
// → heap-level `HeapOverflow` second-chance ring → this spin loop) changed
// under R6-OPT-P0-4. Originally (RAD-4/RAD-4b), the spin loop ran FIRST,
// retrying the segment ring for the FULL `RING_PUSH_RETRY_SPINS` budget
// before ever trying `HeapOverflow` — and every failed poll inside that
// budget ticked BOTH `RemoteFreeRing`'s diagnostic counters (`overflow()` +
// `DBG_RING_OVERFLOW`, each a locked RMW), so a single logical free landing
// on a saturated ring with a LIVE owner (the common case) could pay up to
// 8,193 full ring-state checks and 16,386 counter RMWs before ever trying the
// second-chance ring that was sitting right there the whole time with 8x the
// capacity (`HeapOverflow::HEAP_OVERFLOW_CAP` = 2048 vs. `RING_CAP` = 256).
// The policy is now: one counted `RemoteFreeRing::push` attempt, then
// IMMEDIATELY `push_to_heap_overflow` on failure — BEFORE any spinning — and
// only if BOTH fail does the spin loop below run. Every poll inside that loop
// retries BOTH tiers: the segment ring via `RemoteFreeRing::try_push_uncounted`
// (not `push`, so a failed poll does not re-tick either ring counter) AND the
// heap-level overflow ring via another `push_to_heap_overflow` call (retrying
// ONLY the ring inside the loop was tried first and measurably regressed the
// high-fan-in judge — see `HeapCore::push_with_overflow_retry`'s doc comment
// for the measured numbers). See that doc comment (`heap_core_xthread.rs`)
// for the full four-step policy and the `owner_slot_is_live` gate this
// reordering does NOT change. If the owner is not live (nothing to spin for
// on the ring), a single further `push_to_heap_overflow` attempt still runs
// (that ring is drained by whichever thread next claims the slot); only if
// every avenue fails does the push fall through to the original documented-
// sound bounded leak — `DBG_RING_PUSH_RETRY_EXHAUSTED` (below) counts ONLY
// this genuinely-unrecovered case, distinct from `DBG_RING_OVERFLOW` (which
// now ticks exactly ONCE per logical free that ever saw a full segment ring —
// the single counted attempt in step 1 — not on every retry poll, since the
// spin loop's ring polls are uncounted).
//
// ## Calibrated budget (task #99 / round4 finding R2)
//
// RAD-4's original value was 262,144 (2^18). The round4 review (finding R2)
// flagged this: under sustained fan-in, one logical free could burn up to
// ~524,288 atomic RMWs (each failed push attempt ticks both
// `DBG_RING_OVERFLOW` and the per-segment `overflow_count`) before conceding.
// Task #99 calibrated the actual need empirically via a live-owner fan-in
// sweep {1,2,8,32 producers} against `tests/remote_fanin.rs` as the judge:
//
// - A two-phase backoff shape (tight spin + exponential `spin_loop()` padding
//   between push attempts) was tried first — the idea being that the
//   atomic-storm cost scales with push-ATTEMPT count, not spin-HINT count.
//   REJECTED: the backoff gaps caused producers to MISS drain windows the
//   flat spin catches (a drain empties the full ring in one pass; if no push
//   attempt polls during that brief window, the capacity goes unused). Under
//   release-mode contention this lost blocks at just 2 producers
//   (`DBG_RING_PUSH_RETRY_EXHAUSTED` > 0). The flat spin IS the polling
//   mechanism — backoff breaks it.
// - A flat budget of 8,192 (= 32 × `RING_CAP = 256`, giving the owner ~32
//   drain opportunities) was measured sufficient: `DBG_RING_PUSH_RETRY_
//   EXHAUSTED` stays at 0 across the full sweep (verified in BOTH debug and
//   release, 3 consecutive runs each, plus an isolated 32-producer release
//   check). The review's suggested "8-32 attempts" was measured far too
//   small for this codebase's ring/drain geometry.
// - The 32× budget cut (262,144 → 8,192) reduces the worst-case per-overflow
//   atomic-storm cost proportionally, and makes `remote_fanin`'s debug test
//   ~27× faster (37.8s → 1.4s) — the retry path is exercised identically,
//   just with a shorter spin.
// Under miri (interpreted execution, orders of magnitude slower than
// native), the full retry budget makes an overflow-heavy test impractically
// slow — a single genuinely-exhausted retry loop at the native bound was
// measured to make `cargo +nightly miri test` on `tests/remote_fanin.rs`
// not finish in a reasonable time. `#[cfg(miri)]` narrows the bound to a
// small but still-meaningful value (large enough to exercise the loop body,
// the atomic CAS retry, and both counter-increment branches; small enough
// for miri's interpreter to reach retry exhaustion in a bounded test)
// WITHOUT changing the retry protocol/logic itself — the exact same idiom
// `alloc_core_small.rs`/`bootstrap.rs` already use elsewhere in this crate
// for miri-only initialisation gates (`grep -rn 'cfg(miri)' src/`), applied
// here to a workload-size constant instead. Real (non-miri) builds are
// completely unaffected.
#[cfg(all(feature = "alloc-xthread", not(miri)))]
pub(super) const RING_PUSH_RETRY_SPINS: u32 = 8_192;
#[cfg(all(feature = "alloc-xthread", miri))]
pub(super) const RING_PUSH_RETRY_SPINS: u32 = 64;

/// TEST/DIAGNOSTIC-ONLY (RAD-4, task E3a; reordered by R6-OPT-P0-4): process-
/// wide count of small-block ring pushes that reached the BOUNDED SPIN-RETRY
/// tier (i.e. both the initial counted `RemoteFreeRing::push` AND an
/// immediate `push_to_heap_overflow` attempt already failed — see
/// `HeapCore::push_with_overflow_retry`'s doc comment for the current
/// four-step policy) and EVENTUALLY succeeded within
/// [`RING_PUSH_RETRY_SPINS`]. Bumped exactly ONCE per push that took this
/// path and recovered — not per spin-loop poll (the loop's own polls use
/// `RemoteFreeRing::try_push_uncounted`, which ticks no counter on failure).
/// A non-zero value means the fan-in pressure was high enough to
/// double-saturate BOTH the segment ring and the heap-level overflow ring
/// transiently, but the retry recovered every one of those blocks. Relaxed:
/// diagnostic only, like `DBG_LARGE_XTHREAD_RECLAIMED` / `DBG_RING_OVERFLOW`.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub static DBG_RING_PUSH_RETRIED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// TEST/DIAGNOSTIC-ONLY (RAD-4, task E3a; reordered by R6-OPT-P0-4): process-
/// wide count of small-block ring pushes for which EVERY tier of the fallback
/// chain failed — the initial counted `RemoteFreeRing::push`, the immediate
/// `push_to_heap_overflow` attempt, the full `RING_PUSH_RETRY_SPINS` bounded
/// spin-retry (uncounted polls), AND a final `push_to_heap_overflow` retry
/// after the spin budget ran out — the genuinely-unrecovered residual of the
/// original bounded-leak behaviour. Distinct from
/// [`crate::alloc_core::remote_free_ring::DBG_RING_OVERFLOW`], which (as of
/// R6-OPT-P0-4) ticks exactly ONCE per logical free that ever saw a full
/// segment ring (the single counted attempt in step 1 of
/// `push_with_overflow_retry`), not on every retry poll — a `remote_fanin`-
/// style harness asserts this stays at (or very near) zero to demonstrate the
/// fix; a non-zero value here — not just a non-zero `DBG_RING_OVERFLOW` — is
/// the honest signal of an actual lost block under this fix.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub static DBG_RING_PUSH_RETRY_EXHAUSTED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// The thin, slot-resident heap value.
///
/// Lives inside a [`HeapSlot`](super::heap_slot::HeapSlot)'s `UnsafeCell` and
/// is handed out to a thread via
/// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) as a
/// `*mut HeapCore`. Single-writer invariant (the owning thread is the only
/// mutator of its heap's bins) makes the `UnsafeCell` sound.
pub struct HeapCore {
    /// The owning slot's index in the registry. Used by
    /// [`recycle`](super::heap_registry::HeapRegistry::recycle) to find the
    /// slot back from a `*mut HeapCore` (12.3 stamps this into segment
    /// headers as the ownership key).
    /// `u32::MAX` is reserved as "not yet bound to a slot" (a freshly-init'd
    /// slot has `id = u32::MAX` until `claim` overwrites it).
    pub(crate) id: u32,
    /// The segment substrate this heap owns. Owns the primordial + any
    /// additionally-reserved small/large segments. Phase 12.1: free-list
    /// state lives in each segment's `BinTable`, so this is the heap's entire
    /// small-allocation engine.
    pub(crate) core: AllocCore,
    /// Stable `&'static` handle to the cross-thread free-stack head / identity
    /// stamp — which, task H1, now lives in the OWNING
    /// [`HeapSlot::thread_free`](super::heap_slot::HeapSlot::thread_free)
    /// (a `Sync`, process-`'static` slot field) rather than INLINE in this
    /// `HeapCore`.
    ///
    /// **Why it moved out of `HeapCore` (task H1 — the W3 hoist, repeated).**
    /// The head is CASed by REMOTE threads (a cross-thread free of a Large
    /// segment → [`push_large_deferred_free`](Self::push_large_deferred_free) →
    /// a `compare_exchange` reconstructed through EXPOSED provenance). When the
    /// `AtomicPtr<u8>` lived INSIDE `HeapCore`, that foreign write landed inside
    /// the byte range of the owner's protected `&mut HeapCore` (materialised on
    /// EVERY `alloc`/`dealloc`) — a protector / data-race violation under
    /// Stacked/Tree Borrows, empirically confirmed by miri (a retag-write vs.
    /// atomic-load data race between the owner's
    /// `stamp_segment_owner(&mut self)` fn-entry retag and the remote's
    /// `head.load()`). This is EXACTLY the conflict class W3 already paid to fix
    /// for the diagnostic counters (there a foreign READ; here a foreign WRITE,
    /// strictly stronger). See
    /// `tests/regression_xthread_thread_free_alias_miri.rs` for the reproducer.
    ///
    /// Storing the head in the `HeapSlot` (shared by design, `Sync`) removes it
    /// from every `&mut HeapCore` retag range: the owner reaches it through this
    /// `&'static` handle (planted at
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) time,
    /// exactly like [`tcache_hits`](Self::tcache_hits)), and remote freers reach
    /// the SAME slot word through the `owner_thread_free_at(base)` segment-header
    /// stamp — which now stores the slot field's stable `'static` address.
    ///
    /// **M5-clean, no recursion.** Like the old inline field, resolving this
    /// handle allocates NOTHING (the slot's `thread_free` is null-initialised in
    /// the registry bootstrap's zeroed pages), so the `#[global_allocator]`
    /// bind-path recursion the inline field was introduced to avoid
    /// (`Box::new` → `SeferAlloc::alloc` → …) remains impossible.
    ///
    /// **Dual role, unchanged.** The head's ADDRESS is this heap's identity
    /// token, compared by `dealloc_routing` (`owner_thread_free_at(base) == our
    /// head` — an address compare that never dereferences the value); its VALUE
    /// (`AtomicPtr<u8>`) is the head of this heap's deferred-free Treiber stack
    /// over Large segment BASES (0.3.0 task A1 — reclaims cross-thread-freed
    /// Large segments, drained on the owner's `alloc_large` slow path via
    /// [`drain_large_deferred_free`](Self::drain_large_deferred_free)). The two
    /// uses touch disjoint parts of the same word. Single-consumer (only the
    /// owner pops), multi-producer (any remote may push) — a plain CAS-loop
    /// push needs no ABA tag. `null` VALUE = empty stack.
    ///
    /// ⚠️ The `deferred_next` header field this stack reuses as its intrusive
    /// link was historically shared with the (now-removed) abandoned-segments
    /// stack — see the "ABA defence" note in `heap_registry.rs`. With that
    /// substrate gone (task #97 / R4-5), this stack is the SOLE user of
    /// `deferred_next`, so the field-sharing collision it warned about is no
    /// longer possible.
    ///
    /// Stored as a SAFE `Option<&'static _>` (not a raw pointer): this module is
    /// `#![deny(unsafe_code)]`, so a raw-pointer deref would be a hard error.
    /// The `&'static` is minted by `HeapRegistry::claim` / `fallback::heap_ptr`
    /// (both in unsafe-permitted seams) from the owning slot's / the
    /// `FALLBACK_TFS` static's `AtomicPtr`. `None` only in the transient
    /// pre-bind window (never observed on any alloc/free path — every stamp /
    /// drain / push runs only after the handle was planted).
    ///
    /// Only present under `alloc-xthread` (the cross-thread feature).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) thread_free: Option<&'static AtomicPtr<u8>>,

    /// RAD-4b (task #72): stable `&'static` handle to THIS heap's
    /// slot-resident [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// second-chance ring. Planted by
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// (via `bind_slot_counters` → [`bind_overflow`](Self::bind_overflow)),
    /// mirroring [`thread_free`](Self::thread_free) /
    /// [`tcache_hits`](Self::tcache_hits) exactly — same rationale: resolving
    /// `&reg.slots[idx].overflow` fresh on every
    /// [`drain_heap_overflow`](Self::drain_heap_overflow) call (a
    /// `bootstrap::ensure()` + array index) is strictly more work than a
    /// pre-resolved `'static` reference, and this field is on the
    /// magazine-MISS refill path — see `IAI_BASELINE.md`'s RAD-4b entry for
    /// the measured churn-gate cost this hoist recovers. `None` only in the
    /// transient pre-bind window (never observed on any alloc/free path —
    /// `drain_heap_overflow`/`push_to_heap_overflow` are the only readers,
    /// and both run only on/after a claimed heap).
    ///
    /// `push_to_heap_overflow` is a free function called from a REMOTE
    /// thread targeting `base`'s OWNER — a different heap than `self` — so it
    /// cannot use this field (which is `self`'s OWN handle); it still
    /// resolves the owner's slot via `bootstrap::ensure().slots[owner_id]`
    /// (unavoidable — the whole point is finding a heap this thread does not
    /// own). This hoist applies ONLY to the OWNER's own opportunistic drain,
    /// the hot(ter) path the churn benches actually measure.
    #[cfg(feature = "alloc-xthread")]
    pub(super) overflow: Option<&'static super::heap_overflow::HeapOverflow>,

    /// Per-thread, per-class magazine cache (Phase P2 — fastbin).
    /// Gated on `alloc-global + fastbin`. Owner-private (single-writer):
    /// only the owning thread touches it. See `registry::tcache`.
    ///
    /// ## D1 invariant (Phase 5/P5)
    ///
    /// A magazine-resident block COUNTS AS LIVE for the purposes of
    /// `live_count` / decommit. The invariant chain:
    ///   - refill_class pulls via alloc_small → inc_live per block.
    ///   - magazine push/pop do NOT touch live_count.
    ///   - magazine flush calls dealloc_small → dec_live → maybe_decommit.
    ///
    /// So `live_count` = blocks carved AND not on a BinTable free list
    /// = (blocks handed out to user) + (blocks in our magazine). Decommit
    /// fires only when a segment's blocks are ALL on the BinTable free
    /// list (none handed out, none in magazine).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache: super::tcache::Tcache,

    /// TEST/DIAGNOSTIC-ONLY (task C1 → #133 → W3): stable handle to THIS
    /// heap's magazine HIT counter, which now lives in the owning
    /// [`HeapSlot::tcache_hits`](super::heap_slot::HeapSlot::tcache_hits) — a
    /// `Sync`, process-`'static` slot — rather than inline in this `HeapCore`.
    /// See the module-level comment above [`TcacheHitCounter`] for the full
    /// aliasing-gap rationale (task W3: an aggregator materialising a shared
    /// `&HeapCore` over a struct another thread holds a protected `&mut` into
    /// is UB under Stacked Borrows).
    ///
    /// Planted by [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// immediately after the slot is bound (`bind_counters`): it points at the
    /// slot's `AtomicU64`. Because the slot lives in the `'static` registry
    /// array, this pointer is sound for the slot's (process) lifetime and is
    /// never re-pointed. `null` only in the transient window before the first
    /// bind (never observed on any alloc path — `alloc` runs only after
    /// `claim` planted it); the increment/read helpers treat `null` as "no
    /// counter" defensively.
    ///
    /// The increment (owner-only, single writer) and the cross-thread
    /// aggregate read both go through the SAME slot `AtomicU64` (Relaxed),
    /// so `HeapCore::tcache_hits()` and `tcache_hits_total()` agree.
    ///
    /// Stored as a SAFE `Option<&'static _>` (not a raw pointer): this module
    /// is `#![deny(unsafe_code)]` with no local `allow`, so a raw-pointer
    /// deref would be a hard error. The `&'static` is minted by
    /// `HeapRegistry::claim` (which lives in the unsafe-permitted registry
    /// seam) from the slot's counter and planted here — a shared reference to
    /// a process-`'static` atomic, entirely sound to hold and read/write from
    /// the owning thread. `None` only in the transient pre-bind window (never
    /// observed on an alloc path).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) tcache_hits: Option<&'static TcacheHitCounter>,

    /// OPT-C (task #66): lazy stamp cache.
    ///
    /// The base address of the last segment for which this heap successfully
    /// ran `stamp_segment_owner`. On the next alloc from the SAME segment the
    /// cache-hit fast path performs only a Relaxed load of `owner_state` (to
    /// confirm ownership is still ours) instead of a full Acquire-load +
    /// conditional Release-store. This eliminates the costly Release-store on
    /// the 99 % of allocations that stay in the hot segment.
    ///
    /// **Null** means "no segment cached yet" (initial state).
    ///
    /// **Cache invalidation safety:**
    /// - *Segment migration* — when the active segment changes (new small
    ///   segment carved, large-segment alloc) `base != last_stamped_segment`
    ///   → cache miss → slow path stamps and updates the cache.
    /// - *Segment recycle / decommit* — a recycled segment may reuse the same
    ///   base address. The Relaxed-load in the fast path re-reads `owner_state`
    ///   and compares against `self.id`; if the segment was recycled and its
    ///   `owner_state` reset to `OWNER_ID_NONE`, the comparison fails → slow
    ///   path re-stamps.
    /// - *Inter-heap segment transfer* — if a future phase introduces
    ///   transferring segments between heaps, the code doing the transfer MUST
    ///   null this cache field (`last_stamped_segment = null`) so the stale
    ///   entry is cleared before the next alloc. Currently no such path exists
    ///   (the shard model: each segment stays with its original heap forever).
    ///
    /// Only present under `alloc-global` (the feature that enables
    /// `stamp_segment_owner`).
    #[cfg(feature = "alloc-global")]
    pub(super) last_stamped_segment: *mut u8,

    /// RAD-4b (task #72): owner-private cache of the last `tail` value
    /// observed on this heap's slot-resident `HeapOverflow` ring, refreshed
    /// from [`HeapOverflow::drain`]'s return value. Lets
    /// [`drain_heap_overflow`](Self::drain_heap_overflow) skip the full
    /// Acquire-pair drain protocol (and its unconditional `head.store`) with
    /// a single `Relaxed` load via
    /// [`HeapOverflow::is_likely_empty`](super::heap_overflow::HeapOverflow::is_likely_empty)
    /// on the overwhelmingly common "nothing ever overflowed into this ring"
    /// case — mirrors the OPT-C `last_stamped_segment` cache immediately
    /// above and `RemoteFreeRing`'s own documented `is_likely_empty`
    /// caller-cached-`head` idiom (PERF-PASS-4 G9/C2), adapted to cache
    /// `tail` here (the field a REMOTE push moves) since the OWNER is the
    /// sole writer of `head`/reader of `tail`'s progress via this cache.
    /// Starts at `0` (matches `HeapOverflow`'s all-zero initial `tail`).
    #[cfg(feature = "alloc-xthread")]
    pub(super) overflow_tail_cache: usize,
}

impl HeapCore {
    /// Construct a fresh heap value bound to slot `id`. Bootstraps the
    /// segment substrate via [`AllocCore::new`] (which goes through the OS
    /// aperture — `mmap`/`VirtualAlloc` — and never `std::alloc`, upholding
    /// M5). Returns `None` only on primordial OOM (the OS refused the
    /// reservation).
    ///
    /// **M5-clean:** this performs NO `std::alloc`. The cross-thread TFS
    /// handle ([`thread_free`](Self::thread_free)) is `None` here; the stable
    /// `&'static` handle to the OWNING slot's `thread_free` word (or
    /// `FALLBACK_TFS` for the fallback heap) is planted separately by
    /// [`bind_thread_free`](Self::bind_thread_free), called from
    /// `HeapRegistry::claim` / `fallback::heap_ptr` right after the slot binds
    /// and before any allocation on this heap runs. That binding also allocates
    /// nothing (task H1 hoisted the head out of `HeapCore`; there is no `Box`),
    /// so the whole path stays M5-clean.
    ///
    /// Called lazily by [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// when it transitions a slot `FREE → LIVE` and needs to materialise the
    /// heap value in the slot's `UnsafeCell`.
    #[must_use]
    pub(crate) fn new(id: u32) -> Option<Self> {
        let core = AllocCore::new()?;
        Some(Self {
            id,
            core,
            // task H1: the head now lives in the owning slot; this handle is
            // planted by `HeapRegistry::claim` (via `bind_thread_free`) —
            // or by `fallback::heap_ptr` for the fallback heap — right after
            // the slot binds. `None` until then (never observed on any
            // alloc/free path — stamping/draining/pushing all run only after
            // the handle was planted).
            #[cfg(feature = "alloc-xthread")]
            thread_free: None,
            // RAD-4b: planted by `bind_slot_counters` → `bind_overflow`
            // right after the slot binds — see the field's doc comment.
            #[cfg(feature = "alloc-xthread")]
            overflow: None,
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: super::tcache::Tcache::new(),
            // W3: the counter now lives in the owning HeapSlot; this handle
            // is planted by `HeapRegistry::claim` (via `bind_tcache_hits`)
            // right after the slot binds. `None` until then (never observed on
            // any alloc path — alloc runs only after claim planted it).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: None,
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: core::ptr::null_mut(),
            // RAD-4b: matches `HeapOverflow`'s all-zero initial `tail`.
            #[cfg(feature = "alloc-xthread")]
            overflow_tail_cache: 0,
        })
    }

    /// Construct a fresh heap value bound to slot `id`, using `config` to
    /// tune the large-segment free-cache. Only present under `alloc-decommit`.
    ///
    /// Identical to [`new`](Self::new) except it calls
    /// [`AllocCore::new_with_config`] so per-thread large-cache behaviour
    /// matches the compile-time `SeferAlloc::with_config(...)` choice.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub(crate) fn new_with_config(
        id: u32,
        config: crate::alloc_core::LargeCacheConfig,
    ) -> Option<Self> {
        let core = AllocCore::new_with_config(config)?;
        Some(Self {
            id,
            core,
            // task H1: the head now lives in the owning slot; this handle is
            // planted by `HeapRegistry::claim` (via `bind_thread_free`) —
            // or by `fallback::heap_ptr` for the fallback heap — right after
            // the slot binds. `None` until then (never observed on any
            // alloc/free path — stamping/draining/pushing all run only after
            // the handle was planted).
            #[cfg(feature = "alloc-xthread")]
            thread_free: None,
            // RAD-4b: planted by `bind_slot_counters` → `bind_overflow`
            // right after the slot binds — see the field's doc comment.
            #[cfg(feature = "alloc-xthread")]
            overflow: None,
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache: super::tcache::Tcache::new(),
            // W3: the counter now lives in the owning HeapSlot; this handle
            // is planted by `HeapRegistry::claim` (via `bind_tcache_hits`)
            // right after the slot binds. `None` until then (never observed on
            // any alloc path — alloc runs only after claim planted it).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: None,
            #[cfg(feature = "alloc-global")]
            last_stamped_segment: core::ptr::null_mut(),
            // RAD-4b: matches `HeapOverflow`'s all-zero initial `tail`.
            #[cfg(feature = "alloc-xthread")]
            overflow_tail_cache: 0,
        })
    }

    /// The slot index this heap is bound to. Read by `recycle` to locate the
    /// owning slot from a `*mut HeapCore`.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Iterate over the segment bases this heap owns (read-only). Delegates to
    /// the substrate's segment-table iterator.
    ///
    /// `#[doc(hidden)] pub` so integration tests can obtain a real segment
    /// base (the test-only pub surface of the registry, documented in
    /// `mod.rs`).
    #[doc(hidden)]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> {
        self.core.segment_bases()
    }

    /// Compare this heap's live (resolved) cache/pool policy against a
    /// requested config. Forwards to `AllocCore::live_config_matches`.
    /// Used by `HeapRegistry::claim_with_config` (N2) to detect a config
    /// mismatch on a recycled, already-materialised slot.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn live_config_matches(
        &self,
        requested: &crate::alloc_core::LargeCacheConfig,
    ) -> bool {
        self.core.live_config_matches(requested)
    }
}
