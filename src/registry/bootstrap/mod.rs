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
//! touch" for a single reservation of this shape (see `crates/aligned-vmem/src/lib.rs`).
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
//! (`heap_overflow.rs`), a `[AtomicPtr<u8>; HEAP_OVERFLOW_CAP] +
//! [AtomicU32; HEAP_OVERFLOW_CAP]` pair inline in EVERY `HeapSlot`
//! (`HEAP_OVERFLOW_CAP = 2048` native), 24 KiB/slot. Round 2 shrinks this by
//! splitting `HeapOverflow`'s storage into a small always-inline "emergency"
//! tier (`INLINE_CAP` entries) plus a lazily-materialised sidecar for the
//! rest — see `heap_overflow.rs`'s module doc for the full two-tier design
//! and the wedge-hazard correctness argument.
//!
//! **Unsafe-seam placement decision:** the sidecar's materialisation
//! machinery ([`ensure_overflow_sidecar`] / [`deref_overflow_sidecar`]) lives
//! HERE, in `bootstrap`'s EXISTING `#![allow(unsafe_code)]` seam, rather
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
//! raw pointers" finds the answer unchanged (`bootstrap`, still the only
//! one in `registry/`); (3) `heap_overflow.rs`'s `push`/`drain` need only a
//! SAFE `&HeapOverflowSidecar` once materialised — [`deref_overflow_sidecar`]
//! is the one safe membrane function that hands that out, exactly mirroring
//! how [`Registry::slot`] hands out a safe `&'static HeapSlot` from chunk
//! memory. This mirrors round 1's own choice (`registry_chunk.rs` stays
//! unsafe-free; all raw-pointer work lives in `bootstrap`) — the SAME
//! reasoning applied one level further down. Because `bootstrap` is
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
//! `crates/aligned-vmem/src/lib.rs` in full):
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
//! [`once_ptr_cell::OncePtrCell`] (CRATE-P3 extraction), which uses the SAME
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
// extracted it into `once_ptr_cell::OncePtrCell` (aliasing its atomics to
// `loom` so the shipped loom suite exercises the real type). What remains here:
//  1. Casting the leaked `aligned_vmem::leak_zeroed_pages` reservation to
//     `*mut RegistryChunk` and dereferencing the pointer the cell publishes
//     (`p.as_ref()` in `ensure_chunk`/`ensure_chunk_slow`) after the cell
//     observed it under `Acquire` — sound because the cell's `Release` publish
//     establishes happens-before (OS-zeroed pages are already a valid
//     `RegistryChunk`).
//  2. The `alloc-xthread` overflow-sidecar path (still an inline instance of
//     the same protocol — see the CRATE-P3 note in `ensure_chunk_slow` for why
//     that one did NOT migrate onto `OncePtrCell`): its own CAS/reserve/publish/
//     spin and `unsafe { &*p }` deref, each with its own `// SAFETY:` proof.
// Every `unsafe` block carries a `// SAFETY:` proof below.
#![allow(unsafe_code)]

// Structural reorg step 5: the flat `bootstrap.rs` became this directory.
// `mod.rs` is decls + path-preserving re-exports only (per the
// one-export-per-file rule); the code lives in the child files:
//
// - [`registry`] — the `Registry` struct, `MAX_HEAPS`, `static REGISTRY`, and
//   its slot-resolution / dbg accessor methods.
// - [`ensure`] — the process-global `ensure()` accessor, the per-chunk
//   materialisation slow path, and the test-only dbg hooks.
// - [`chunk`] — `RegistryChunk` (the former `registry_chunk.rs`, moved
//   verbatim; the module path becomes `bootstrap::chunk`).
// - [`loom_shim`] — the `#[cfg(loom)]` const-capable atomics shim (formerly
//   an inner module; the `bootstrap::loom_shim` path is preserved for
//   `heap_registry`'s cfg-gated `StackStorage` impl).
// - [`overflow_sidecar`] — the `alloc-xthread` lazy `HeapOverflow` sidecar
//   materialisation (formerly an inner module; the `bootstrap::overflow_sidecar`
//   path is preserved).
mod chunk;
mod ensure;
#[cfg(loom)]
pub(crate) mod loom_shim;
#[cfg(feature = "alloc-xthread")]
mod overflow_sidecar;
mod registry;

// Re-exports preserving the flat file's item paths (`bootstrap::X` — consumed
// by `heap_registry`, `heap_overflow`, `heap_core_xthread`, and the
// integration tests):
pub use ensure::count_for_test;
pub use ensure::dbg_num_chunks;
pub use ensure::dbg_rollback_chunk_sentinel_reenterable;
#[cfg(feature = "internals")]
pub use ensure::dbg_set_inject_chunk_oom;
#[cfg(feature = "internals")]
pub use ensure::dbg_slot_or_none;
pub use ensure::ensure;
/// Re-exported so a consumer of [`dbg_rollback_chunk_sentinel_reenterable`]
/// can match on its result without depending on `once-ptr-cell` directly
/// (it is an OPTIONAL dependency of this crate). The probe's result type is
/// pure data with no atomics, so it is loom-agnostic and the same enum
/// serves both the real cell and the `#[cfg(loom)]` shim.
pub use once_ptr_cell::RollbackProbe;
#[cfg(feature = "alloc-xthread")]
pub(crate) use overflow_sidecar::{
    dbg_rollback_overflow_sidecar_sentinel_reenterable, deref_overflow_sidecar,
    ensure_overflow_sidecar,
};
pub use registry::{Registry, MAX_HEAPS};
