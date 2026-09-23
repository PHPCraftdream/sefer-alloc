#![allow(unsafe_code)]
//! The [`Registry`] struct: `MAX_HEAPS`, the per-chunk slot resolver
//! (`slot`/`slot_or_none`), the sync assert, the test-only dbg accessors, and
//! the process-global `static REGISTRY`. Split out of the flat `bootstrap.rs`
//! (structural reorg step 5); the module-level history and soundness
//! narrative lives in [`super`]'s doc.

use core::sync::atomic::{AtomicU32, Ordering};

// The extracted lazy CAS-published pointer cell (CRATE-P3). Under a NORMAL or
// `production` build sefer uses the real crate type. Under `--cfg loom`, this
// file is compiled to link sefer's OWN shadow-model loom harnesses (e.g.
// `loom_xthread_protocol`) — which model UNRELATED protocols and never touch the
// chunk cells; but `RUSTFLAGS=--cfg loom` is global, so the real crate would
// then be built in its loom-aliased mode, where `OncePtrCell::new` is NOT
// `const` (loom's `AtomicPtr::new` has no const constructor) and sefer's
// `static REGISTRY: Registry = Registry::new()` would fail to const-evaluate.
// Sefer already keeps its OWN production atomics on `core::sync::atomic` under
// loom (see the imports above) for exactly this reason. So under loom we swap in
// a const-capable, core-atomic shim with the identical API surface `bootstrap`
// uses. This is sound: the real-type loom VERIFICATION of `OncePtrCell` lives in
// the crate's OWN suite (`crates/once-ptr-cell/tests/loom_once_ptr_cell.rs`, run
// via `-p once-ptr-cell`), which is the whole point of the extraction; sefer's
// loom harnesses never exercise these chunk cells, so the shim is never on any
// modeled interleaving — it exists only to keep the const static compiling.
use super::chunk::{RegistryChunk, CHUNK_SLOTS, NUM_CHUNKS};
use super::ensure::ensure_chunk_slow;
#[cfg(loom)]
use super::loom_shim::OncePtrCell;
use crate::registry::heap_slot::HeapSlot;
#[cfg(not(loom))]
use once_ptr_cell::OncePtrCell;
// CRATE-P7: the `free_slots` stack head. Under a NORMAL/`production` build sefer
// uses the real `tagged_index_stack::StackHead`. Under `--cfg loom` the crate
// aliases its atomics to `loom`, so `StackHead::new` is NOT `const`
// (loom's `AtomicU64::new` has no const ctor) and `static REGISTRY: Registry =
// Registry::new()` would fail to const-evaluate — the SAME const-static hazard
// the `OncePtrCell` shim above solves. So under loom we swap in a const-capable,
// `core`-atomic shim with the identical `new`/`StackStorage`/`StackOps` API
// `bootstrap` and `heap_registry` use (over the shim's mirrored
// `StackStorage`/`StackOps` traits — only the `AtomicU64` head must be `core`,
// not `loom`, for const-ness). This is
// sound: the real-type loom VERIFICATION of the tagged stack lives in the
// crate's OWN suite (`crates/tagged-index-stack/tests/loom_aba.rs`, run via
// `-p tagged-index-stack`); sefer's loom harnesses never exercise `free_slots`
// contention (the former in-tree `loom_free_slots_aba` model was replaced by
// that crate suite), so the shim is NEVER on any modeled interleaving — it
// exists only to keep the const static compiling.
#[cfg(loom)]
use super::loom_shim::StackHead;
#[cfg(not(loom))]
use tagged_index_stack::StackHead;

/// Maximum number of heaps the registry can hold. Each live thread claims one
/// slot for its heap; `recycle` returns it. 4096 is generous for realistic
/// thread counts (a process with > 4096 simultaneous threads is pathological
/// for an allocator; the cap can be raised if a measured workload needs it).
/// The slot space is chunked (see the module doc) into
/// [`chunk::NUM_CHUNKS`](super::chunk::NUM_CHUNKS) chunks of
/// [`chunk::CHUNK_SLOTS`](super::chunk::CHUNK_SLOTS)
/// slots, each materialised lazily via `aligned_vmem::reserve_aligned` on
/// first touch of an index inside it — NOT a `.data`/`.bss` cost, and no
/// longer a single whole-array reservation either.
pub const MAX_HEAPS: usize = 4096;

/// The bootstrap outcome: [`chunk::NUM_CHUNKS`](super::chunk::NUM_CHUNKS)
/// lazily-materialised chunk pointers plus the dynamic atomics that drive
/// `claim`/`recycle`.
///
/// Small and entirely `Atomic*`-typed (`NUM_CHUNKS` pointers + two more
/// atomics — 512 + 12 bytes at `NUM_CHUNKS = 64`), so — unlike the pre-chunking
/// `Registry`, which inlined the whole feature-dependent-size slot array and
/// therefore had to live behind a lazily-heap-allocated `AtomicPtr<Registry>`
/// — this struct is const-initialisable and lives as a genuine
/// `static REGISTRY: Registry = Registry::new()`. See [`ensure`].
pub struct Registry {
    /// One lazy CAS-published pointer cell per chunk of the slot space
    /// ([`once_ptr_cell::OncePtrCell`], the extracted `UNINIT -> INITIALIZING
    /// -> READY` state machine — see [`Registry::ensure_chunk`] /
    /// [`ensure_chunk_slow`]). `OncePtrCell` internally drives the same
    /// `null -> sentinel(1) -> real *mut RegistryChunk` transition this field
    /// used to spell out inline, with the identical Release-publish +
    /// spin-`Acquire`-while-INITIALIZING + OOM-rollback-then-re-race discipline;
    /// the seam here only reserves the OS pages and dereferences the published
    /// pointer.
    ///
    /// `pub(super)` (not private) so the sibling [`ensure`] module's
    /// `dbg_rollback_chunk_sentinel_reenterable` hook can drive the live
    /// registry's chunk cells; flat-bootstrap visibility is otherwise
    /// unchanged.
    pub(super) chunks: [OncePtrCell<RegistryChunk>; NUM_CHUNKS],
    /// High-water mark of allocated slots (the next unused slot index). A
    /// `claim` that finds `free_slots` empty `fetch_add`s this to mint a new
    /// slot. Capped at `MAX_HEAPS`.
    pub(crate) count: AtomicU32,
    /// The `free_slots` recycler: the ABA-tagged Treiber free-index stack,
    /// extracted to the `tagged-index-stack` crate (CRATE-P7). Its head is one
    /// `AtomicU64` packing `(index:16 | tag:48)`; the per-slot next links live
    /// slot-resident in [`HeapSlot::next_free`] and are bound to the head by
    /// `Registry`'s own `StackStorage` impl (see `heap_registry`'s
    /// `StackStorage<16> for Registry` impl and
    /// `pop_free_slot`/`push_free_slot`). The H-2 empty-transition tag
    /// preservation and the RAD-1 lazy-link discipline both live inside the
    /// crate's `StackStorage`/`StackOps` traits now. `INDEX_BITS = 16` holds
    /// every valid slot index
    /// (`0..MAX_HEAPS = 4096`) with the empty sentinel `0xFFFF` reserved above
    /// the cap, leaving the 48-bit ABA tag (the W7a repack). Initialised empty.
    pub(crate) free_slots: StackHead<16>,
}

impl Registry {
    /// Const-construct an all-`UNINIT` registry: every chunk pointer `null`,
    /// `count` zero, `free_slots` the empty tagged sentinel.
    ///
    /// Uses the `[const { .. }; N]` inline-const-in-array-repeat-expression
    /// syntax (stable since Rust 1.79, well under this crate's MSRV floor of
    /// 1.88 per `Cargo.toml`) to const-construct an array of `AtomicPtr` —
    /// `AtomicPtr` is `Copy`-free but IS const-constructible
    /// (`AtomicPtr::new` is a `const fn`), and `[const { EXPR }; N]`
    /// evaluates `EXPR` fresh for each element instead of requiring `EXPR: Copy`
    /// the way the bare `[EXPR; N]` repeat-expression form does. This avoids
    /// the alternative (a `const fn` looping and building the array by hand,
    /// or `[NULL_PTR; N]` after`unsafe`ly transmuting into `AtomicPtr` —
    /// unnecessary here since the inline-const form is directly available).
    const fn new() -> Self {
        Registry {
            chunks: [const { OncePtrCell::new() }; NUM_CHUNKS],
            count: AtomicU32::new(0),
            free_slots: StackHead::new(),
        }
    }

    /// Resolve a slot index to a `&'static HeapSlot`, materialising the
    /// owning chunk first if it has not been touched yet.
    ///
    /// **This is the SINGLE place in the crate that resolves an index to a
    /// `&'static HeapSlot`.** Every call site that used to index the old
    /// inline `slots: [HeapSlot; MAX_HEAPS]` array directly now calls this
    /// instead, so there is exactly one path that can ever dereference chunk
    /// memory, and it always guarantees the chunk exists before returning.
    /// Callers that already resolved an index via `pick_slot`/`bump_count`/
    /// `pop_free_slot` do NOT need any extra "ensure my chunk exists" step of
    /// their own — calling `slot()` (which they already do, immediately after
    /// obtaining the index) handles it uniformly, whether the index was
    /// freshly minted or popped off the free list.
    ///
    /// # Panics
    ///
    /// Panics if `idx >= MAX_HEAPS` (an internal contract violation — every
    /// caller in this crate derives `idx` from `pick_slot`/a previously
    /// `claim`ed heap's `id()`, both of which are range-checked before
    /// reaching here; see each call site's own range check).
    ///
    /// # OOM
    ///
    /// If the owning chunk has not yet been materialised and the OS refuses
    /// the VM reservation, this method **aborts the process** (preserving the
    /// historic infallible `&'static HeapSlot` contract for alloc-path
    /// callers). Free-path callers that must not abort should use
    /// [`slot_or_none`](Self::slot_or_none) instead (R34-15/task #534).
    #[inline]
    pub(crate) fn slot(&self, idx: usize) -> &'static HeapSlot {
        debug_assert!(idx < MAX_HEAPS, "slot index out of range: {idx}");
        let chunk_idx = idx / CHUNK_SLOTS;
        let slot_in_chunk = idx % CHUNK_SLOTS;
        let chunk = self.ensure_chunk(chunk_idx);
        // SAFETY: `slot_in_chunk < CHUNK_SLOTS` by construction (`% CHUNK_SLOTS`).
        unsafe { chunk.slots.get_unchecked(slot_in_chunk) }
    }

    /// Fallible variant of [`slot`](Self::slot) for the **free path**
    /// (R34-15/task #534). Returns `None` when the owning chunk has not yet
    /// been materialised AND the OS refuses the VM reservation, instead of
    /// aborting. Every free-path caller already has a defensive "unstamped /
    /// garbled owner id" early-return two lines above its call site; the
    /// `None` case folds into that same graceful bail.
    ///
    /// Alloc-path callers continue to use the infallible [`slot`](Self::slot)
    /// — they run on the allocating thread where an OOM abort is the correct
    /// policy (the allocation itself would fail immediately afterward), so
    /// this method is deliberately NOT a drop-in replacement for `slot()`
    /// across the crate.
    ///
    /// **F-3 context (documented, not fixed):** the two production callers —
    /// `resolve_dirty_bit_target` and `resolve_heap_overflow` in
    /// `heap_core_xthread` — read `owner_id` from *foreign* segment
    /// memory with only an `idx < MAX_HEAPS` range check before indexing the
    /// registry. A garbled-but-in-range id therefore triggers a FRESH OS
    /// reservation of a registry chunk on the dealloc path. By itself this is
    /// harmless (the chunk is a small leaked reservation that a future
    /// `claim()` would have materialised anyway), but it is the same input
    /// that reaches this OOM branch — which is exactly why the free path must
    /// not abort here. For a single legitimate cross-thread free the segment
    /// cannot be released under the freer (the block holds `live_count >= 1`
    /// until the owner's drain), so there is no additional UAF window; the
    /// residual risk is the same caller-contract-violation surface (double
    /// free / stale pointer) every allocator has.
    #[inline]
    pub(crate) fn slot_or_none(&self, idx: usize) -> Option<&'static HeapSlot> {
        debug_assert!(idx < MAX_HEAPS, "slot index out of range: {idx}");
        let chunk_idx = idx / CHUNK_SLOTS;
        let slot_in_chunk = idx % CHUNK_SLOTS;
        let chunk = self.try_ensure_chunk(chunk_idx)?;
        // SAFETY: `slot_in_chunk < CHUNK_SLOTS` by construction (`% CHUNK_SLOTS`).
        Some(unsafe { chunk.slots.get_unchecked(slot_in_chunk) })
    }

    /// Genuinely non-materialising peek for the **stats/diagnostic read path**
    /// (R2-11/task #2013). Returns `Some(&'static HeapSlot)` only if the
    /// owning chunk is ALREADY published (a pure `Acquire` load — the exact
    /// same fast-path check [`ensure_chunk`](Self::ensure_chunk) /
    /// [`try_ensure_chunk`](Self::try_ensure_chunk) already perform before
    /// falling through to [`ensure_chunk_slow`]), and `None` otherwise —
    /// WITHOUT ever calling `ensure_chunk_slow`. Unlike [`slot`](Self::slot)
    /// and [`slot_or_none`](Self::slot_or_none), this method never performs an
    /// OS reservation, never spin-waits on a concurrent initialiser, and never
    /// aborts: a chunk observed not-yet-`READY` is simply reported absent for
    /// THIS call.
    ///
    /// **Why this exists.** `walk_initialised_slots`
    /// (`heap_registry::counters`) — the shared loop backing
    /// `SeferAlloc::stats()`'s hit-rate aggregation — used to call
    /// [`slot`](Self::slot) on every index in `0..count`, on the (refuted)
    /// assumption that every such index's owning chunk was already
    /// materialised by the time the walk reached it. In fact `bump_count`
    /// (`heap_registry::stack`) mints a fresh index by bumping `count` and
    /// returns immediately — it does NOT call `slot()` itself; the caller
    /// (`claim`/`claim_with_config`) calls `reg.slot(idx)` as a SEPARATE,
    /// later step. A concurrent `stats()` call can observe the just-bumped
    /// `count` and reach that index before the claiming thread's own
    /// `slot()` call has materialised the chunk — driving `stats()` (meant to
    /// be a cheap, read-only diagnostic snapshot) into a real OS reservation,
    /// a spin-wait on the claimer's in-flight initialisation, or — on
    /// chunk-materialisation OOM — `std::process::abort()`. This accessor
    /// closes that: the walk now SKIPS an index whose chunk is not yet
    /// ready, exactly as it already skips an index whose slot is not yet
    /// `initialised` — both are the same "this slot hasn't finished being
    /// claimed yet" case, benign to omit from one snapshot (the very next
    /// `stats()` call observes it once the real claim finishes).
    ///
    /// # Panics
    ///
    /// Panics if `idx >= MAX_HEAPS` (same internal-contract-violation
    /// discipline as [`slot`](Self::slot) / [`slot_or_none`](Self::slot_or_none)
    /// — every caller derives `idx` from a `count`-bounded loop).
    ///
    /// `alloc-stats`-gated: its sole caller, `walk_initialised_slots`
    /// (`heap_registry::counters`), only exists under that feature (the
    /// per-slot counters it aggregates are themselves only ever incremented
    /// under `alloc-stats` — see that function's own doc comment). Gating
    /// this method the same way avoids an unused-`pub(crate)`-method dead-code
    /// lint in any feature configuration without `alloc-stats`.
    #[cfg(feature = "alloc-stats")]
    #[inline]
    pub(crate) fn slot_if_materialised(&self, idx: usize) -> Option<&'static HeapSlot> {
        debug_assert!(idx < MAX_HEAPS, "slot index out of range: {idx}");
        let chunk_idx = idx / CHUNK_SLOTS;
        let slot_in_chunk = idx % CHUNK_SLOTS;
        let p = self.chunks[chunk_idx].get()?;
        // SAFETY: identical argument to `ensure_chunk`'s fast path above —
        // `OncePtrCell::get` returned `Some` only after observing a real
        // (non-null, non-sentinel) pointer under `Acquire`, which the
        // initialising thread published with `Release` after the OS
        // reservation's pages were fully valid (OS-zeroed pages already form
        // a valid `RegistryChunk`). The reservation is leaked
        // (`leak_zeroed_pages`) and lives for the process lifetime, so
        // `&'static` is sound.
        let chunk = unsafe { p.as_ref() };
        // SAFETY: `slot_in_chunk < CHUNK_SLOTS` by construction (`% CHUNK_SLOTS`).
        Some(unsafe { chunk.slots.get_unchecked(slot_in_chunk) })
    }

    /// Ensure chunk `chunk_idx` is materialised, then return a `&'static
    /// RegistryChunk` reference to it. Fast path: [`OncePtrCell::get`] (one
    /// `Acquire` load + non-null/non-sentinel check, inside the cell). Slow
    /// path (first touch or race): [`ensure_chunk_slow`], which drives the
    /// cell's `get_or_try_init` (CAS-reserve, OS reservation, Release-publish,
    /// spin-while-INITIALIZING loser, OOM rollback).
    ///
    /// **OOM policy (alloc path):** on chunk-materialisation OOM this method
    /// ABORTS the process. This preserves the historic infallible
    /// `&'static RegistryChunk` contract for every alloc-path caller of
    /// [`slot`](Self::slot) / `pick_slot` / `claim` (an OOM abort on the
    /// alloc path is the correct policy — the allocation itself would fail
    /// immediately afterward). Free-path callers use [`try_ensure_chunk`]
    /// instead (R34-15/task #534).
    #[inline]
    fn ensure_chunk(&self, chunk_idx: usize) -> &'static RegistryChunk {
        if let Some(p) = self.chunks[chunk_idx].get() {
            // SAFETY: `OncePtrCell::get` returned `Some` only after observing
            // a real (non-null, non-sentinel) pointer under `Acquire`. The
            // initialising thread published it with `Release` AFTER the OS
            // reservation's pages were fully valid (OS-zeroed pages already
            // form a valid `RegistryChunk` — see `ensure_chunk_slow`), so this
            // Acquire-observed pointer sees all those bytes. The reservation is
            // leaked (`leak_zeroed_pages`) and lives for the process lifetime,
            // so `&'static` is sound.
            return unsafe { p.as_ref() };
        }
        match ensure_chunk_slow(&self.chunks[chunk_idx]) {
            Some(chunk) => chunk,
            None => {
                // Chunk-materialisation OOM (alloc path). The cell has ALREADY
                // rolled its sentinel back to null (anti-livelock — losers
                // re-race; a future `slot()` call can retry this chunk index).
                //
                // We keep the historic ABORT policy for the alloc path
                // (unchanged in effect from before R34-15): `Registry::slot` /
                // `pick_slot` / `claim` assume `slot()` always succeeds
                // (`&'static HeapSlot`, not `Option<..>`), and a
                // chunk-materialisation OOM is exceedingly rare (the OS
                // refusing a tens-of-KiB-to-low-MiB reservation while the
                // process is already so starved that the `HeapCore::new()`
                // segment reservation a few lines later would fail anyway).
                // The free path now has a non-aborting path via
                // [`try_ensure_chunk`] / [`slot_or_none`] (R34-15/task #534);
                // widening `slot()` itself to `Option` remains deliberately
                // out of scope (alloc-path callers still need the infallible
                // `&'static` contract).
                std::process::abort();
            }
        }
    }

    /// Fallible variant of [`ensure_chunk`](Self::ensure_chunk) for the
    /// **free path** (R34-15/task #534). Returns `None` on
    /// chunk-materialisation OOM instead of aborting. The cell's anti-livelock
    /// rollback (sentinel back to null) runs identically in both variants —
    /// only the caller-visible policy differs.
    #[inline]
    fn try_ensure_chunk(&self, chunk_idx: usize) -> Option<&'static RegistryChunk> {
        if let Some(p) = self.chunks[chunk_idx].get() {
            // SAFETY: same argument as `ensure_chunk`'s fast path above.
            return Some(unsafe { p.as_ref() });
        }
        ensure_chunk_slow(&self.chunks[chunk_idx])
    }
}

// `Registry` is shared across threads via `&'static Registry`. All mutable
// access to its fields goes through atomics (`chunks`, `count`, `free_slots`)
// or the slot-level single-writer protocol inside a materialised chunk (see
// `HeapSlot`'s own `Sync` proof in `heap_slot.rs`). Every field is `Atomic*`,
// so `Registry` AUTO-derives `Sync`; no `unsafe impl` is needed (task #21 /
// review L1 — carried forward from the pre-chunking design). This
// compile-time assert documents the intent AND enforces it: adding a `!Sync`
// field makes THIS line fail to compile with a clear "`Registry: Sync` is not
// satisfied" error.
const _: () = {
    fn assert_sync<T: Sync>() {}
    let _ = assert_sync::<Registry>;
};

// -------------------------------------------------------------------------
// Test-only `#[doc(hidden)]` accessors (task #93 / R4-MS-4, adapted for
// chunking — R6-OPT-P0-2 round 1).
//
// `HeapSlot`'s state/generation fields are `pub(crate)`: safe code OUTSIDE
// the crate must not be able to mutate the slot state machine or push onto
// `free_slots`. The integration tests in `tests/` that legitimately need to
// OBSERVE slot state/generation (and, in one counterfactual, preset a
// generation near the u32 boundary) go through these narrow accessors
// instead. The reads are plain atomic loads — always sound — so they stay
// safe `fn`. The single write (`dbg_slot_preset_generation`) is `unsafe fn`
// because its soundness needs the slot to not be racing a concurrent
// `claim()`; the only caller (`tests/regression_counter_wrap.rs`) wraps it in
// `unsafe { .. }` under a documented precondition. These are NOT stable
// public API.
// -------------------------------------------------------------------------
impl Registry {
    /// Read a slot's `state` atomically (test helper). Materialises the
    /// slot's chunk if not already materialised (mirrors production `slot()`
    /// behaviour — a test reading a not-yet-claimed slot's state observes
    /// `STATE_FREE` from the freshly-materialised, OS-zeroed chunk).
    #[doc(hidden)]
    #[inline]
    pub fn dbg_slot_state(&self, idx: usize) -> u8 {
        self.slot(idx).state.load(Ordering::Acquire)
    }

    /// Read a slot's `generation` atomically (test helper).
    #[doc(hidden)]
    #[inline]
    pub fn dbg_slot_generation(&self, idx: usize) -> u64 {
        self.slot(idx).generation.load(Ordering::Acquire)
    }

    /// Preset a slot's `generation` to `val` (test helper).
    ///
    /// # Safety
    ///
    /// The caller must ensure no other thread is concurrently `claim`ing or
    /// `recycle`ing this slot. The only legitimate use is the
    /// `tests/regression_counter_wrap.rs` u64-width counterfactual, which holds
    /// the sole live handle to the slot under a single-threaded test and only
    /// presets the generation of the slot it itself owns. `generation` is
    /// written by the slot's owner on (re)claim; presetting it out from under a
    /// live owner would corrupt the M8/M9 owner key stamped into segment
    /// headers. The body is a plain atomic store (sound by itself); the
    /// `unsafe fn` boundary carries the protocol precondition above.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn dbg_slot_preset_generation(&self, idx: usize, val: u64) {
        self.slot(idx).generation.store(val, Ordering::Release)
    }

    /// Test-only introspection (R6-OPT-P0-2 round 1): has chunk `chunk_idx`
    /// been materialised yet? `true` iff `self.chunks[chunk_idx]` holds a
    /// real (non-null, non-sentinel) pointer. Used by the "chunking actually
    /// happens" test to assert that claiming slot 0 does NOT materialise
    /// chunk 1..63 — the core deliverable this round exists to prove.
    #[doc(hidden)]
    #[inline]
    #[must_use]
    pub fn dbg_chunk_is_materialised(&self, chunk_idx: usize) -> bool {
        self.chunks[chunk_idx].dbg_is_ready()
    }
}

// -------------------------------------------------------------------------
// The process-global registry — now a plain `static` (see the module doc's
// "R6-OPT-P0-2 (round 1)" section for why this is sound: `Registry` shrank
// from an inline feature-dependent-size slot array to 64 pointers + 2
// atomics once the slot array itself moved behind per-chunk laziness).
// -------------------------------------------------------------------------

// `pub(super)` (not private) so the sibling [`ensure`] module's `ensure()`
// accessor and `dbg_rollback_chunk_sentinel_reenterable` hook can reach the
// singleton; flat-bootstrap visibility is otherwise unchanged.
pub(super) static REGISTRY: Registry = Registry::new();
