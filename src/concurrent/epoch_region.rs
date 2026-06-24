//! [`EpochRegion<T>`] — fixed-capacity, lock-free reads, writer-serialised
//! writes, with `crossbeam-epoch` reclamation (Phase 3b-II).
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
//! - **Writers serialised** by an internal `Mutex` (writers are rare; only
//!   READS must be lock-free — that is the whole goal of this tier). The same
//!   `Mutex` owns the free-list bookkeeping, so writes are serialised against
//!   each other and the free list is mutated under that lock.
//! - **Reads are lock-free:** a reader pins an epoch guard and calls
//!   [`AtomicSlot::read_with`]; no mutex is taken.
//!
//! ## Reclamation & region drop
//!
//! Removed values are reclaimed by `crossbeam-epoch`: on
//! [`remove`](EpochRegion::remove) the old pointer is scheduled for destruction
//! via `guard.defer_destroy` and freed once no reader can still be holding it
//! (at an epoch boundary; if the process exits first they may not run their
//! destructors — the standard epoch caveat). Values still LIVE when the region
//! is dropped ARE dropped by [`EpochRegion`]'s `Drop` (under `&mut` exclusivity),
//! so I5 holds for them.

use std::sync::Mutex;

use crossbeam_epoch as epoch;

use crate::concurrent::hand::AtomicSlot;
use crate::concurrent::EpochHandle;

/// Writer-serialised bookkeeping: the free list (a stack of vacant slot
/// indices) and the live count. Held inside the writer `Mutex`, so the writer
/// that holds the lock owns these exclusively.
struct FreeState {
    /// Stack of indices of vacant, reusable slots. Retired (saturated) slots
    /// are never pushed back, so they vanish from circulation.
    free: Vec<u32>,
    /// Number of currently-live (occupied) entries.
    len: usize,
}

/// A fixed-capacity, handle-addressed store of `T` with **lock-free reads**,
/// writer-serialised writes, and `crossbeam-epoch` reclamation.
///
/// This is Phase 3b-II: the heavier lock-free design that admits the crate's
/// single confined `unsafe` organ ([`AtomicSlot<T>`]) in exchange for O(1)
/// per-slot writes (no snapshot clone, unlike [`LockFreeRegion`](super::LockFreeRegion)).
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
/// ## Invariants upheld
///
/// - **I1 — resolution:** a fresh [`EpochHandle<T>`] resolves to its value
///   until `remove`d.
/// - **I2 — tombstone:** after `remove(h)`, `get_with(h, …)` is `None` forever;
///   a second `remove(h)` is a no-op `false`.
/// - **I3 — no ABA:** `remove` **bumps the slot's generation** (via
///   [`AtomicSlot::evict`]), so a stale handle (slot reused) never resolves to
///   a live value.
/// - **I4 — accounting:** [`len`](Self::len) equals the number of live entries.
///
/// ## Concurrency notes
///
/// Writers are serialised by an internal `Mutex`. Under a read-mostly workload
/// (the target — per-packet lookups vastly outnumber connect/disconnect) this
/// is not on the hot path. Readers never contend on the mutex; they pin an
/// epoch guard and read a slot atomically.
pub struct EpochRegion<T> {
    slots: Box<[AtomicSlot<T>]>,
    state: Mutex<FreeState>,
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
            state: Mutex::new(FreeState { free, len: 0 }),
        }
    }

    /// Number of live values (I4).
    ///
    /// Under concurrency this is a momentary observation — a writer may change
    /// it immediately afterwards.
    ///
    /// # Panics
    ///
    /// Panics if the writer mutex is poisoned (a writer panicked while holding
    /// it). Readers are unaffected.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .expect("writer mutex poisoned")
            .len
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

    /// Inserts `value`, returning a fresh handle that resolves to it (I1), or
    /// `Err(value)` if the region is full (no vacant slot).
    ///
    /// Serialised against other writers; readers are never blocked. Claims a
    /// vacant slot from the free list, installs the value under a pinned epoch
    /// guard, and returns the handle carrying the slot's current generation.
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
        // Pop a vacant slot; if none, give the value back honestly (no panic).
        let Some(index) = state.free.pop() else {
            return Err(value);
        };
        let slot = &self.slots[index as usize];
        let generation = slot.install(value, &guard);
        state.len += 1;
        Ok(EpochHandle::new(index, generation))
    }

    /// Removes the value for `handle`, returning `true` if it was live (and is
    /// now tombstoned), or `false` if it was already stale/removed/out-of-range
    /// (I2 — a second remove is a no-op `false`).
    ///
    /// Serialised against other writers; readers are never blocked. Validates
    /// the slot's generation matches the handle (and the slot is non-vacant),
    /// evicts the value (scheduling epoch reclamation), bumps the generation
    /// (I3), and returns the slot to the free list unless it saturated (in
    /// which case it is retired).
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
        let mut state = self.state.lock().expect("writer mutex poisoned");
        let Some(slot) = self.slots.get(handle.index as usize) else {
            return false;
        };
        // Validate generation: a stale or already-removed handle has a
        // generation that no longer matches (I2/I3).
        if slot.generation() != handle.generation {
            return false;
        }
        let reused = slot.evict(&guard);
        state.len = state.len.checked_sub(1).expect("len underflow: double-remove?");
        if reused {
            // Generation was bumped below saturation: return the slot to the
            // free list for reuse.
            state.free.push(handle.index);
        }
        // else: slot saturated → retire (never reused). It simply stays out of
        // the free list.
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
    /// exactly once — on `remove` or on region drop). `&mut self` proves no
    /// reader or writer can race, so each occupied slot's value is taken and
    /// dropped directly. Values already `remove`d were handed to
    /// `crossbeam-epoch` and are reclaimed at an epoch boundary.
    fn drop(&mut self) {
        for slot in &mut self.slots {
            slot.drop_value();
        }
    }
}
