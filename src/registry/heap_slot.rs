//! [`HeapSlot`] — one entry of the registry's slot array (the "fractal slot
//! table" of §1 of `ALLOC_PLAN_PHASE12-13.md`: the heap pool itself becomes a
//! slot table).
//!
//! Each slot is a fixed-size record carved from the registry's primordial
//! segment. Its lifecycle is `FREE → LIVE → FREE → …`: a `claim` flips it
//! `FREE → LIVE` and bumps `generation`; a `recycle` flips it back `FREE`.
//! The `generation` field is the M8/M9 coherence key used elsewhere in the
//! registry (segment-header owner stamping) to distinguish successive
//! occupants of the same slot index.
//!
//! **It is NOT read on the TLS alloc path.** The stale-TLS-pointer hazard
//! (a thread's cached `*mut HeapCore` outliving its slot's `recycle`) is
//! guarded by a different, cheaper mechanism: `global::tls_heap`'s `TORN`
//! sentinel (task #129). The owning thread's `AbandonGuard::drop` stamps its
//! OWN thread-local cache to `TORN` before it recycles the slot, and every
//! resolver checks for `TORN` before dereferencing. This is a same-thread
//! poison-then-check, not a cross-thread generation compare — it needs no
//! read of this slot's `generation` at resolve time. See
//! `global::tls_heap`'s "TLS destructor ordering" module doc for the full
//! argument.
//!
//! ## Why `MaybeUninit` (not a live `HeapCore`)
//!
//! `HeapCore::new` bootstraps a full segment substrate
//! ([`AllocCore::new`](crate::AllocCore::new) reserves a 4 MiB primordial
//! segment). We CANNOT materialise a live
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
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

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
    ///
    /// **Width — `AtomicU64` (task W7a):** a `u32` generation wraps at `2^32`
    /// recycles (`FREE → LIVE → FREE` cycles = thread deaths). On a
    /// thread-per-request server that is reachable over weeks/months, at which
    /// point a wrapped generation could re-collide with a value a stale
    /// `(index, generation)` owner key still holds — reintroducing the ABA on
    /// slot RECYCLE→reCLAIM this field exists to defeat. `u64` wraps at `2^64`
    /// (∼10^19 recycles) — unreachable in any process lifetime. The widening
    /// is Ir-neutral (this field is off every hot alloc/dealloc path — it is
    /// bumped once per claim on the cold registry-protocol path only).
    pub generation: AtomicU64,
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
    /// Release-published "heap is materialised" flag (task #133 hardening).
    ///
    /// Starts `false` and becomes `true` EXACTLY ONCE, at the end of the
    /// slot's first `claim` (`new_gen == 1` branch, immediately after
    /// `heap_ptr.write(hc)` completes) — see `HeapRegistry::claim` /
    /// `claim_with_config`. NEVER reset back to `false` afterwards: once a
    /// slot's `HeapCore` is materialised it is reused as-is across every
    /// later `recycle` → `claim` cycle (it is never dropped or
    /// re-`MaybeUninit`'d — see `heap`'s doc comment above), so once this
    /// flag is `true` it stays `true` for the process lifetime of the slot.
    ///
    /// **Why this exists — the bug it fixes:** `count` (bumped by
    /// `bump_count`, BEFORE the CAS to LIVE and BEFORE `HeapCore::new()`
    /// runs) and `generation` (bumped to 1 by `claim` BEFORE
    /// `heap_ptr.write(hc)`) are both insufficient gates for a reader that
    /// wants to safely dereference `heap` from a DIFFERENT thread than the
    /// claimer: a diagnostic walk (`tcache_hits_total` /
    /// `large_cache_hits_total`) that used `idx < count` or `generation >=
    /// 1` as its "safe to read" condition could observe a slot mid-claim —
    /// `count` already bumped, generation already 1, but `HeapCore::new()`
    /// (which reserves an OS segment — not fast) still in flight and
    /// `heap_ptr.write(hc)` not yet executed. Reading `heap` in that window
    /// is a read of `MaybeUninit::uninit()` racing a concurrent non-atomic
    /// write — undefined behaviour, not merely stale data.
    ///
    /// **The fix:** the writer publishes readiness with a `Release` store
    /// to this flag ONLY after `heap_ptr.write(hc)` returns; the reader
    /// gates its dereference of `heap` on an `Acquire` load observing
    /// `true`. This Release/Acquire pair establishes happens-before from
    /// the write of `hc` into the `UnsafeCell` to the reader's subsequent
    /// access — the standard "publish a fully-constructed value" pattern.
    /// A reader that observes `false` skips the slot entirely (treats it as
    /// "not yet materialised, contributes nothing to the aggregate" — sound
    /// because a slot that has never been claimed has never incremented any
    /// per-heap counter either).
    pub initialised: AtomicBool,

    /// DIAGNOSTIC (task W3): this slot's process-lifetime magazine (tcache)
    /// HIT counter. Lives in the SLOT — which is `Sync` and designed to be
    /// shared — rather than inside the owner's `HeapCore`, closing a formal
    /// aliasing gap: the process-wide aggregator
    /// ([`super::heap_registry::tcache_hits_total`]) reads this via the
    /// `&HeapSlot` it already holds, WITHOUT ever materialising a shared
    /// `&HeapCore` over a struct the owning thread concurrently holds a
    /// protected `&mut` into (a foreign-read of a protected `Unique` — UB
    /// under Stacked Borrows). The owning thread increments this through a
    /// stable `&'static AtomicU64` handed to its `HeapCore` at
    /// [`super::heap_registry::HeapRegistry::claim`] time (the slot lives in
    /// the `'static` registry array, so the reference is sound for the
    /// process lifetime).
    ///
    /// Zero-initialised: an un-bound slot reads 0, so it contributes nothing
    /// to the aggregate even before `initialised` is published. Written only
    /// by the slot's current owner (single writer); read Relaxed by that
    /// owner and by the cross-thread aggregator.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub tcache_hits: AtomicU64,

    /// DIAGNOSTIC (task W3): this slot's process-lifetime large-segment cache
    /// HIT counter. Same design and rationale as [`tcache_hits`](Self::tcache_hits)
    /// (moved into the shared slot to close the Stacked-Borrows aliasing gap);
    /// read by [`super::heap_registry::large_cache_hits_total`] from the
    /// `&HeapSlot`, written by the owner through a stable `&'static AtomicU64`.
    #[cfg(feature = "alloc-decommit")]
    pub large_cache_hits: AtomicU64,
}

impl HeapSlot {
    /// Construct a slot in its bootstrap state: `FREE`, generation 0,
    /// `next_free = NEXT_FREE_TAIL`, heap uninitialised.
    ///
    /// Previously used by `Registry::new_zeroed()` to populate a `static`
    /// initialiser. Now retained as a self-documenting spec of the slot's
    /// initial state; the actual in-place initialisation of the heap-allocated
    /// registry (see [`super::bootstrap::ensure`]) writes the same values
    /// directly via `addr_of_mut!` field writes.
    ///
    /// This does NOT allocate a `HeapCore` — that is deferred to `claim`.
    #[allow(dead_code)]
    pub(crate) const fn new_uninit() -> Self {
        Self {
            state: AtomicU8::new(STATE_FREE),
            generation: AtomicU64::new(0),
            heap: UnsafeCell::new(MaybeUninit::uninit()),
            next_free: AtomicU32::new(NEXT_FREE_TAIL),
            initialised: AtomicBool::new(false),
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            tcache_hits: AtomicU64::new(0),
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits: AtomicU64::new(0),
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
        self.state.compare_exchange(expected, new, success, failure)
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
