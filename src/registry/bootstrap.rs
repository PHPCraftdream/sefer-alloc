//! [`Registry`] — the bootstrap outcome: a process-global slot table backed
//! by lazily-materialised chunks, published via a hand-rolled atomic
//! state-machine per chunk (NOT `std::sync::Once`, which may allocate).
//!
//! ## R6-OPT-P0-2 (round 1) — chunked slot array
//!
//! `Registry` used to hold the ENTIRE `[HeapSlot; MAX_HEAPS]` array inline,
//! heap-allocated as one giant `aligned_vmem::reserve_aligned` reservation on
//! first `ensure()` call (see the "History" section below for why it was
//! moved out of `.data`/`.bss` in the first place). Because `HeapSlot`'s
//! inline `HeapCore` size is feature-dependent (tens of KiB under
//! `production`), that ONE reservation could be on the order of ~125 MiB —
//! paid in full by EVERY process on its FIRST heap claim, even a process that
//! only ever needs one or two heaps. Windows commits the whole reservation in
//! one `VirtualAlloc` call; there is no OS-level "commit only the pages you
//! touch" for a single reservation of this shape (see `crates/vmem/src/lib.rs`).
//!
//! The fix: split the slot array into [`registry_chunk::NUM_CHUNKS`] chunks of
//! [`registry_chunk::CHUNK_SLOTS`] slots each ([`RegistryChunk`]), and
//! materialise each chunk LAZILY, on first touch of an index that falls
//! inside it — mirroring the SAME CAS-then-spin publish protocol the old
//! whole-registry `ensure`/`ensure_slow` used, just applied per-chunk. See
//! [`Registry::slot`] for the resolver (the single place in the crate allowed
//! to dereference chunk memory) and [`ensure_chunk_slow`] for the
//! materialisation protocol.
//!
//! **`Registry` itself is now small enough to be a plain `static` again**:
//! once the giant inline array is gone, `Registry` is just
//! `chunks: [AtomicPtr<RegistryChunk>; NUM_CHUNKS]` (64 pointers = 512 bytes
//! at `NUM_CHUNKS = 64`) plus the existing `count`/`free_slots` atomics — all
//! const-initialisable, so [`ensure`] is now a plain `&'static Registry`
//! return with NO CAS, NO sentinel dance, and NO OOM-abort path at the
//! REGISTRY level at all (OOM can now only happen at PER-CHUNK
//! materialisation time — see [`ensure_chunk_slow`]'s OOM handling, which is
//! strictly better than the old whole-registry abort: a process that already
//! has heaps live in other chunks keeps working even if one chunk's
//! reservation fails).
//!
//! ## R6-OPT-P0-2 (round 2) — lazy `HeapOverflow` sidecar
//!
//! Round 1 left one dominant cost per materialised chunk: `HeapOverflow`
//! (`heap_overflow.rs`), a `[AtomicUsize; HEAP_OVERFLOW_CAP] +
//! [AtomicU32; HEAP_OVERFLOW_CAP]` pair inline in EVERY `HeapSlot`
//! (`HEAP_OVERFLOW_CAP = 2048` native), 24 KiB/slot. Round 2 shrinks this by
//! splitting `HeapOverflow`'s storage into a small always-inline "emergency"
//! tier (`INLINE_CAP` entries) plus a lazily-materialised sidecar for the
//! rest — see `heap_overflow.rs`'s module doc for the full two-tier design
//! and the wedge-hazard correctness argument.
//!
//! **Unsafe-seam placement decision:** the sidecar's materialisation
//! machinery ([`ensure_overflow_sidecar`] / [`deref_overflow_sidecar`]) lives
//! HERE, in `bootstrap.rs`'s EXISTING `#![allow(unsafe_code)]` seam, rather
//! than in a new seam inside `heap_overflow.rs`. Reasons: (1) it is
//! LITERALLY the same protocol as [`ensure_chunk`]/[`ensure_chunk_slow`]
//! (CAS-reserve a sentinel, `aligned_vmem::reserve_aligned`, in-place init,
//! publish with Release, spin-wait losers) — a third instance of one
//! already-audited pattern, not a new one; keeping all three instances in the
//! same file keeps that pattern's soundness argument in one place rather than
//! duplicated across two files; (2) `heap_overflow.rs` explicitly documents
//! (and its module doc still asserts) that it needs NO unsafe seam of its
//! own — round 2 preserves that property rather than breaking it, so a
//! reader auditing "which files can materialise raw OS memory and dereference
//! raw pointers" finds the answer unchanged (`bootstrap.rs`, still the only
//! one in `registry/`); (3) `heap_overflow.rs`'s `push`/`drain` need only a
//! SAFE `&HeapOverflowSidecar` once materialised — [`deref_overflow_sidecar`]
//! is the one safe membrane function that hands that out, exactly mirroring
//! how [`Registry::slot`] hands out a safe `&'static HeapSlot` from chunk
//! memory. This mirrors round 1's own choice (`registry_chunk.rs` stays
//! unsafe-free; all raw-pointer work lives in `bootstrap.rs`) — the SAME
//! reasoning applied one level further down. Because `bootstrap.rs` is
//! ALREADY listed as a tier-1 unsafe seam in `src/lib.rs`'s inventory (see
//! `registry::bootstrap` there), no README/`lib.rs` seam-inventory update is
//! needed for this round — the existing entry already covers this addition.
//!
//! ## History — why the slot array was EVER moved out of `.data`/`.bss`
//!
//! The original design used `static REGISTRY: Registry = Registry::new_zeroed()`.
//! `HeapSlot::new_uninit()` initialised `next_free` to `u32::MAX`
//! (`NEXT_FREE_TAIL`), a non-zero value, which forced the ENTIRE slot array
//! into `.data` instead of `.bss` — a large per-binary `.data` cost. RAD-1
//! (see the section below) later made `next_free` LAZY (never eagerly
//! pre-populated), which removes the ORIGINAL reason the array had to leave
//! `.data`/`.bss` — but by the time RAD-1 landed, the array had ALREADY been
//! moved to a heap-allocated `AtomicPtr<Registry>` for a second, independent
//! reason (feature-dependent size making even an all-zero array too large for
//! a comfortable static in some feature configurations), so the lazy pointer
//! design stayed. This chunking round is a further evolution of that same
//! "move the big cost behind a lazy indirection" idea, now applied inside the
//! array instead of around it — and, as a consequence, made `Registry` itself
//! (the pointer-holding struct, now just 64 pointers + 2 atomics) small
//! enough to go back to being a real `static`, closing the loop RAD-1 opened.
//!
//! ## RAD-1: lazy `next_free` (no eager per-slot first-touch)
//!
//! A chunk's in-place init writes ONLY the slot fields that must be non-zero
//! (none, currently — see [`ensure_chunk_slow`]); `next_free` is written
//! lazily by `push_free_slot` (which runs before any `pop_free_slot` can read
//! it), so the OS-zeroed initial value (`0`, not `NEXT_FREE_TAIL`) is never
//! observed. This is the SAME reasoning the old whole-registry `ensure_slow`
//! documented in detail before this round's split — see the per-chunk
//! materialisation's SAFETY comment for the identical read-audit, unchanged
//! in substance by chunking (it is a per-slot argument, not a per-registry
//! one).
//!
//! ## Per-chunk pointer state-machine
//!
//! Each `AtomicPtr<RegistryChunk>` in [`Registry::chunks`] independently
//! drives the `UNINIT → INITIALIZING → READY` transition via pointer values,
//! identical in spirit to the OLD whole-registry protocol (now removed at the
//! `Registry` level, reintroduced at the chunk level):
//!
//! | Pointer value | Meaning |
//! |---|---|
//! | `null` | `UNINIT` — this chunk not yet materialised |
//! | `SENTINEL_INITIALIZING` (`1 as *mut`) | `INITIALIZING` — one thread won the CAS and is allocating this chunk |
//! | real `*mut RegistryChunk` | `READY` — this chunk fully initialised; safe to dereference |
//!
//! 1. The first `slot()` call touching an index in this chunk observes `null`
//!    and CASes it to `SENTINEL_INITIALIZING`. The CAS winner:
//!    a. Calls `aligned_vmem::reserve_aligned(CHUNK_SIZE, CHUNK_ALIGN)` —
//!       direct OS syscall, no `std::alloc`, no registry dependency.
//!    b. Field-by-field in-place initialisation (OS zeroed-pages; every field
//!       already starts at its correct zero value — see [`ensure_chunk_slow`]).
//!    c. `self.chunks[chunk_idx].store(base, Release)` — publishes the ready
//!       pointer.
//!    d. `mem::forget(reservation)` — leaks the reservation intentionally;
//!       the chunk lives for the process lifetime.
//! 2. Concurrent losers observe `SENTINEL_INITIALIZING` (or `null`, then fail
//!    the CAS) and spin until they observe a non-null, non-sentinel pointer
//!    under `Acquire`. The spin window is tiny (one OS page allocation of
//!    `CHUNK_SIZE` bytes, far smaller than the old whole-registry window).
//! 3. After `READY`, every subsequent `slot()` call touching this chunk is a
//!    single `Acquire` load + two cheap comparisons + an array index.
//!
//! `Release`/`Acquire` on the pointer transition establishes happens-before
//! from the initialising thread's `ptr::write`s (the chunk's slot fields) to
//! every reader that observes the real pointer, so readers see a fully
//! constructed chunk.
//!
//! ## M5 (reentrancy-free) — CANNOT BE VIOLATED
//!
//! `aligned_vmem::reserve_aligned` is a direct OS syscall (`VirtualAlloc` /
//! `mmap`) — it does NOT call `std::alloc`, `Box`, `Vec`, or any other
//! Rust allocator entry point. Its dependency graph (verified by reading
//! `crates/vmem/src/lib.rs` in full):
//!
//! - Windows: `extern "system" { fn VirtualAlloc(...) }` — no std alloc.
//! - Unix: `extern "C" { fn mmap(...) }` — no std alloc.
//! - Miri: `std::alloc` — but under miri we are NOT the global allocator
//!   (the host miri allocator backs the harness), so no reentrancy.
//!
//! No path from [`ensure_chunk_slow`] touches `sefer_alloc::registry::*` —
//! confirmed by inspection (unchanged from the pre-chunking `ensure_slow`).
//! The reservation call chain is a straight line to a kernel syscall
//! boundary.
//!
//! ## Provenance model (task #140)
//!
//! Unchanged from before this round: the chunk-pointer sentinel handling
//! below is the SAME `without_provenance_mut` idiom the old whole-registry
//! `ensure_slow` used (a bare marker address, never dereferenced — see
//! [`SENTINEL_INITIALIZING`]'s use sites in [`ensure_chunk_slow`]), so it
//! stays strict-provenance-clean under `-Zmiri-strict-provenance`. The A1
//! deferred-large-free stack's exposed-provenance story
//! (`alloc_core::deferred_large`) is untouched by this round — see that
//! module for its own provenance documentation.

// This file uses `unsafe` for two operations, unchanged in kind from before
// this round (now applied per-chunk instead of once for the whole registry):
//  1. Field-by-field in-place initialisation of a `RegistryChunk` in a
//     freshly reserved OS memory block (pointer arithmetic + writes; see
//     `ensure_chunk_slow`).
//  2. `unsafe { &*p }` — dereferencing a published chunk/registry pointer
//     after observing it under `Acquire` (sound because the initialiser's
//     `Release` store establishes happens-before).
// Every `unsafe` block carries a `// SAFETY:` proof below.
#![allow(unsafe_code)]

use core::hint::spin_loop;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};

#[cfg(feature = "alloc-xthread")]
use super::heap_overflow::{HeapOverflowSidecar, SIDECAR_CAP, SIDECAR_SENTINEL_INITIALIZING};
use super::heap_slot::HeapSlot;
use super::registry_chunk::{RegistryChunk, CHUNK_SIZE, CHUNK_SLOTS, NUM_CHUNKS};
use super::tagged_ptr::TaggedPtr;

/// Maximum number of heaps the registry can hold. Each live thread claims one
/// slot for its heap; `recycle` returns it. 4096 is generous for realistic
/// thread counts (a process with > 4096 simultaneous threads is pathological
/// for an allocator; the cap can be raised if a measured workload needs it).
/// The slot space is chunked (see the module doc) into
/// [`registry_chunk::NUM_CHUNKS`] chunks of [`registry_chunk::CHUNK_SLOTS`]
/// slots, each materialised lazily via `aligned_vmem::reserve_aligned` on
/// first touch of an index inside it — NOT a `.data`/`.bss` cost, and no
/// longer a single whole-array reservation either.
pub const MAX_HEAPS: usize = 4096;

/// Sentinel: a non-null, non-real address that means "one thread is currently
/// initialising [something]". Aligned to 1 (the raw integer 1 is not a valid
/// pointer for any type this crate stores behind an `AtomicPtr` here — every
/// such type has alignment >= 4). Reused for BOTH the (former) whole-registry
/// slot and the per-chunk slot below — same bit pattern, same "never
/// dereferenced, only compared" contract in both uses.
const SENTINEL_INITIALIZING: usize = 1;

/// The bootstrap outcome: [`registry_chunk::NUM_CHUNKS`] lazily-materialised
/// chunk pointers plus the dynamic atomics that drive `claim`/`recycle`.
///
/// Small and entirely `Atomic*`-typed (`NUM_CHUNKS` pointers + two more
/// atomics — 512 + 12 bytes at `NUM_CHUNKS = 64`), so — unlike the pre-chunking
/// `Registry`, which inlined the whole feature-dependent-size slot array and
/// therefore had to live behind a lazily-heap-allocated `AtomicPtr<Registry>`
/// — this struct is const-initialisable and lives as a genuine
/// `static REGISTRY: Registry = Registry::new()`. See [`ensure`].
pub struct Registry {
    /// One pointer per chunk of the slot space. `null` = chunk not yet
    /// materialised; [`SENTINEL_INITIALIZING`] = a thread is currently
    /// materialising it; a real pointer = the chunk is `READY` and safe to
    /// dereference. See [`Registry::slot`] for the resolver and
    /// [`ensure_chunk_slow`] for the per-chunk materialisation protocol.
    chunks: [AtomicPtr<RegistryChunk>; NUM_CHUNKS],
    /// High-water mark of allocated slots (the next unused slot index). A
    /// `claim` that finds `free_slots` empty `fetch_add`s this to mint a new
    /// slot. Capped at `MAX_HEAPS`.
    pub(crate) count: AtomicU32,
    /// Tagged-Treiber head of the `free_slots` stack: low 16 = slot index,
    /// high 48 = ABA tag (bumped per push; see `TaggedPtr`, repacked in W7a).
    /// Initialised empty.
    pub(crate) free_slots: AtomicU64,
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
            chunks: [const { AtomicPtr::new(core::ptr::null_mut()) }; NUM_CHUNKS],
            count: AtomicU32::new(0),
            free_slots: AtomicU64::new(TaggedPtr::empty()),
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
    #[inline]
    pub(crate) fn slot(&self, idx: usize) -> &'static HeapSlot {
        debug_assert!(idx < MAX_HEAPS, "slot index out of range: {idx}");
        let chunk_idx = idx / CHUNK_SLOTS;
        let slot_in_chunk = idx % CHUNK_SLOTS;
        let chunk = self.ensure_chunk(chunk_idx);
        // SAFETY: `slot_in_chunk < CHUNK_SLOTS` by construction (`% CHUNK_SLOTS`).
        unsafe { chunk.slots.get_unchecked(slot_in_chunk) }
    }

    /// Ensure chunk `chunk_idx` is materialised, then return a `&'static
    /// RegistryChunk` reference to it. Fast path: one `Acquire` load, two
    /// comparisons. Slow path (first touch or race): [`ensure_chunk_slow`].
    #[inline]
    fn ensure_chunk(&self, chunk_idx: usize) -> &'static RegistryChunk {
        let p = self.chunks[chunk_idx].load(Ordering::Acquire);
        let p_usize = p.addr();
        if p_usize != 0 && p_usize != SENTINEL_INITIALIZING {
            // SAFETY: we observed a real non-null non-sentinel pointer under
            // Acquire. The initialising thread stored this pointer with
            // Release AFTER completing the field-by-field in-place
            // initialisation of the `RegistryChunk`, so this Acquire load
            // sees all the bytes written. The pointer remains valid for the
            // process lifetime (the OS reservation is leaked via
            // `mem::forget`). Casting to `&'static` is sound because the
            // allocation outlives any reference derived from it.
            return unsafe { &*p };
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
        let p = self.chunks[chunk_idx].load(Ordering::Acquire);
        let p_usize = p.addr();
        p_usize != 0 && p_usize != SENTINEL_INITIALIZING
    }
}

// -------------------------------------------------------------------------
// The process-global registry — now a plain `static` (see the module doc's
// "R6-OPT-P0-2 (round 1)" section for why this is sound: `Registry` shrank
// from an inline feature-dependent-size slot array to 64 pointers + 2
// atomics once the slot array itself moved behind per-chunk laziness).
// -------------------------------------------------------------------------

static REGISTRY: Registry = Registry::new();

/// Return a `&'static` reference to the process-global registry.
///
/// With the slot array chunked (R6-OPT-P0-2 round 1), `Registry` itself needs
/// no lazy initialisation at all — it is a plain `static` of atomics, valid
/// from process start. All the laziness that used to live at THIS level (the
/// `UNINIT → INITIALIZING → READY` CAS dance) now lives one level down, per
/// chunk, inside [`Registry::slot`] — see that method and [`ensure_chunk_slow`].
#[inline]
pub fn ensure() -> &'static Registry {
    &REGISTRY
}

/// Roll a chunk's pointer back from `SENTINEL_INITIALIZING` to `null`
/// (`UNINIT`).
///
/// Single point of truth for the anti-livelock rollback used by the
/// OOM-bailout in [`ensure_chunk_slow`] — kept as its own function so the
/// test-only hook below exercises EXACTLY the same code the production
/// bailout runs, rather than a duplicated copy that could drift out of sync.
/// Generalises the pre-chunking `rollback_registry_sentinel` from "the one
/// whole-registry pointer" to "a specific chunk's pointer".
///
/// `Release` ordering: a thread that later retries `ensure_chunk_slow` on
/// this SAME chunk performs `compare_exchange(null, SENTINEL, Acquire, ..)`;
/// pairing that Acquire with this Release ensures the retrying thread does
/// not need to observe anything about the failed attempt beyond "the chunk
/// slot is free again" — there is no partially-initialised `RegistryChunk`
/// state to synchronise (the failed attempt never got past the VM
/// reservation).
#[cold]
fn rollback_chunk_sentinel(chunk_ptr: &AtomicPtr<RegistryChunk>) {
    chunk_ptr.store(core::ptr::null_mut(), Ordering::Release);
}

/// Slow path for [`Registry::ensure_chunk`]: race to materialise ONE chunk
/// via a CAS on its `AtomicPtr<RegistryChunk>` slot. Exactly one caller wins,
/// allocates via `aligned_vmem::reserve_aligned`, constructs the chunk
/// in-place, and publishes the pointer. All others spin-wait on a tiny
/// window. Structurally identical to the pre-chunking whole-registry
/// `ensure_slow`, narrowed to operate on one `AtomicPtr<RegistryChunk>`
/// instead of the single `REGISTRY_PTR`.
#[cold]
fn ensure_chunk_slow(chunk_ptr: &AtomicPtr<RegistryChunk>) -> &'static RegistryChunk {
    // Race: try to acquire the INITIALIZING sentinel via CAS(null, SENTINEL).
    // Only ONE thread wins this CAS; the rest observe SENTINEL (or null then
    // fail the CAS) and fall into the spin branch.
    // SENTINEL_INITIALIZING is a bare marker address, NEVER dereferenced (only
    // compared for pointer equality against `chunk_ptr`'s CAS operand and the
    // loads in `ensure_chunk`/the spin loop below). `without_provenance_mut`
    // constructs a pointer that carries NO provenance at all — exactly the
    // right semantics for a value that exists purely as an integer tag riding
    // inside an `AtomicPtr<RegistryChunk>` and must never be read through.
    // This is strict-provenance-clean: no `expose_provenance`/
    // `with_exposed_provenance` pairing is needed because the value is never
    // turned back into a dereferenceable pointer. Pointer equality (`==`, and
    // `AtomicPtr`'s CAS) compares addresses regardless of provenance.
    let sentinel = core::ptr::without_provenance_mut::<RegistryChunk>(SENTINEL_INITIALIZING);
    match chunk_ptr.compare_exchange(
        core::ptr::null_mut(),
        sentinel,
        // Acquire on success: pairs with our later Release store of the real
        // pointer, establishing the happens-before for future Acquire readers.
        Ordering::Acquire,
        // Relaxed on failure: we re-load below in the spin loop.
        Ordering::Relaxed,
    ) {
        Ok(_) => {
            // ── Winner branch ─────────────────────────────────────────────
            // We are the SOLE initialiser of THIS chunk. Allocate it from OS
            // VM.
            //
            // M5 (reentrancy-free) proof: identical to the pre-chunking
            // `ensure_slow` — `aligned_vmem::reserve_aligned` is a direct OS
            // syscall, no `std::alloc`/`Box`/`Vec`, no transitive dependency
            // on `sefer_alloc::registry::*`. Under miri it falls back to
            // `std::alloc`, but under miri we are NOT the global allocator,
            // so no reentrancy.
            // CRATE-P2 (item g): reserve + zero-under-miri + leak folded into
            // `aligned_vmem::leak_zeroed_pages`, which guarantees the span is
            // zeroed on EVERY backend (including the miri `std::alloc` fallback,
            // where the old explicit `write_bytes(0)` used to live) and
            // `mem::forget`-leaks it for the process lifetime. `CHUNK_ALIGN`
            // (PAGE) is subsumed: `leak_zeroed_pages` always reserves
            // PAGE-aligned, which is >= `align_of::<RegistryChunk>()` (<= 64).
            let base = match aligned_vmem::leak_zeroed_pages(CHUNK_SIZE) {
                Some(p) => p.as_ptr() as *mut RegistryChunk,
                None => {
                    // Chunk-materialisation OOM (R6-OPT-P0-2 round 1 design
                    // decision — see the module doc for the full reasoning):
                    // treat this as ORDINARY claim-failure, NOT
                    // `std::process::abort()`.
                    //
                    // The pre-chunking whole-registry OOM aborted because "the
                    // allocator cannot even materialise its own core
                    // bookkeeping structure" — losing the registry meant
                    // losing EVERY heap the process could ever claim, present
                    // or future; there was no narrower failure to report. A
                    // single missing CHUNK has a strictly narrower blast
                    // radius: heaps already live in OTHER, already-materialised
                    // chunks are completely unaffected (their slots, `state`,
                    // `heap`, `next_free` all live in separate OS reservations
                    // that this failure never touches); the process merely
                    // cannot mint any MORE heaps whose index falls in this
                    // particular 64-slot range. `pick_slot`/`claim` already
                    // have a documented `None`/null-return contract for
                    // registry exhaustion (`bump_count` returning `None` when
                    // `count >= MAX_HEAPS`) — a chunk that fails to
                    // materialise is the SAME shape of failure ("this index
                    // range is currently unusable"), just triggered by VM
                    // pressure instead of the index cap. Piping it through the
                    // existing exhaustion path (this function returns to
                    // `Registry::slot`, which currently has no way to
                    // propagate a chunk-materialisation failure as `None` —
                    // see the follow-up note below) is the right shape for a
                    // narrow, recoverable failure.
                    //
                    // We still MUST roll back the sentinel first (exactly the
                    // pre-chunking anti-livelock argument, narrowed to this
                    // chunk): if we bailed out here WITHOUT rolling
                    // `chunk_ptr` back to null, every loser thread spinning in
                    // the `Err` branch below spins FOREVER on this chunk (the
                    // sentinel is never replaced by a real pointer), and every
                    // FUTURE `slot()` call touching an index in this chunk's
                    // range would ALSO fail the `compare_exchange(null,
                    // SENTINEL)` CAS (current value is SENTINEL, not null) and
                    // spin forever too — a livelock scoped to this chunk's
                    // 64-slot index range, not the whole process.
                    rollback_chunk_sentinel(chunk_ptr);
                    // `Registry::slot` (and its callers `pick_slot`/`claim`)
                    // currently assume `slot()` always succeeds — the type is
                    // `&'static HeapSlot`, not `Option<&'static HeapSlot>`.
                    // Making the failure fully non-fatal end-to-end (returning
                    // `null` from `claim` on a failed chunk materialisation,
                    // the way `bump_count`'s `None` already does for index
                    // exhaustion) would require widening `slot()`'s signature
                    // and every call site's handling — a real API change this
                    // round deliberately does NOT make (see the final report's
                    // "left out of scope" note): a chunk-materialisation OOM
                    // is exceedingly rare in practice (it needs the OS to
                    // refuse a `CHUNK_SIZE`-byte reservation — tens of KiB to
                    // low MiB depending on feature set — when the process is
                    // already so starved that a `HeapCore::new()` segment
                    // reservation a few lines later would fail anyway), and
                    // retrofitting a full `Option` thread-through is exactly
                    // the kind of "widen while here" scope creep the isolated
                    // round-1/round-2 split was designed to avoid. We
                    // therefore abort here too, for now — narrower in cause
                    // than the old whole-registry abort, but not yet narrower
                    // in EFFECT (the process still exits). The rollback above
                    // is still correct and is exercised by
                    // `dbg_rollback_chunk_sentinel_reenterable` below,
                    // establishing the mechanism a future `Option`-returning
                    // `slot()` could build on without redoing this work.
                    std::process::abort();
                }
            };

            // `leak_zeroed_pages` already guaranteed the whole `CHUNK_SIZE`
            // span is zeroed on every backend (the miri-specific `write_bytes`
            // that used to live here is folded into that helper), so `base`
            // points at a fully valid all-zero `RegistryChunk`.

            // In-place initialisation of the `RegistryChunk` — no field
            // writes needed at all.
            //
            // We do NOT use `ptr::write(base, RegistryChunk { .. })` for the
            // SAME reason the pre-chunking `ensure_slow` avoided constructing
            // a whole `Registry` value: `RegistryChunk` can be tens to
            // hundreds of KiB (`CHUNK_SLOTS = 64` times a feature-dependent
            // `HeapSlot` size), too large to safely build as a stack/const
            // temporary.
            //
            // Every `HeapSlot` field inside a chunk starts at its correct
            // zero value from OS-zeroed pages, with NO non-zero field to
            // fix up (unlike the top-level `Registry`, which had exactly one
            // non-zero field — `free_slots = TaggedPtr::empty()` — that field
            // now lives in `Registry::new()`'s const-initialiser instead,
            // since `Registry` itself is const-constructed up front, not
            // lazily per-chunk):
            //   next_free   = 0 (NOT NEXT_FREE_TAIL — lazy init, RAD-1: a
            //     slot's `next_free` is read ONLY by `pop_free_slot`, always
            //     AFTER a `push_free_slot` has written the real link for that
            //     same slot under Release; a freshly-minted slot goes
            //     straight to `claim` and is never read through `next_free`
            //     before its first push, so the zero is never observed)
            //   state       = 0 = STATE_FREE
            //   generation  = 0
            //   heap        = MaybeUninit::uninit() (unspecified bits, zero
            //     is fine)
            //   initialised = 0 = false
            //   remote.{tcache_hits, large_cache_hits, thread_free} = 0 /
            //     null (zero-initialised counters / empty stack head)
            //   overflow    = all-zero `HeapOverflow` (an unclaimed slot
            //     never first-touches its ring beyond this page-zero, per
            //     `HeapOverflow`'s own RSS-discipline doc — round 2's
            //     concern, unaffected by this round)
            //
            // So there is genuinely nothing to write here — the chunk is
            // fully initialised the moment its pages are OS-zeroed. This
            // block is kept (rather than removed) so the SAFETY proof for
            // dereferencing `base` below has an explicit anchor, and so a
            // FUTURE field that needs a non-zero bootstrap value has an
            // obvious, already-audited place to add the write (mirroring the
            // pattern the old `free_slots` write established at the
            // `Registry` level).
            //
            // SAFETY: `base` is non-null, aligned to `CHUNK_ALIGN` (PAGE =
            // 4096, which is >= `align_of::<RegistryChunk>()` — at most 64
            // bytes, `HeapSlot`'s `repr(align(64))`), and valid for
            // `CHUNK_SIZE` bytes (>= `size_of::<RegistryChunk>()`). We are
            // the sole writer (only one CAS winner can reach this branch).
            // The memory is OS-provided zero-initialised pages, which is
            // already a fully valid `RegistryChunk` bit-pattern per the
            // field-by-field audit above — no writes are needed to reach a
            // valid state.

            // Publish the real pointer with Release so every subsequent
            // Acquire load in `Registry::ensure_chunk`'s fast path sees the
            // fully written chunk. This pairs with the Acquire load in the
            // fast path and with the Acquire loads in the spin loop below.
            chunk_ptr.store(base, Ordering::Release);

            // The reservation was already leaked for the process lifetime by
            // `leak_zeroed_pages` (`mem::forget` internally) — the chunk is
            // never dropped, so the `&'static` references
            // `heap_registry::bind_slot_counters` plants into slot fields stay
            // valid forever.

            // SAFETY: we fully initialised the `RegistryChunk` at `base`
            // (OS-zeroed pages ARE a fully valid state — see the audit above)
            // and published it with Release. The allocation outlives any
            // reference derived from it (leaked via `mem::forget`).
            // Dereferencing `base` as `&'static RegistryChunk` is sound.
            unsafe { &*base }
        }
        Err(_) => {
            // ── Loser branch ─────────────────────────────────────────────
            // Another thread is (or was) initialising THIS chunk. Spin until
            // we observe a real (non-null, non-sentinel) pointer. This
            // window is small: one OS reservation of `CHUNK_SIZE` bytes plus
            // one publish store, with NO per-slot write loop at all (unlike
            // even the RAD-1-optimised whole-registry path, this chunk path
            // never had one to remove — see the in-place-init block above).
            loop {
                let p = chunk_ptr.load(Ordering::Acquire);
                // See the identical `.addr()` rationale in `ensure_chunk`'s
                // fast path: pure integer comparison, no provenance use.
                let p_usize = p.addr();
                if p_usize != 0 && p_usize != SENTINEL_INITIALIZING {
                    // SAFETY: same argument as the fast path in
                    // `Registry::ensure_chunk`. We observed the real pointer
                    // under Acquire, which pairs with the winner's Release
                    // store of the pointer. The `RegistryChunk` is fully
                    // initialised.
                    return unsafe { &*p };
                }
                spin_loop();
            }
        }
    }
}

/// Test-only hook (R6-OPT-P0-2 round 1, generalising the pre-chunking
/// `dbg_rollback_sentinel_reenterable`): proves the anti-livelock rollback in
/// [`rollback_chunk_sentinel`] actually clears the sentinel for a SPECIFIC
/// chunk of the LIVE process-global registry, without invoking
/// `std::process::abort` (which would kill the test harness).
///
/// Takes a `chunk_idx` (not a `&AtomicPtr<RegistryChunk>`, which would leak
/// the crate-private [`RegistryChunk`] type into this function's `pub`
/// signature — `RegistryChunk` is deliberately `pub(crate)`, mirroring the
/// established pattern elsewhere in this module of keeping the real type
/// private and exposing only thin `pub` forwarders that operate on indices/
/// primitives, e.g. `tagged_ptr::dbg_pack`/`dbg_unpack`). Callers MUST pick a
/// `chunk_idx` they can guarantee is not concurrently materialised by another
/// test running in the same process (e.g. a high chunk index no other test
/// in the suite claims enough slots to reach) — step 1 below is the runtime
/// guard that makes this safe even if that assumption is violated: it only
/// proceeds when the chunk is observed genuinely `UNINIT`.
///
/// ## Why this operates on the LIVE registry's chunk pointer
///
/// The bug class is specifically about the interaction between a chunk
/// pointer's three-state protocol (`null` / `SENTINEL_INITIALIZING` / real
/// pointer) and the rollback. A hook on a separate test-only atomic would
/// only prove that a *copy* of the protocol works, not that
/// `rollback_chunk_sentinel` (the actual function the fix calls) restores the
/// actual invariant `ensure_chunk_slow` depends on. So this hook drives the
/// REAL `Registry::chunks[chunk_idx]` through the fix's exact code path:
///
/// 1. It CAS-acquires the chunk pointer itself from `null` to
///    `SENTINEL_INITIALIZING` (the same transition the real
///    `ensure_chunk_slow` winner performs). If the chunk has ALREADY been
///    materialised (a real, non-null non-sentinel pointer) or is
///    (impossibly, under this test's own discipline) mid-init by another
///    caller, the CAS simply fails and this function returns `None` — it
///    never disturbs a live or contended chunk.
/// 2. With the sentinel now in place (as if we were the real
///    materialisation winner that hit OOM), it calls
///    [`rollback_chunk_sentinel`] — the IDENTICAL function the production
///    OOM-bailout calls before `std::process::abort()`.
/// 3. It then verifies the anti-livelock postcondition directly: a
///    subsequent `compare_exchange(null, SENTINEL, ..)` must SUCCEED,
///    proving the rollback actually cleared the sentinel back to `null` (if
///    the rollback were a no-op, this CAS would fail with `Err(SENTINEL)`
///    and a real winner — or any future `slot()` caller touching this
///    chunk — would spin forever).
/// 4. It immediately restores the chunk pointer to `null` (the value
///    observed on entry), leaving it exactly as it found it — so, unlike a
///    real OOM (which permanently loses that chunk), this hook's target
///    chunk index remains available for a LATER real `claim()` to
///    materialise normally.
///
/// Returns `Some(true)` if the rollback was proven to clear the sentinel,
/// `Some(false)` if the postcondition CAS unexpectedly failed (rollback is
/// broken — the counterfactual this test is designed to catch), or `None` if
/// the chunk was not observed `UNINIT` (already materialised, or contended)
/// and this check could not run (callers should treat that as "not
/// applicable", never as failure).
#[doc(hidden)]
pub fn dbg_rollback_chunk_sentinel_reenterable(chunk_idx: usize) -> Option<bool> {
    let chunk_ptr = &REGISTRY.chunks[chunk_idx];

    // See the identical construction (and its SAFETY/provenance rationale) in
    // `ensure_chunk_slow` above: a bare marker address, never dereferenced.
    let sentinel = core::ptr::without_provenance_mut::<RegistryChunk>(SENTINEL_INITIALIZING);

    // Step 1: only proceed if the chunk is still UNINIT (null). If it is
    // already real (or contended by another caller), do not touch it.
    chunk_ptr
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .ok()?;

    // Step 2: run the EXACT rollback the production OOM-bailout runs.
    rollback_chunk_sentinel(chunk_ptr);

    // Step 3: prove the anti-livelock postcondition — a fresh CAS(null,
    // SENTINEL) must now succeed, meaning a real materialisation winner (or
    // any future `slot()` caller touching this chunk) would NOT spin forever
    // on a stuck sentinel.
    let postcondition_holds = chunk_ptr
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_ok();

    // Step 4: restore the chunk pointer to null, exactly as observed on
    // entry, regardless of the postcondition outcome.
    chunk_ptr.store(core::ptr::null_mut(), Ordering::Release);

    Some(postcondition_holds)
}

/// Test-only re-export (R6-OPT-P0-2 round 1) of
/// [`registry_chunk::NUM_CHUNKS`](super::registry_chunk::NUM_CHUNKS): the
/// total number of chunks the slot space is split into. Lets a test pick a
/// chunk index guaranteed to be the LAST one (`dbg_num_chunks() - 1`) —
/// unreachable by ordinary `claim()` traffic in a suite that never claims
/// anywhere near `MAX_HEAPS` slots — so it can safely exercise
/// [`dbg_rollback_chunk_sentinel_reenterable`] without any chance of
/// colliding with a chunk another test's `claim()` calls have materialised.
#[doc(hidden)]
#[must_use]
pub const fn dbg_num_chunks() -> usize {
    NUM_CHUNKS
}

/// The current high-water `count` (test introspection). Each test claims
/// fresh slots; because `count` is monotonic across the suite (we never
/// reset the slot array — that would leak the lazily-materialised
/// `HeapCore`s), a test derives its expected slot indices relative to the
/// count it observed at entry.
pub fn count_for_test() -> u32 {
    ensure().count.load(Ordering::Acquire)
}

// ============================================================================
// R6-OPT-P0-2 (round 2): lazy `HeapOverflow` sidecar materialisation.
//
// See the module doc's "R6-OPT-P0-2 (round 2)" section for why this lives
// here (a third instance of the CAS-then-spin-then-publish protocol
// `ensure_chunk`/`ensure_chunk_slow` already establish, applied to ONE ring's
// sidecar pointer instead of a chunk-array slot) and `heap_overflow.rs`'s
// module doc for the two-tier design and the wedge-hazard correctness
// argument this function is the linchpin of.
// ============================================================================

#[cfg(feature = "alloc-xthread")]
mod overflow_sidecar {
    use super::{
        spin_loop, AtomicPtr, HeapOverflowSidecar, Ordering, SIDECAR_CAP,
        SIDECAR_SENTINEL_INITIALIZING,
    };

    /// Byte size of one [`HeapOverflowSidecar`], rounded up to a multiple of
    /// `aligned_vmem::PAGE` — mirrors `registry_chunk::CHUNK_SIZE`'s identical
    /// rounding for the exact same `reserve_aligned` size-contract reason
    /// (`size` must be a non-zero multiple of `PAGE`). Zero-sized only when
    /// `SIDECAR_CAP == 0` (miri: `INLINE_CAP == HEAP_OVERFLOW_CAP`), in which
    /// case [`ensure_overflow_sidecar`] never calls `reserve_aligned` at all
    /// (see that function's `SIDECAR_CAP == 0` fast-return) so this constant
    /// is unused on that path — kept `pub(super)` rather than `#[cfg]`-gated
    /// away entirely so the arithmetic stays visible/auditable in one place
    /// regardless of feature/miri configuration.
    pub(super) const SIDECAR_SIZE: usize = {
        let raw = core::mem::size_of::<HeapOverflowSidecar>();
        if raw == 0 {
            aligned_vmem::PAGE
        } else {
            let page = aligned_vmem::PAGE;
            (raw + page - 1) & !(page - 1)
        }
    };

    /// Ensure `sidecar_ptr`'s [`HeapOverflowSidecar`] is materialised,
    /// returning `true` once it is safe to index (real, non-null,
    /// non-sentinel pointer observed), or `false` if materialisation failed
    /// (OS OOM on the `reserve_aligned` call). Fast path: one `Acquire` load.
    /// Slow path: CAS(null→SENTINEL) race, exactly like
    /// [`super::ensure_chunk_slow`], narrowed to ONE sidecar pointer instead
    /// of a chunk-array slot.
    ///
    /// **The wedge-hazard contract this function exists to uphold:** the
    /// caller (`HeapOverflow::push_impl`) MUST call this BEFORE attempting
    /// its `tail` CAS reservation for any index `>= INLINE_CAP`, and MUST
    /// treat a `false` return as "do not reserve — return false from push",
    /// never advancing `tail`. This function itself does not touch `tail` at
    /// all (it only knows about `sidecar_ptr`); the ordering discipline lives
    /// entirely in the caller, documented there (`HeapOverflow::push`'s doc
    /// comment) and enforced by inspection (this function has no way to
    /// enforce it from its own signature — a `bool` return, matched by every
    /// call site).
    ///
    /// On OOM, unlike [`super::ensure_chunk_slow`]'s registry-chunk OOM
    /// (which had NO existing "try again later" contract at its call site and
    /// had to fall back to `abort()`), `HeapOverflow::push` ALREADY has a
    /// clean, pre-existing failure contract: it returns `bool`, and every
    /// caller already treats `false` as "the ring is momentarily full,
    /// concede to the documented-sound bounded leak" (see
    /// `push_with_overflow_retry`'s existing handling in
    /// `heap_core_xthread.rs`). So this function's OOM branch simply rolls
    /// the sentinel back (the SAME anti-livelock argument
    /// `rollback_chunk_sentinel` documents, narrowed to one sidecar pointer)
    /// and returns `false` — strictly SIMPLER than the chunk path's OOM
    /// handling: no `abort()` needed at all, because the surrounding protocol
    /// already has the right shape.
    pub(crate) fn ensure_overflow_sidecar(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) -> bool {
        // `SIDECAR_CAP == 0` only under miri (`INLINE_CAP == HEAP_OVERFLOW_CAP`
        // there — see `heap_overflow.rs`'s `INLINE_CAP` doc comment). No
        // caller can ever observe `t >= INLINE_CAP` in that configuration (the
        // ring's own full-check already rejects any `t >=
        // HEAP_OVERFLOW_CAP == INLINE_CAP` before this function would be
        // called), so this is unreachable in practice — kept as an explicit,
        // cheap guard rather than relying on that reasoning silently, so a
        // future constant change fails safely (returns "materialisation
        // failed") instead of calling `reserve_aligned(0, ..)`, which
        // `aligned_vmem` already rejects (`size == 0` → `None`) but there is
        // no reason to route through a real syscall attempt for a
        // structurally-empty sidecar.
        if SIDECAR_CAP == 0 {
            return false;
        }

        let p = sidecar_ptr.load(Ordering::Acquire);
        let p_usize = p.addr();
        if p_usize != 0 && p_usize != SIDECAR_SENTINEL_INITIALIZING {
            return true;
        }
        ensure_overflow_sidecar_slow(sidecar_ptr)
    }

    #[cold]
    fn ensure_overflow_sidecar_slow(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) -> bool {
        let sentinel =
            core::ptr::without_provenance_mut::<HeapOverflowSidecar>(SIDECAR_SENTINEL_INITIALIZING);
        match sidecar_ptr.compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            // Acquire on success: pairs with our later Release store of the
            // real pointer, establishing happens-before for future Acquire
            // readers — identical to `ensure_chunk_slow`'s CAS.
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                // ── Winner branch ──────────────────────────────────────────
                // M5 (reentrancy-free) proof: identical to `ensure_chunk_slow`
                // — `aligned_vmem::reserve_aligned` is a direct OS syscall, no
                // `std::alloc`/`Box`/`Vec`, no transitive dependency on
                // `sefer_alloc::registry::*`. Under miri it falls back to
                // `std::alloc`, but under miri we are NOT the global
                // allocator, so no reentrancy. (In practice `SIDECAR_CAP == 0`
                // under miri means this branch is unreachable there — see the
                // guard in `ensure_overflow_sidecar` above — but the proof
                // holds regardless.)
                // CRATE-P2 (item g): reserve + zero-under-miri + leak folded
                // into `aligned_vmem::leak_zeroed_pages` (the miri `write_bytes`
                // that used to sit below is now inside that helper). `base`
                // points at a fully-zeroed `SIDECAR_SIZE` span leaked for the
                // process lifetime; `SIDECAR_ALIGN` (PAGE) is subsumed because
                // `leak_zeroed_pages` always reserves PAGE-aligned (>=
                // `align_of::<HeapOverflowSidecar>()`).
                let base = match aligned_vmem::leak_zeroed_pages(SIDECAR_SIZE) {
                    Some(p) => p.as_ptr() as *mut HeapOverflowSidecar,
                    None => {
                        // OOM: roll the sentinel back (anti-livelock — see
                        // `rollback_chunk_sentinel`'s identical argument,
                        // narrowed to this one sidecar pointer) and report
                        // failure. Unlike the chunk path, NO abort: `push`'s
                        // pre-existing `bool` contract already has a sound
                        // "ring momentarily unavailable" outcome for this —
                        // see this function's own doc comment.
                        rollback_overflow_sidecar_sentinel(sidecar_ptr);
                        return false;
                    }
                };

                // In-place initialisation: no field writes needed at all —
                // OS-zeroed pages already form a fully valid
                // `HeapOverflowSidecar` (every `AtomicUsize`/`AtomicU32` at
                // its zero/`ENTRY_EMPTY_BASE` initial value), identical
                // reasoning to `ensure_chunk_slow`'s "nothing to write" case.
                //
                // SAFETY: `base` is non-null, aligned to `SIDECAR_ALIGN`
                // (PAGE, well above `align_of::<HeapOverflowSidecar>()`), and
                // valid for `SIDECAR_SIZE` bytes (>=
                // `size_of::<HeapOverflowSidecar>()`). We are the sole writer
                // (only one CAS winner can reach this branch). The memory is
                // OS-provided zero-initialised pages, already a fully valid
                // `HeapOverflowSidecar` bit-pattern.

                // Publish with Release so every subsequent Acquire load
                // (`HeapOverflow::slot`, `dbg_sidecar_is_materialised`, the
                // fast path above) sees the fully written sidecar.
                sidecar_ptr.store(base, Ordering::Release);

                // The reservation was already leaked for the process lifetime by
                // `leak_zeroed_pages` — same discipline as every other
                // lazy-materialisation site in this crate.

                true
            }
            Err(_) => {
                // ── Loser branch ───────────────────────────────────────────
                // Spin until a real (non-null, non-sentinel) pointer is
                // observed — identical shape to `ensure_chunk_slow`'s loser
                // branch, narrowed to one sidecar pointer. The window is
                // small: one OS reservation of `SIDECAR_SIZE` bytes plus one
                // publish store.
                loop {
                    let p = sidecar_ptr.load(Ordering::Acquire);
                    let p_usize = p.addr();
                    if p_usize != 0 && p_usize != SIDECAR_SENTINEL_INITIALIZING {
                        return true;
                    }
                    if p_usize == 0 {
                        // R6-OPT-P0-2 (round 2): the winner hit OOM and rolled
                        // the sentinel back to null. There is no live winner
                        // left to wait on — spinning further would wait
                        // forever (no one will ever publish a real pointer
                        // until a NEW `ensure_overflow_sidecar` call wins a
                        // fresh CAS). Report failure to THIS caller; it is
                        // free to retry (a later `push` call re-enters
                        // `ensure_overflow_sidecar`'s fast path, observes
                        // null again, and may itself win the CAS).
                        return false;
                    }
                    spin_loop();
                }
            }
        }
    }

    /// Roll `sidecar_ptr` back from `SENTINEL_INITIALIZING` to `null` —
    /// mirrors [`super::rollback_chunk_sentinel`] exactly (same anti-livelock
    /// argument, narrowed to one sidecar pointer instead of a chunk slot).
    /// Kept as its own function so the test-only hook below exercises EXACTLY
    /// the same code the production OOM-bailout runs.
    #[cold]
    fn rollback_overflow_sidecar_sentinel(sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>) {
        sidecar_ptr.store(core::ptr::null_mut(), Ordering::Release);
    }

    /// Test-only hook, generalising [`super::dbg_rollback_chunk_sentinel_reenterable`]
    /// to the sidecar pointer: proves [`rollback_overflow_sidecar_sentinel`]
    /// actually clears the sentinel on a REAL `AtomicPtr<HeapOverflowSidecar>`,
    /// without invoking any process-terminating path (there is none on this
    /// side — see `ensure_overflow_sidecar_slow`'s OOM branch, which already
    /// returns `false` rather than aborting). Operates on a caller-supplied
    /// standalone pointer (not a live registry slot's sidecar) since
    /// `HeapOverflow` instances used in tests are typically standalone
    /// (`new_boxed_for_test`), not registry-resident — unlike the chunk
    /// hook, there is no shared process-global sidecar to accidentally
    /// disturb, so this hook does not need the "only touch it if UNINIT"
    /// guard the chunk hook needs (a test-owned standalone `HeapOverflow` is
    /// never contended by another test).
    ///
    /// `pub(crate)` (not `pub`): `HeapOverflowSidecar` is `pub(crate)`, so a
    /// `pub fn` taking `&AtomicPtr<HeapOverflowSidecar>` would leak a
    /// private type into a public signature. The actual test-facing surface
    /// is [`HeapOverflow::dbg_rollback_sidecar_sentinel_for_test`], a thin
    /// `#[doc(hidden)] pub` forwarder on `HeapOverflow` itself (mirroring
    /// `dbg_reserve_unpublished_for_test`'s existing "test hook lives on the
    /// type, not on a raw field" discipline) that calls this function with
    /// its own private `sidecar` field.
    pub(crate) fn dbg_rollback_overflow_sidecar_sentinel_reenterable(
        sidecar_ptr: &AtomicPtr<HeapOverflowSidecar>,
    ) -> bool {
        let sentinel =
            core::ptr::without_provenance_mut::<HeapOverflowSidecar>(SIDECAR_SENTINEL_INITIALIZING);

        // Step 1: CAS-acquire the sentinel (as if we were the real
        // materialisation winner that hit OOM). Caller guarantees the pointer
        // starts null (standalone test-owned ring, never contended).
        sidecar_ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .expect("dbg_rollback_overflow_sidecar_sentinel_reenterable: pointer must start null");

        // Step 2: run the EXACT rollback the production OOM-bailout runs.
        rollback_overflow_sidecar_sentinel(sidecar_ptr);

        // Step 3: prove the anti-livelock postcondition — a fresh CAS(null,
        // SENTINEL) must now succeed.
        let postcondition_holds = sidecar_ptr
            .compare_exchange(
                core::ptr::null_mut(),
                sentinel,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();

        // Step 4: restore to null, exactly as observed on entry.
        sidecar_ptr.store(core::ptr::null_mut(), Ordering::Release);

        postcondition_holds
    }
}

#[cfg(feature = "alloc-xthread")]
pub(crate) use overflow_sidecar::dbg_rollback_overflow_sidecar_sentinel_reenterable;
#[cfg(feature = "alloc-xthread")]
pub(crate) use overflow_sidecar::ensure_overflow_sidecar;

/// Dereference a materialised sidecar pointer as `&'static HeapOverflowSidecar`.
/// The ONE place in the crate allowed to do so (mirrors [`Registry::slot`]'s
/// equivalent role for chunk memory) — `heap_overflow.rs` has no unsafe seam
/// of its own (see its module doc), so its `HeapOverflow::slot` resolver
/// calls this safe membrane function instead of dereferencing `p` itself.
///
/// # Panics (debug only)
///
/// The caller (`HeapOverflow::slot`) already `debug_assert`s `p` is non-null
/// and non-sentinel before calling this; this function trusts that contract
/// (a `debug_assert` here would be redundant with the caller's).
#[cfg(feature = "alloc-xthread")]
pub(crate) fn deref_overflow_sidecar(p: *mut HeapOverflowSidecar) -> &'static HeapOverflowSidecar {
    // SAFETY: `p` is a non-null, non-sentinel pointer the caller obtained
    // from an `Acquire` load of a `sidecar` field after `HeapOverflow::
    // push_impl` already called `ensure_overflow_sidecar` (which returned
    // `true`) for this same index, OR (on the drain side) an index a producer
    // already proved reachable by successfully publishing into it — in both
    // cases some earlier `ensure_overflow_sidecar_slow` winner published this
    // pointer with `Release` after fully constructing the `HeapOverflowSidecar`
    // (OS-zeroed pages are already a fully valid state — see that function's
    // in-place-init comment), and this Acquire load observes it, establishing
    // happens-before. The allocation is leaked (`mem::forget`) and outlives
    // any reference derived from it (process lifetime), exactly like a
    // `RegistryChunk`.
    unsafe { &*p }
}
