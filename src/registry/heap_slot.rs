//! [`HeapSlot`] — one entry of the registry's slot array (the "fractal slot
//! table" of §1 of `MALLOC_PLAN_PHASE12-13.md`: the heap pool itself becomes a
//! slot table).
//!
//! Each slot is a fixed-size record carved from the registry's primordial
//! segment. Its lifecycle is `FREE → LIVE → FREE → …`: a `claim` flips it
//! `FREE → LIVE` and bumps `generation`; a `recycle` flips it back `FREE`.
//! The `generation` is the M8/M9 coherence key — a stale TLS raw pointer
//! (cached pre-recycle) reading a slot that has since been recycled + reclaimed
//! by another thread will see a mismatched generation and refuse (the 12.3
//! never-stale-pointer guard).
//!
//! ## Why `MaybeUninit` (not a live `HeapCore`)
//!
//! `HeapCore::new` bootstraps a full segment substrate ([`AllocCore::new`]
//! reserves a 4 MiB primordial segment). We CANNOT materialise a live
//! `HeapCore` per slot at registry-init time: that would reserve
//! `MAX_HEAPS × 4 MiB` of OS memory up front. Instead each slot holds a
//! [`MaybeUninit<HeapCore>`]; the slot starts `FREE` with an *uninitialised*
//! heap value, and `claim` lazily `HeapCore::new`s into the slot on its first
//! `FREE → LIVE` transition. On a later `recycle → reclaim` the slot's
//! `HeapCore` is already live and is reused as-is (its `AllocCore` and its
//! segments persist; only `id` may be refreshed). This is the standard
//! lazy-materialise pattern for a slot pool whose values are expensive to
//! construct.
//!
//! ## Why `UnsafeCell`
//!
//! `MaybeUninit` alone does not permit handing out `&mut HeapCore` through a
//! `&HeapSlot` (shared reference); `UnsafeCell` interior-mutates that access.
//! The single-writer invariant (the owning thread is the sole mutator of its
//! heap's bins) makes this sound without runtime borrow checking; the 12.3
//! raw-pointer TLS will rely on exactly this.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); the registry is a documented atomic seam (the slot table's
// `Sync`/`Send` impls are the new `unsafe` surface of Phase 12.2). `allow`
// lifts the crate-level `deny` for this file only — `unsafe` anywhere else
// in the crate is a hard error. The ONLY `unsafe` here is the `unsafe impl
// Sync` / `unsafe impl Send` on `HeapSlot`, each carrying a `// SAFETY:`
// proof (the registry's atomic single-writer protocol).
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, AtomicU8};

use super::heap_core::HeapCore;

/// Slot state: `FREE` (available for claim) or `LIVE` (owned by a thread).
/// Stored as a `u8` so the `FREE → LIVE` / `LIVE → FREE` transitions are
/// single-word atomic CASes (the linearization points of claim / recycle).
pub const STATE_FREE: u8 = 0;
pub const STATE_LIVE: u8 = 1;

/// Sentinel stored in [`HeapSlot::next_free`] to denote "this is the stack
/// tail" (no next free slot). No real slot index is `u32::MAX` (the registry
/// caps at `MAX_HEAPS`, far below).
pub const NEXT_FREE_TAIL: u32 = u32::MAX;

/// One registry slot. `#[repr(C)]` so the bootstrap can compute the slot
/// array's footprint deterministically and lay it down at a fixed offset in
/// the primordial segment.
#[repr(C)]
pub struct HeapSlot {
    /// `FREE` or `LIVE`. The claim/recycle CAS target.
    pub state: AtomicU8,
    /// Bumped on every successful (re)claim — the M8/M9 generation. Combined
    /// with the slot index it forms the unique `(index, generation)` owner
    /// key stamped into segment headers (12.3). Starts at 0; the first claim
    /// sees generation 1, the first recycle-then-claim sees 2, etc.
    ///
    /// **Atomic** (not a plain `u32` as in the §2.1 sketch) because a later
    /// reader — the 12.3 stale-TLS-pointer check — loads this from a
    /// DIFFERENT thread than the writer (the claimer). The single-writer
    /// invariant (the CAS winner is the sole writer until it recycles) holds,
    /// but cross-thread reads still require atomic synchronisation to avoid a
    /// data race. `Release` on the bump (in `claim`) pairs with the reader's
    /// `Acquire` load after observing `state == LIVE`.
    pub generation: AtomicU32,
    /// The heap value, lazily materialised by `claim` on the slot's first
    /// `FREE → LIVE` transition and reused on later reclaims. Wrapped in
    /// `UnsafeCell` so `claim` can return `&mut HeapCore` through a shared
    /// `&HeapSlot`, and `MaybeUninit` so a `FREE` slot carries no live
    /// (expensive-to-construct) `HeapCore`.
    pub heap: UnsafeCell<MaybeUninit<HeapCore>>,
    /// Intrusive link for the `free_slots` Treiber stack. Holds the NEXT free
    /// slot's index (or [`NEXT_FREE_TAIL`] for the stack tail) while the slot
    /// is FREE. Read/written by the registry only while the slot is FREE (no
    /// concurrent LIVE access).
    pub next_free: AtomicU32,
}

impl HeapSlot {
    /// Construct a slot in its bootstrap state: `FREE`, generation 0,
    /// `next_free = NEXT_FREE_TAIL`, heap uninitialised. The bootstrap lays
    /// down one of these per slot in the primordial segment's slot array
    /// (via raw writes through the `node` seam — see [`bootstrap`](super::bootstrap)).
    ///
    /// This does NOT allocate a `HeapCore` — that is deferred to `claim`.
    pub(crate) const fn new_uninit() -> Self {
        Self {
            state: AtomicU8::new(STATE_FREE),
            generation: AtomicU32::new(0),
            heap: UnsafeCell::new(MaybeUninit::uninit()),
            next_free: AtomicU32::new(NEXT_FREE_TAIL),
        }
    }

    /// CAS the slot's state from `expected` to `new` with the given ordering.
    /// Returns `Ok` on success. The linearization point of claim/recycle.
    #[inline]
    pub(crate) fn cas_state(
        &self,
        expected: u8,
        new: u8,
        success: core::sync::atomic::Ordering,
        failure: core::sync::atomic::Ordering,
    ) -> Result<u8, u8> {
        self.state
            .compare_exchange(expected, new, success, failure)
    }
}

// SAFETY (Sync): `HeapSlot` is shared across threads (the registry array is
// process-global). Synchronisation is provided by its atomic fields (`state`,
// `next_free`) and the single-writer invariant on `heap` (the `UnsafeCell`):
// at most one thread — the slot's owner, established by the `FREE → LIVE` CAS
// in `claim` — may mutate `heap` at any time, and that owner has observed the
// CAS that excludes all other writers. Reads of `heap` (the `*mut HeapCore`
// handed out by `claim`) are sound because the slot array is immovable and
// lives for the process lifetime (the primordial registry segment is never
// freed). This is exactly the soundness argument for `UnsafeCell` under a
// single-writer discipline; the registry's atomic protocol is what establishes
// the single writer. `MaybeUninit` adds no new hazard: the registry's contract
// is that `heap` is read only while `state == LIVE` (which means `claim` has
// init'd it).
unsafe impl Sync for HeapSlot {}

// SAFETY (Send): a `&HeapSlot` can be sent to another thread — the slot's
// atomic fields are `Send` (atomics are `Send + Sync`), and `heap` is only
// mutated through the CAS-gated single-writer protocol, so sending a shared
// reference cannot create a data race. (We never move a `HeapSlot` itself —
// the array is static; `Send` is for the shared-reference send.)
unsafe impl Send for HeapSlot {}
