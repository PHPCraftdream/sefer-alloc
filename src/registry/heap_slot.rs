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
// `Sync` impl is the `unsafe` surface of Phase 12.2). `allow` lifts the
// crate-level `deny` for this file only — `unsafe` anywhere else in the crate
// is a hard error. The ONLY `unsafe` here is the `unsafe impl Sync` on
// `HeapSlot`, carrying a `// SAFETY:` proof (the registry's atomic
// single-writer protocol). There is deliberately NO `unsafe impl Send` — see
// the M6 note above that impl at the bottom of the file (a by-value
// cross-thread move is neither needed nor proven; `Sync` alone carries the
// process-global sharing).
//
// ## M7 note — field visibility and the soundness boundary
//
// The slot's `Sync` proof (and every neighbouring registry SAFETY proof)
// relies on the CAS-gated single-writer protocol on `state`/`heap`. Safe code
// that could flip `state` LIVE→FREE or write `heap`/`next_free` directly would
// break that invariant with NO `unsafe` keyword at the violation site — this
// is the general "safe membrane over a seam" limit spelled out in
// `src/lib.rs`. EVERY slot field is therefore `pub(crate)` (reachable only
// inside the crate's confined registry code). Task #93 / R4-MS-4 narrowed the
// last two holdouts — `state` and `generation` — down from `pub` to
// `pub(crate)`: while they were `pub`, safe downstream code could execute the
// `LIVE → FREE` transition and re-push the slot onto `free_slots` itself,
// handing a LIVE `HeapCore` to a second thread and breaking the very
// single-writer invariant below. Integration tests that still need to
// read/preset these fields go through the narrow `#[doc(hidden)]` accessors on
// `Registry` (`dbg_slot_state`/`dbg_slot_generation`/`dbg_slot_preset_generation`,
// in `bootstrap.rs`) — read accessors are safe, the ONE writer is `unsafe fn`.
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

use super::heap_core::HeapCore;
#[cfg(feature = "alloc-xthread")]
use super::heap_overflow::HeapOverflow;
#[cfg(feature = "class-aware-dirty")]
use crate::alloc_core::dirty_by_class::PerClassDirty;
#[cfg(feature = "class-aware-dirty")]
use racy_ptr_cell::RacyPtrCell;

/// Slot state: `FREE` (available for claim) or `LIVE` (owned by a thread).
/// Stored as a `u8` so the `FREE → LIVE` / `LIVE → FREE` transitions are
/// single-word atomic CASes (the linearization points of claim / recycle).
pub const STATE_FREE: u8 = 0;
pub const STATE_LIVE: u8 = 1;

/// Sentinel stored in [`HeapSlot::next_free`] to denote "this is the stack
/// tail" (no next free slot). No real slot index is `u32::MAX` (the registry
/// caps at `MAX_HEAPS`, far below).
pub const NEXT_FREE_TAIL: u32 = u32::MAX;

/// PERF-PASS-4 (G8/ML2, task #52) — the remote/foreign-access fields of a
/// [`HeapSlot`], grouped into their own 64-byte-aligned sub-struct.
///
/// **The false-sharing residue this fixes:** measured via nightly
/// `-Zprint-type-sizes` (`--features production`) before this change, the
/// single cache line at `HeapSlot` byte offset 6976..7040 simultaneously
/// held FOUR unrelated access patterns:
///   1. `last_stamped_segment`/`id` (inside `heap: HeapCore`) — owner-hot,
///      read on every stamp fast-path check (`heap_core.rs` OPT-C).
///   2. `thread_free`@7016 — remote-CASed by a cross-thread Large free on
///      EVERY such free (`push_large_deferred_free`), Acquire-loaded on the
///      drain check. This is the H1 word: the earlier UB fix this session
///      hoisted it OUT of `HeapCore` (to escape the owner's `&mut HeapCore`
///      Stacked-Borrows retag range) but physically re-created the adjacency
///      — a remote CAS on `thread_free` invalidates the very line holding
///      the owner's stamp cache.
///   3. `tcache_hits`/`large_cache_hits`@7000/7008 — read CROSS-THREAD by the
///      `stats()` aggregator (`tcache_hits_total`/`large_cache_hits_total`).
///   4. The START of the NEXT slot's `state`/`generation` (the un-padded
///      7024-byte stride is not a multiple of 64, so slot boundaries drift
///      through cache-line phase across the 4096-slot array).
///
/// **The fix:** every field a REMOTE thread ever touches — the CASed
/// `thread_free` word and the cross-thread-read diagnostic counters — moves
/// into this sub-struct, `#[repr(C, align(64))]` so it starts its own cache
/// line, disjoint from `HeapSlot`'s owner-hot fields (`heap`'s
/// `last_stamped_segment`/`id`) and from the next array element (paired with
/// `#[repr(align(64))]` on `HeapSlot` itself, below, which rounds the
/// per-slot stride up to a 64-multiple).
///
/// Field ORDER inside is unchanged from the flat layout (still `#[repr(C)]`,
/// still initialised the same way by the bootstrap's `addr_of_mut!`
/// writes) — only the GROUPING and alignment
/// changed. Every external reference (`&'static AtomicU64`/`&'static
/// AtomicPtr<u8>` handles bound at `claim` time — see
/// `heap_registry::bind_slot_counters`) is unaffected: a Rust field
/// reference's address is stable regardless of struct nesting.
#[repr(C, align(64))]
pub(crate) struct HeapSlotRemote {
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
    pub(crate) tcache_hits: AtomicU64,

    /// DIAGNOSTIC (task W3): this slot's process-lifetime large-segment cache
    /// HIT counter. Same design and rationale as [`tcache_hits`](Self::tcache_hits)
    /// (moved into the shared slot to close the Stacked-Borrows aliasing gap);
    /// read by [`super::heap_registry::large_cache_hits_total`] from the
    /// `&HeapSlot`, written by the owner through a stable `&'static AtomicU64`.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) large_cache_hits: AtomicU64,

    /// Cross-thread free-stack head / identity stamp (task H1 — the W3 hoist
    /// applied to the TFS head).
    ///
    /// This is the storage that used to be the INLINE `HeapCore::thread_free`
    /// `AtomicPtr<u8>` field. It was moved OUT of `HeapCore` and into this
    /// `Sync`, process-`'static` slot for exactly the reason W3 moved the
    /// diagnostic counters: a REMOTE thread cross-thread-freeing a Large
    /// segment owned by this heap CASes this word (through EXPOSED provenance —
    /// `Node::atomic_ptr_ref` → `with_exposed_provenance_mut` →
    /// `compare_exchange`, see `alloc_core::deferred_large::push`), while the
    /// OWNING thread concurrently holds a protected `&mut HeapCore` spanning
    /// the whole struct. When this word lived INSIDE `HeapCore`, that foreign
    /// write landed inside the range of the owner's protected `&mut` — a
    /// protector/data-race violation under Stacked/Tree Borrows (empirically
    /// confirmed by miri: a retag-write vs. atomic-load data race between the
    /// owner's `stamp_segment_owner(&mut self)` fn-entry retag and the remote's
    /// `head.load()` in `push_large_deferred_free`). See
    /// `tests/regression_xthread_thread_free_alias_miri.rs`.
    ///
    /// Moving the word into the slot removes it from every `&mut HeapCore`
    /// retag range: the owner reaches it through a stable `&'static AtomicPtr`
    /// handle (planted at [`super::heap_registry::HeapRegistry::claim`] time,
    /// like `tcache_hits`), and remote freers reach the SAME word through the
    /// `owner_thread_free_at(base)` segment-header stamp — which now stores
    /// this slot field's address. The slot lives in the `'static` registry
    /// array, so the address is stable for the slot's (process) lifetime and
    /// never re-pointed across `recycle`→`claim`.
    ///
    /// Dual role, unchanged from the old inline field: the ADDRESS is the
    /// per-heap identity token compared by `dealloc_routing`
    /// (`owner_thread_free_at(base) == our head`); the VALUE (`AtomicPtr<u8>`)
    /// is the head of this heap's deferred-free Treiber stack over Large
    /// segment bases (`null` = empty). The two uses touch disjoint parts of the
    /// same word, so there is no conflation.
    ///
    /// `null`-initialised (empty stack). Only present under `alloc-xthread`.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) thread_free: AtomicPtr<u8>,

    /// R7-A4: per-slot dirty-segment bitmap — 16 `AtomicU64` words covering
    /// all 1024 segment-table slot indices (`MAX_SEGMENTS / 64 = 16`).
    ///
    /// A cross-thread freer (producer) sets bit `segment_id % 64` of word
    /// `segment_id / 64` via `fetch_or(bit, Release)` AFTER a successful
    /// `RemoteFreeRing::push` / `try_push_uncounted`. The owning thread
    /// (consumer) `swap(0, Acquire)`s each word, iterates set bits, and
    /// drains ONLY those segments' rings — replacing the O(S) "drain every
    /// ring" scan with O(dirty) targeted drains.
    ///
    /// Lives in `HeapSlotRemote` (the STABLE, cross-thread-reachable part of
    /// the slot) so producers can reach it from any thread via the registry's
    /// `slot(owner_id)` — the same resolution path `push_to_heap_overflow`
    /// and `resolve_heap_overflow` use. Zero-initialised (OS-zeroed pages):
    /// no segment is dirty until a producer sets a bit.
    ///
    /// **Lost-wakeup safety:** the producer sets the bit AFTER publishing
    /// the ring entry (the `Release` store on `push`/`try_push_uncounted`
    /// happens-before the `Release` `fetch_or` here). A producer arriving
    /// after the owner's `swap(0, Acquire)` re-sets the bit for the next
    /// drain pass. A push during a drain is either seen by that drain
    /// (the ring's `drain` reads up to the current `tail`) or leaves the
    /// bit set for the next pass. Slot reuse is always revalidated via
    /// `base_at(slot) + kind + segment_id` checks before draining.
    ///
    /// **P4 (visibility contract change):** a producer stalled between
    /// `push` and `fetch_or` is invisible to the dirty-routing drain until
    /// its bit lands (or until the linear-scan fallback, which still drains
    /// every ring unconditionally, eventually finds it). This is bounded
    /// deferral of the same class as the existing "later drain picks it up"
    /// contract. See `remote_free_ring.rs` module doc for the pinned note.
    ///
    /// Only compiled under `alloc-xthread` AND `alloc-segment-directory`
    /// (the dirty routing only matters when the directory drives the drain).
    #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
    pub(crate) dirty_segments: [AtomicU64; DIRTY_BITMAP_WORDS],

    /// R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): the lazily-
    /// materialised per-(segment, class) dirty-bit sidecar — see
    /// `dirty_by_class`'s module doc for the full design. `null` (UNINIT)
    /// until this heap's first class-routed cross-thread free; a heap that
    /// never receives one never pays the ~6.1 KiB reservation.
    ///
    /// Additive over [`dirty_segments`](Self::dirty_segments): the existing
    /// per-segment bitmap is set unconditionally on every push regardless of
    /// this feature, so it remains the fallback/ground-truth signal even
    /// when this sidecar is in use.
    #[cfg(feature = "class-aware-dirty")]
    pub(crate) dirty_by_class: RacyPtrCell<PerClassDirty>,
}

/// R7-A4: number of `AtomicU64` words in the per-slot dirty-segment bitmap.
/// `MAX_SEGMENTS / 64 = 16`. Mirrors `segment_directory::WORDS_PER_CLASS` but
/// defined here so this module does not depend on the `alloc-segment-directory`-
/// gated `segment_directory` module at the type level (the array size must be
/// available whenever both `alloc-xthread` and `alloc-segment-directory` are
/// active, without requiring a cfg-conditional import of the directory module).
#[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
pub(crate) const DIRTY_BITMAP_WORDS: usize = crate::alloc_core::segment_table::MAX_SEGMENTS / 64;

/// One registry slot. `#[repr(C, align(64))]`: `repr(C)` so the bootstrap can
/// compute the slot array's footprint deterministically and lay it down at a
/// fixed offset in the primordial segment (unchanged from before this task);
/// `align(64)` (PERF-PASS-4, G8/ML2, task #52) so the per-slot stride is a
/// multiple of the cache-line size — the un-padded pre-task stride (7024
/// bytes) was NOT a multiple of 64, so consecutive slots' cache-line
/// boundaries drifted out of phase across the 4096-slot array, meaning a
/// remote CAS near one slot's tail could ALSO dirty the next slot's `state`/
/// `generation` header fields. Rounding the stride to a 64-multiple costs at
/// most 63 bytes of padding per slot (+64 KiB across the whole registry —
/// negligible; the registry's pages are lazily committed by the OS, so an
/// idle process never touches the extra padding bytes at all). See
/// [`HeapSlotRemote`]'s doc comment for the paired fix (grouping the
/// remote-access fields onto their OWN 64-byte-aligned line, disjoint from
/// this slot's owner-hot fields).
#[repr(C, align(64))]
pub struct HeapSlot {
    /// `FREE` or `LIVE`. The claim/recycle CAS target.
    ///
    /// `pub(crate)` (task #93 / R4-MS-4): while this was `pub`, safe downstream
    /// code could `state.store(STATE_FREE, ..)` on a LIVE slot and re-push it
    /// onto `free_slots`, breaking the single-writer invariant the
    /// `unsafe impl Sync` below depends on (R4-MS-4). Integration tests read it
    /// through the narrow `Registry::dbg_slot_state` accessor (`bootstrap.rs`).
    pub(crate) state: AtomicU8,
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
    ///
    /// `pub(crate)` (task #93 / R4-MS-4): a `pub` field let safe downstream
    /// code forge owner epochs or preset a LIVE slot's generation directly.
    /// Integration tests read it via `Registry::dbg_slot_generation`, and the
    /// ONE write site (`tests/regression_counter_wrap.rs`'s u64-wrap
    /// counterfactual) goes through the `unsafe fn
    /// Registry::dbg_slot_preset_generation` accessor, whose `# Safety` carries
    /// the no-concurrent-claim precondition that makes the direct write sound.
    pub(crate) generation: AtomicU64,
    /// The heap value, lazily materialised by `claim` on the slot's first
    /// `FREE → LIVE` transition and reused on later reclaims. Wrapped in
    /// `UnsafeCell` so `claim` can return `&mut HeapCore` through a shared
    /// `&HeapSlot`, and `MaybeUninit` so a `FREE` slot carries no live
    /// (expensive-to-construct) `HeapCore`.
    pub(crate) heap: UnsafeCell<MaybeUninit<HeapCore>>,
    /// Intrusive link for the `free_slots` Treiber stack. Holds the NEXT free
    /// slot's index (or [`NEXT_FREE_TAIL`] for the stack tail) while the slot
    /// is FREE. Read/written by the registry only while the slot is FREE (no
    /// concurrent LIVE access).
    pub(crate) next_free: AtomicU32,
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
    pub(crate) initialised: AtomicBool,

    /// PERF-PASS-4 (G8/ML2, task #52): the remote/foreign-access fields
    /// (`tcache_hits`, `large_cache_hits`, `thread_free`), grouped into their
    /// own 64-byte-aligned sub-struct — see [`HeapSlotRemote`]'s doc comment
    /// for the false-sharing residue this fixes. Always present in the
    /// layout (mirroring the fields' own prior discipline of being "present
    /// but inert" under a non-matching feature set) — `HeapSlotRemote` itself
    /// degrades to a smaller/empty `#[repr(align(64))]` struct when every
    /// gated field is compiled out. A zero-sized type stays 0 bytes even
    /// under `align(64)` (Rust does not pad a ZST up to its alignment), so
    /// in that degenerate config `remote` contributes no bytes at all — which
    /// is still sound, because with zero live fields there is nothing left
    /// for a remote thread to touch, so the false-sharing question this
    /// grouping exists to answer does not arise in the first place.
    pub(crate) remote: HeapSlotRemote,

    /// RAD-4b (task #72): the slot-resident second-chance MPSC overflow ring
    /// — see [`HeapOverflow`]'s module doc for the full design rationale.
    /// Materialised unconditionally (like `remote`, above) so both the
    /// owner's drain and a remote producer's push reach it through the SAME
    /// `&'static HeapSlot` the registry already hands out, with no separate
    /// claim-time wiring step (unlike `HeapCore::thread_free`'s `&'static`
    /// handle, this ring needs no handle at all — a remote producer resolves
    /// it directly from `bootstrap::ensure().slot(owner_id)`, see
    /// `HeapCore::push_with_overflow_retry`).
    ///
    /// Only present under `alloc-xthread` (the cross-thread feature this
    /// mechanism exists to serve) — mirrors `HeapSlotRemote::thread_free`'s
    /// own gate. All-zero initial state, so an unclaimed or never-overflowing
    /// slot never first-touches its 96 KiB array (see
    /// `HeapOverflow::HEAP_OVERFLOW_CAP`'s RSS-discipline doc comment).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) overflow: HeapOverflow,
}

impl HeapSlot {
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
//
// This single-writer invariant ADDITIONALLY rests on `state`, `generation`,
// `next_free` and every `Registry` control atomic being `pub(crate)`
// (task #93 / R4-MS-4): safe code OUTSIDE the crate cannot execute the
// `LIVE → FREE` transition or push onto `free_slots`, so it cannot smuggle a
// second owner past the CAS gate. While those fields were `pub`, a safe
// downstream crate could re-claim a LIVE slot under a thread that still held
// a cached TLS `*mut HeapCore`, materialising two `&mut HeapCore` over one
// `UnsafeCell` — exactly the aliasing this proof rules out.
unsafe impl Sync for HeapSlot {}

// NOTE (no `unsafe impl Send`): `HeapSlot` is deliberately NOT `Send` (task
// #21 / review M6). The previous `unsafe impl Send for HeapSlot` proved the
// wrong statement — its SAFETY text argued about sending a *`&HeapSlot`* to
// another thread, which is what `Send + Sync` on the atomic fields plus the
// `Sync` impl above already provide (a `&T` is `Send` iff `T: Sync`). What
// `Send` on `HeapSlot` itself actually authorises is moving a `HeapSlot`
// *by value* to another thread — carrying a possibly-LIVE `HeapCore`
// (`AllocCore`'s raw segment pointers, `last_stamped_segment: *mut u8`) with
// it. Nobody wrote (or needs) a proof for that: a by-value move bypasses the
// entire `claim` CAS single-writer discipline every neighbouring SAFETY proof
// depends on, and no code path in the crate ever moves a `HeapSlot` by value
// across threads — the registry array is `'static` and immovable, reached
// only through `&`/`&mut`. The process-global sharing the registry needs is
// carried by `Sync` alone (`Registry`'s `Sync` requires only
// `[HeapSlot; N]: Sync`, i.e. `HeapSlot: Sync`, NOT `Send`). Removing the
// impl lets the `HeapCore` raw pointers make `HeapSlot` auto-`!Send`, so a
// future by-value cross-thread send (e.g. a shadow `Vec<HeapSlot>` shipped to
// a worker) becomes a compile error rather than a silent auto-blessed hazard.
// This does not regress any real caller (full `cargo build`/`cargo test`
// green with the impl removed).
