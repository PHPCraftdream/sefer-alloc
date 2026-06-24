//! # sefer-alloc
//!
//! A safe, *handle-addressed* region store. Instead of handing out raw
//! pointers, a [`Region<T>`] hands out small generational [`Handle<T>`]
//! values; the bytes live in a dense, cache-friendly backing store that the
//! region is free to move. A stale handle never resolves to a live value — it
//! returns `None`, never undefined behaviour.
//!
//! ## The three organs
//!
//! - **Cartographer** (safe): all placement / free-list logic — pure integer
//!   arithmetic over indices, never touching memory.
//! - **Membrane** (safe): the typed API ([`Handle`], generation checks) —
//!   *total*; it cannot express UB.
//! - **Hand** (unsafe): a single, audited organ that touches raw memory — and
//!   it appears *only* in the lower tiers (the concurrent epoch tier and the
//!   byte / allocator mode). The typed, single-threaded core in this file is
//!   `#![forbid(unsafe_code)]`: the upper world is pure.
//!
//! ## Scope (honest)
//!
//! This is an *application-level* store, not a drop-in global allocator. The
//! global-allocator descent (`ByteRegion` + `GlobalAlloc`) is a later,
//! research-flagged phase; see `docs/PLAN.md`. For a process-wide allocator,
//! reach for `mimalloc`.
//!
//! See `docs/INVARIANTS.md` for the safety invariants this crate upholds and
//! `docs/DESIGN.md` for the architecture.
//!
//! ## Example
//!
//! ```
//! use sefer_alloc::Region;
//!
//! let mut region = Region::new();
//! let a = region.insert("alpha");
//! let b = region.insert("beta");
//!
//! assert_eq!(region.get(a), Some(&"alpha"));
//!
//! region.remove(a);
//! assert_eq!(region.get(a), None); // stale handle → None, never UB
//! assert_eq!(region.get(b), Some(&"beta")); // others stay valid
//! ```

#![forbid(unsafe_code)]

use core::marker::PhantomData;

/// An opaque, copyable reference to a value stored in a [`Region`].
///
/// A handle is an index plus a generation. It is `Copy` and unconditionally
/// `Send + Sync` regardless of `T` — it owns no `T`, it only names one. The
/// `PhantomData<fn() -> T>` keeps the handle *typed* (so a `Handle<A>` cannot
/// be passed to a `Region<B>`) while staying covariant in `T` and free of any
/// drop/auto-trait obligations.
pub struct Handle<T> {
    index: u32,
    generation: u32,
    _ty: PhantomData<fn() -> T>,
}

// Hand-written impls: a handle is "two `u32`s", so these must hold for *every*
// `T`, not only `T: Clone`/`Eq`/… that `#[derive]` would (wrongly) require.
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}
impl<T> Eq for Handle<T> {}
impl<T> core::hash::Hash for Handle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<T> core::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle")
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}

/// One entry in the stable slot array. The `generation` is bumped every time
/// the slot is vacated, so any handle minted against an older generation is
/// detectably stale.
struct SlotEntry {
    generation: u32,
    state: SlotState,
}

enum SlotState {
    /// Live: points at the value's position in the dense array.
    Occupied { dense: u32 },
    /// Free: threaded into the free list (`None` = end of list).
    Vacant { next_free: Option<u32> },
}

/// A handle-addressed store of `T`.
///
/// Values live in a **dense** `Vec<T>` (always compact — the cache-friendly,
/// fragmentation-free layout), while a parallel **slot** array provides stable
/// handles with generation checks. All operations are `O(1)`.
///
/// Capacity note: indices are `u32`, so a region holds up to `u32::MAX`
/// entries. The generation counter wraps after `2^32` reuses of a single slot;
/// a handle that survives that many reuses of *its* slot could alias (the
/// standard generational-arena caveat). Slot retirement at saturation is a
/// planned hardening step (see `docs/PLAN.md`, Phase 1 gate).
pub struct Region<T> {
    /// Stable address space: `handle.index` indexes here.
    slots: Vec<SlotEntry>,
    /// Compact value storage.
    dense: Vec<T>,
    /// `dense_to_slot[i]` is the slot index that owns `dense[i]` — the back
    /// pointer that lets `remove` fix up the element a swap-remove moved.
    dense_to_slot: Vec<u32>,
    /// Head of the vacant-slot free list.
    free_head: Option<u32>,
}

impl<T> Region<T> {
    /// Creates an empty region that allocates nothing until first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            dense: Vec::new(),
            dense_to_slot: Vec::new(),
            free_head: None,
        }
    }

    /// Creates an empty region with space pre-reserved for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            dense: Vec::with_capacity(capacity),
            dense_to_slot: Vec::with_capacity(capacity),
            free_head: None,
        }
    }

    /// Number of live values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    /// Whether the region holds no live values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// Current value-storage capacity, in entries.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.dense.capacity()
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    pub fn insert(&mut self, value: T) -> Handle<T> {
        let dense = self.dense.len() as u32;
        self.dense.push(value);

        match self.free_head {
            // Reuse a vacated slot — its generation was already bumped on the
            // removal that freed it, so old handles to it stay stale (I3).
            Some(slot_idx) => {
                let entry = &mut self.slots[slot_idx as usize];
                let next_free = match entry.state {
                    SlotState::Vacant { next_free } => next_free,
                    SlotState::Occupied { .. } => {
                        unreachable!("free list head pointed at an occupied slot")
                    }
                };
                self.free_head = next_free;
                entry.state = SlotState::Occupied { dense };
                self.dense_to_slot.push(slot_idx);
                Handle {
                    index: slot_idx,
                    generation: entry.generation,
                    _ty: PhantomData,
                }
            }
            // No free slot: grow the stable array.
            None => {
                let slot_idx = self.slots.len() as u32;
                self.slots.push(SlotEntry {
                    generation: 0,
                    state: SlotState::Occupied { dense },
                });
                self.dense_to_slot.push(slot_idx);
                Handle {
                    index: slot_idx,
                    generation: 0,
                    _ty: PhantomData,
                }
            }
        }
    }

    /// Borrows the value for `handle`, or `None` if the handle is stale or
    /// removed (I1, I2, I3).
    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let entry = self.slots.get(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        match entry.state {
            SlotState::Occupied { dense } => Some(&self.dense[dense as usize]),
            SlotState::Vacant { .. } => None,
        }
    }

    /// Mutably borrows the value for `handle`, or `None` if stale/removed.
    #[must_use]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let entry = self.slots.get(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        match entry.state {
            SlotState::Occupied { dense } => Some(&mut self.dense[dense as usize]),
            SlotState::Vacant { .. } => None,
        }
    }

    /// Whether `handle` currently resolves to a live value.
    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.get(handle).is_some()
    }

    /// Removes and returns the value for `handle`, or `None` if it is already
    /// stale/removed. After this, `handle` resolves to `None` forever (I2).
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let entry = self.slots.get_mut(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        let dense = match entry.state {
            SlotState::Occupied { dense } => dense,
            SlotState::Vacant { .. } => return None,
        };

        // Tombstone the slot: bump generation (invalidates this and any other
        // handle to this slot — I3) and thread it back into the free list.
        entry.generation = entry.generation.wrapping_add(1);
        entry.state = SlotState::Vacant {
            next_free: self.free_head,
        };
        self.free_head = Some(handle.index);

        // Swap-remove from the dense arrays, then repair the back pointer of
        // whatever element was moved into the hole.
        let value = self.dense.swap_remove(dense as usize);
        self.dense_to_slot.swap_remove(dense as usize);
        if (dense as usize) < self.dense.len() {
            let moved_slot = self.dense_to_slot[dense as usize];
            match &mut self.slots[moved_slot as usize].state {
                SlotState::Occupied { dense: d } => *d = dense,
                SlotState::Vacant { .. } => {
                    unreachable!("a dense element mapped to a vacant slot")
                }
            }
        }
        Some(value)
    }

    /// Iterates the live values in dense (cache-friendly) order. The order is
    /// unspecified and changes as elements are removed.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.dense.iter()
    }

    /// Mutably iterates the live values in dense order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.dense.iter_mut()
    }

    /// Removes every value, invalidating all outstanding handles, while
    /// retaining allocated capacity.
    pub fn clear(&mut self) {
        self.dense.clear();
        self.dense_to_slot.clear();
        self.free_head = None;
        for i in 0..self.slots.len() {
            let entry = &mut self.slots[i];
            if let SlotState::Occupied { .. } = entry.state {
                entry.generation = entry.generation.wrapping_add(1);
            }
            entry.state = SlotState::Vacant {
                next_free: self.free_head,
            };
            self.free_head = Some(i as u32);
        }
    }
}

impl<T> Default for Region<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_keeps_others_valid() {
        let mut r = Region::new();
        let a = r.insert(10u32);
        let b = r.insert(20u32);
        let c = r.insert(30u32);

        assert_eq!(r.len(), 3);
        assert_eq!(r.get(a), Some(&10));
        assert_eq!(r.get(b), Some(&20));
        assert_eq!(r.get(c), Some(&30));

        // Removing the middle handle must not disturb the others (the
        // swap-remove back-pointer repair, I1 preserved for survivors).
        assert_eq!(r.remove(b), Some(20));
        assert_eq!(r.len(), 2);
        assert_eq!(r.get(b), None); // I2
        assert_eq!(r.remove(b), None); // I2: removing twice is a no-op
        assert_eq!(r.get(a), Some(&10));
        assert_eq!(r.get(c), Some(&30));
    }

    #[test]
    fn stale_handle_after_reuse_is_none() {
        // I3 / ABA: a slot reused after removal must not honour the old handle.
        let mut r = Region::new();
        let a = r.insert(1u32);
        assert_eq!(r.remove(a), Some(1));
        let b = r.insert(2u32); // reuses a's slot with a bumped generation
        assert_eq!(r.get(a), None, "stale generation must not resolve");
        assert_eq!(r.get(b), Some(&2));
    }

    #[test]
    fn get_mut_mutates_in_place() {
        let mut r = Region::new();
        let h = r.insert(String::from("a"));
        r.get_mut(h).unwrap().push_str("bc");
        assert_eq!(r.get(h).map(String::as_str), Some("abc"));
    }

    #[test]
    fn drops_each_value_exactly_once() {
        // I5: every value is dropped exactly once — on remove or on Region
        // drop — and never twice.
        use std::cell::Cell;
        use std::rc::Rc;

        struct DropCounter(Rc<Cell<usize>>);
        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let counter = Rc::new(Cell::new(0));
        {
            let mut r = Region::new();
            let _a = r.insert(DropCounter(counter.clone()));
            let b = r.insert(DropCounter(counter.clone()));
            let _c = r.insert(DropCounter(counter.clone()));
            drop(r.remove(b)); // drops exactly one here
            assert_eq!(counter.get(), 1);
            // region drops the remaining two on scope exit
        }
        assert_eq!(
            counter.get(),
            3,
            "expected exactly three drops, no double-free"
        );
    }

    #[test]
    fn clear_invalidates_all_handles() {
        let mut r = Region::new();
        let a = r.insert(1u32);
        let b = r.insert(2u32);
        r.clear();
        assert!(r.is_empty());
        assert_eq!(r.get(a), None);
        assert_eq!(r.get(b), None);
        // Region is reusable after clear.
        let c = r.insert(3u32);
        assert_eq!(r.get(c), Some(&3));
    }
}
