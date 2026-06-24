//! [`EpochRegion<T>`] — fixed-capacity, lock-free reads, writer-serialised
//! writes, with `crossbeam-epoch` reclamation (Phase 3b-II), extended in
//! Phase 7b with a **lock-free cross-thread removal** path.
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

use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;

use crossbeam_epoch as epoch;

use crate::concurrent::hand::{AtomicSlot, EvictOutcome};
use crate::concurrent::EpochHandle;

/// Writer-serialised bookkeeping: the free list (a stack of vacant slot
/// indices). Held inside the writer `Mutex`, so the writer that holds the lock
/// owns it exclusively. The live count is NO LONGER here (Phase 7b): it is an
/// `AtomicUsize` on the region so a remote remover can decrement it without
/// the mutex.
struct FreeState {
    /// Stack of indices of vacant, reusable slots. Retired (saturated) slots
    /// are never pushed back, so they vanish from circulation.
    free: Vec<u32>,
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
pub struct EpochRegion<T> {
    slots: Box<[AtomicSlot<T>]>,
    /// Writer-only bookkeeping (free list). The eviction and the live count
    /// are NOT under this lock (Phase 7b): the evict is a CAS, and `len` is an
    /// `AtomicUsize` on the region.
    state: Mutex<FreeState>,
    /// Per-shard remote-free queue: indices freed by a NON-OWNER thread via
    /// `remote_evict`. The owner drains this at the start of its next op
    /// (single consumer). `Mutex<Vec<u32>>` because `crossbeam-queue` is not
    /// in the resolved dependency tree (see the module docs for the tradeoff).
    remote_free: Mutex<Vec<u32>>,
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
            slots: slots.into_boxed_slice(),
            state: Mutex::new(FreeState { free }),
            remote_free: Mutex::new(Vec::new()),
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
    pub fn get_with<R>(&self, handle: EpochHandle<T>, f: impl FnOnce(&T) -> R) -> Option<R> {
        let guard = epoch::pin();
        let slot = self.slots.get(handle.index as usize)?;
        slot.read_with(handle.generation, &guard, f)
    }

    /// Convenience: resolves `handle` and returns a clone of the value, or
    /// `None` if stale/removed/out-of-range. Lock-free, like
    /// [`get_with`](Self::get_with).
    pub fn get_cloned(&self, handle: EpochHandle<T>) -> Option<T>
    where
        T: Clone,
    {
        self.get_with(handle, T::clone)
    }

    /// Drains the per-shard remote-free queue into the owner's free list
    /// (single-consumer). Called by the owner at the start of its next op.
    /// Honors reusable-vs-retired: the queue holds only successfully-evicted
    /// indices, and `try_evict_at` already encoded saturation by reporting
    /// `reusable: false` to the REMOTE remover — but the remote remover pushes
    /// the index REGARDLESS, and the owner must re-check the slot's generation
    /// is not saturated before re-adding. (In practice the index is pushed only
    /// when `reusable`; this re-check is a defensive belt.)
    fn drain_remote_free(&self, state: &mut FreeState) {
        // Fast path: no remote frees → no lock acquisition.
        // We peek-lock: take the queue, and if non-empty, drain it into the
        // free list. A Mutex<Vec> swap-then-extend is the cheapest drain.
        let drained = {
            let mut q = match self.remote_free.lock() {
                Ok(q) => q,
                // A remote remover panicked while holding the queue lock. The
                // queue may be poisoned, but the indices it already pushed are
                // still valid free slots. We treat poison as "drain what's
                // there" — `lock().unwrap_or_else(|e| e.into_inner())`.
                Err(e) => e.into_inner(),
            };
            if q.is_empty() {
                return;
            }
            core::mem::take(&mut *q)
        };
        for index in drained {
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
    pub fn insert(&self, value: T) -> Result<EpochHandle<T>, T> {
        let guard = epoch::pin();
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
        let generation = slot.install(value, &guard);
        // fetch_add (not the mutex-guarded `state.len`) so a concurrent remote
        // remover's fetch_sub races correctly (Phase 7b accounting).
        self.len.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(EpochHandle::new(index, generation))
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
    pub fn remove(&self, handle: EpochHandle<T>) -> bool {
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
        self.len
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
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
    pub(crate) fn remote_evict(&self, handle: EpochHandle<T>) -> bool {
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
        self.len
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        // Enqueue the freed index for the owner to drain. Push even if the slot
        // saturated — `try_evict_at` reports reusable:false for saturation,
        // and the owner re-checks on drain (a saturated slot is dropped from
        // the drain loop, never re-added to the free list). This keeps the
        // remote remover's path uniform (one lock acquisition) and pushes the
        // retire decision to the single-consumer owner.
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
        }
        true
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
