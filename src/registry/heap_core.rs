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

use core::alloc::Layout;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::pack_owner;
#[cfg(feature = "hardened")]
use crate::alloc_core::segment_header::SegmentKind;
#[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

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
// The plan's own precedent for a heap-level MPSC fallback is `abandoned_segs`
// / the A1 deferred-large-free stack (`HeapCore::thread_free`, reused as a
// Treiber-stack head over segment BASES, chained through each segment's own
// `next_abandoned` header field — see that field's doc comment). That idiom
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
//   - reusing `next_abandoned` for LIVE Small segments too (it is unused
//     while a Small segment is live) — rejected: the UB audit already flags
//     `next_abandoned` sharing between `abandoned_segs` and A1 as a latent,
//     documented reactivation hazard (Finding 5 / M-7 in
//     `docs/reviews/2026-07-10-ub-audit-registry.md`); adding a THIRD
//     consumer of the same link field on LIVE segments widens that hazard's
//     blast radius instead of leaving it exactly as dormant as it is today.
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
// of whether this bound is generous enough in practice. If the bound is
// exhausted, the push falls through to RAD-4b's `HeapOverflow` second-chance
// ring (and only if THAT is also saturated, the original documented-sound
// bounded leak) — `DBG_RING_PUSH_RETRY_EXHAUSTED` (below) counts ONLY this
// genuinely-unrecovered case, distinct from `DBG_RING_OVERFLOW` (which ticks
// on every individual full-ring push ATTEMPT, including ones a retry goes on
// to recover).
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

/// TEST/DIAGNOSTIC-ONLY (RAD-4, task E3a): process-wide count of small-block
/// ring pushes that retried at least once (i.e. hit `Err(PushOverflow)` on
/// their first attempt) and EVENTUALLY succeeded within
/// [`RING_PUSH_RETRY_SPINS`]. A non-zero value means the fan-in pressure was
/// high enough to transiently fill a ring, but the retry recovered every one
/// of those blocks — the leak this task closes. Relaxed: diagnostic only,
/// like `DBG_LARGE_XTHREAD_RECLAIMED` / `DBG_RING_OVERFLOW`.
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub static DBG_RING_PUSH_RETRIED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// TEST/DIAGNOSTIC-ONLY (RAD-4, task E3a): process-wide count of small-block
/// ring pushes that exhausted [`RING_PUSH_RETRY_SPINS`] retries WITHOUT the
/// ring ever draining enough to accept the push — the genuinely-unrecovered
/// residual of the original bounded-leak behaviour. Distinct from
/// [`crate::alloc_core::remote_free_ring::DBG_RING_OVERFLOW`], which counts
/// every individual full-ring push attempt (including ones a retry later
/// recovers). A `remote_fanin`-style harness asserts this stays at (or very
/// near) zero to demonstrate the fix; a non-zero value here — not just a
/// non-zero `DBG_RING_OVERFLOW` — is the honest signal of an actual lost
/// block under this fix.
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
    /// The owning slot's index in the registry. Used by `recycle`/`abandon`
    /// to find the slot back from a `*mut HeapCore` (12.3 stamps this into
    /// segment headers as the ownership key).
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
    /// ⚠️ The `next_abandoned` header field this stack reuses as its intrusive
    /// link was historically shared with the (now-removed) abandoned-segments
    /// stack — see the "ABA defence" note in `heap_registry.rs`. With that
    /// substrate gone (task #97 / R4-5), this stack is the SOLE user of
    /// `next_abandoned`, so the field-sharing collision it warned about is no
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
    /// **Null** means "no segment cached yet" (initial state, or after
    /// [`reset_stamp_cache`](Self::reset_stamp_cache) was called).
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
    ///   call `reset_stamp_cache()` so the stale cache entry is cleared before
    ///   the next alloc. Currently no such path exists (the shard model: each
    ///   segment stays with its original heap forever). See the doc on
    ///   `reset_stamp_cache`.
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

    /// TEST/DIAGNOSTIC-ONLY (task #133): this heap's own magazine-hit count.
    /// Relaxed load of [`tcache_hits`](Self::tcache_hits) — sound for a
    /// cross-thread diagnostic read (see the field's doc comment). Used by
    /// [`super::heap_registry::tcache_hits_total`] to aggregate across every
    /// LIVE slot into the process-wide view `stats()` exposes.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[doc(hidden)]
    #[must_use]
    pub fn tcache_hits(&self) -> u64 {
        // W3: read THIS heap's counter out of its owning slot (via the stable
        // `&'static` handle planted by `claim`). Reads the SAME `AtomicU64`
        // the aggregator reads, so per-heap and process-wide views agree.
        // `None` only in the pre-bind window (never on an alloc path) — 0.
        self.tcache_hits.map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// W3: plant the stable handle to THIS heap's slot-resident magazine
    /// (tcache) hit counter. Called by
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) once,
    /// right after the slot is bound (and the `HeapCore` materialised), before
    /// any allocation on this heap runs. `counter` is a `&'static` reference to
    /// the owning slot's `tcache_hits`. Idempotent — on a slot re-claim the
    /// handle already references the same `'static` slot counter, so
    /// re-planting is a harmless no-op store.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn bind_tcache_hits(&mut self, counter: &'static TcacheHitCounter) {
        self.tcache_hits = Some(counter);
    }

    /// W3: plant the stable handle to THIS heap's slot-resident large-segment
    /// cache hit counter (forwarded into the inner `AllocCore`). Same contract
    /// as [`bind_tcache_hits`](Self::bind_tcache_hits). Gated on
    /// `alloc-decommit` (independent of `fastbin`), mirroring
    /// `AllocCore::large_cache_hits`'s gate.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn bind_large_cache_hits(
        &mut self,
        counter: &'static core::sync::atomic::AtomicU64,
    ) {
        self.core.bind_large_cache_hits(counter);
    }

    /// task H1: plant the stable `&'static` handle to THIS heap's slot-resident
    /// (or fallback-static) cross-thread free-stack head. Called once, right
    /// after the slot / fallback heap is materialised and before any allocation
    /// on this heap runs, by
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// (via `bind_slot_counters`) / `fallback::heap_ptr`. Idempotent — on a
    /// slot re-claim the handle already references the same `'static` word, so
    /// re-planting is a harmless no-op store. Same discipline as
    /// [`bind_tcache_hits`](Self::bind_tcache_hits).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn bind_thread_free(&mut self, head: &'static AtomicPtr<u8>) {
        self.thread_free = Some(head);
    }

    /// RAD-4b (task #72): plant the stable `&'static` handle to THIS heap's
    /// slot-resident [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// ring. Same discipline as [`bind_thread_free`](Self::bind_thread_free) /
    /// [`bind_tcache_hits`](Self::bind_tcache_hits) — called once, right
    /// after the slot binds, from `bind_slot_counters`.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn bind_overflow(&mut self, overflow: &'static super::heap_overflow::HeapOverflow) {
        self.overflow = Some(overflow);
    }

    /// The stable `*const AtomicPtr<u8>` head pointer of this heap's TFS, or
    /// null in the transient pre-bind window (no cross-thread stamping has
    /// happened yet → cross-thread frees to this heap's segments are a safe
    /// no-op). Used by the drain / routing paths on the owning thread. task H1:
    /// resolves to the OWNING slot's `thread_free` word (via the `&'static`
    /// handle), NOT an inline `HeapCore` field — so the returned address is
    /// outside every `&mut HeapCore` retag range.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn thread_free_head(&self) -> *const AtomicPtr<u8> {
        self.thread_free
            .map_or(core::ptr::null(), |h| h as *const AtomicPtr<u8>)
    }

    // -----------------------------------------------------------------------
    // Allocation entry points (12.3). Delegate to the substrate; under
    // `alloc-xthread` also drain the TFS and stamp segment ownership.
    // -----------------------------------------------------------------------

    /// Allocate `layout.size()` bytes satisfying `layout.align()`. Returns a
    /// non-null `*mut u8` on success, or null on OOM. Memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`).
    ///
    /// Own-thread path: delegates to [`AllocCore::alloc`] (the single-thread
    /// substrate, no adoption hook — a heap owns its segments exclusively and
    /// never pulls in segments from other heaps). Under `alloc-xthread`,
    /// cross-thread frees that targeted this heap's segments sit in each
    /// segment's [`RemoteFreeRing`](crate::alloc_core::remote_free_ring) and are
    /// reclaimed LAZILY by [`AllocCore::find_segment_with_free`] on a free-list
    /// miss (it drains every owned segment's ring via `reclaim_offset`, which
    /// trusts the class carried in the ring entry — never the owner's `page_map`,
    /// unreliable for mixed-class pages, §13). This is the `ShardedRegion` 7b
    /// shard-reuse discipline; everything else is single-writer (this thread
    /// owns the slot, ergo its segments).
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Mirrors the RemoteFreeRing's
        // lazy-drain discipline (see the comment block below) but is scoped
        // to ONLY the Large-request case (checked here, not inside
        // `AllocCore`, because `AllocCore` has no `HeapCore` back-reference
        // to drain from) — small-classified requests pay zero cost for this
        // check beyond the one `class_for` call already needed below.
        // Э9 (P7.1, task #160): classify ONCE. `size`, `align` and
        // `class_for(size, align)` are pure functions of `layout`; they were
        // previously computed TWICE per alloc under production (once in the
        // xthread Large-drain check, once in the fastbin magazine-routing
        // block). We compute them a single time here and thread the result
        // through both consumers. The binding is gated on `any(...)` so it
        // exists whenever EITHER consumer is compiled in, and each consuming
        // block stays behind its own cfg. Behaviour is byte-identical
        // (`class_for` is pure → same index; the A1 Large-drain fires for
        // exactly the same Large-classified layouts).
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let align = layout.align();
        #[cfg(any(
            feature = "alloc-xthread",
            all(feature = "alloc-global", feature = "fastbin")
        ))]
        let class = crate::alloc_core::size_classes::SizeClasses::class_for(size, align);

        // 0.3.0 (task A1): drain this heap's cross-thread Large-segment
        // deferred-free stack before a Large-classified request reaches
        // `AllocCore::alloc_large`'s slow path. Uses the single `class`
        // computed above (Large ⇔ `class.is_none()`).
        #[cfg(feature = "alloc-xthread")]
        {
            if class.is_none() {
                self.drain_large_deferred_free();
            }
        }

        // RAD-4b (task #72): opportunistically drain this heap's
        // slot-resident `HeapOverflow` second-chance ring — see
        // `push_to_heap_overflow`'s doc comment for the full design. Under
        // `fastbin`, the drain is placed INSIDE `refill_magazine_slow`
        // instead (a `#[cold] #[inline(never)]` magazine-MISS-only path —
        // see that function), so the magazine-HIT fast path this file's own
        // churn benchmarks measure pays NOTHING extra: adding an unconditional
        // two-atomic-load check here, ahead of the magazine fast path below,
        // would tax every alloc including hits, which is exactly the
        // hot-path leak the task's iai gate exists to catch. Builds WITHOUT
        // `fastbin` have no magazine and hence no `refill_magazine_slow`
        // cold-path hook, so for them this call is the only opportunistic
        // site — unconditional here, but that configuration has no magazine
        // fast path to protect in the first place.
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        {
            self.drain_heap_overflow();
        }

        // Cross-thread-freed blocks are reclaimed LAZILY, inside
        // `AllocCore::find_segment_with_free` (the alloc-slow-path drains each
        // owned segment's `RemoteFreeRing` → `reclaim_offset`). We do NOT drain
        // eagerly on every alloc: that was a redundant deviation from the
        // `ShardedRegion` lazy discipline, and draining-before-alloc under a
        // real allocation workload (the installed `#[global_allocator]` serving
        // libtest's own cross-thread frees) corrupted the free list, while the
        // lazy slow-path drain handles the identical workload correctly
        // (verified: `global_alloc_installed` + `race_repro` ×5). Reclaim
        // completeness is preserved — the owner drains a segment's ring the
        // moment it needs a free block from it; until then cross-thread frees
        // sit in the bounded ring (overflow → bounded leak, the original 7b
        // discipline).

        // ── Magazine fast path (P2+P4, fastbin) ─────────────────────────
        // Small-class allocations are served from the per-thread magazine.
        // On a hit: array pop, return — NO per-alloc stamp (P4 hoist).
        // On a miss: batch-refill via `refill_class_stamped` (stamps each
        // distinct source segment exactly once inside the refill), then pop
        // one. The large path still stamps per-alloc (it does not go
        // through the magazine/refill).
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        {
            // Э9 (P7.1): `size`, `align`, `class` come from the single
            // classification hoisted above — no recompute here.
            // C1 (0.3.0): the magazine fast path used to be gated on
            // `align <= SMALL_ALIGN_MAX` (16), so every align>16 request
            // (tokio `Cell` at align=128, page-aligned buffers, etc.) fell
            // through to the substrate on EVERY alloc/dealloc, bypassing the
            // magazine entirely. This is unnecessary: `class_for(size, align)`
            // already guarantees (for any `Some(c)` it returns) that
            // `block_size(c) % align == 0` — see its divisibility-walk slow
            // path in `size_classes.rs`. Every block carved for class `c` sits
            // at an offset that is a multiple of `block_size(c)` (see
            // `carve_block`'s `align_up(bump, block_size)`), and the segment
            // itself is 4 MiB (SEGMENT)-aligned, so any block of class `c` is
            // automatically `align`-aligned regardless of what `align` was —
            // the SAME guarantee the substrate's own `alloc_small` relies on.
            // Keying the magazine purely by `class_idx` (derived from the
            // caller-supplied `Layout` on both alloc and dealloc, per the
            // `GlobalAlloc` contract) is therefore sound for any align that
            // `class_for` accepted. Cross-thread routing is unaffected: this
            // whole block is the OWN-THREAD path (`dealloc_routing` decides
            // ownership BEFORE reaching `dealloc_own_thread`/the magazine).
            {
                if let Some(c) = class {
                    let cnt = self.tcache.classes[c].count as usize;
                    if cnt > 0 {
                        // Magazine hit: pop from the top of the stack.
                        // P4: NO stamp here — the block's source segment was
                        // already stamped during the refill that originally
                        // pulled it. The OPT-C cache guarantees the segment
                        // header still carries our ownership.
                        let new_cnt = cnt - 1;
                        self.tcache.classes[c].count = new_cnt as u8;
                        // Э5 (task #145): load+store instead of `fetch_add` — no
                        // `lock xadd` on the churn hot path. SOUND because this
                        // thread is the SOLE WRITER of ITS OWN counter: it is a
                        // per-heap/per-slot counter and this magazine-hit path
                        // runs only on the owning thread (the single-writer
                        // invariant `tls_heap.rs` establishes — `current_for_alloc`
                        // yields `Own(&mut HeapCore)` only to the thread that won
                        // the slot's claim CAS). No other thread ever increments
                        // it, so a non-atomic RMW split into a Relaxed load +
                        // Relaxed store cannot lose an update. The remote
                        // `stats()` reader (`tcache_hits_total`) still does a
                        // Relaxed atomic load and observes a monotonically
                        // non-decreasing value — identical visibility to the old
                        // `fetch_add(Relaxed)` (Relaxed gives no ordering either
                        // way; only atomicity of the single word, which `store`
                        // preserves). Only the lock prefix is dropped.
                        //
                        // W3: the counter STORAGE lives in the owning `HeapSlot`
                        // (closing the Stacked-Borrows aliasing gap — see the
                        // `TcacheHitCounter` module comment); `self.tcache_hits`
                        // is the stable `&'static AtomicU64` handle `claim`
                        // planted at bind time. `Some` on every alloc path (alloc
                        // only runs after `claim` bound it). Same 2 mem-ops as
                        // before, now to the slot's field rather than an inline
                        // one. Safe reference; no `unsafe` (deny-unsafe module).
                        //
                        // W3 Part B: the per-hit bump is gated behind
                        // `alloc-stats` (default OFF, NOT in `production`) —
                        // when off it compiles OUT of the churn hot path and
                        // `stats().tcache_hits` reads 0; when on it costs ~2
                        // mem-ops + one `Option` branch per hit. See the
                        // `alloc-stats` feature doc in Cargo.toml and the Part-B
                        // Ir measurement in the task W3 report.
                        #[cfg(feature = "alloc-stats")]
                        if let Some(hits) = self.tcache_hits {
                            hits.store(
                                hits.load(Ordering::Relaxed).wrapping_add(1),
                                Ordering::Relaxed,
                            );
                        }
                        let issued = self.tcache.classes[c].slots[new_cnt];
                        // RAD-5 (E4) GO/NO-GO EXPERIMENT: clear the
                        // magazine-residency bit — this block leaves the
                        // magazine for the caller. THE HOT PATH: unlike the
                        // `hardened`-only gen-table bump below, this runs on
                        // EVERY magazine hit under `production`, so it forces
                        // a `segment_base_of_ptr` + bitmap read-modify-write
                        // that this path previously did not pay AT ALL. See
                        // `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
                        // measured cost of this specific store on
                        // `small_churn_16b` et al.
                        {
                            let base = os::segment_base_of_ptr(issued);
                            let off = (issued as usize - base as usize) as u32;
                            SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
                        }
                        // X7 Ф3 (task #191) touch (a): bump the generation at
                        // ISSUE. The block leaves the allocator's bookkeeping
                        // (the magazine) and enters the caller's hands — this
                        // is the life transition. Compiled ONLY under
                        // `hardened`; non-hardened is byte-identical (the
                        // `cfg(not)` branch is a bare passthrough).
                        #[cfg(feature = "hardened")]
                        {
                            let base = os::segment_base_of_ptr(issued);
                            let off = (issued as usize) - (base as usize);
                            // SAFETY: `base` is a live, exclusively-owned
                            // segment; `off` is a MIN_BLOCK-aligned offset.
                            #[allow(unsafe_code)]
                            unsafe {
                                crate::alloc_core::segment_header::bump_gen(base, off)
                            };
                        }
                        return issued;
                    }

                    // Magazine miss: batch-refill + stamp hoist (P4).
                    // We inline the refill+stamp here instead of calling
                    // `refill_class_stamped` because borrowing `self.core`
                    // and `self.tcache.classes[c].slots` separately avoids a
                    // double-mutable-borrow conflict on `self`.
                    //
                    // P3 (Э1, task #147): the miss refills via
                    // `refill_class_bump` — bump-direct batched carve. On a
                    // cold miss it drains existing free blocks first
                    // (pop_free / find_segment_with_free, which reclaims
                    // cross-thread frees — source order preserved), then
                    // bump-carves the remaining slots DIRECTLY into the
                    // magazine, skipping the old carve→BinTable→pop_free
                    // round-trip (a tautology on freshly-carved virgin
                    // blocks — bit 0 is already "allocated", so setting it
                    // free and immediately clearing it was pure overhead).
                    // D1/M2 end-state is byte-identical to the former
                    // `refill_class` (see `refill_class_bump`'s proofs).
                    //
                    // P3 (task #147): the P7 alloc-side bulk-bypass and the
                    // `alloc_streak` counter are RETIRED. bump-direct IS the
                    // ideal bulk path — a magazine miss now carves straight
                    // into the magazine at near-`memcpy` cost, so the
                    // "skip the magazine on an alloc-without-free streak"
                    // heuristic no longer buys anything. Retiring the alloc
                    // side also retires the dealloc-side companion flush
                    // (see `dealloc_own_thread`): without a streak counter it
                    // could never fire, so keeping it would be dead code.
                    //
                    // D3: the refill amount is a per-class BYTE budget, not
                    // the fixed `TCACHE_CAP` for every class — see
                    // `refill_n_for_class`. Small classes still get the full
                    // `TCACHE_CAP` (unchanged behaviour); large small-classes
                    // (block_size approaching SMALL_MAX) get fewer blocks per
                    // refill, so one magazine miss cannot park megabytes in a
                    // single idle thread's cache.
                    // Magazine miss: refill via the outlined slow path.
                    // `#[cold] #[inline(never)]` keeps the closure/split-borrow
                    // complexity out of `alloc`'s frame (task #164 Ir shaping).
                    return self.refill_magazine_slow(c);
                }
                // not a small class -> fall through to large path
            }
        }

        // Existing path: reclaim+alloc through AllocCore (large, or non-fastbin).
        let ptr = self.core.alloc(layout);
        if !ptr.is_null() {
            self.stamp_segment_owner(ptr);
        }
        ptr
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    #[must_use]
    #[inline]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let ptr = self.alloc(layout);
        if !ptr.is_null() {
            Node::zero(
                ptr,
                layout
                    .size()
                    .max(crate::alloc_core::size_classes::MIN_BLOCK),
            );
        }
        ptr
    }

    /// Deallocate `ptr` (previously returned by [`alloc`](Self::alloc)).
    ///
    /// Own-thread path: routes to the owning segment's `BinTable` via
    /// [`AllocCore::dealloc`] (which applies the M2 double-free guard).
    /// Under `alloc-xthread`: if the segment is stamped with another heap's
    /// head, route cross-thread via the TFS (the §2.2 protocol re-based on
    /// the registry). Foreign pointers (not a sefer segment) are a safe
    /// no-op.
    #[inline(always)]
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        #[cfg(feature = "alloc-xthread")]
        {
            self.dealloc_routing(ptr, layout);
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            self.dealloc_own_thread(ptr, layout);
        }
    }

    /// Own-thread dealloc: small frees go to the magazine (under fastbin),
    /// everything else to `core.dealloc`. Called from the `!alloc-xthread`
    /// path (no routing needed) and from `dealloc_routing` after confirming
    /// the block is ours.
    ///
    /// Э9 (P7.1, task #160): under fastbin this delegates to
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base),
    /// computing `base = os::segment_base_of_ptr(ptr)` itself. The
    /// Э9 (P7.1): under fastbin the magazine body lives in
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base)
    /// (which takes the pre-computed `base`), and BOTH callers of the
    /// own-thread path under fastbin already hold `base` (the cross-thread
    /// `dealloc_routing` from its `contains_base` check; there is no
    /// `!alloc-xthread` caller under fastbin since `fastbin ⟹ alloc-xthread`).
    /// So this own-arg wrapper is compiled ONLY when fastbin is OFF — where the
    /// own-thread path has no magazine and simply delegates to `core.dealloc`.
    /// Callers: the `!alloc-xthread` branch of [`dealloc`](Self::dealloc) and
    /// the non-fastbin arm of `dealloc_routing`.
    #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
    #[inline(always)]
    pub(super) fn dealloc_own_thread(&mut self, ptr: *mut u8, layout: Layout) {
        // Non-fastbin own-thread free: no magazine — delegate to core.
        self.core.dealloc(ptr, layout);
    }

    /// Э9 (P7.1, task #160): own-thread dealloc body, taking a pre-computed
    /// `base = os::segment_base_of_ptr(ptr)` so the cross-thread
    /// [`dealloc_routing`](Self::dealloc_routing) path — which already
    /// computed `base` for its `contains_base` ownership check — does not
    /// recompute it. Behaviour is byte-identical to the former inline body:
    /// the R1 `off >= bump` stale-free guard and the Э6 magazine/bitmap M2
    /// oracles all operate on this passed-in `base` (which equals what they
    /// used to compute locally, `segment_base_of_ptr` being pure).
    ///
    /// Only compiled under fastbin (the only build with a magazine + the only
    /// consumer of `base` on this path).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(super) fn dealloc_own_thread_with_base(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        base: *mut u8,
    ) {
        {
            use super::tcache::{FLUSH_N, TCACHE_CAP};
            use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};
            let size = layout.size().max(MIN_BLOCK);
            let align = layout.align();
            // C1 (0.3.0): gate removed — see the matching comment in `alloc`'s
            // magazine fast path above for the full soundness argument
            // (`class_for` guarantees `block_size % align == 0` for any
            // `Some(c)` it returns, so keying the magazine by class alone is
            // sound for any align it accepted, not just align<=16).
            {
                if let Some(c) = SizeClasses::class_for(size, align) {
                    let cnt = self.tcache.classes[c].count as usize;

                    // ── F7 (task #25): Large-segment kind guard (HARDENED) ──
                    // `class_for` returning `Some(c)` above keys the free on
                    // the *layout*, not on where `ptr` actually lives. If the
                    // caller frees a pointer that sits in a LARGE segment with
                    // a small layout (a GlobalAlloc-contract violation — the
                    // real UB is on the caller side), the M2 oracles below
                    // would read the "bitmap"/magazine state out of the bytes
                    // of the Large allocation's PAYLOAD — potentially routing
                    // that block into the magazine and later re-issuing it as a
                    // small block, aliasing the still-live Large block.
                    //
                    // The substrate path (`AllocCore::dealloc`) routes by
                    // segment `kind` FIRST and so degrades to a no-op on the
                    // same violation. This restores that symmetry: reject the
                    // Large-in-small-layout free as a no-op BEFORE the oracles.
                    // A single `kind_at(base)` header read (a table-free field
                    // load), gated behind `hardened` (default OFF) like the
                    // interior-pointer guard just below it.
                    #[cfg(feature = "hardened")]
                    {
                        if SegmentHeader::kind_at(base) == SegmentKind::Large {
                            return; // Large-segment free via small layout — no-op
                        }
                    }

                    // ── H1 (task #167): interior-pointer guard (HARDENED) ──
                    // A block start of class `c` always sits at a segment
                    // offset that is a whole multiple of `block_size(c)`
                    // (carve aligns the bump to `block_size`). An INTERIOR
                    // pointer (offset into a live block, not its start) has
                    // `off % block_size(c) != 0`. The M2 oracles below are
                    // BLIND to this: the alloc bitmap is indexed at
                    // `off >> MIN_BLOCK_SHIFT` (16 B granularity), so an
                    // interior offset that is still 16 B-aligned maps to a
                    // DIFFERENT bit that reads "allocated" → the bogus pointer
                    // falls through and is pushed into the magazine → a later
                    // alloc hands out a mid-block address → silent aliasing /
                    // corruption. This guard rejects it as a no-op.
                    //
                    // Cost: a `%` by a non-power-of-two `block_size` (a real
                    // division, ~tens of cycles) on EVERY small free — NOT
                    // free, so gated behind `hardened` (default OFF), never on
                    // the production hot path. `block_size(c)` is a table load.
                    #[cfg(feature = "hardened")]
                    {
                        let off_h = (ptr as usize).wrapping_sub(base as usize);
                        let bs = SizeClasses::block_size(c);
                        if !off_h.is_multiple_of(bs) {
                            return; // interior-pointer free — no-op
                        }
                    }

                    // ── M2 double-free guard (Э6, P6.1) ──────────────────
                    // The two exact oracles are consulted on every free (no
                    // block-body filter gates them), and the block body is never
                    // read or written on the free path. They are EXACT for the
                    // two own-thread resting places (this class's magazine + the
                    // BinTable free list); see the RESIDUAL M2 LIMIT note below
                    // for the cross-thread-double-free case (undrained
                    // RemoteFreeRing entry) they do NOT cover — task #164.
                    //
                    // The pre-Э6 design used a per-heap key stamped into the
                    // block's word1 (bytes 8..16) as a fast-path FILTER:
                    // `word1 != key` skipped the oracles and pushed directly.
                    // That filter cost a read+write of the BLOCK BODY on every
                    // push (a cold/conflict cache line at block stride — the
                    // 256 B churn regression), and — worse — it was UNSOUND
                    // under user writes: once the user wrote to bytes 8..16 of
                    // a block (legitimate use of allocated memory), a later
                    // double-free saw `word1 != key`, SKIPPED the oracles, and
                    // fell through to push → the block landed BOTH in the
                    // magazine AND on a BinTable free list → the same pointer
                    // issued twice.
                    //
                    // Э6 removes the filter and always runs the two exact
                    // oracles, in this exact order:
                    //
                    //   (1) in-magazine scan  — catches a block freed but not
                    //       yet flushed (still queued in `slots`). Bounded by
                    //       `cnt <= TCACHE_CAP` (16); in churn cnt is 1–3 and
                    //       the array is hot/L1.
                    //   (2) BinTable bitmap   — catches a block that was
                    //       flushed to a free list (`is_free(off)` set). The
                    //       bitmap line is shared by hundreds of blocks → hot.
                    //
                    // A genuinely live block is in neither → push. Order is
                    // load-bearing: scan FIRST (unflushed), bitmap SECOND
                    // (flushed); do NOT reorder.
                    //
                    // This STRENGTHENS M2 for the OWN-THREAD double-free: the
                    // pre-Э6 flushed-double-free hole (user overwrote word1 →
                    // stale/garbage key → oracles skipped → double-issue) is
                    // now closed unconditionally — the bitmap oracle no longer
                    // depends on the block body being pristine. That is a strict
                    // correctness improvement, not a trade, and it is EXACT for
                    // the two own-thread resting places a freed block can be in:
                    // (1) this class's magazine (the scan), and (2) the segment's
                    // BinTable free list (the bitmap). The magazine free path now
                    // touches no block body at all (mimalloc, by contrast, must
                    // write `next` into the body on every free — we are
                    // structurally cheaper per free on cold working sets).
                    //
                    // ── RESIDUAL M2 LIMIT (cross-thread double-free) — #164 NARROWED
                    // ─────────────────────────────────────────────────────────
                    // The two oracles are exact ONLY for those two resting
                    // places. They are BLIND to a third, transient one: a block
                    // whose CROSS-THREAD free is still in-flight — packed into
                    // its segment's `RemoteFreeRing` but NOT YET DRAINED by the
                    // owner.
                    //
                    // Task #164 NARROWED the window: ALL production drain paths
                    // now consult the magazine via `reclaim_offset_checked`'s
                    // `is_in_magazine` predicate (refill via `refill_magazine_slow`,
                    // realloc via `try_realloc_inplace` + `HeapCore::alloc`,
                    // debug via `dbg_drain_all_rings_checked`). A block that is
                    // simultaneously magazine-resident AND in the ring is detected
                    // and the ring entry is dropped (the magazine copy stays
                    // canonical). GREEN tests:
                    // `drain_resident_xthread_double_free_no_corruption`,
                    // `realloc_path_drain_respects_magazine`.
                    //
                    // Task R1 (retro C1, 2026-07-06) closed a SECOND leg the X2
                    // campaign missed: the refill-window in-out-buffer leg.
                    // `refill_class_bump_impl` pulls freelist blocks into the
                    // caller-owned `out[0..filled]` buffer BEFORE draining rings;
                    // the predicate's `if k == c { return false; }` shortcut
                    // (justified only by count[c]==0 borrow-safety) is blind to
                    // those magazine-destined blocks, so a stale ring note for a
                    // block already in `out` was reclaimed → relinked → re-pulled
                    // in the SAME refill call (P double-issued at consecutive
                    // positions). Fix: wrap the predicate with an out-membership
                    // guard (`is_in_magazine(ptr,k) || (k == c && out[..filled].contains(ptr))`).
                    // GREEN test: `refill_window_does_not_double_issue_in_out_buffer_resident_block`.
                    //
                    // REMAINING residual = re-issue-before-drain / delayed xfree
                    // (the THIRD leg): if the block was popped (re-issued to the
                    // user) before the drain runs, the state is information-
                    // theoretically identical to a genuine delayed cross-thread
                    // free (bitmap allocated, not in magazine, not in the refill
                    // out-buffer, (off,class)-only entry). Pinned RED by
                    // `residual_xthread_double_free_no_corruption` (#[ignore]d).
                    // Full fix: task X7 (hardened, generational ring entry — see
                    // RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md §8.4).
                    //
                    // (1) in-magazine DF oracle — ALWAYS. RAD-5 (E4) GO/NO-GO
                    // EXPERIMENT: replaced the Э10 branchless chunked scan
                    // (which walked up to `cnt` <= TCACHE_CAP=16 magazine
                    // slots) with an O(1) probe of the second
                    // (magazine-residency) bitmap. `off`/`meta` are hoisted
                    // here (previously computed AFTER this oracle, for the
                    // flushed-DF oracle below) so both oracles share them.
                    // Semantics: exact replacement — a block is
                    // magazine-resident in the bitmap's view iff it is one of
                    // `{slots[c][i] : i < cnt}` in the old scan's view, by
                    // construction (mark on push, clear on pop/flush — see
                    // `magazine_bitmap.rs`'s module doc). See
                    // `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
                    // measured verdict on whether this probe is actually
                    // cheaper than the scan it replaces.
                    let off = (ptr as usize - base as usize) as u32;
                    let meta = SegmentMeta::new(base);
                    if meta.magazine_bitmap().is_in_magazine(off) {
                        return; // in-magazine double-free — no-op
                    }
                    // (2) flushed DF oracle — ALWAYS. `base`/`off`/bitmap are
                    // read on a segment already PROVEN ours and mapped by
                    // `dealloc_routing`'s `contains_base` ownership check
                    // (fastbin ⇒ alloc-xthread structurally), exactly as
                    // before. Э9 (P7.1): `base` is the pre-computed argument
                    // (same value `segment_base_of_ptr` would return — pure),
                    // threaded in from `dealloc_routing` so it is computed
                    // once on the own-thread free path.
                    // Stale-free guard, parity with `dealloc_small`
                    // (alloc_core.rs). A block that was carved into a segment
                    // later decommitted+reset has `off >= bump` (bump was reset
                    // to small_meta_end and the bitmap zeroed = "allocated", so
                    // the bitmap oracle below would NOT catch it); likewise a
                    // never-carved in-segment address. A real, currently-carved
                    // live block always has `off < bump`, so no false positive
                    // on a legitimate free. Owner-only `bump` read
                    // (single-writer), gated to the feature that resets the
                    // bump — exactly as `dealloc_small`.
                    #[cfg(feature = "alloc-decommit")]
                    if (off as usize) >= meta.bump_of() {
                        return;
                    }
                    if meta.alloc_bitmap().is_free(off) {
                        return; // flushed-then-double-freed — no-op
                    }

                    if cnt < TCACHE_CAP {
                        // Legit free → push. NO key stamp, NO block-body write.
                        //
                        // RAD-5 (E4) GO/NO-GO EXPERIMENT: mark this block
                        // magazine-resident in the second bitmap. `meta`/`off`
                        // are already computed above for the M2 bitmap read —
                        // this reuses them, paying only the new bitmap's own
                        // read-modify-write. See `magazine_bitmap.rs`'s module
                        // doc; `docs/perf/IAI_BASELINE.md`'s RAD-5 entry has
                        // the measured verdict on whether this is worth it.
                        meta.magazine_bitmap().mark_magazine(off);
                        self.tcache.classes[c].slots[cnt] = ptr;
                        self.tcache.classes[c].count = (cnt + 1) as u8;
                        return;
                    }
                    // ── Magazine overflow (cnt == TCACHE_CAP) ──────────
                    // P3 (task #147): the P7 dealloc-side bulk-mode bypass is
                    // RETIRED together with the alloc-side bypass and the
                    // `alloc_streak` counter. That branch fired only when the
                    // alloc side had advanced the streak past BULK_THRESHOLD;
                    // with the counter gone it could never fire, so keeping it
                    // would be dead code guarded by a stuck-at-0 condition.
                    // The always-taken half-flush + compact + push below is the
                    // sole overflow policy now. D1/M2 unchanged: `flush_class`
                    // returns blocks to the substrate via `dealloc_small`
                    // (mark_free + dec_live) exactly as before.
                    //
                    // RAD-5: the FLUSH_N blocks about to be flushed leave the
                    // magazine — clear their bit BEFORE calling `flush_class`
                    // (mirrors "flush = clear_magazine + mark_free": the
                    // AllocBitmap side of `mark_free` happens inside
                    // `flush_class`/`flush_run`; this bitmap's clear happens
                    // here since `AllocCore` has no magazine concept).
                    for &flushed in &self.tcache.classes[c].slots[0..FLUSH_N] {
                        let fbase = os::segment_base_of_ptr(flushed);
                        let foff = (flushed as usize - fbase as usize) as u32;
                        SegmentMeta::new(fbase)
                            .magazine_bitmap()
                            .clear_magazine(foff);
                    }
                    // Normal overflow: half-flush, then push.
                    self.core
                        .flush_class(c, &self.tcache.classes[c].slots[0..FLUSH_N]);
                    // Compact: shift entries [FLUSH_N..CAP] down to [0..CAP-FLUSH_N].
                    let remaining = TCACHE_CAP - FLUSH_N;
                    for i in 0..remaining {
                        self.tcache.classes[c].slots[i] = self.tcache.classes[c].slots[i + FLUSH_N];
                    }
                    // Push (Э6: NO key stamp, NO block-body write). The oracles
                    // above already ran before this overflow branch, so a
                    // double-free is caught even when the magazine is full.
                    // RAD-5: mark the newly-pushed block magazine-resident.
                    meta.magazine_bitmap().mark_magazine(off);
                    self.tcache.classes[c].slots[remaining] = ptr;
                    self.tcache.classes[c].count = (remaining + 1) as u8;
                    return;
                }
            }
        }
        // Large / non-small / non-fastbin: delegate to core.
        self.core.dealloc(ptr, layout);
    }

    /// Shrink/grow an allocation. Returns null on OOM (leaving the old
    /// allocation intact). Null `ptr` returns null.
    ///
    /// ## Own-segment pointers (task C2 + #164)
    ///
    /// If `ptr` lives in one of THIS heap's segments (`contains_base` — the
    /// same O(1) ownership test `dbg_owner_id_for` uses), the resize takes the
    /// magazine-aware fast path:
    ///
    ///   1. **A1 deferred-large drain** (`alloc-xthread` only): BEFORE the
    ///      in-place attempt, if the NEW size classifies as Large
    ///      (`class_for(...).is_none()`), drain this heap's deferred-free
    ///      stack (MUST-1/A1 — a realloc-growth-only thread still reclaims
    ///      cross-thread-freed large segments; otherwise its stack
    ///      accumulates unboundedly). This drain is load-bearing — it runs
    ///      whether or not the in-place path succeeds.
    ///   2. **In-place attempt**: call `AllocCore::try_realloc_inplace_known_base`, which
    ///      applies the OPT-F (Small same-class) and OPT-G (Large grow-in-span)
    ///      short-circuits. On success it returns the SAME `ptr` (mutating the
    ///      block's header in place, never moving it) — we return immediately.
    ///   3. **Move leg**: on in-place failure, build the new `Layout`, call
    ///      `HeapCore::alloc` (magazine-aware — drains via the checked
    ///      predicate and stamps per #169), copy `min(old, new)` bytes, then
    ///      `HeapCore::dealloc` the old pointer.
    ///
    /// The move leg routes through `HeapCore::alloc`/`HeapCore::dealloc`
    /// (NOT `AllocCore::realloc`'s internal alloc+copy+dealloc) so that the
    /// two ownership hooks `HeapCore::alloc` applies — segment-ownership
    /// stamping (`stamp_segment_owner`, which under `alloc-xthread` also
    /// writes `owner_thread_free`, the field that makes a remote free route
    /// back here instead of leaking) and the checked drain — fire on the
    /// freshly allocated block. Without them a Vec grown via realloc on
    /// thread A would live in an UNSTAMPED Large segment; when A hands it
    /// to thread B and B drops it, `dealloc_routing` sees not-ours +
    /// magic OK + `owner_tf == null` → silent no-op → the whole segment
    /// (4+ MiB) and its `SegmentTable` slot leak forever (the resurrected
    /// A1/#114 leak-to-abort).
    ///
    /// ## Foreign pointers
    ///
    /// A `ptr` we do NOT own (e.g. under `alloc-xthread`, a block that lives
    /// in ANOTHER heap's segment) takes the foreign leg. R2-1: before copying,
    /// the leg now validates that `ptr` resolves to a LIVE sefer segment
    /// (segment-header magic check, mirroring `dealloc_foreign_slow`'s first
    /// guard) AND that `old_layout.size()` does not exceed that segment's
    /// committed span. A bogus/foreign pointer (stack, foreign allocator,
    /// dangling) or an oversized claim is rejected (null) BEFORE any copy —
    /// never read out of bounds. A legitimate cross-heap sefer pointer passes
    /// both checks, copies `min(old, new)`, then frees the OLD pointer via
    /// `self.dealloc` (which routes cross-thread correctly under
    /// `alloc-xthread`). Without `alloc-xthread` there is no legitimate
    /// cross-heap owner, so the foreign leg returns null outright (symmetric
    /// with `AllocCore::realloc`'s foreign-pointer null and `dealloc`'s
    /// foreign no-op).
    pub fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        #[cfg(feature = "alloc-global")]
        {
            let base = os::segment_base_of_ptr(ptr);
            // Task #135 (Part 2): O(1) membership test (`AllocCore::contains_base`
            // → the OPT-B hash table) replaces the O(segment count) linear scan
            // `segment_bases().any(|b| b == base)`. Same semantics: `true` iff
            // `base` is one of THIS heap's registered, live segments.
            if self.core.contains_base(base) {
                // Own-segment pointer. The resize proceeds in up to three
                // phases (see the doc comment above): A1 Large drain, in-place
                // attempt, then move leg — all funnelled through the
                // magazine-aware `HeapCore::alloc`/`dealloc` (NOT
                // `AllocCore::realloc`'s internal alloc, which bypasses the
                // ownership hooks).
                //
                // MUST-1 (0.3.0, C2 regression fix): the in-place attempt and
                // any move-leg alloc may carve a FRESH segment. That substrate
                // alloc does NOT run the two ownership hooks `HeapCore::alloc`
                // applies — segment-ownership stamping (`stamp_segment_owner`,
                // which under `alloc-xthread` also writes `owner_thread_free`,
                // the field that makes a remote free route back here instead
                // of leaking) and the A1 deferred-large drain
                // (`drain_large_deferred_free`). Without them, a Vec grown via
                // realloc on thread A lives in an UNSTAMPED
                // (`owner_thread_free == null`) Large segment; when A hands it
                // to thread B and B drops it, `dealloc_routing` sees not-ours
                // + magic OK + `owner_tf == null` → silent no-op → the whole
                // segment (4+ MiB) and its `SegmentTable` slot leak forever
                // (the resurrected A1/#114 leak-to-abort).
                //
                //   (1) A1 Large drain — BEFORE the in-place attempt, if the
                //       NEW request classifies as Large
                //       (`class_for(...).is_none()`, the exact predicate
                //       `alloc` uses), drain this heap's deferred-free stack
                //       so a realloc-growth-only thread still reclaims
                //       cross-thread-freed large segments (otherwise its stack
                //       accumulates unboundedly — the A1 drain-bypass leg of
                //       the bug). This drain is load-bearing and runs
                //       regardless of whether the in-place path then succeeds.
                #[cfg(feature = "alloc-xthread")]
                {
                    let class = crate::alloc_core::size_classes::SizeClasses::class_for(
                        new_size.max(crate::alloc_core::size_classes::MIN_BLOCK),
                        old_layout.align(),
                    );
                    if class.is_none() {
                        self.drain_large_deferred_free();
                    }
                }
                //   (2) In-place attempt — try OPT-F (Small same-class) and
                //       OPT-G (Large grow-in-span) via the substrate. On
                //       success the block's header is mutated IN PLACE and
                //       the SAME `ptr` is returned (no alloc leg, hence no
                //       alloc-leg drain — the A1 drain above already covered
                //       the Large case). On failure fall through to the move
                //       leg, which routes through `HeapCore::alloc`
                //       (magazine-aware, checked drain) — NOT through
                //       `AllocCore::realloc`'s blind alloc→alloc_small path.
                if let Some(p) = self
                    .core
                    .try_realloc_inplace_known_base(base, ptr, old_layout, new_size)
                {
                    // `try_realloc_inplace_known_base` mutates the block's header in
                    // place and always returns the SAME pointer on success
                    // (it never moves the block). The segment was already
                    // stamped when first allocated, so there is nothing to
                    // re-stamp here.
                    debug_assert_eq!(p, ptr, "known-base realloc must return the same pointer");
                    return p;
                }
                //   (3) Move leg — in-place did not apply: alloc a fresh block
                //       through `HeapCore::alloc` (magazine-aware: drains via
                //       the checked predicate + stamps per #169), copy the
                //       preserved prefix, then `HeapCore::dealloc` the old
                //       pointer (own-segment → routes through `core.dealloc`).
                //
                //       R2-1 (soundness): bound the move leg's read by the
                //       block's actual committed span, not the caller-supplied
                //       `old_layout.size()`. This is a SAFE `pub fn`; a bogus
                //       layout (e.g. 8 MiB for a 16-byte block) must not drive
                //       an OOB read. `base` was proven live above by
                //       `contains_base`. The write side is always safe (`copy
                //       <= new_size`); the read is bounded here.
                if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
                    return core::ptr::null_mut();
                }
                let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
                    Ok(l) => l,
                    Err(_) => return core::ptr::null_mut(),
                };
                let new_ptr = self.alloc(new_layout);
                if new_ptr.is_null() {
                    return core::ptr::null_mut();
                }
                let copy = old_layout.size().min(new_size);
                crate::alloc_core::node::Node::copy_nonoverlapping(ptr, new_ptr, copy);
                self.dealloc(ptr, old_layout);
                return new_ptr;
            }
        }
        // Foreign pointer (not one of our segments). Before copying from it,
        // the pointer MUST resolve to a live sefer segment of sufficient
        // committed span; otherwise a safe caller passing a bogus/foreign
        // pointer triggers an out-of-bounds read (R2-1, gap 1).
        //
        // Under `alloc-xthread` this leg is the deliberately-designed
        // cross-heap path (a pointer from ANOTHER live heap is legitimate,
        // and `self.dealloc` routes its free cross-thread). The membership
        // barrier is the segment-header magic check (mirrors
        // `dealloc_foreign_slow`'s first guard): a pointer whose computed
        // base is not a live sefer segment — stack, foreign allocator,
        // dangling — is rejected (null) before any copy. A REAL cross-heap
        // sefer segment passes magic, then the same R2-1 span bound as the
        // own-seg leg applies.
        //
        // Without `alloc-xthread` there is no cross-thread routing and thus
        // no legitimate owner for a pointer this heap does not recognise:
        // copying from it would read arbitrary caller-supplied memory under
        // a safe fn. Return null, `ptr` untouched — symmetric with
        // `AllocCore::realloc`'s foreign-pointer null and `dealloc`'s foreign
        // no-op.
        #[cfg(feature = "alloc-xthread")]
        {
            let base = os::segment_base_of_ptr(ptr);
            // R4-2 (memory_safety_review, R4-MS-1/MS-2): guard the degenerate
            // base BEFORE the raw `magic_at` read. `segment_base_of_ptr` masks
            // the address down to the SEGMENT boundary; a garbage pointer like
            // `1 as *mut u8` masks to `base == 0` (null), and `magic_at(0)`
            // would then dereference address `offset_of!(SegmentHeader, magic)`
            // with no guard — an immediate read of a structurally-impossible
            // "segment". Reject null (and anything that masks to null) as a
            // foreign pointer. This does NOT attempt cross-heap staleness
            // detection (case (a) vs (b) in `dealloc_foreign_slow`); it closes
            // only the narrower class where `base` cannot be a real segment by
            // construction.
            if base.is_null() {
                return core::ptr::null_mut();
            }
            if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
                return core::ptr::null_mut();
            }
            if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
                return core::ptr::null_mut();
            }
            let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
                Ok(l) => l,
                Err(_) => return core::ptr::null_mut(),
            };
            let new_ptr = self.alloc(new_layout);
            if new_ptr.is_null() {
                return core::ptr::null_mut();
            }
            let copy = old_layout.size().min(new_size);
            Node::copy_nonoverlapping(ptr, new_ptr, copy);
            self.dealloc(ptr, old_layout);
            new_ptr
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            let _ = new_size;
            core::ptr::null_mut()
        }
    }

    /// Task #164 (Ir shaping): outlined refill-miss path. All split-borrow
    /// and closure complexity lives here, behind a `#[cold] #[inline(never)]`
    /// call boundary, so `alloc`'s own frame is not bloated by register spills
    /// from the closure / `split_at_mut` machinery. Returns the popped pointer
    /// (the block to hand out), or null on true OOM.
    ///
    /// UBFIX-10 (M-9): opportunistic Large-deferred-free drain. Before this
    /// task, `drain_large_deferred_free` was called ONLY from the two
    /// Large-classified sites in [`alloc`](Self::alloc)/[`realloc`](Self::realloc)
    /// — a heap that stopped allocating Large blocks entirely (e.g. a workload
    /// that starts Large-heavy and settles into Small-only churn) never drained
    /// again, so any cross-thread-freed Large segments queued on its deferred
    /// stack stayed mapped-but-dead for the rest of the process's life
    /// (unbounded resource retention, not UB — see
    /// `docs/reviews/2026-07-10-ub-audit-final-synthesis.md` M-9). This is the
    /// SMALL-path drain site: every magazine MISS (never a hit — this function
    /// runs only when the fast-path pop in `alloc` found `count[c] == 0`)
    /// opportunistically reclaims any queued Large segments too, so a
    /// Small-only workload still recovers them. Placement here (rather than
    /// unconditionally in `alloc`) keeps the check off the actual hot path —
    /// `refill_magazine_slow` is `#[cold] #[inline(never)]`, reached only on a
    /// miss, so the extra call costs nothing on the magazine-hit fast path
    /// this file's own churn benchmarks measure.
    ///
    /// The call is the SAME cheap-precheck shape draining always has:
    /// `drain_large_deferred_free`'s pop loop starts with a single Acquire
    /// load of the stack head and returns immediately if it is null (see
    /// `alloc_core::deferred_large::drain_large_deferred_free`) — an empty
    /// stack costs exactly one atomic load here, no CAS, no further work.
    /// `fastbin` requires `alloc-xthread` (`Cargo.toml`: `fastbin =
    /// ["alloc-global", "alloc-xthread"]`), so the call is unconditional
    /// inside this `fastbin`-gated function — no extra `cfg` needed.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[cold]
    #[inline(never)]
    fn refill_magazine_slow(&mut self, c: usize) -> *mut u8 {
        use crate::alloc_core::size_classes::SizeClasses;

        // UBFIX-10 (M-9): opportunistic drain on every magazine miss — see
        // the doc comment above. Cheap when empty (one Acquire load).
        self.drain_large_deferred_free();

        // RAD-4b (task #72): opportunistic drain of this heap's
        // `HeapOverflow` second-chance ring — same placement rationale as
        // the M-9 drain immediately above (magazine-MISS-only, so the
        // magazine-HIT fast path in `alloc` pays nothing extra). See
        // `push_to_heap_overflow`'s doc comment for the full design and
        // `alloc`'s matching non-fastbin call site.
        self.drain_heap_overflow();

        let want = super::tcache::refill_n_for_class(SizeClasses::block_size(c));
        // Task #164 / PERF-PASS-5 (G7): zero-copy split borrow. The refill
        // writes DIRECTLY into `tcache.classes[c].slots` (no buffer, no
        // copy). `split_at_mut`/`split_first_mut` is still needed to obtain
        // `cur: &mut PerClass` (the refill's write target) while the rest of
        // `self` stays usable inside the closure below.
        //
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: the magazine predicate closure used
        // to scan OTHER classes' magazine slots (`entry.slots[0..cnt]`,
        // O(cnt) per candidate offset drained from a remote ring, via the
        // `before`/`after` split halves). Replaced with an O(1) probe of the
        // second (magazine-residency) bitmap — the probe is keyed by segment
        // offset, not by class, so `before`/`after` are no longer read (only
        // `cur.slots` as the write target survives from the original split).
        //
        // KEY INVARIANT (load-bearing): at refill time, `count[c] == 0` —
        // the refill runs ONLY on a magazine miss (the pop in `alloc` failed
        // because `cnt == 0`). So the predicate for class `c` itself is
        // trivially false (0 slots to scan), and the mutable borrow of
        // `classes[c].slots` (for the refill output) is never read by the
        // closure.
        let (_before, rest) = self.tcache.classes.split_at_mut(c);
        let (cur, _after) = rest.split_first_mut().expect("c < SMALL_CLASS_COUNT");
        let n = self
            .core
            .refill_class_bump_checked(c, &mut cur.slots[0..want], &|ptr, k| {
                if k == c {
                    return false;
                }
                let pbase = os::segment_base_of_ptr(ptr);
                let poff = (ptr as usize - pbase as usize) as u32;
                SegmentMeta::new(pbase)
                    .magazine_bitmap()
                    .is_in_magazine(poff)
            });
        if n == 0 {
            return core::ptr::null_mut(); // true OOM
        }
        // P4 stamp hoist + Э11 (task #161) stamp-dedupe: stamp each
        // pulled block's source segment, but call `stamp_segment_owner`
        // only when the block's segment base CHANGES from the previous
        // block's. Idempotent per segment; one stamp per distinct source.
        let mut prev_base = usize::MAX;
        for i in 0..n {
            let p = self.tcache.classes[c].slots[i];
            if !p.is_null() {
                let base = os::segment_base_of_ptr(p) as usize;
                if base != prev_base {
                    self.stamp_segment_owner(p);
                    prev_base = base;
                }
            }
        }
        // Pop the top, leave n-1 in the magazine.
        let new_cnt = n - 1;
        self.tcache.classes[c].count = new_cnt as u8;
        // RAD-5: mark the n-1 blocks REMAINING in the magazine as
        // magazine-resident (refill = existing `mark_alloc`/leave-unset on
        // `AllocBitmap` inside `refill_class_bump_checked`, unchanged, PLUS
        // this `mark_magazine` for every block landing in the magazine). The
        // block at `new_cnt` is popped to the caller below and must NOT be
        // marked (it is being issued, not retained).
        for &p in &self.tcache.classes[c].slots[0..new_cnt] {
            let pbase = os::segment_base_of_ptr(p);
            let poff = (p as usize - pbase as usize) as u32;
            SegmentMeta::new(pbase)
                .magazine_bitmap()
                .mark_magazine(poff);
        }
        let issued = self.tcache.classes[c].slots[new_cnt];
        // X7 Ф3 (task #191) touch (a): bump the generation at ISSUE. The block
        // leaves the allocator's bookkeeping (the magazine) and enters the
        // caller's hands — this is the life transition. This is the refill
        // path's issue point (the refill fills n slots, then pops ONE off the
        // top for the caller; the remaining n-1 are still allocator-owned in
        // the magazine and are bumped on THEIR respective pops). Compiled ONLY
        // under `hardened`; non-hardened is byte-identical.
        #[cfg(feature = "hardened")]
        {
            let base = os::segment_base_of_ptr(issued);
            let off = (issued as usize) - (base as usize);
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset.
            #[allow(unsafe_code)]
            unsafe {
                crate::alloc_core::segment_header::bump_gen(base, off)
            };
        }
        issued
    }

    /// Stamp a segment's header with this heap's ownership. Two parts:
    ///
    /// 1. **`owner_state = LIVE(self.id, 0)`** — the ownership field. Set on
    ///    every alloc so cross-thread free routing can resolve a segment's
    ///    owning heap from its `owner_id`. Idempotent: a segment already
    ///    stamped with our id is left alone.
    /// 2. **(alloc-xthread only) `owner_thread_free` head pointer** — the
    ///    cross-thread free routing target, so a remote freer can find this
    ///    heap's TFS. Idempotent: only stamps if currently null.
    ///
    /// Called on the alloc path after a successful allocation. The segment is
    /// exclusively ours (single-writer invariant from the claim CAS), so the
    /// `owner_state` store is race-free.
    ///
    /// ## OPT-C fast path (task #66)
    ///
    /// `last_stamped_segment` caches the base of the most recently stamped
    /// segment. On a cache hit the function performs only a **Relaxed** load
    /// of `owner_state` and compares it with `self.id`. If they match, we
    /// know the segment is already stamped → return immediately with NO
    /// Release-store (the expensive part on x86 — an `MFENCE`-equivalent).
    /// On a miss or on a ownership mismatch the original slow path runs.
    ///
    /// The Relaxed load is safe because:
    /// - This is the **owning thread** — the single writer of `owner_state`
    ///   on this segment. A Relaxed load cannot race with our own prior
    ///   Release-store (same thread → SC-in-program-order).
    /// - A cache miss (base changed or Relaxed-load mismatch) falls through
    ///   to the slow path which restores the Acquire/Release protocol.
    #[inline(always)]
    fn stamp_segment_owner(&mut self, ptr: *mut u8) {
        use crate::alloc_core::segment_header::{unpack_owner_id, OWNER_STATE_LIVE};
        let base = os::segment_base_of_ptr(ptr);

        // -----------------------------------------------------------------------
        // OPT-C fast path: cache-hit check.
        //
        // If the cached segment base matches the current allocation's segment
        // base, do a cheap Relaxed load of `owner_state` to confirm ownership.
        // If ownership is confirmed → early return (no Release-store, no memory
        // fence). If ownership is not confirmed (e.g., segment was recycled and
        // reset to OWNER_ID_NONE) → fall through to the slow path below.
        // -----------------------------------------------------------------------
        if base == self.last_stamped_segment && !self.last_stamped_segment.is_null() {
            // Cache hit: re-check ownership with a cheaper Relaxed load.
            // Owner-only read (we are the sole writer of owner_state on OUR
            // segments), so Relaxed ordering is race-free here.
            let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
            let cur = owner_atomic.load(Ordering::Relaxed);
            if unpack_owner_id(cur) == self.id {
                // Still our segment, already stamped. Skip the Release-store.
                // The alloc-xthread TFS stamp is also idempotent (once
                // stamped it stays); if `last_stamped_segment` is set then
                // the TFS was already written on the slow path below.
                return;
            }
            // Ownership mismatch (e.g., recycled segment): clear the cache
            // and run the slow path.
            self.last_stamped_segment = core::ptr::null_mut();
        }

        // -----------------------------------------------------------------------
        // Slow path: full Acquire-load + conditional Release-store.
        // -----------------------------------------------------------------------
        // `mut` is needed under `alloc-xthread` (the stamp branch below calls
        // `meta.stamp_owner_thread_free(&mut self)`). Silence the unused-mut
        // warning under plain `alloc-global` where the branch is absent.
        #[allow(unused_mut)]
        let mut meta = SegmentMeta::new(base);
        // 1. Stamp owner_state (ownership resolution).
        let owner_atomic = meta.owner_state_atomic();
        let cur = owner_atomic.load(Ordering::Acquire);
        if unpack_owner_id(cur) != self.id {
            let me = pack_owner(OWNER_STATE_LIVE, self.id, 0);
            // Release: a later cross-thread freer's Acquire read of owner_state
            // (to resolve the owning heap) must observe our stamp.
            owner_atomic.store(me, Ordering::Release);
        }
        // 2. (alloc-xthread) Stamp the TFS head for cross-thread routing.
        // Phase 12.5 (shard model): stamped ONCE, when the segment is first
        // allocated from, and NEVER cleared or re-stamped. The inline TFS
        // head's address is stable for the slot's lifetime (it does not
        // change across release→claim), so the stamp remains valid for as
        // long as the slot owns this segment — which is forever in the shard
        // model (segments do not leave their heap).
        //
        // Field-specific write (task #33 root-cause fix): we stamp ONLY the
        // `owner_thread_free` field via `stamp_owner_thread_free`, NOT a
        // full-struct `write_header`. A full-struct RMW here rewrote `bump`
        // and every other field, and — although the stamp itself runs on the
        // owning thread — the struct read it performed (`meta.header()`)
        // raced the Owner's own later `bump` writes is not the issue; the
        // issue is that `write_header` writes `magic`/`kind`/`bump` bytes
        // that a concurrent Remote `dealloc_routing` field-read may observe
        // mid-update. Writing only the `owner_thread_free` word touches bytes
        // disjoint from every field a Remote reads, so there is no race.
        // The single-writer invariant (the slot's owner is the sole writer
        // of its segments' headers) makes the plain field write race-free.
        #[cfg(feature = "alloc-xthread")]
        {
            let cur_head =
                crate::alloc_core::segment_header::SegmentHeader::owner_thread_free_at(base);
            if cur_head.is_null() {
                // Task #142: expose this atomic's provenance so a REMOTE
                // freer can reconstruct a wildcard pointer to it (via
                // `Node::atomic_ptr_ref` → `with_exposed_provenance_mut`)
                // rather than inheriting a reference provenance a concurrent
                // remote write would disable, corrupting other remotes' access
                // (see `Node::atomic_ptr_ref`).
                //
                // task H1: the head is the OWNING SLOT's `thread_free` word,
                // reached through the `&'static` handle planted at claim time
                // — NOT an inline `HeapCore` field. This is the whole point of
                // the H1 hoist: the exposed address is outside every `&mut
                // HeapCore` retag range, so a remote CAS onto it no longer
                // races the owner's `alloc(&mut self)` protector. `handle as
                // *const _` takes the slot field's stable address without any
                // `&mut self`-rooted retag; `expose_provenance` registers it
                // for the paired `with_exposed_provenance_mut`. `None` cannot
                // occur here — stamping runs only after the claim that planted
                // the handle (defensive: skip the stamp if somehow unbound).
                if let Some(handle) = self.thread_free {
                    let tf_ptr = handle as *const AtomicPtr<u8>;
                    let _ = tf_ptr.expose_provenance();
                    meta.stamp_owner_thread_free(tf_ptr as *const _);
                }
            }
        }

        // Slow path succeeded: cache the segment base so the next alloc from
        // the same segment takes the fast path.
        self.last_stamped_segment = base;
    }

    /// OPT-C (task #66): reset the stamp cache.
    ///
    /// Sets `last_stamped_segment` to null, forcing the next call to
    /// `stamp_segment_owner` to take the slow path (Acquire-load +
    /// conditional Release-store).
    ///
    /// Call this whenever segment ownership may have changed out of band —
    /// specifically if a future phase re-introduces inter-heap segment
    /// adoption (the prior `try_adopt` mechanism that transferred a segment
    /// from an abandoned heap into another one was removed — see
    /// CHANGELOG.md — a hypothetical future equivalent is what this hook
    /// exists for). Without a reset, the cache might hit on a segment whose
    /// `owner_state` has already been updated by the adopter's CAS, and the
    /// Relaxed-load fast-path check would detect the mismatch and fall
    /// through correctly — but defensive reset is cleaner.
    ///
    /// **Current status:** In the shard model (Phase 12.5+) segments never
    /// leave their original heap, so this method is not called from any
    /// production path. It is provided as a safety hook for future phases
    /// that might re-introduce cross-heap segment transfer.
    #[cfg(feature = "alloc-global")]
    #[allow(dead_code)]
    pub(crate) fn reset_stamp_cache(&mut self) {
        self.last_stamped_segment = core::ptr::null_mut();
    }

    /// Flush every tcache class's magazine back to the substrate via
    /// `flush_class` → `dealloc_small` → `dec_live` → `maybe_decommit`.
    ///
    /// After this call, every magazine slot is empty (`count[c] == 0` for
    /// all classes) and the blocks have been returned to their owning
    /// segments. If any segment reaches `live_count == 0` during the flush,
    /// decommit/release fires (or the segment is pooled, subject to the
    /// pool's cap).
    ///
    /// This is the production teardown-trim primitive (task #95 / N1),
    /// called from [`trim_for_recycle`](Self::trim_for_recycle). The
    /// `#[doc(hidden)] pub dbg_flush_all` test hook delegates here so test
    /// coverage is preserved.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn flush_all_tcache(&mut self) {
        use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;
        for c in 0..SMALL_CLASS_COUNT {
            let n = self.tcache.classes[c].count as usize;
            if n == 0 {
                continue;
            }
            // RAD-5 (E4) GO/NO-GO EXPERIMENT: clear every flushed block's
            // magazine-residency bit BEFORE the flush, mirroring the
            // production overflow-flush site in `dealloc_own_thread_with_base`.
            for &flushed in &self.tcache.classes[c].slots[0..n] {
                let fbase = os::segment_base_of_ptr(flushed);
                let foff = (flushed as usize - fbase as usize) as u32;
                SegmentMeta::new(fbase)
                    .magazine_bitmap()
                    .clear_magazine(foff);
            }
            self.core
                .flush_class(c, &self.tcache.classes[c].slots[0..n]);
            self.tcache.classes[c].count = 0;
        }
    }

    /// TEST-ONLY (P5): force-flush every class's magazine back to the
    /// substrate. Delegates to the production [`flush_all_tcache`](Self::flush_all_tcache)
    /// teardown-trim primitive. Used by decommit-soak tests to drain
    /// magazine-buffered blocks before asserting decommit invariants.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_flush_all(&mut self) {
        self.flush_all_tcache();
    }

    /// Production teardown trim (task #95 / N1): flush every tcache class,
    /// drain the small-segment pool, and evict the entire large cache.
    ///
    /// Called by the TLS `AbandonGuard::drop` on thread exit, BEFORE the
    /// `HeapRegistry::recycle` CAS flips the slot `LIVE → FREE`. At that
    /// point this thread is still the slot's sole owner/writer (same
    /// single-writer window every other mutation relies on), so no
    /// cross-thread quiescence is needed.
    ///
    /// **Why:** without this trim, a wave of short-lived threads leaves
    /// tcache-buffered blocks, pooled small segments (up to 16 MiB each),
    /// and cached large spans pinned on each recycled slot — RSS/commit
    /// stays proportional to the peak thread count, not the current load.
    /// Draining here returns retained memory to the OS on the cold thread-
    /// exit path (never on the alloc/dealloc hot path).
    ///
    /// Each sub-operation carries its own feature gate; in a build without
    /// the relevant feature the corresponding step compiles to nothing.
    pub(crate) fn trim_for_recycle(&mut self) {
        // Flush every tcache class → blocks return to segments → segments
        // may empty → decommit/release or pool.
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        self.flush_all_tcache();
        // Drain the small-segment hysteresis pool → release every pooled
        // segment to the OS. Evict the entire large cache → release every
        // cached span.
        #[cfg(feature = "alloc-decommit")]
        {
            self.core.drain_small_pool();
            self.core.evict_all();
        }
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
