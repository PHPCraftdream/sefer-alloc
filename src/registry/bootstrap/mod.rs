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
//! The fix: split the slot array into `chunk::NUM_CHUNKS` chunks of
//! `chunk::CHUNK_SLOTS` slots each (`RegistryChunk`), and
//! materialise each chunk LAZILY, on first touch of an index that falls
//! inside it — mirroring the SAME CAS-then-spin publish protocol the old
//! whole-registry `ensure`/`ensure_slow` used, just applied per-chunk. See
//! `Registry::slot` for the resolver (the single place in the crate allowed
//! to dereference chunk memory) and `ensure_chunk_slow` for the
//! materialisation protocol.
//!
//! **`Registry` itself is now small enough to be a plain `static` again**:
//! once the giant inline array is gone, `Registry` is just
//! `chunks: [OncePtrCell<RegistryChunk>; NUM_CHUNKS]` plus claim/scan/hint
//! control words — all const-initialisable. `ensure()` is a plain `&'static Registry`
//! return with NO CAS, NO sentinel dance, and NO OOM-abort path at the
//! REGISTRY level at all (OOM can now only happen at PER-CHUNK
//! materialisation time — see `ensure_chunk_slow`'s OOM handling, which is
//! strictly better than the old whole-registry abort: a process that already
//! has heaps live in other chunks keeps working even if one chunk's
//! reservation fails).
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
//! (none, currently — see `ensure_chunk_slow`); `next_free` is written
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
//! Each `AtomicPtr<RegistryChunk>` in `Registry::chunks` independently
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
//! 1. The first `slot_or_none()` or `slot()` call touching this chunk observes `null`
//!    and CASes it to `SENTINEL_INITIALIZING`. The CAS winner:
//!    a. Calls `aligned_vmem::reserve_aligned(CHUNK_SIZE, CHUNK_ALIGN)` —
//!       direct OS syscall, no `std::alloc`, no registry dependency.
//!    b. Field-by-field in-place initialisation (OS zeroed-pages; every field
//!       already starts at its correct zero value — see `ensure_chunk_slow`).
//!    c. `self.chunks[chunk_idx].store(base, Release)` — publishes the ready
//!       pointer.
//!    d. `mem::forget(reservation)` — leaks the reservation intentionally;
//!       the chunk lives for the process lifetime.
//! 2. Concurrent losers observe `SENTINEL_INITIALIZING` (or `null`, then fail
//!    the CAS) and spin until they observe a non-null, non-sentinel pointer
//!    under `Acquire`. The spin window is tiny (one OS page allocation of
//!    `CHUNK_SIZE` bytes, far smaller than the old whole-registry window).
//! 3. After `READY`, every subsequent slot lookup touching this chunk is a
//!    single `Acquire` load + two cheap comparisons + an array index.
//!
//! `Release`/`Acquire` on the pointer transition establishes happens-before
//! from the initialising thread's `ptr::write`s (the chunk's slot fields) to
//! every reader that observes the real pointer, so readers see a fully
//! constructed chunk.
//!
//! ## M5 (reentrancy-free) — CANNOT BE VIOLATED
//!
//! `aligned_vmem::reserve_aligned` uses direct OS syscalls (`VirtualAlloc` /
//! `mmap`) natively and direct `System.alloc` under Miri — it does NOT call
//! the installed global allocator, `Box`, or `Vec`. Its dependency graph
//! (verified by reading `crates/aligned-vmem/src/lib.rs` in full):
//!
//! - Windows: `extern "system" { fn VirtualAlloc(...) }` — no std alloc.
//! - Unix: `extern "C" { fn mmap(...) }` — no std alloc.
//! - Miri: direct `System.alloc`, bypassing the installed global allocator.
//!
//! No path from `ensure_chunk_slow` touches `sefer_alloc::registry::*` —
//! confirmed by inspection (unchanged from the pre-chunking `ensure_slow`).
//! The reservation call chain reaches a kernel syscall on native targets or
//! `System` directly under Miri.
//!
//! ## Provenance model (task #140)
//!
//! The chunk-pointer sentinel handling now lives inside
//! [`once_ptr_cell::OncePtrCell`] (CRATE-P3 extraction), which uses the SAME
//! `without_provenance_mut` idiom the old whole-registry `ensure_slow` used (a
//! bare marker address, never dereferenced — only compared), so it stays
//! strict-provenance-clean under `-Zmiri-strict-provenance`. This file's own
//! remaining raw-pointer work is casting the leaked reservation to
//! `*mut RegistryChunk` and dereferencing the published pointer.

// R1-07 (src review round 1): this file is `mod.rs` — decls and
// path-preserving re-exports only, per the "mod.rs — reexports only, no
// code" rule — so it carries NO `#![allow(unsafe_code)]` of its own; a
// blanket allow here used to silently widen to every descendant, including
// `loom_shim` (a `#[cfg(loom)]`-only module that was invisible to both this
// crate's README/`src/lib.rs` unsafe inventory and this file's own
// seam-narrative comment, until R1-07 fixed both). Each child file that
// actually contains `unsafe` now carries its OWN tier-1 `#![allow(unsafe_code)]`
// with its own `// SAFETY:` proof at every block:
//  - [`registry`] — casts the leaked `aligned_vmem::leak_zeroed_pages`
//    reservation to `*mut RegistryChunk` and dereferences the pointer the
//    cell publishes (`p.as_ref()`), sound because the cell's `Release`
//    publish establishes happens-before.
//  - [`ensure`] — the per-chunk `ensure_chunk`/`ensure_chunk_slow` CAS/
//    reserve/publish/spin protocol that drives `registry`'s dereference
//    above (the state-machine ITSELF is `once_ptr_cell::OncePtrCell`,
//    CRATE-P3-extracted; this file's own `unsafe` is the pointer cast/deref
//    around calling it).
//  - [`loom_shim`] (`--cfg loom` only) — `unsafe impl Send`/`Sync` for its
//    const-capable `OncePtrCell` stand-in, plus `NonNull::new_unchecked`
//    calls proved by the sentinel-address check immediately above each site.
// [`chunk`] has no `unsafe` of its own and carries no allow — its layout
// constants are computed with safe `core::mem::size_of`/`align_of`.

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
// - [`loom_shim`] — the `#[cfg(loom)]` const-capable `OncePtrCell` stand-in
//   needed by the static initializer. Its separate local stack mirror has
//   no current source caller; root stack models use the member crate directly.
mod chunk;
mod ensure;
#[cfg(loom)]
pub(crate) mod loom_shim;
mod registry;
pub(crate) mod saturation;

// Re-exports preserving the flat file's item paths (`bootstrap::X` — consumed
// by registry heaps and integration tests):
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
pub use registry::{Registry, MAX_HEAPS};
