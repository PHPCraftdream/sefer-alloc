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
//! The chunk-pointer sentinel handling now lives inside
//! [`racy_ptr_cell::RacyPtrCell`] (CRATE-P3 extraction), which uses the SAME
//! `without_provenance_mut` idiom the old whole-registry `ensure_slow` used (a
//! bare marker address, never dereferenced — only compared), so it stays
//! strict-provenance-clean under `-Zmiri-strict-provenance`. This file's own
//! remaining raw-pointer work is (1) casting the leaked `leak_zeroed_pages`
//! reservation to `*mut RegistryChunk` and dereferencing the published pointer
//! the cell hands back, and (2) the `alloc-xthread` overflow-sidecar path below
//! (still spelled out inline — see the CRATE-P3 note in [`ensure_chunk_slow`]).
//! The A1 deferred-large-free stack's exposed-provenance story
//! (`alloc_core::deferred_large`) is untouched by this round — see that
//! module for its own provenance documentation.

// This file uses `unsafe` for these operations. The CAS-reserve / sentinel /
// Release-publish / spin-while-INITIALIZING / OOM-rollback STATE MACHINE that
// drove the per-chunk pointer transition inline used to live here; CRATE-P3
// extracted it into `racy_ptr_cell::RacyPtrCell` (aliasing its atomics to
// `loom` so the shipped loom suite exercises the real type). What remains here:
//  1. Casting the leaked `aligned_vmem::leak_zeroed_pages` reservation to
//     `*mut RegistryChunk` and dereferencing the pointer the cell publishes
//     (`p.as_ref()` in `ensure_chunk`/`ensure_chunk_slow`) after the cell
//     observed it under `Acquire` — sound because the cell's `Release` publish
//     establishes happens-before (OS-zeroed pages are already a valid
//     `RegistryChunk`).
//  2. The `alloc-xthread` overflow-sidecar path (still an inline instance of
//     the same protocol — see the CRATE-P3 note in `ensure_chunk_slow` for why
//     that one did NOT migrate onto `RacyPtrCell`): its own CAS/reserve/publish/
//     spin and `unsafe { &*p }` deref, each with its own `// SAFETY:` proof.
// Every `unsafe` block carries a `// SAFETY:` proof below.
#![allow(unsafe_code)]

// `spin_loop` and `AtomicPtr` are used ONLY by the `alloc-xthread`
// overflow-sidecar module below (the chunk path now goes through
// `RacyPtrCell`, which owns its own spin + atomic internally), so gate their
// imports on that feature to stay warning-clean on the non-xthread builds.
#[cfg(feature = "alloc-xthread")]
use core::hint::spin_loop;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// The extracted lazy CAS-published pointer cell (CRATE-P3). Under a NORMAL or
// `production` build sefer uses the real crate type. Under `--cfg loom`, this
// file is compiled to link sefer's OWN shadow-model loom harnesses (e.g.
// `loom_free_slots_aba`) — which model UNRELATED protocols and never touch the
// chunk cells; but `RUSTFLAGS=--cfg loom` is global, so the real crate would
// then be built in its loom-aliased mode, where `RacyPtrCell::new` is NOT
// `const` (loom's `AtomicPtr::new` has no const constructor) and sefer's
// `static REGISTRY: Registry = Registry::new()` would fail to const-evaluate.
// Sefer already keeps its OWN production atomics on `core::sync::atomic` under
// loom (see the imports above) for exactly this reason. So under loom we swap in
// a const-capable, core-atomic shim with the identical API surface `bootstrap`
// uses. This is sound: the real-type loom VERIFICATION of `RacyPtrCell` lives in
// the crate's OWN suite (`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`, run
// via `-p racy-ptr-cell`), which is the whole point of the extraction; sefer's
// loom harnesses never exercise these chunk cells, so the shim is never on any
// modeled interleaving — it exists only to keep the const static compiling.
#[cfg(loom)]
use loom_shim::RacyPtrCell;
#[cfg(not(loom))]
use racy_ptr_cell::RacyPtrCell;

#[cfg(loom)]
mod loom_shim {
    //! Const-capable, `core::sync::atomic`-backed stand-in for
    //! `racy_ptr_cell::RacyPtrCell`, used ONLY under `--cfg loom` so sefer's own
    //! (unrelated) shadow-model loom harnesses can link the crate with its const
    //! `REGISTRY` static intact — see the import-site comment above. Mirrors the
    //! real cell's API surface `bootstrap.rs` calls; behaviourally faithful (same
    //! CAS/Release-publish/spin/rollback), but built on `core` atomics so `new`
    //! stays `const`. It is NEVER on a loom-modeled interleaving (sefer's loom
    //! tests do not touch chunk cells), so it needs no loom atomics.
    use core::marker::PhantomData;
    use core::ptr::NonNull;
    use core::sync::atomic::{AtomicPtr, Ordering};

    const SENTINEL_INITIALIZING: usize = 1;

    pub(crate) struct RacyPtrCell<T> {
        ptr: AtomicPtr<T>,
        _marker: PhantomData<*mut T>,
    }

    // SAFETY: mirrors the real cell — only a raw `*mut T` ever crosses threads.
    unsafe impl<T> Send for RacyPtrCell<T> {}
    // SAFETY: see the `Send` impl.
    unsafe impl<T> Sync for RacyPtrCell<T> {}

    impl<T> RacyPtrCell<T> {
        pub(crate) const fn new() -> Self {
            RacyPtrCell {
                ptr: AtomicPtr::new(core::ptr::null_mut()),
                _marker: PhantomData,
            }
        }

        fn is_ready(p: *mut T) -> bool {
            let a = p.addr();
            a != 0 && a != SENTINEL_INITIALIZING
        }

        pub(crate) fn get(&self) -> Option<NonNull<T>> {
            let p = self.ptr.load(Ordering::Acquire);
            if Self::is_ready(p) {
                Some(unsafe { NonNull::new_unchecked(p) })
            } else {
                None
            }
        }

        pub(crate) fn get_or_try_init<F>(&self, mut init: F) -> Option<NonNull<T>>
        where
            F: FnMut() -> Option<NonNull<T>>,
        {
            let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
            loop {
                let p = self.ptr.load(Ordering::Acquire);
                if Self::is_ready(p) {
                    return Some(unsafe { NonNull::new_unchecked(p) });
                }
                match self.ptr.compare_exchange(
                    core::ptr::null_mut(),
                    sentinel,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => match init() {
                        Some(ptr) => {
                            self.ptr.store(ptr.as_ptr(), Ordering::Release);
                            return Some(ptr);
                        }
                        None => {
                            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
                            return None;
                        }
                    },
                    Err(_) => loop {
                        let p = self.ptr.load(Ordering::Acquire);
                        let a = p.addr();
                        if a == SENTINEL_INITIALIZING {
                            core::hint::spin_loop();
                            continue;
                        }
                        if a != 0 {
                            return Some(unsafe { NonNull::new_unchecked(p) });
                        }
                        break;
                    },
                }
            }
        }

        pub(crate) fn dbg_is_ready(&self) -> bool {
            Self::is_ready(self.ptr.load(Ordering::Acquire))
        }

        pub(crate) fn dbg_rollback_reenterable(&self) -> Option<bool> {
            let sentinel = core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING);
            self.ptr
                .compare_exchange(
                    core::ptr::null_mut(),
                    sentinel,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .ok()?;
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
            let held = self
                .ptr
                .compare_exchange(
                    core::ptr::null_mut(),
                    sentinel,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok();
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
            Some(held)
        }
    }
}

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
    /// One lazy CAS-published pointer cell per chunk of the slot space
    /// ([`racy_ptr_cell::RacyPtrCell`], the extracted `UNINIT -> INITIALIZING
    /// -> READY` state machine — see [`Registry::ensure_chunk`] /
    /// [`ensure_chunk_slow`]). `RacyPtrCell` internally drives the same
    /// `null -> sentinel(1) -> real *mut RegistryChunk` transition this field
    /// used to spell out inline, with the identical Release-publish +
    /// spin-`Acquire`-while-INITIALIZING + OOM-rollback-then-re-race discipline;
    /// the seam here only reserves the OS pages and dereferences the published
    /// pointer.
    chunks: [RacyPtrCell<RegistryChunk>; NUM_CHUNKS],
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
            chunks: [const { RacyPtrCell::new() }; NUM_CHUNKS],
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
    /// RegistryChunk` reference to it. Fast path: [`RacyPtrCell::get`] (one
    /// `Acquire` load + non-null/non-sentinel check, inside the cell). Slow
    /// path (first touch or race): [`ensure_chunk_slow`], which drives the
    /// cell's `get_or_try_init` (CAS-reserve, OS reservation, Release-publish,
    /// spin-while-INITIALIZING loser, OOM rollback).
    #[inline]
    fn ensure_chunk(&self, chunk_idx: usize) -> &'static RegistryChunk {
        if let Some(p) = self.chunks[chunk_idx].get() {
            // SAFETY: `RacyPtrCell::get` returned `Some` only after observing
            // a real (non-null, non-sentinel) pointer under `Acquire`. The
            // initialising thread published it with `Release` AFTER the OS
            // reservation's pages were fully valid (OS-zeroed pages already
            // form a valid `RegistryChunk` — see `ensure_chunk_slow`), so this
            // Acquire-observed pointer sees all those bytes. The reservation is
            // leaked (`leak_zeroed_pages`) and lives for the process lifetime,
            // so `&'static` is sound.
            return unsafe { p.as_ref() };
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

/// Slow path for [`Registry::ensure_chunk`]: drive the chunk's
/// [`racy_ptr_cell::RacyPtrCell`] through its `get_or_try_init` — the CAS-reserve
/// / OS-reserve / Release-publish / spin-while-INITIALIZING-loser / OOM-rollback
/// protocol now lives INSIDE the cell (the extracted `UNINIT -> INITIALIZING ->
/// READY` state machine). This function supplies only the two things the cell
/// leaves to the caller: the winner's fallible OS reservation closure, and the
/// process-abort OOM policy this specific call site keeps.
///
/// The cell guarantees exactly-once init, a single published pointer for all
/// racers, Release/Acquire happens-before, and — critically for M5 — that a
/// loser observing the OOM rollback (sentinel back to null) re-races the CAS
/// rather than spinning forever on a READY that will never come.
///
/// ## CRATE-P3 — why the overflow-sidecar path below did NOT also migrate
///
/// The chunk site maps cleanly onto `RacyPtrCell<RegistryChunk>`: it wants a
/// `&'static RegistryChunk`, aborts on OOM, and `RegistryChunk` is never a ZST.
/// The `alloc-xthread` overflow-sidecar path (`ensure_overflow_sidecar` below)
/// deliberately stays spelled out inline because it does NOT fit the generic
/// cell's shape without weakening it: (a) it returns a `bool`
/// materialised-or-not and the DEREF happens separately in
/// `deref_overflow_sidecar` (a different membrane split than the chunk's
/// `&'static`-returning resolver); (b) its OOM contract is "return `false`, let
/// the caller's existing bounded-leak path retry LATER" — a loser that observes
/// the rollback returns `false` immediately rather than re-racing within the
/// same call, the opposite of the cell's re-race-now liveness; and (c) under
/// miri `SIDECAR_CAP == 0` makes `HeapOverflowSidecar` a ZST (align 1), which
/// would trip `RacyPtrCell`'s `align_of >= 2` sentinel-collision guard at
/// `const` construction. Forcing it would risk the M5-critical wedge-hazard
/// ordering for no real dedup gain, so it is left as an honest inline second
/// instance — the shared protocol it relies on is still proved by the crate's
/// real-type loom suite.
#[cold]
fn ensure_chunk_slow(chunk_cell: &RacyPtrCell<RegistryChunk>) -> &'static RegistryChunk {
    let published = chunk_cell.get_or_try_init(|| {
        // ── Winner init closure ───────────────────────────────────────────
        // We hold the cell's INITIALIZING sentinel; we are the SOLE
        // initialiser of THIS chunk. Allocate it from OS VM.
        //
        // M5 (reentrancy-free) proof: unchanged from the pre-extraction inline
        // winner branch — `aligned_vmem::leak_zeroed_pages` is a direct OS
        // syscall (reserve + zero-under-miri + `mem::forget`-leak), no
        // `std::alloc`/`Box`/`Vec`, no transitive dependency on
        // `sefer_alloc::registry::*`. Under miri it falls back to `std::alloc`,
        // but under miri we are NOT the global allocator, so no reentrancy.
        // The whole `CHUNK_SIZE` span is guaranteed zeroed on every backend, so
        // `base` points at a fully valid all-zero `RegistryChunk`:
        //   next_free   = 0 (NOT NEXT_FREE_TAIL — lazy init, RAD-1)
        //   state       = 0 = STATE_FREE
        //   generation  = 0
        //   heap        = MaybeUninit::uninit() (zero is fine)
        //   initialised = 0 = false
        //   remote.*    = 0 / null
        //   overflow    = all-zero `HeapOverflow`
        // — genuinely nothing to write; OS-zeroed pages ARE a valid state. The
        // reservation is PAGE-aligned (>= `align_of::<RegistryChunk>()` <= 64)
        // and leaked for the process lifetime, so the `&'static` references
        // `heap_registry::bind_slot_counters` plants into slot fields stay
        // valid forever.
        //
        // Returning `None` on OS refusal makes the cell roll its sentinel back
        // to null (anti-livelock — losers re-race) BEFORE we get control back
        // to run the abort policy below.
        let p = aligned_vmem::leak_zeroed_pages(CHUNK_SIZE)?;
        Some(p.cast::<RegistryChunk>())
    });

    match published {
        Some(p) => {
            // SAFETY: `RacyPtrCell::get_or_try_init` returned `Some` only after
            // the winner published a real (non-null, non-sentinel) pointer with
            // `Release` and every other racer observed it under `Acquire`. The
            // pointee is the fully-zeroed `RegistryChunk` reserved above (a
            // valid state), leaked for the process lifetime, so `&'static` is
            // sound.
            unsafe { p.as_ref() }
        }
        None => {
            // Chunk-materialisation OOM. The cell has ALREADY rolled its
            // sentinel back to null (so no loser is wedged and a future
            // `slot()` call can retry this chunk index) — that is the
            // anti-livelock guarantee the inline code used to spell out via
            // `rollback_chunk_sentinel`, now internal to `RacyPtrCell`.
            //
            // We keep the historic ABORT policy for THIS call site (unchanged
            // in EFFECT from before the extraction): `Registry::slot` /
            // `pick_slot` / `claim` assume `slot()` always succeeds (`&'static
            // HeapSlot`, not `Option<..>`), so a chunk that cannot be
            // materialised has no non-fatal propagation path yet. A
            // chunk-materialisation OOM is exceedingly rare (the OS refusing a
            // tens-of-KiB-to-low-MiB reservation while the process is already
            // so starved that the `HeapCore::new()` segment reservation a few
            // lines later would fail anyway); widening `slot()` to `Option` is
            // deliberately out of scope. The cell's rollback establishes the
            // mechanism a future `Option`-returning `slot()` could build on.
            std::process::abort();
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
    // Forward to `RacyPtrCell::dbg_rollback_reenterable`, which drives the
    // chunk's REAL cell through the EXACT `null -> sentinel -> rollback ->
    // re-CAS` sequence the internal OOM-bailout runs and proves the
    // anti-livelock postcondition (after rollback, a fresh CAS(null -> sentinel)
    // succeeds — no future materialisation winner or spinning loser is wedged).
    // The rollback logic now lives inside `RacyPtrCell` (the extracted state
    // machine), so this hook exercises the shipped code path, not a copy — and
    // on the LIVE process-global registry chunk, exactly as before the
    // extraction. The cell's own entry CAS is the "only touch it if UNINIT"
    // guard (returns `None` on a materialised/contended chunk), so a
    // caller-chosen high chunk index no other test claims stays safe.
    REGISTRY.chunks[chunk_idx].dbg_rollback_reenterable()
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
