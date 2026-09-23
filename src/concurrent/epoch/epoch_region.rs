#![allow(deprecated)]
//! [`EpochRegion<T>`] — fixed-capacity, lock-free reads, writer-serialised
//! writes, with `crossbeam-epoch` reclamation (Phase 3b-II), extended in
//! Phase 7b with a **lock-free cross-thread removal** path.
//!
//! **Status — legacy/research-tier:** superseded by the production `alloc-xthread`
//! cross-thread free path; kept under the `experimental` feature for backward
//! compatibility and as a research baseline, and `#[deprecated]` on the struct
//! below. No new development is planned (see the `concurrent` module docs).
//!
//! This tier trades the zero-`unsafe` RCU of [`LockFreeRegion`](super::LockFreeRegion)
//! (3b-I) for **O(1) per-slot writes** (no snapshot clone) at the cost of the
//! crate's single confined `unsafe` organ, [`AtomicSlot<T>`] (see
//! [`hand`](super::hand)). All pointer/`unsafe` work lives in that one module;
//! this file is 100% safe code on top of [`AtomicSlot`]'s safe API.
//!
//! ## Design
//!
//! - **Fixed capacity:** `with_capacity(n)` allocates `n` slots up front in a
//!   boxed slice. There is NO growth. [`insert`](EpochRegion::insert) returns
//!   `Err(value)` when the region is full (no panic-on-full).
//! - **Writers serialised** by an internal `Mutex` — but in Phase 7b the mutex
//!   owns ONLY the free-list bookkeeping and the remote-free queue drain. The
//!   eviction itself (value swap + generation bump) is a single atomic CAS in
//!   [`AtomicSlot::try_evict_at`], which ANY thread may perform. So a
//!   cross-thread [`remote_evict`](Self::remote_evict) NEVER takes the owner
//!   mutex — it is lock-free.
//! - **Reads are lock-free:** a reader pins an epoch guard and calls
//!   [`AtomicSlot::read_with`]; no mutex is taken.
//!
//! ## Phase 7b — accounting under remote removal
//!
//! A remote remover must decrement the live count WITHOUT the owner mutex, so
//! [`len`](Self::len) is an [`AtomicUsize`] (per shard — `EpochRegion` is a
//! public standalone type, so the count lives here, not at the
//! `ShardedRegion`). [`insert`](Self::insert) does `fetch_add(1)`; any
//! successful [`try_evict_at`](AtomicSlot::try_evict_at) does `fetch_sub(1)`.
//!
//! The **free list stays owner-only**: a remote remover, after a successful
//! evict, ENQUEUES the freed index into a per-shard **remote-free queue**
//! (`Mutex<Vec<u32>>` — `crossbeam-queue` is not in the resolved dependency
//! tree, so per the plan we use a Mutex-guarded Vec drained by the owner; the
//! tradeoff is a brief lock on the remote push, but it is NOT the owner's
//! writer mutex, so the read path and the value-swap are untouched). The owner
//! drains the queue at the start of its next
//! [`insert`](Self::insert)/[`remove`](Self::remove) (single consumer).
//! Reusable-vs-retired (generation saturation at `u32::MAX`) is honored when
//! re-adding: a retired slot is never re-added.
//!
//! ## Reclamation & region drop
//!
//! Removed values are reclaimed by `crossbeam-epoch`: on
//! [`remove`](EpochRegion::remove)/[`remote_evict`](Self::remote_evict) the old
//! pointer is scheduled for destruction via `guard.defer_destroy` and freed
//! once no reader can still be holding it (at an epoch boundary; if the process
//! exits first they may not run their destructors — the standard epoch caveat).
//! Values still LIVE when the region is dropped ARE dropped by
//! [`EpochRegion`]'s `Drop` (under `&mut` exclusivity), so I5 holds for them.

use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Mutex;

use crossbeam_epoch as epoch;

use crate::concurrent::epoch::hand::{AtomicSlot, EvictOutcome};
use crate::concurrent::EpochHandle;

/// Source of [`EpochRegion::region_id`]: a process-global, monotonically
/// increasing counter, one tick per `EpochRegion` constructed (R2-03,
/// independent src review round 2, task #2005). Starts at 1 so `0` stays
/// unused (not currently relied upon as a sentinel, but kept free of the
/// live id space defensively). `Relaxed`: this counter establishes no
/// happens-before relationship of its own — it only needs each call to
/// observe a value no other call has ever observed, which `fetch_add`
/// already guarantees atomically regardless of ordering. A `u64` cannot
/// realistically wrap within a process's lifetime (billions of regions
/// constructed per second, sustained for centuries), so no overflow
/// handling is needed, unlike this tier's `u32` index/generation spaces.
static NEXT_EPOCH_REGION_ID: AtomicU64 = AtomicU64::new(1);

/// Writer-serialised bookkeeping: the free list (a stack of vacant slot
/// indices). Held inside the writer `Mutex`, so the writer that holds the lock
/// owns it exclusively. The live count is NO LONGER here (Phase 7b): it is an
/// `AtomicUsize` on the region so a remote remover can decrement it without
/// the mutex.
struct FreeState {
    /// Stack of indices of vacant, reusable slots. Retired (saturated) slots
    /// are never pushed back, so they vanish from circulation.
    free: Vec<u32>,
    /// R2-21 (independent src review round 2): owner-side scratch buffer the
    /// remote-free queue is swapped into by `drain_remote_free`. Swapping
    /// (instead of `core::mem::take`, which left the queue a fresh
    /// zero-capacity `Vec` and destroyed the old buffer) keeps ONE paid-for
    /// allocation on EACH side across drain cycles: the queue keeps the
    /// scratch's former buffer so producer pushes stop reallocating under the
    /// queue mutex, and the scratch keeps the queue's former buffer so the
    /// next drain needs no allocation either. Both capacities grow lazily to
    /// the largest burst seen and never shrink; after the first two drain
    /// cycles the steady state allocates nothing. Owner-only: lives behind
    /// the writer mutex like `free` (the drain runs with `&mut FreeState`).
    /// INVARIANT: empty at the start of every drain (drained to len 0 at the
    /// end of the previous one).
    drain_scratch: Vec<u32>,
}

/// A fixed-capacity, handle-addressed store of `T` with **lock-free reads**,
/// writer-serialised writes, and `crossbeam-epoch` reclamation — plus (Phase 7b)
/// a **lock-free cross-thread removal** path.
///
/// This is Phase 3b-II (extended in 7b): the lock-free design that admits the
/// crate's single confined `unsafe` organ (`AtomicSlot<T>`) in exchange for
/// O(1) per-slot writes (no snapshot clone, unlike
/// [`LockFreeRegion`](super::LockFreeRegion)).
///
/// ## Fixed capacity
///
/// `with_capacity(n)` allocates `n` slots up front; the region **does not
/// grow**. [`insert`](Self::insert) returns `Err(value)` when every slot is
/// occupied or retired — it does NOT panic on full. If a slot saturates its
/// generation counter (after `u32::MAX` reuses of that one slot — astronomically
/// many), it is retired and never reused, so the effective capacity may shrink
/// by one per saturated slot.
///
/// ## Phase 7b — cross-thread removal
///
/// [`remote_evict`](Self::remote_evict) lets ANY thread remove a handle without
/// taking the owner's writer mutex: it performs the generation-CAS eviction
/// (the single linearization point) and, on success, enqueues the freed index
/// into a remote-free queue the owner drains later. The owner's own
/// [`remove`](Self::remove) ALSO goes through the CAS path (it races remote
/// removers); the mutex now only serializes free-list/install bookkeeping.
///
/// ## Invariants upheld
///
/// - **I1 — resolution:** a fresh [`EpochHandle<T>`] resolves to its value
///   until `remove`d/`remote_evict`d.
/// - **I2 — tombstone:** after `remove(h)`/`remote_evict(h)`,
///   `get_with(h, …)` is `None` forever; a second remove is a no-op `false`.
/// - **I3 — no ABA:** `remove`/`remote_evict` **bumps the slot's generation**
///   (via `AtomicSlot::try_evict_at`), so a stale handle (slot reused) never
///   resolves to a live value.
/// - **I4 — accounting:** [`len`](Self::len) equals the number of live entries
///   (now an `AtomicUsize`, correct under concurrent remote removal).
///
/// ## Concurrency notes
///
/// Writers' free-list/install bookkeeping is serialised by an internal
/// `Mutex`; the eviction itself is a lock-free CAS. Readers never contend on
/// the mutex; they pin an epoch guard and read a slot atomically.
#[deprecated(
    since = "0.1.0",
    note = "concurrent regions are legacy/research-tier; use the production allocator stack (`alloc-xthread`) for cross-thread allocation needs"
)]
pub struct EpochRegion<T> {
    /// R2-03 (independent src review round 2, task #2005): this region's
    /// never-reused instance identity, assigned from
    /// [`NEXT_EPOCH_REGION_ID`] at construction. Stamped into every handle
    /// this region mints ([`EpochHandle::new`]) and checked FIRST — before
    /// any slot lookup or generation CAS — by every operation that takes a
    /// handle, so a handle minted by a DIFFERENT `EpochRegion<T>` is
    /// rejected outright rather than being evaluated against this region's
    /// slot table (where its raw `(index, generation)` may coincidentally
    /// match a real slot). See the struct-level and `EpochHandle` docs for
    /// the concrete cross-instance hazard this closes.
    region_id: u64,
    slots: Box<[AtomicSlot<T>]>,
    /// Writer-only bookkeeping (free list). The eviction and the live count
    /// are NOT under this lock (Phase 7b): the evict is a CAS, and `len` is an
    /// `AtomicUsize` on the region.
    state: Mutex<FreeState>,
    /// Per-shard remote-free queue: indices freed by a NON-OWNER thread via
    /// `remote_evict`. The owner drains this at the start of its next op
    /// (single consumer). `Mutex<Vec<u32>>` because `crossbeam-queue` is not
    /// in the resolved dependency tree (see the module docs for the tradeoff).
    /// R2-21 (independent src review round 2): the drain now SWAPS this queue
    /// with the owner-side [`FreeState::drain_scratch`] instead of taking it,
    /// so this buffer's capacity survives every non-empty drain (see
    /// `drain_remote_free`).
    remote_free: Mutex<Vec<u32>>,
    /// "`remote_free` may be non-empty" hint (#1989), so the owner's drain can
    /// skip acquiring [`Self::remote_free`]'s lock entirely in the common
    /// zero-remote-traffic case. Set to `true` by `remote_evict` AFTER its
    /// push releases the queue lock; cleared by `drain_remote_free` while
    /// HOLDING that lock, before it takes the queue. Relaxed throughout: it
    /// orders no data — the queue's own mutex carries every happens-before
    /// edge — and a lost race only costs one spurious lock acquisition or one
    /// owner-op of extra drain latency, never a lost index (the full argument
    /// is in `drain_remote_free`).
    remote_free_pending: core::sync::atomic::AtomicBool,
    /// Number of currently-live (occupied) entries. `AtomicUsize` so a remote
    /// remover can decrement it without the owner mutex (Phase 7b).
    len: AtomicUsize,
}

impl<T> EpochRegion<T> {
    /// Creates a region with `capacity` vacant slots pre-allocated.
    ///
    /// The region **does not grow**: this is the maximum number of simultaneously
    /// live entries (modulo generation-saturation retirement). `insert` returns
    /// `Err(value)` when full rather than panicking.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` overflows `u32` (the index space).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        u32::try_from(capacity).expect("capacity overflows u32 index space");
        let slots: Vec<AtomicSlot<T>> = (0..capacity).map(|_| AtomicSlot::vacant()).collect();
        // Free list starts with every slot, in ascending index order so the
        // first inserts claim the lowest indices.
        let free: Vec<u32> = (0..capacity)
            .map(|i| u32::try_from(i).expect("index fits u32 (checked above)"))
            .collect();
        Self {
            region_id: NEXT_EPOCH_REGION_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
            slots: slots.into_boxed_slice(),
            state: Mutex::new(FreeState {
                free,
                drain_scratch: Vec::new(),
            }),
            remote_free: Mutex::new(Vec::new()),
            remote_free_pending: core::sync::atomic::AtomicBool::new(false),
            len: AtomicUsize::new(0),
        }
    }

    /// Number of live values (I4).
    ///
    /// Under concurrency this is a momentary observation — a writer or a remote
    /// remover may change it immediately afterwards.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Whether the region holds no live values (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Resolves `handle` and applies `f` to a shared borrow of the value,
    /// returning `Some(f(...))`, or `None` if the handle is stale/removed/
    /// out-of-range (I1, I2, I3).
    ///
    /// **Lock-free:** pins an epoch guard and reads the slot atomically; never
    /// takes the writer mutex. The borrow is confined to the call — `f` may not
    /// store the reference.
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Sync` —
    /// `read_with` may share `&T` across concurrently-calling threads; see
    /// its own doc comment (`hand.rs`) for the full rationale.
    ///
    /// R2-03 (independent src review round 2, task #2005): rejects a handle
    /// minted by a DIFFERENT `EpochRegion<T>` instance (`region_id`
    /// mismatch) before any slot lookup — see the struct doc.
    pub fn get_with<R>(&self, handle: EpochHandle<T>, f: impl FnOnce(&T) -> R) -> Option<R>
    where
        T: Sync,
    {
        if handle.region_id != self.region_id {
            return None;
        }
        let guard = epoch::pin();
        let slot = self.slots.get(handle.index as usize)?;
        slot.read_with(handle.generation, &guard, f)
    }

    /// Convenience: resolves `handle` and returns a clone of the value, or
    /// `None` if stale/removed/out-of-range. Lock-free, like
    /// [`get_with`](Self::get_with).
    pub fn get_cloned(&self, handle: EpochHandle<T>) -> Option<T>
    where
        T: Clone + Sync,
    {
        self.get_with(handle, T::clone)
    }

    /// Drains the per-shard remote-free queue into the owner's free list
    /// (single-consumer). Called by the owner at the start of its next op.
    ///
    /// Honors reusable-vs-retired: both push sites
    /// ([`remove`](Self::remove) and [`remote_evict`](Self::remote_evict))
    /// already gate on `EvictOutcome::Evicted { reusable: true }` before
    /// enqueuing an index here, so in practice every index in the queue is
    /// reusable. This drain additionally RE-CHECKS each index's current
    /// generation and skips (drops, does not re-add) any index that reads
    /// `u32::MAX` — a defensive belt, not the primary gate: it protects
    /// against ever handing a retired (saturated) slot back into circulation
    /// even if a future push site stops gating on `reusable`. A skipped index
    /// is simply not pushed to `state.free`; the slot stays vacant at gen
    /// `u32::MAX` forever (intentionally abandoned — see `try_evict_at`).
    ///
    /// The buffer handover is a SWAP with the owner-side
    /// [`FreeState::drain_scratch`] (R2-21, independent src review round 2), not
    /// a `core::mem::take`: taking would hand the queue's buffer to the
    /// (short-lived) local and leave the queue a fresh zero-capacity `Vec`, so
    /// every remote-free burst re-grew it — allocating under the remote-queue
    /// mutex, the one lock producers contend on. The swap keeps one paid-for
    /// buffer on each side across drain cycles; see the `drain_scratch` doc for
    /// the steady-state argument.
    fn drain_remote_free(&self, state: &mut FreeState) {
        // Fast path: no remote frees → no lock acquisition. Before #1989 this
        // comment described an optimization that did not exist — the lock was
        // taken unconditionally and the `is_empty()` check happened INSIDE it,
        // so every owner `insert`/`remove` paid a second mutex acquisition
        // (after the writer mutex) even with zero remote traffic, contending
        // the very cache line remote evictors hammer.
        //
        // The flag is a plain `AtomicBool`, not the pending-COUNT the review
        // sketched: a count incremented AFTER the push can underflow, because
        // a drain may take an item whose increment has not landed yet and then
        // subtract it (counter 1, pusher pushes item 2 without having
        // incremented, drainer takes 2 and subtracts 2 → wrap). A boolean has
        // no such arithmetic and is sufficient — the drain takes the WHOLE
        // queue, so "something is pending" is all it needs to know.
        //
        // Why no index can be stranded: an index is pushed while holding the
        // queue lock, and the flag is set to `true` only AFTER that lock is
        // released; the drain clears the flag while HOLDING the lock and takes
        // the queue in the same critical section. So for any pushed index
        // either (a) it was already in the queue when the drain took it — it is
        // drained now — or (b) it was not, in which case its pusher has still
        // to run its `store(true)`, leaving the flag set for the next drain.
        // The only cost of losing the race is a spurious `true` (one wasted
        // lock acquisition on the next owner op) or a deferred index (drained
        // one owner op later) — the same eventual-drain cadence this design
        // already accepts. An index can never be lost.
        if !self
            .remote_free_pending
            .load(core::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        // We peek-lock: swap the queue with the owner's scratch buffer and
        // release the lock; the index-by-index transfer into the free list
        // then happens OUTSIDE the queue lock. The swap (not
        // `core::mem::take`, R2-21) is O(1) — a pointer exchange — so the
        // critical section does not grow, and it leaves a buffer on EACH
        // side: the queue keeps the scratch's former buffer (producer pushes
        // stop reallocating under this mutex once warm) and the scratch keeps
        // the queue's, so the next drain allocates nothing either. The
        // scratch's clearing (`drain(..)` below) also stays outside the lock.
        {
            let mut q = match self.remote_free.lock() {
                Ok(q) => q,
                // A remote remover panicked while holding the queue lock. The
                // queue may be poisoned, but the indices it already pushed are
                // still valid free slots. We treat poison as "drain what's
                // there" — `lock().unwrap_or_else(|e| e.into_inner())`.
                Err(e) => e.into_inner(),
            };
            // Clear the flag while HOLDING the lock, BEFORE taking the queue —
            // see the stranding argument above. A pusher that has not yet run
            // its `store(true)` will set it again after we release the lock.
            self.remote_free_pending
                .store(false, core::sync::atomic::Ordering::Relaxed);
            if q.is_empty() {
                return;
            }
            debug_assert!(
                state.drain_scratch.is_empty(),
                "drain scratch must be empty between drains",
            );
            core::mem::swap(&mut *q, &mut state.drain_scratch);
        }
        // The scratch now holds the drained indices (and the queue's former
        // buffer); `drain(..)` yields them in FIFO order like the old
        // take-and-iterate while KEEPING the buffer's capacity for the next
        // cycle (a plain `for index in ...` over a taken Vec would destroy
        // it — the exact R2-21 defect).
        for index in state.drain_scratch.drain(..) {
            // Defensive re-check: a retired (gen == u32::MAX) slot must never
            // re-enter the free list (see doc above). `AtomicSlot::generation`
            // is an Acquire load — cheap, and this loop is already off the hot
            // path (mutex held, remote-free queue drain).
            let Some(slot) = self.slots.get(index as usize) else {
                continue;
            };
            if slot.generation() == u32::MAX {
                continue;
            }
            state.free.push(index);
        }
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1), or
    /// `Err(value)` if the region is full (no vacant slot).
    ///
    /// Serialised against other writers for free-list bookkeeping; readers are
    /// never blocked. First drains the remote-free queue (a remote remover may
    /// have freed slots since the owner's last op), then claims a vacant slot,
    /// installs the value under a pinned epoch guard, and returns the handle
    /// carrying the slot's current generation.
    ///
    /// # Errors
    ///
    /// Returns `Err(value)` (handing the value back unchanged) when the region
    /// is full — every slot is occupied or retired. The region does not grow,
    /// so a full region stays full until a slot is `remove`d.
    ///
    /// # Panics
    ///
    /// Panics if the writer mutex is poisoned (a writer panicked while holding
    /// it). Readers are unaffected.
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Send +
    /// 'static` — an inserted value may later be reclaimed via
    /// `AtomicSlot::install`'s eventual `defer_destroy`; see that method's
    /// doc comment (`hand.rs`) for the full rationale.
    pub fn insert(&self, value: T) -> Result<EpochHandle<T>, T>
    where
        T: Send + 'static,
    {
        let mut state = self.state.lock().expect("writer mutex poisoned");
        // Owner drains any indices a remote remover freed since its last op
        // (single-consumer drain). This is what makes a remote `remote_evict`
        // eventually visible to the owner's free list.
        self.drain_remote_free(&mut state);
        // Pop a vacant slot; if none, give the value back honestly (no panic).
        let Some(index) = state.free.pop() else {
            return Err(value);
        };
        let slot = &self.slots[index as usize];
        let generation = slot.install(value);
        // fetch_add (not the mutex-guarded `state.len`) so a concurrent remote
        // remover's fetch_sub races correctly (Phase 7b accounting).
        self.len.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(EpochHandle::new(self.region_id, index, generation))
    }

    /// Removes the value for `handle` from the OWNER thread, returning `true`
    /// if it was live (and is now tombstoned), or `false` if it was already
    /// stale/removed/out-of-range (I2 — a second remove is a no-op `false`).
    ///
    /// **Phase 7b:** the owner's remove path ALSO races remote removers, so it
    /// can no longer rely on the writer mutex for the evict itself. The mutex
    /// is taken ONLY to serialize free-list bookkeeping (and to drain the
    /// remote-free queue); the eviction goes through
    /// [`AtomicSlot::try_evict_at`] (the single linearization CAS), which
    /// returns `Stale` if a remote remover won the race — in which case this
    /// returns `false` (the handle was already removed) and does NOT touch the
    /// free list or `len`.
    ///
    /// On a successful evict: the generation is bumped (I3), the value is
    /// scheduled for epoch reclamation, `len` is decremented, and the slot is
    /// returned to the free list unless it saturated (retired).
    ///
    /// The removed value is reclaimed by `crossbeam-epoch` (its destructor runs
    /// once no reader can still hold it); [`remove`](Self::remove) returns a
    /// `bool`, not the value.
    ///
    /// # Panics
    ///
    /// Panics if the writer mutex is poisoned. Readers are unaffected.
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Send +
    /// 'static` — this calls `AtomicSlot::try_evict_at`, which may
    /// `defer_destroy` the removed value; see that method's doc comment
    /// (`hand.rs`) for the full rationale.
    ///
    /// R2-03 (independent src review round 2, task #2005): rejects a handle
    /// minted by a DIFFERENT `EpochRegion<T>` instance (`region_id`
    /// mismatch) before any slot lookup or generation CAS — see the struct
    /// doc.
    pub fn remove(&self, handle: EpochHandle<T>) -> bool
    where
        T: Send + 'static,
    {
        if handle.region_id != self.region_id {
            return false;
        }
        let guard = epoch::pin();
        let Some(slot) = self.slots.get(handle.index as usize) else {
            return false;
        };
        // The eviction itself is the CAS linearization point — NO mutex held
        // here, so a remote remover can race it correctly. `try_evict_at`
        // returns Stale if a remote remover already transitioned the
        // generation.
        let outcome = slot.try_evict_at(handle.generation, &guard);
        if outcome == EvictOutcome::Stale {
            // Already removed (by us earlier, or by a remote remover). I2: a
            // second remove is a no-op false. No len/free-list mutation.
            return false;
        }
        // We won the CAS: decrement len, and re-add the slot to the free list
        // unless it saturated. These mutations happen UNDER the writer mutex —
        // they are owner-only bookkeeping (a remote remover uses
        // `remote_evict`, which enqueues to the remote-free queue instead).
        let reusable = matches!(outcome, EvictOutcome::Evicted { reusable: true });
        self.len.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        let mut state = self.state.lock().expect("writer mutex poisoned");
        // Drain remote frees opportunistically (cheap if empty) so the owner's
        // free list stays current even under cross-thread churn.
        self.drain_remote_free(&mut state);
        if reusable {
            state.free.push(handle.index);
        }
        // else: slot saturated → retire (never reused). It stays out of the
        // free list.
        true
    }

    /// **Phase 7b:** lock-free cross-thread removal. Any thread (owner OR
    /// remote) may call this to remove `handle` WITHOUT taking the owner's
    /// writer mutex. Performs the generation-CAS eviction (the single
    /// linearization point), and on success enqueues the freed index into the
    /// per-shard remote-free queue (which the owner drains on its next op) and
    /// decrements `len`. Returns `true` if this call evicted a live value,
    /// `false` if the handle was already stale/removed/out-of-range (I2).
    ///
    /// This is the path [`ShardedRegion`](crate::concurrent::ShardedRegion)
    /// uses for a `remove` whose owning shard is not the calling thread's
    /// shard — it does NOT contend on the owner shard's writer mutex.
    ///
    /// # Why this is sound
    ///
    /// Soundness rests entirely on [`AtomicSlot::try_evict_at`]'s generation
    /// CAS: exactly one thread can win the CAS at `handle.generation`, so
    /// exactly one thread schedules `defer_destroy` and decrements `len`. A
    /// concurrent owner `remove` or another `remote_evict` for the same handle
    /// fails the CAS and returns `false` (no double-free, no double-decrement).
    /// See the `try_evict_at` SAFETY proof for the no-reinstall argument.
    ///
    /// R2-02 (independent src review round 2, task #2004): `T: Send +
    /// 'static` — same `defer_destroy` rationale as [`remove`](Self::remove);
    /// see `AtomicSlot::install`'s doc comment (`hand.rs`) for the full
    /// argument.
    ///
    /// R2-03 (independent src review round 2, task #2005): rejects a handle
    /// minted by a DIFFERENT `EpochRegion<T>` instance (`region_id`
    /// mismatch) before any slot lookup or generation CAS — see the struct
    /// doc.
    pub(crate) fn remote_evict(&self, handle: EpochHandle<T>) -> bool
    where
        T: Send + 'static,
    {
        if handle.region_id != self.region_id {
            return false;
        }
        let guard = epoch::pin();
        let Some(slot) = self.slots.get(handle.index as usize) else {
            return false;
        };
        let outcome = slot.try_evict_at(handle.generation, &guard);
        if outcome == EvictOutcome::Stale {
            return false;
        }
        // We won the CAS uniquely. Decrement len (races the owner's fetch_add
        // and other removers' fetch_sub correctly — both are atomic).
        self.len.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        // Enqueue the freed index for the owner to drain — ONLY if reusable.
        // `try_evict_at` reports `reusable: false` when the eviction landed the
        // slot's generation on `u32::MAX` (saturated/retired): such a slot must
        // never be pushed, since `install` never mints a handle on a retired
        // slot and no future `try_evict_at` on it can do real work (it hits the
        // upfront `Stale` guard). `drain_remote_free` also re-checks
        // defensively, but the primary gate is here.
        let reusable = matches!(outcome, EvictOutcome::Evicted { reusable: true });
        if reusable {
            // Only reusable slots are worth re-adding; a retired slot is
            // intentionally abandoned (it stays vacant at gen MAX forever).
            match self.remote_free.lock() {
                Ok(mut q) => q.push(handle.index),
                // Poison: a remote remover panicked mid-push. The index is
                // still a valid free slot; recover the queue and push anyway.
                Err(e) => e.into_inner().push(handle.index),
            }
            // #1989: publish "the queue may be non-empty" AFTER the push has
            // released the queue lock, so the owner's `drain_remote_free` can
            // skip locking when no remote evictor has run. Relaxed: this flag
            // orders no data (the queue mutex does), and the ordering argument
            // for why no index can be stranded is in `drain_remote_free`.
            self.remote_free_pending
                .store(true, core::sync::atomic::Ordering::Relaxed);
        }
        true
    }

    /// **Diagnostics/testing only.** Force-sets the generation of the slot at
    /// `index` to `generation`, bypassing the normal install/evict protocol.
    ///
    /// This exists SOLELY so an integration test (`tests/`) can reach the
    /// generation-saturation edge of [`AtomicSlot::try_evict_at`] (a slot
    /// whose eviction lands its generation on `u32::MAX`, which must be
    /// retired rather than reused — UBFIX-9 / M-8) without driving ~4 billion
    /// real insert/remove cycles, which the project's short-scenario test
    /// policy forbids. Same established `#[doc(hidden)]` test-only-export
    /// pattern as [`ShardedRegion`](crate::concurrent::ShardedRegion)'s
    /// `_reset_my_shard_binding_for_tests`.
    ///
    /// Production code MUST NEVER call this. The caller MUST only target a
    /// slot it knows is currently vacant (e.g. index `0` on a freshly
    /// `with_capacity`-created region, before any insert).
    ///
    /// R2-04 (independent src review round 2, task #2006): previously took
    /// `&self`, reachable under plain `experimental` (no `internals`
    /// needed) on any `&EpochRegion<T>` — a call could hand an old, already-
    /// stale handle CAS rights back (including mid-race with a concurrent
    /// eviction), and the CODE never enforced the prose-only "vacant slots
    /// only" contract, so calling it on an occupied slot silently desynced
    /// the generation from the installed value's true generation (a self-
    /// inflicted ABA hazard). Now takes `&mut self`: the borrow checker
    /// proves EXCLUSIVE access to the WHOLE region for the call (no other
    /// thread can hold any reference to it, structurally ruling out the
    /// mid-eviction race), and [`AtomicSlot::set_generation_for_tests`]
    /// asserts the target slot is actually vacant before touching the
    /// generation.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range for this region's capacity, or if
    /// the target slot currently holds a live value (is occupied).
    #[doc(hidden)]
    pub fn _set_slot_generation_for_tests(&mut self, index: u32, generation: u32) {
        self.slots[index as usize].set_generation_for_tests(generation);
    }

    /// **Diagnostics/testing only.** Identity of the remote-free queue's
    /// backing buffer, captured under the queue lock: `(data pointer as
    /// usize, len, capacity)`.
    ///
    /// Exists SOLELY so an integration test can prove (R2-21) that
    /// `drain_remote_free` REUSES the queue's buffer across drain cycles
    /// (pointer and capacity survive a non-empty drain) instead of replacing
    /// it with a fresh zero-capacity `Vec`. The pointer value is for identity
    /// comparison ONLY — it MUST NOT be dereferenced, offset, or converted
    /// back to a reference.
    #[doc(hidden)]
    pub fn _remote_free_queue_buffer_identity_for_tests(&self) -> (usize, usize, usize) {
        let q = match self.remote_free.lock() {
            Ok(q) => q,
            Err(e) => e.into_inner(),
        };
        (q.as_ptr() as usize, q.len(), q.capacity())
    }
}

impl<T> Default for EpochRegion<T> {
    fn default() -> Self {
        // A zero-capacity region: every insert returns Err. This is a sensible
        // Default (no allocation) and matches the fixed-capacity contract.
        Self::with_capacity(0)
    }
}

impl<T> Drop for EpochRegion<T> {
    /// Drops every still-live value, upholding I5 (every value is dropped
    /// exactly once — on `remove`/`remote_evict` or on region drop). `&mut
    /// self` proves no reader or writer can race, so each occupied slot's value
    /// is taken and dropped directly. Values already `remove`d/`remote_evict`d
    /// were handed to `crossbeam-epoch` and are reclaimed at an epoch boundary.
    fn drop(&mut self) {
        for slot in &mut self.slots {
            slot.drop_value();
        }
    }
}
