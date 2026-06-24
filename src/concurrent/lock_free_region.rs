//! [`LockFreeRegion`] — lock-free reads via RCU with page-granularity copy-on-write.
//!
//! The crate's differentiated value (Phase 3b-I): concurrent handle-addressed
//! storage that is safe *and* fast for read-mostly workloads. Readers load an
//! immutable snapshot of the page table through [`arc_swap`] and look up
//! lock-free; rare writers serialise among themselves, copy only the touched
//! page, and atomically publish the new snapshot. Reclamation is plain `Arc`
//! refcounting — no epoch handoff, no `unsafe` of our own. [`arc_swap`]
//! encapsulates the atomic pointer + memory ordering, so this module stays
//! under the crate's `#![forbid(unsafe_code)]`.

use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::concurrent::LockFreeHandle;

/// Log2 of the number of slots per page. A page holds `1 << PAGE_BITS` = 64
/// slots — small enough that copy-on-write of one page is cheap, large enough
/// that a typical region needs only a handful of pages.
const PAGE_BITS: u32 = 6;
/// Number of slots in a single page.
const PAGE: usize = 1 << PAGE_BITS;

/// The occupancy of a single slot. Values live behind [`Arc<T>`] so cloning a
/// page (for copy-on-write) shares the values cheaply and requires no
/// `T: Clone` bound.
enum SlotState<T> {
    /// Holds a live value, shared via `Arc<T>`.
    Occupied(Arc<T>),
    /// Empty; `next_free` is the global slot index of the next vacant slot in
    /// the free list (or `None` for the tail).
    Vacant { next_free: Option<u32> },
}

// Hand-written `Clone` (not `#[derive]`): cloning clones the `Arc<T>` (a cheap
// refcount bump), NOT the inner `T`, so it holds for every `T` — no `T: Clone`
// bound. This is the whole point of storing values behind `Arc<T>`.
impl<T> Clone for SlotState<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Occupied(v) => Self::Occupied(Arc::clone(v)),
            Self::Vacant { next_free } => Self::Vacant {
                next_free: *next_free,
            },
        }
    }
}

/// A single slot: a generation counter plus its occupancy state. The hand-written
/// `Clone` clones the `Arc<T>` (cheap refcount bump), not the value — no
/// `T: Clone` bound.
struct Slot<T> {
    generation: u32,
    state: SlotState<T>,
}

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self {
            generation: self.generation,
            state: self.state.clone(),
        }
    }
}

/// An immutable snapshot of the whole region, published atomically by writers.
///
/// `pages` is a table of `Arc`-shared pages; cloning a `Snapshot` (which a
/// writer does to begin a mutation) clones only the `Arc` pointers and the two
/// scalars, never the values. A writer then performs a page copy-on-write
/// (`CoW`) on exactly the one page it touches and swaps the `Arc` in
/// `pages[p]`; every other page stays shared with the prior snapshot (and
/// therefore with any reader still holding it).
struct Snapshot<T> {
    pages: Vec<Arc<Vec<Slot<T>>>>,
    /// Head of the vacant free list, as a global slot index. `None` when there
    /// is no vacant slot (the next insert must grow a fresh page).
    free_head: Option<u32>,
    /// Number of live (`Occupied`) entries.
    len: usize,
}

// Hand-written `Clone`: cloning clones the page-table `Arc`s (cheap refcount
// bumps) and copies the two scalars — never the values — so it holds for every
// `T`. This is what makes writer copy-on-write cheap (only the touched page is
// actually duplicated, later).
impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self {
            pages: self.pages.clone(),
            free_head: self.free_head,
            len: self.len,
        }
    }
}

/// A handle-addressed store of `T` with **lock-free reads** and serialised,
/// page-granularity copy-on-write writes.
///
/// This is the crate's concurrent crown jewel (Phase 3b-I): it collapses the
/// usual choice between "`RwLock` (safe, slow under contention)" and
/// "hand-rolled lock-free (fast, unsafe, easy to get wrong)". The design has
/// **zero `unsafe` of our own** — all the atomic-pointer machinery lives in
/// the `arc-swap` dependency.
///
/// ## How it works
///
/// - **Reads** ([`get`](Self::get), [`contains`](Self::contains),
///   [`len`](Self::len), [`is_empty`](Self::is_empty)) load an immutable
///   [`Arc`]-shared snapshot via [`arc_swap`] and look up without taking any
///   lock. `get` returns an owned [`Arc<T>`], so a reader holds no guard after
///   returning — the snapshot (and any shared pages) is reclaimed by plain
///   `Arc` refcounting once the last reader of that version releases it.
/// - **Writes** ([`insert`](Self::insert), [`remove`](Self::remove)) take a
///   `Mutex` that serialises *writers only* — readers are never blocked. A
///   writer clones the current snapshot (cheap: just the page-table `Arc`s),
///   copies **only the one page** it mutates, and atomically publishes the new
///   snapshot with a single `store` (Release).
///
/// ## Invariants upheld
///
/// - **I1 — resolution:** a fresh [`LockFreeHandle<T>`] resolves to its value
///   until `remove`d.
/// - **I2 — tombstone:** after `remove(h)`, `get(h)` is `None` forever; a
///   second `remove(h)` is a no-op `None`.
/// - **I3 — no ABA:** `remove` **bumps the slot's generation**, so a stale
///   handle (one whose slot was reused) never resolves to a live value.
/// - **I4 — accounting:** [`len`](Self::len) equals the number of live entries
///   in the snapshot a read observes.
///
/// ## Generation saturation
///
/// If a slot's generation would reach `u32::MAX` on removal, the slot is
/// **retired**: it is left `Vacant` (so old handles go stale as usual) but is
/// **not** threaded back onto the free list, so it is never reused. This
/// mirrors the classic generational-arena rule and keeps generation wrap (and
/// therefore ABA) impossible. Unlike the slotmap-backed [`Region`](crate::Region),
/// this tier owns its own slot table, so it handles saturation itself.
///
/// ## Concurrency notes
///
/// Writers are serialised by an internal `Mutex`. Under a read-mostly workload
/// (the target — per-packet lookups vastly outnumber connect/disconnect) this
/// is not on the hot path. Readers never contend on the mutex.
pub struct LockFreeRegion<T> {
    /// The atomically-published snapshot. `load` is lock-free; `store` is a
    /// single Release swap.
    state: ArcSwap<Snapshot<T>>,
    /// Serialises writers only. Readers never touch this.
    writers: Mutex<()>,
}

impl<T> LockFreeRegion<T> {
    /// Creates an empty region that allocates nothing until first use.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: ArcSwap::new(Arc::new(Snapshot {
                pages: Vec::new(),
                free_head: None,
                len: 0,
            })),
            writers: Mutex::new(()),
        }
    }

    /// Creates an empty region with `page_count` empty pages pre-allocated
    /// (each `PAGE` slots, threaded into the free list).
    ///
    /// The argument is a page count, not an entry count, so the region is
    /// usable immediately without growth for up to `page_count * PAGE` inserts.
    /// The exact capacity is `page_count * PAGE`.
    ///
    /// # Panics
    ///
    /// Panics on `u32` index overflow in the astronomically unlikely case of a
    /// page table whose global slot indices exceed `u32::MAX`.
    #[must_use]
    pub fn with_pages(page_count: usize) -> Self {
        let page_len = u32::try_from(PAGE).expect("PAGE fits u32");
        let mut pages: Vec<Arc<Vec<Slot<T>>>> = Vec::with_capacity(page_count);
        // Thread every slot of every pre-allocated page into a single free list
        // in ASCENDING global-index order: slot[i].next_free = i+1 (or None at
        // the very last slot). free_head then points at the smallest index.
        let mut free_head: Option<u32> = None;
        let total_pages = u32::try_from(page_count).unwrap_or(0);
        for page_idx in 0..total_pages {
            let base = page_idx
                .checked_mul(page_len)
                .expect("page base overflows u32");
            let mut slots: Vec<Slot<T>> = Vec::with_capacity(PAGE);
            for off in 0..page_len {
                let global = base
                    .checked_add(off)
                    .expect("global slot index overflows u32");
                // `next_free` points at the next slot in ascending order, or
                // None if this is the global last slot.
                let next_free = if global + 1 >= total_pages * page_len {
                    None
                } else {
                    Some(global + 1)
                };
                slots.push(Slot {
                    generation: 0,
                    state: SlotState::Vacant { next_free },
                });
            }
            // The very first slot is the smallest index — record it as head.
            if page_idx == 0 {
                free_head = Some(base);
            }
            pages.push(Arc::new(slots));
        }
        Self {
            state: ArcSwap::new(Arc::new(Snapshot {
                pages,
                free_head,
                len: 0,
            })),
            writers: Mutex::new(()),
        }
    }

    /// Number of live values in the current snapshot (I4).
    ///
    /// Under concurrency this is a momentary observation, not a stable
    /// property — a writer may publish a new snapshot immediately afterwards.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state.load().len
    }

    /// Whether the current snapshot holds no live values (I4).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.load().len == 0
    }

    /// Resolves `handle` to a shared [`Arc<T>`], or `None` if the handle is
    /// stale, removed, or out of range (I1, I2, I3).
    ///
    /// Lock-free: loads an immutable snapshot and inspects it without taking
    /// any lock. The returned [`Arc<T>`] is owned, so the caller holds no guard
    /// after this returns — the snapshot is reclaimed by refcounting once no
    /// reader pins it.
    #[must_use]
    pub fn get(&self, handle: LockFreeHandle<T>) -> Option<Arc<T>> {
        let snap = self.state.load();
        let page = snap.pages.get((handle.index >> PAGE_BITS) as usize)?;
        let slot = &page[(handle.index as usize) & (PAGE - 1)];
        if slot.generation != handle.generation {
            return None;
        }
        match &slot.state {
            SlotState::Occupied(v) => Some(Arc::clone(v)),
            SlotState::Vacant { .. } => None,
        }
    }

    /// Whether `handle` currently resolves to a live value.
    ///
    /// Lock-free, like [`get`](Self::get).
    #[must_use]
    pub fn contains(&self, handle: LockFreeHandle<T>) -> bool {
        self.get(handle).is_some()
    }

    /// Inserts `value`, returning a fresh handle that resolves to it (I1).
    ///
    /// Serialised against other writers; readers are never blocked. Reuses a
    /// vacant slot from the free list if one is available, else grows the page
    /// table by one page and claims a slot from it.
    ///
    /// # Panics
    ///
    /// Panics if the writer `Mutex` is poisoned (a writer panicked while
    /// holding it), or on `u32` index overflow in the astronomically unlikely
    /// case of more than `u32::MAX` pages. Readers are unaffected.
    pub fn insert(&self, value: T) -> LockFreeHandle<T> {
        let _guard = self.writers.lock().expect("writer mutex poisoned");
        let cur = self.state.load_full();
        let mut next: Snapshot<T> = (*cur).clone();
        let value = Arc::new(value);

        let (index, generation) = match next.free_head {
            // Reuse the head of the free list.
            Some(head) => insert_reusing(&mut next, head, value),
            // No vacant slot: grow a fresh page and claim a slot from it.
            None => insert_growing(&mut next, value),
        };

        next.len += 1;
        self.state.store(Arc::new(next));
        LockFreeHandle::new(index, generation)
    }

    /// Removes and returns the value for `handle`, or `None` if it is already
    /// stale/removed (I2).
    ///
    /// Serialised against other writers; readers are never blocked. Performs a
    /// page copy-on-write (`CoW`) on only the slot's page, sets the slot
    /// `Vacant`, **bumps its generation** so old handles go stale (I3 — no ABA),
    /// and threads it onto the free list for reuse. Returns the removed
    /// [`Arc<T>`].
    ///
    /// # Panics
    ///
    /// Panics if the writer `Mutex` is poisoned (a writer panicked while
    /// holding it). Readers are unaffected.
    ///
    /// **Generation saturation:** if the slot's generation is `u32::MAX`, the
    /// slot is **retired** — left `Vacant` (so old handles still go stale) but
    /// *not* threaded onto the free list, so it is never reused. This keeps
    /// generation wrap impossible at the cost of one slot per `2^32` reuses.
    pub fn remove(&self, handle: LockFreeHandle<T>) -> Option<Arc<T>> {
        let _guard = self.writers.lock().expect("writer mutex poisoned");
        let cur = self.state.load_full();
        let mut next: Snapshot<T> = (*cur).clone();

        let page_idx = (handle.index >> PAGE_BITS) as usize;
        let off = (handle.index as usize) & (PAGE - 1);
        let page_arc = next.pages.get(page_idx)?;
        let mut new_page: Vec<Slot<T>> = (**page_arc).clone();
        let slot = &mut new_page[off];

        // Validate generation: stale or already-vacant → None (I2/I3).
        if slot.generation != handle.generation {
            return None;
        }
        let value = match core::mem::replace(&mut slot.state, SlotState::Vacant { next_free: None })
        {
            SlotState::Occupied(v) => v,
            // Generation matched but slot is Vacant: impossible — a Vacant slot
            // never carries a generation that a live handle was minted at.
            SlotState::Vacant { .. } => return None,
        };

        // Bump generation (I3): old handles for this index now fail the check.
        // Saturation: if we are at the top, retire the slot instead of wrapping.
        if slot.generation < u32::MAX {
            slot.generation += 1;
            // Thread onto the free list: this slot now points at the old head,
            // and becomes the new head. (Retired slots skip this — never reused.)
            slot.state = SlotState::Vacant {
                next_free: next.free_head,
            };
            next.free_head = Some(handle.index);
        }
        // else: generation is u32::MAX — RETIRE. Leave Vacant{next_free:None}
        // (already set by the mem::replace above), do NOT thread onto the free
        // list. Old handles still go stale (generation stays at MAX, slot is
        // Vacant), and no fresh handle is ever minted at MAX for a reused slot.

        next.pages[page_idx] = Arc::new(new_page);
        next.len -= 1;
        self.state.store(Arc::new(next));
        Some(value)
    }
}

impl<T> Default for LockFreeRegion<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Insert by reusing the vacant slot at the head of the free list.
///
/// Performs a page copy-on-write (`CoW`) on only the slot's page, installs the
/// value as `Occupied` (in place of the `Vacant` state), advances `next.free_head` to the slot's old
/// `next_free`, and returns `(head, generation)` for the new handle.
fn insert_reusing<T>(next: &mut Snapshot<T>, head: u32, value: Arc<T>) -> (u32, u32) {
    let page_idx = (head >> PAGE_BITS) as usize;
    let off = (head as usize) & (PAGE - 1);
    let mut new_page: Vec<Slot<T>> = (*next.pages[page_idx]).clone();
    let slot = &mut new_page[off];
    // Generation is unchanged on reuse — the slot was Vacant with this
    // generation, and a handle minted now carries it.
    let gen = slot.generation;
    // Invariant: free_head always points at a Vacant slot. Take the old state
    // out so we can read `next_free` and install the Occupied value in one move.
    let old_state = std::mem::replace(&mut slot.state, SlotState::Vacant { next_free: None });
    let SlotState::Vacant {
        next_free: advanced_head,
    } = old_state
    else {
        unreachable!("free_head pointed at an Occupied slot — free list corrupted")
    };
    slot.state = SlotState::Occupied(value);
    next.pages[page_idx] = Arc::new(new_page);
    next.free_head = advanced_head;
    (head, gen)
}

/// Insert by growing a fresh page and claiming slot 0 of it.
///
/// Appends a new `PAGE`-slot page whose slots `1..PAGE` form an ascending free
/// list, installs the value in slot 0, sets `next.free_head` to slot 1, and
/// returns `(base, 0)` for the new handle.
fn insert_growing<T>(next: &mut Snapshot<T>, value: Arc<T>) -> (u32, u32) {
    let page_idx = u32::try_from(next.pages.len()).expect("page count overflows u32");
    let page_len = u32::try_from(PAGE).expect("PAGE fits u32");
    let base = page_idx
        .checked_mul(page_len)
        .expect("new page base overflows u32");

    // Build the page in index order. Slots 1..PAGE form a free list threaded in
    // ASCENDING index order: slot[i].next_free = i+1, tail → None. Slot 0 is
    // claimed immediately.
    let mut new_page: Vec<Slot<T>> = Vec::with_capacity(PAGE);
    // Slot 0 — placeholder Vacant, overwritten below with the Occupied value.
    new_page.push(Slot {
        generation: 0,
        state: SlotState::Vacant { next_free: None },
    });
    for off in 1..page_len {
        let global = base
            .checked_add(off)
            .expect("global slot index overflows u32");
        // Each vacant slot points at the next slot in the page, so the list
        // walks base+1 → base+2 → … → base+PAGE-1 → None.
        let next_in_page = if off + 1 == page_len {
            None
        } else {
            Some(global + 1)
        };
        new_page.push(Slot {
            generation: 0,
            state: SlotState::Vacant {
                next_free: next_in_page,
            },
        });
    }
    let claimed = base;
    new_page[0].state = SlotState::Occupied(value);
    next.pages.push(Arc::new(new_page));
    // New free head = slot 1 (if the page has room beyond slot 0).
    next.free_head = (page_len > 1).then_some(base + 1);
    (claimed, 0)
}
