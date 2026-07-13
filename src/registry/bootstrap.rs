//! [`Registry`] — the bootstrap outcome: a process-global, self-hosted slot
//! array plus its dynamic atomics, initialised exactly once via a hand-rolled
//! atomic state-machine (NOT `std::sync::Once`, which may allocate).
//!
//! ## The discipline (reused from `bootstrap::primordial`)
//!
//! The Phase 8 primordial bootstrap (`alloc_core::bootstrap::primordial`)
//! hand-carves the `SegmentTable` from a freshly-reserved segment: one OS
//! reservation, then safe composition over its bytes via the `node` seam. The
//! registry reuses that discipline's SHAPE — allocation-free init guarded by
//! an atomic state-machine — but NOW stores the slot array in a
//! HEAP-ALLOCATED reservation obtained via `aligned_vmem::reserve_aligned`
//! (a direct OS syscall, NOT `std::alloc`) rather than a `static`.
//!
//! ## Why lazy heap-allocation instead of a `static`?
//!
//! The original design used `static REGISTRY: Registry = Registry::new_zeroed()`.
//! `HeapSlot::new_uninit()` initialised `next_free` to `u32::MAX`
//! (`NEXT_FREE_TAIL`), a non-zero value, which forced the ENTIRE slot array
//! into `.data` instead of `.bss` — a large per-binary `.data` cost for every
//! binary that linked sefer-alloc with the `production` feature (the inline
//! `HeapCore` in each slot carries the `fastbin` magazine + `alloc-decommit`
//! large-cache state, so under `production` a `HeapSlot` is ~7.5 KiB and the
//! whole array is ~29 MiB; without those features a slot is ~192 B and the
//! array ~768 KiB — the `.data` cost tracked the feature-dependent slot size).
//!
//! The fix: replace the `static` with an `AtomicPtr<Registry>` (8 bytes of
//! `.data`). On first call to [`ensure`], the winner of a CAS race allocates
//! `size_of::<Registry>()` bytes via `aligned_vmem::reserve_aligned` (a direct
//! OS `VirtualAlloc`/`mmap` syscall — M5-clean, no `std::alloc`) and writes
//! `Registry::new_zeroed()` into it in place, then publishes the pointer with
//! a Release store. The reservation is leaked for the process lifetime (the
//! registry is process-global, never torn down). After that, every call is a
//! single Acquire load + non-null, non-sentinel check — branch-light and
//! allocation-free.
//!
//! ## RAD-1: lazy `next_free` (no eager per-slot first-touch)
//!
//! The in-place init writes ONLY the single non-zero `Registry` field
//! (`free_slots = TaggedPtr::empty()`); it does NOT pre-populate each slot's
//! `next_free`. `next_free` is written lazily by `push_free_slot` (which runs
//! before any `pop_free_slot` can read it), so the OS-zeroed initial value is
//! never observed. This removed a ~16 MiB demand-zero first-touch (4096 slots ×
//! ~7.5 KiB stride under `production` = 4096 distinct pages) and several ms of
//! first-allocation latency, paid once per process and invisible to every
//! warm-process wall-clock/iai bench. The judge is
//! `examples/first_alloc_process.rs` (driven by `scripts/first-alloc-bench.mjs`)
//! — a process-per-sample RSS/latency probe. See the SAFETY comment at the
//! init site in `ensure_slow` for the full read-audit.
//!
//! ## The pointer state-machine
//!
//! `AtomicPtr<Registry>` drives the `UNINIT → INITIALIZING → READY` transition
//! via pointer values:
//!
//! | Pointer value | Meaning |
//! |---|---|
//! | `null` | `UNINIT` — not yet initialised |
//! | `SENTINEL_INITIALIZING` (`1 as *mut`) | `INITIALIZING` — one thread won the CAS and is allocating |
//! | real `*mut Registry` | `READY` — fully initialised; safe to dereference |
//!
//! 1. The first caller observes `null` and CASes it to `SENTINEL_INITIALIZING`.
//!    The CAS winner:
//!    a. Calls `aligned_vmem::reserve_aligned(SIZE, ALIGN)` — direct OS syscall,
//!       no `std::alloc`, no registry dependency.
//!    b. Field-by-field in-place initialisation (OS zeroed-pages + fix-up of
//!       non-zero fields: `next_free` per slot and `free_slots`).
//!    c. `REGISTRY_PTR.store(base, Release)` — publishes the ready pointer.
//!    d. `mem::forget(reservation)` — leaks the reservation intentionally; the
//!       registry lives for the process lifetime.
//! 2. Concurrent losers observe `SENTINEL_INITIALIZING` (or `null`, then fail
//!    the CAS) and spin until they observe a non-null, non-sentinel pointer
//!    under `Acquire`. The spin window is tiny (one OS page allocation).
//! 3. After `READY`, every subsequent call is a single `Acquire` load + two
//!    cheap comparisons + return.
//!
//! `Release`/`Acquire` on the pointer transition establishes happens-before
//! from the initialising thread's `ptr::write` (the registry fields) to every
//! reader that observes the real pointer, so readers see a fully constructed
//! registry.
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
//! No path from `ensure_slow` touches `sefer_alloc::registry::*` — confirmed
//! by inspection. The reservation call chain is a straight line to a kernel
//! syscall boundary.
//!
//! ## Provenance model (task #140)
//!
//! This module's int↔pointer conversions fall into two DIFFERENT provenance
//! classes; conflating them was the source of the "just cast" style this
//! task replaced with explicit APIs.
//!
//! **1. The `REGISTRY_PTR` sentinel is STRICT-provenance-clean.**
//! `SENTINEL_INITIALIZING` is a bare marker value living inside
//! `AtomicPtr<Registry>` — it is compared for equality (in the CAS operand
//! and in `ensure`'s/the spin loop's `.addr()` checks) but is NEVER
//! dereferenced. [`core::ptr::without_provenance_mut`] constructs it as a
//! pointer that carries no provenance at all, which is exactly right for a
//! value that must never be read through — and it passes
//! `-Zmiri-strict-provenance` cleanly, because strict provenance only
//! objects to DEREFERENCING a pointer whose provenance doesn't cover the
//! memory it points at; a pointer that is only ever compared is fine.
//!
//! **2. `abandoned_segs` (this module) and the A1 deferred-large-free stack
//! (`alloc_core::deferred_large`) are, by design, EXPOSED-provenance-only —
//! full strict-provenance conformance is unreachable for them.** Both are
//! cross-allocation intrusive Treiber stacks: the "next" link for segment
//! `A` is stored INSIDE segment `A`'s own header, but the VALUE of that link
//! is the address of a DIFFERENT segment `B`, which came from a distinct OS
//! reservation with its own, unrelated provenance. A single `u64`/`AtomicU64`
//! word (the stack head, or a header's link field) can carry only an
//! address, not a provenance token, and there is no way to smuggle segment
//! `B`'s provenance into a slot owned by segment `A`. This is a structural
//! property of ANY tagged/intrusive lock-free stack that chains
//! cross-allocation nodes through packed integer words — not a gap specific
//! to this codebase.
//!
//! The sanctioned fallback for this shape is Rust's **exposed-provenance**
//! model: every store site that packs a real pointer's address into such a
//! word calls `<*mut T>::expose_provenance` first (explicitly registering
//! that pointer's provenance in the global exposed-provenance table); every
//! load site that reconstructs a dereferenceable pointer from such a word
//! calls [`core::ptr::with_exposed_provenance_mut`] (validly re-deriving a
//! pointer with the previously-exposed provenance for that address). Every
//! `expose_provenance` store site in this crate is paired with a
//! `with_exposed_provenance_mut` load site — see [`pack_abandoned_head`]/
//! [`unpack_abandoned_head`] below, `HeapRegistry::pop_abandoned_segment`/
//! `push_abandoned_segment_into` in `heap_registry.rs`, and
//! `push_large_deferred_free`/`drain_large_deferred_free` in
//! `alloc_core::deferred_large`.
//!
//! **Consequence for miri:** the registry (and the A1 deferred-large stack)
//! validate cleanly under plain `cargo +nightly miri test` (the
//! exposed-provenance model, miri's default) but are NOT expected to pass
//! `-Zmiri-strict-provenance` — that flag will flag the
//! `with_exposed_provenance_mut` reconstructions in `unpack_abandoned_head`/
//! `pop_abandoned_segment`/`drain_large_deferred_free` as provenance
//! violations, which is miri correctly reporting the documented structural
//! limit above, not a bug. The `REGISTRY_PTR` sentinel handling (class 1) is
//! the one part of this module's provenance story that DOES pass
//! `-Zmiri-strict-provenance` — see the sentinel construction in
//! `ensure_slow`/`dbg_rollback_sentinel_reenterable`.

// This file uses `unsafe` for two operations:
//  1. Field-by-field in-place initialisation of the `Registry` object in
//     freshly reserved OS memory (pointer arithmetic + writes through
//     `addr_of_mut!`).
//  2. `unsafe { &*p }` — dereferencing the published pointer after observing
//     it under `Acquire` (sound because the initialiser's `Release` store
//     establishes happens-before).
// Every `unsafe` block carries a `// SAFETY:` proof below.
#![allow(unsafe_code)]

use core::hint::spin_loop;
use core::sync::atomic::{AtomicPtr, Ordering};

// NOTE (RAD-1): `NEXT_FREE_TAIL` is no longer imported here — the eager
// per-slot `next_free = NEXT_FREE_TAIL` pre-population was removed (lazy init;
// `push_free_slot` writes `next_free` before any read — see the SAFETY comment
// at the removed loop's former site in `ensure_slow`).
use super::heap_slot::HeapSlot;
use super::tagged_ptr::TaggedPtr;

/// Maximum number of heaps the registry can hold. Each live thread claims one
/// slot for its heap; `recycle` returns it. 4096 is generous for realistic
/// thread counts (a process with > 4096 simultaneous threads is pathological
/// for an allocator; the cap can be raised if a measured workload needs it).
/// With lazy allocation this is now a runtime cap (size of the heap-allocated
/// slot array), NOT a `.data`/`.bss` cost — the array is allocated on first
/// use via `aligned_vmem::reserve_aligned`.
pub const MAX_HEAPS: usize = 4096;

/// The segment size used for the abandoned-segment address packing. Mirrors
/// [`crate::alloc_core::os::SEGMENT`] (kept as a literal here to avoid a
/// cross-feature dependency from the registry bootstrap — the value is
/// structural, set in `ALLOC_PLAN.md`, and a `const _: () = assert!` below
/// ties them together so they cannot drift).
const ABANDON_SEG_SHIFT: u32 = 22; // log2(4 MiB)
/// Only exists to feed the `alloc-core`-gated `const _: ()` tie-assert below;
/// gated with the SAME `cfg` so it is not a dead constant on non-`alloc-core`
/// builds. The `#[allow(dead_code)]` is for MSRV 1.88 only: its dead-code
/// analysis does not count the `const _: ()` assert reference as a use, so it
/// false-positives here (newer rustc counts it and needs no allow — the allow
/// is simply inert there, not a suppressed real signal).
#[cfg(feature = "alloc-core")]
#[allow(dead_code)]
const ABANDON_SEG_SIZE: u64 = 1u64 << ABANDON_SEG_SHIFT;
/// Number of low bits available for the ABA tag in the abandoned-segment head
/// packing (a segment base is `ABANDON_SEG_SIZE`-aligned, so its low
/// `ABANDON_SEG_SHIFT = 22` bits are always zero — we reuse them for the tag).
const ABANDON_TAG_BITS: u32 = ABANDON_SEG_SHIFT;
pub(crate) const ABANDON_TAG_MASK: u64 = (1u64 << ABANDON_TAG_BITS) - 1;

/// Compile-time tie: if the real `SEGMENT` ever diverges from
/// `ABANDON_SEG_SIZE`, this fails to compile (the abandoned-segment packing
/// would silently corrupt high address bits). `cfg`-gated so it only fires
/// when `alloc-core` (and thus `os::SEGMENT`) is in the build graph.
#[cfg(feature = "alloc-core")]
const _: () = assert!(
    ABANDON_SEG_SIZE == crate::alloc_core::os::SEGMENT as u64,
    "ABANDON_SEG_SIZE must match os::SEGMENT (the abandoned-segment head packing relies on SEGMENT alignment)"
);

/// Pack `(base, tag)` into one `AtomicU64` head word for the
/// abandoned-segments intrusive stack. `base` MUST be SEGMENT-aligned (its low
/// `ABANDON_SEG_SHIFT` bits are zero — true for every segment base by
/// construction); the tag occupies those low bits and is bumped on every push
/// (ABA defence). The full 64-bit base is recoverable, so addresses above 4
/// GiB (ASLR) are handled correctly (the bug fixed in Phase 12.4 — FINDINGS №1).
///
/// Not `const` because `*mut u8 as u64` is not stable in `const fn` (it needs
/// `const_raw_ptr_to_int_transmute`, unstable). Runtime-only use.
#[doc(hidden)]
pub fn pack_abandoned_head(base: *mut u8, tag: u64) -> u64 {
    // EXPOSED-PROVENANCE STORE SITE: `base` is a real, dereferenceable
    // segment pointer whose address is about to be packed into a plain
    // `u64` word (the Treiber head, later CASed into `Registry::abandoned_segs`
    // and — after unpacking — dereferenced again by a popper, possibly on a
    // different thread). `expose_provenance` is the explicit, sanctioned
    // exposed-provenance API for exactly this pattern: it records `base`'s
    // provenance in the global exposed-provenance table so that a LATER
    // `with_exposed_provenance_mut` call on the same address can validly
    // reconstruct a dereferenceable pointer. Paired load site:
    // `unpack_abandoned_head` below (and every popper that dereferences the
    // reconstructed base). See the module "Provenance model" section.
    let addr = base.expose_provenance() as u64;
    // The tag lives in the low ABANDON_TAG_BITS (which are zero in `addr`
    // because `base` is SEGMENT-aligned). OR them together.
    (addr & !ABANDON_TAG_MASK) | (tag & ABANDON_TAG_MASK)
}

/// Unpack the abandoned-segment head word back into `(base, tag)`. The base's
/// low `ABANDON_TAG_BITS` are restored to zero.
#[doc(hidden)]
pub fn unpack_abandoned_head(word: u64) -> (*mut u8, u64) {
    // EXPOSED-PROVENANCE LOAD SITE: reconstructs a dereferenceable pointer
    // from a plain integer address under the exposed-provenance model.
    // Sound ONLY because every producer of an `abandoned_segs` head word
    // packed the address via `pack_abandoned_head`, which calls
    // `expose_provenance` on the real segment pointer before storing it (see
    // that function's doc comment) — `with_exposed_provenance_mut` may
    // legally "re-derive" a pointer with that exposed provenance from a
    // matching address. The empty-stack sentinel (address 0) is also a valid
    // input: `with_exposed_provenance_mut(0)` yields a null pointer, which
    // `abandoned_head_is_empty`/callers check for before any dereference.
    let base = core::ptr::with_exposed_provenance_mut::<u8>((word & !ABANDON_TAG_MASK) as usize);
    let tag = word & ABANDON_TAG_MASK;
    (base, tag)
}

/// The empty-stack sentinel for the abandoned-segment head: base = null, tag = 0.
/// A null base unambiguously denotes "empty" (no real segment base is null).
#[doc(hidden)]
pub const ABANDONED_HEAD_EMPTY: u64 = 0;

/// Whether an abandoned-segment head word denotes the empty stack.
#[doc(hidden)]
pub fn abandoned_head_is_empty(word: u64) -> bool {
    // Empty iff base is null (tag is irrelevant — only the base distinguishes).
    (word & !ABANDON_TAG_MASK) == 0
}

/// The bootstrap outcome: the fixed slot array plus the dynamic atomics that
/// drive `claim`/`recycle`/`abandon`. Allocated via `aligned_vmem::reserve_aligned`
/// on first call to [`ensure`] (NOT by `std::alloc` — M5-clean). Lives for
/// the process lifetime (the reservation is leaked after init via `mem::forget`).
///
/// The struct is constructed in-place (via `ptr::write`) inside the OS
/// reservation; this is the same discipline as the primordial segment bootstrap.
pub struct Registry {
    /// The fixed slot array. Indexed by slot id; `MAX_HEAPS` entries. Lives
    /// for the process lifetime (the reservation is never dropped).
    //
    // `pub(crate)` (task #93 / R4-MS-4): a `pub` slot array let safe
    // downstream code reach `slots[idx].state.store(..)` and re-claim a LIVE
    // `HeapCore`. Tests read slot state/generation through `dbg_slot_state`/
    // `dbg_slot_generation` below.
    pub(crate) slots: [HeapSlot; MAX_HEAPS],
    /// High-water mark of allocated slots (the next unused slot index). A
    /// `claim` that finds `free_slots` empty `fetch_add`s this to mint a new
    /// slot. Capped at `MAX_HEAPS`.
    pub(crate) count: core::sync::atomic::AtomicU32,
    /// Tagged-Treiber head of the `free_slots` stack: low 16 = slot index,
    /// high 48 = ABA tag (bumped per push; see `TaggedPtr`, repacked in W7a).
    /// Initialised empty.
    //
    // `pub(crate)` (task #93 / R4-MS-4): a `pub` free-list head let safe
    // downstream code complete the re-claim attack by pushing a slot back onto
    // the stack (`free_slots.store(dbg_pack(idx, ..))`).
    pub(crate) free_slots: core::sync::atomic::AtomicU64,
    /// Phase 12.4: the intrusive abandoned-segments Treiber stack head. Packs
    /// the full 64-bit segment base (in the high bits, since bases are
    /// `SEGMENT`-aligned → low `ABANDON_SEG_SHIFT` bits are zero) with an
    /// ABA tag in those low bits. Each abandoned segment's
    /// `next_abandoned` header field chains to the next base. This fixes
    /// FINDINGS №1 (the old `AtomicU64` packing truncated bases >4 GiB);
    /// the full base is now preserved.
    pub(crate) abandoned_segs: core::sync::atomic::AtomicU64,
}

// `Registry` is shared across threads via the `AtomicPtr`. All mutable access
// to its fields goes through atomics (`count`, `free_slots`, `abandoned_segs`)
// or the slot-level single-writer protocol (`slots`). Every field is ALREADY
// `Sync`: the three `Atomic*` fields, and `[HeapSlot; MAX_HEAPS]` (which is
// `Sync` because `HeapSlot` carries its own `unsafe impl Sync` — see
// `heap_slot.rs`). So `Registry` AUTO-derives `Sync`; no `unsafe impl` is
// needed (task #21 / review L1). The former `unsafe impl Sync for Registry`
// only restated the auto-impl but FROZE it: a future `!Sync` field (e.g. a
// `Cell<..>` diagnostic) would silently keep `Registry: Sync` — unsound —
// where the auto-impl would honestly drop it. This compile-time assert
// documents the intent AND enforces it: adding a `!Sync` field makes THIS line
// fail to compile with a clear "`Registry: Sync` is not satisfied" error.
const _: () = {
    fn assert_sync<T: Sync>() {}
    let _ = assert_sync::<Registry>;
};

// -------------------------------------------------------------------------
// Test-only `#[doc(hidden)]` accessors (task #93 / R4-MS-4).
//
// `Registry`'s fields are `pub(crate)`: safe code OUTSIDE the crate must not be
// able to mutate the slot state machine or push onto `free_slots` (R4-MS-4 — a
// `pub` field let safe downstream code re-claim a LIVE `HeapCore` and break
// the `HeapSlot` single-writer invariant). The integration tests in `tests/`
// that legitimately need to OBSERVE slot state/generation (and, in one
// counterfactual, preset a generation near the u32 boundary) go through these
// narrow accessors instead. The reads are plain atomic loads — always sound —
// so they stay safe `fn`. The single write (`dbg_slot_preset_generation`) is
// `unsafe fn` because its soundness needs the slot to not be racing a
// concurrent `claim()`; the only caller
// (`tests/regression_counter_wrap.rs`) wraps it in `unsafe { .. }` under a
// documented precondition. (`count` reads already have the standalone
// `count_for_test` accessor below.) These are NOT stable public API.
impl Registry {
    /// Read a slot's `state` atomically (test helper).
    #[doc(hidden)]
    #[inline]
    pub fn dbg_slot_state(&self, idx: usize) -> u8 {
        self.slots[idx].state.load(Ordering::Acquire)
    }

    /// Read a slot's `generation` atomically (test helper).
    #[doc(hidden)]
    #[inline]
    pub fn dbg_slot_generation(&self, idx: usize) -> u64 {
        self.slots[idx].generation.load(Ordering::Acquire)
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
        self.slots[idx].generation.store(val, Ordering::Release)
    }
}

// -------------------------------------------------------------------------
// Lazy pointer: replaces the large `.data` `static REGISTRY: Registry` (the
// slot array's size is feature-dependent — ~29 MiB under `production`, ~768 KiB
// without the inline-`HeapCore` magazine/large-cache state).
// -------------------------------------------------------------------------

/// Sentinel: a non-null, non-real address that means "one thread is currently
/// initialising the registry". Aligned to 1 (the raw integer 1 is not a valid
/// `Registry` pointer — `Registry` has alignment ≥ 4). Any real `*mut Registry`
/// will differ from this value and from null.
const SENTINEL_INITIALIZING: usize = 1;

/// The process-global registry pointer. Starts null (`UNINIT`).
/// Transitions: `null → SENTINEL_INITIALIZING → real *mut Registry`.
/// After the final store (Release), every subsequent load (Acquire) sees a
/// valid, fully-constructed `Registry`.
///
/// BINARY SIZE: this is 8 bytes of `.data` (one pointer). The old
/// `static REGISTRY: Registry = Registry::new_zeroed()` put the WHOLE slot array
/// into `.data` (up to ~29 MiB under `production`) because
/// `HeapSlot::new_uninit()` sets `next_free = u32::MAX` (`NEXT_FREE_TAIL`), a
/// non-zero value that forced the full slot array into `.data` instead of `.bss`.
static REGISTRY_PTR: AtomicPtr<Registry> = AtomicPtr::new(core::ptr::null_mut());

/// Size of `Registry` rounded up to a multiple of `aligned_vmem::PAGE` (4 KiB).
/// `reserve_aligned` requires `size` to be a non-zero multiple of `PAGE`.
const REGISTRY_SIZE: usize = {
    let raw = core::mem::size_of::<Registry>();
    let page = aligned_vmem::PAGE;
    // Round up to the next page boundary.
    (raw + page - 1) & !(page - 1)
};

/// Alignment for the `reserve_aligned` call. `Registry`'s natural alignment is
/// at most 8 bytes (its largest-aligned field is `AtomicU64`). `reserve_aligned`
/// requires `align >= PAGE` (4 KiB), so we use `PAGE` — the registry occupies
/// whole pages anyway.
const REGISTRY_ALIGN: usize = aligned_vmem::PAGE;

/// Ensure the registry is initialised, then return a `&'static` reference to
/// it. The first call performs the (M5-clean, `std::alloc`-free) OS-reservation
/// init and publication; concurrent and later calls observe the real pointer
/// under `Acquire` and return immediately.
///
/// ## Fast path (typical: already initialised)
///
/// One `Acquire` load of `REGISTRY_PTR`. If the pointer is non-null AND not the
/// sentinel, return `&*p` immediately (branch-light, allocation-free).
///
/// ## Slow path (first call or race)
///
/// See `ensure_slow`.
#[inline]
pub fn ensure() -> &'static Registry {
    let p = REGISTRY_PTR.load(Ordering::Acquire);
    // `.addr()` reads the pointer's address for a pure integer comparison
    // (against `0`/`SENTINEL_INITIALIZING`); it does not strip or use
    // provenance, so this is strict-provenance-clean — `p` itself (not an
    // integer reconstructed from `p_usize`) is what gets dereferenced below.
    let p_usize = p.addr();
    if p_usize != 0 && p_usize != SENTINEL_INITIALIZING {
        // SAFETY: we observed a real non-null non-sentinel pointer under
        // Acquire. The initialising thread stored this pointer with Release
        // AFTER completing the field-by-field in-place initialisation of the
        // `Registry`, so this Acquire load sees all the bytes written. The
        // pointer remains valid for the process lifetime (the OS reservation
        // is leaked via `mem::forget`). Casting to `&'static` is sound because
        // the allocation outlives any reference derived from it.
        return unsafe { &*p };
    }
    ensure_slow()
}

/// Roll `REGISTRY_PTR` back from `SENTINEL_INITIALIZING` to `null` (`UNINIT`).
///
/// Single point of truth for the anti-livelock rollback used by the
/// OOM-bailout in [`ensure_slow`] (Task #131) — kept as its own function so
/// the test-only hook below exercises EXACTLY the same code the production
/// bailout runs, rather than a duplicated copy that could drift out of sync.
///
/// `Release` ordering: a thread that later retries `ensure_slow` performs
/// `compare_exchange(null, SENTINEL, Acquire, ..)`; pairing that Acquire with
/// this Release ensures the retrying thread does not need to observe
/// anything about the failed attempt beyond "the slot is free again" — there
/// is no partially-initialised `Registry` state to synchronise (the failed
/// attempt never got past the VM reservation).
#[cold]
fn rollback_registry_sentinel() {
    REGISTRY_PTR.store(core::ptr::null_mut(), Ordering::Release);
}

/// Slow path for [`ensure`]: race to initialise the registry via a CAS on
/// `REGISTRY_PTR`. Exactly one caller wins, allocates via
/// `aligned_vmem::reserve_aligned`, constructs the `Registry` in-place, and
/// publishes the pointer. All others spin-wait on a tiny window.
#[cold]
fn ensure_slow() -> &'static Registry {
    // Race: try to acquire the INITIALIZING sentinel via CAS(null, SENTINEL).
    // Only ONE thread wins this CAS; the rest observe SENTINEL (or null then
    // fail the CAS) and fall into the spin branch.
    // SENTINEL_INITIALIZING is a bare marker address, NEVER dereferenced (only
    // compared for pointer equality against `REGISTRY_PTR`'s CAS operand and
    // the loads in `ensure`/the spin loop below). `without_provenance_mut`
    // constructs a pointer that carries NO provenance at all — exactly the
    // right semantics for a value that exists purely as an integer tag riding
    // inside an `AtomicPtr<Registry>` and must never be read through. This is
    // strict-provenance-clean: no `expose_provenance`/`with_exposed_provenance`
    // pairing is needed because the value is never turned back into a
    // dereferenceable pointer. Pointer equality (`==`, and `AtomicPtr`'s CAS)
    // compares addresses regardless of provenance, so this is semantically
    // identical to the old `SENTINEL_INITIALIZING as *mut Registry` cast.
    let sentinel = core::ptr::without_provenance_mut::<Registry>(SENTINEL_INITIALIZING);
    match REGISTRY_PTR.compare_exchange(
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
            // We are the SOLE initialiser. Allocate the registry from OS VM.
            //
            // M5 (reentrancy-free) proof: `aligned_vmem::reserve_aligned` is a
            // direct OS syscall (`VirtualAlloc` on Windows, `mmap` on Unix).
            // It does NOT call `std::alloc`, `Box`, `Vec`, or any other Rust
            // allocator entry point. Its source (`crates/vmem/src/lib.rs`) was
            // verified to have no transitive dependency on
            // `sefer_alloc::registry::*`. Under miri it falls back to
            // `std::alloc`, but under miri we are NOT the global allocator
            // (the host miri allocator handles the harness), so no reentrancy.
            let reservation = match aligned_vmem::reserve_aligned(REGISTRY_SIZE, REGISTRY_ALIGN) {
                Some(r) => r,
                None => {
                    // Task #131: OOM during registry bootstrap. We already
                    // published SENTINEL_INITIALIZING above (the CAS at the
                    // top of this function), so if we bail out here WITHOUT
                    // rolling it back, every loser thread spinning in the
                    // `Err` branch below spins FOREVER (the sentinel is never
                    // replaced by a real pointer), and every FUTURE call to
                    // `ensure` (from any thread, including ones that have not
                    // even called it yet) sees the non-null SENTINEL, falls
                    // into `ensure_slow`, fails the
                    // `compare_exchange(null, SENTINEL)` CAS (the current
                    // value is SENTINEL, not null), and ALSO spins forever.
                    // The whole process livelocks on the next registry touch.
                    rollback_registry_sentinel();
                    // Fail fast via `abort`, not `panic!`/`.expect(..)`. A
                    // panic here would unwind, and unwinding formats the
                    // panic message / captures a backtrace, which allocates
                    // -- reentering the global allocator, which calls
                    // `ensure()` again. Even with the rollback above in
                    // place that reentrant call would itself race a fresh
                    // bootstrap attempt (which would ALSO fail, since we are
                    // still OOM), risking recursion/deadlock instead of a
                    // clean exit. `std::process::abort` performs no unwind
                    // and no allocation, so it cannot re-enter `ensure`. A
                    // registry that cannot be backed by VM is unrecoverable
                    // for this allocator (it cannot even materialise its own
                    // core bookkeeping structure) -- exactly the situation
                    // `handle_alloc_error` exists for on a normal allocation
                    // failure; we take the analogous immediate-abort exit
                    // here. See the module-level `std::process::abort`
                    // availability note below.
                    std::process::abort();
                }
            };

            let base = reservation.as_ptr() as *mut Registry;

            // Task #139: under miri, `aligned_vmem::reserve_aligned` falls back
            // to `std::alloc` (miri has no `VirtualAlloc`/`mmap`), which does
            // NOT zero the bytes the way real OS pages do. The field-by-field
            // init below relies on OS zero-pages for every field it does not
            // explicitly write (see item 1 in the comment below): `state = 0`,
            // `generation = 0`, `count = 0`, `abandoned_segs = 0`,
            // `HeapSlot::initialised = 0`, etc. Under miri those reads would be
            // uninitialised-memory UB, which blocked miri from validating the
            // whole registry module (incl. the task #133 per-heap-counter
            // aggregation). Zero the reservation ourselves under miri so the
            // "OS zero-pages" assumption holds. Compiled out entirely on real
            // targets — zero cost in production.
            //
            // SAFETY: `base` is a fresh `REGISTRY_SIZE`-byte reservation we
            // solely own (the CAS winner), not yet published; zero is a valid
            // bit-pattern for every field (`AtomicU8/U32/U64/Bool` = 0,
            // `MaybeUninit<HeapCore>` = any bytes).
            #[cfg(miri)]
            unsafe {
                core::ptr::write_bytes(base as *mut u8, 0, REGISTRY_SIZE);
            }

            // In-place initialisation of the Registry — field by field.
            //
            // We do NOT use `ptr::write(base, Registry::new_zeroed())` because
            // `Registry` is large (up to ~29 MiB under `production` — 4096 ×
            // HeapSlot each ~7.5 KiB). Creating that value as a `const fn` result
            // and passing it to `ptr::write` would either: (a) put ~29 MiB in
            // `.rodata` (defeating the binary-size goal) or (b) create a ~29 MiB
            // stack temporary in debug builds (stack overflow). Instead we
            // initialise in-place through raw pointer arithmetic, exploiting two
            // facts:
            //
            //  1. OS-allocated pages are zero-initialised (both `VirtualAlloc`
            //     on Windows and anonymous `mmap` on Linux guarantee this). ALL
            //     slot fields start at their correct zero value — including, per
            //     RAD-1, `next_free = 0` (lazy init: `push_free_slot` writes the
            //     real link before any pop reads it, so the zero is never
            //     observed): `state = 0 = STATE_FREE`, `generation = 0`,
            //     `heap = MaybeUninit::uninit()` (unspecified bits, zeroes are
            //     fine), `initialised = 0`, `count = 0`, `abandoned_segs = 0`.
            //
            //  2. The ONLY non-zero initial value is `Registry::free_slots =
            //     TaggedPtr::empty() = 0x0000_0000_0000_FFFF` (task W7a: the
            //     index half is now 16 bits, so the empty sentinel is `0xFFFF`,
            //     not the old 32-bit `0xFFFF_FFFF`). We write it in-place using
            //     `addr_of_mut!` + `write`. (RAD-1: the per-slot `next_free`
            //     pre-population is gone — see the SAFETY comment at the write
            //     site below.)
            //
            // `AtomicU32` and `AtomicU64` are `#[repr(transparent)]` over their
            // inner `UnsafeCell<u32/u64>`, which is `#[repr(transparent)]` over
            // the integer. So writing a `u32`/`u64` to the byte address of the
            // atomic is equivalent to constructing `AtomicU32::new(val)` there
            // and is fully defined.

            // SAFETY: `base` is non-null, aligned to `REGISTRY_ALIGN` (PAGE =
            // 4096, which is >= `align_of::<Registry>()` — at most 8 bytes), and
            // valid for `REGISTRY_SIZE` bytes (>= `size_of::<Registry>()`). We
            // are the sole writer (only one CAS winner can reach this branch).
            // The memory is OS-provided zero-initialised pages; we write the
            // non-zero fields below. After all writes the `Registry` at `base`
            // is fully initialised and valid.
            unsafe {
                // RAD-1 (Phase 1, E1): the eager per-slot `next_free =
                // NEXT_FREE_TAIL` write loop (over all `MAX_HEAPS = 4096` slots)
                // was REMOVED here. It was the dominant startup RSS/latency cost:
                // under `production`, `HeapSlot` is ~7488 B (the inline
                // `HeapCore` carries the `fastbin` magazine + `alloc-decommit`
                // large-cache state), so the 4096 writes landed on 4096 DISTINCT
                // pages (stride > 4 KiB) — ~16 MiB of demand-zero first-touch RSS
                // and several ms of latency, paid once on the first allocation of
                // every process, invisible to every wall-clock/iai bench (they
                // measure a warm long-lived process). Measured RED baseline:
                // `examples/first_alloc_process.rs` → RSS Δ 1 heap ≈ 16.1 MiB,
                // first-alloc latency ≈ 8.6 ms.
                //
                // Why removing it is SOUND (the `next_free` read-audit, RAD-1
                // Step 1): a slot's `next_free` field is READ in exactly ONE
                // place — `pop_free_slot` (`heap_registry.rs`), and only for a
                // slot the pop just observed as the `free_slots` stack head. A
                // slot only becomes a stack head via `push_free_slot`, which
                // WRITES `next_free` (under `Release`) BEFORE the CAS that
                // publishes the slot as the head. So every `next_free` read is
                // preceded (happens-after) by a `push_free_slot` write of that
                // same slot's `next_free`. A freshly-MINTED slot (via
                // `bump_count`) goes straight to `claim` and is NEVER read
                // through `next_free` before it is first pushed — the bootstrap
                // sentinel value was dead on arrival for every mint. Lazy init
                // (the push writes it) is therefore observationally identical to
                // eager init, at zero first-touch cost. (`NEXT_FREE_TAIL` is
                // still the tail sentinel `push_free_slot` writes for the
                // empty-stack case — the constant is unchanged; only the eager
                // pre-population is gone.)
                //
                // Everything else in each slot correctly starts at OS-zeroed:
                //   next_free   = 0 (NOT NEXT_FREE_TAIL anymore — lazy, see
                //     above; 0 is never read before a push overwrites it)
                //   state       = 0 = STATE_FREE
                //   generation  = 0
                //   heap        = MaybeUninit::uninit() (unspecified, zero is fine)
                //   initialised = 0 = false
                //   tcache_hits / large_cache_hits = 0
                //
                // Write `free_slots = TaggedPtr::empty()` (the ONE non-zero
                // Registry field — a single 8-byte store, not 4096).
                // `count` and `abandoned_segs` start at zero (already correct).
                core::ptr::addr_of_mut!((*base).free_slots)
                    .cast::<u64>()
                    .write(TaggedPtr::empty());
                // `count` is already 0. `abandoned_segs` is already 0.
                // The `Registry` at `base` is now fully initialised.
            }

            // Publish the real pointer with Release so every subsequent
            // Acquire load in `ensure` (fast path) sees the fully written
            // registry. This pairs with the Acquire load in `ensure`'s fast
            // path and with the Acquire loads in the spin loop below.
            REGISTRY_PTR.store(base, Ordering::Release);

            // Leak the reservation intentionally. The registry lives for the
            // process lifetime and is never dropped. `mem::forget` suppresses
            // the `Drop` impl that would call `VirtualFree`/`munmap`, which
            // would be catastrophic (a live `'static` reference would dangle).
            core::mem::forget(reservation);

            // SAFETY: we fully initialised the `Registry` at `base` (all fields
            // written above — zero-init from OS + explicit non-zero field
            // writes) and published it with Release. The allocation outlives any
            // reference derived from it (leaked via `mem::forget`). Dereferencing
            // `base` as `&'static Registry` is sound.
            unsafe { &*base }
        }
        Err(_) => {
            // ── Loser branch ─────────────────────────────────────────────
            // Another thread is (or was) initialising. Spin until we observe
            // a real (non-null, non-sentinel) pointer. This window is now tiny:
            // the winner does one OS reservation + ONE `free_slots` store + one
            // publish store (RAD-1 removed the eager 4096-slot `next_free` write
            // loop — that loop, not the reservation, was the multi-millisecond
            // part of this window under `production`). On the first call this may
            // take a few microseconds; on any subsequent call to `ensure`, the
            // fast path in `ensure()` returns before reaching here.
            loop {
                let p = REGISTRY_PTR.load(Ordering::Acquire);
                // See the identical `.addr()` rationale in `ensure`'s fast
                // path above: pure integer comparison, no provenance use.
                let p_usize = p.addr();
                if p_usize != 0 && p_usize != SENTINEL_INITIALIZING {
                    // SAFETY: same argument as the fast path in `ensure()`.
                    // We observed the real pointer under Acquire, which pairs
                    // with the winner's Release store of the pointer after
                    // `ptr::write`. The `Registry` is fully initialised.
                    return unsafe { &*p };
                }
                spin_loop();
            }
        }
    }
}

/// Test-only hook (Task #131): proves the anti-livelock rollback in
/// [`rollback_registry_sentinel`] actually clears the sentinel, without
/// invoking `std::process::abort` (which would kill the test harness) and
/// without racing any OTHER test that may concurrently be calling `ensure`
/// on the real, possibly-already-initialised `REGISTRY_PTR`.
///
/// ## Why this operates on the LIVE `REGISTRY_PTR` (and how it stays safe)
///
/// The bug is specifically about the interaction between `REGISTRY_PTR`'s
/// three-state protocol (`null` / `SENTINEL_INITIALIZING` / real pointer) and
/// the rollback. A hook on a separate test-only atomic would only prove that
/// a *copy* of the protocol works, not that `rollback_registry_sentinel`
/// (the actual function the fix calls) restores the actual invariant the
/// rest of `bootstrap.rs` depends on. So this hook drives the real
/// `REGISTRY_PTR` through the fix's exact code path — but ONLY when it is
/// safe to do so:
///
/// 1. It CAS-acquires `REGISTRY_PTR` from `null` to `SENTINEL_INITIALIZING`
///    itself (the same transition the real `ensure_slow` winner performs).
///    If the registry has ALREADY been initialised by an earlier test in
///    this process (a real, non-null non-sentinel pointer), the CAS simply
///    fails and this function returns `None` — it never disturbs a live
///    registry. Callers must treat `None` as "inconclusive here" and are
///    expected to run under the crate's usual registry `SERIAL` guard so no
///    concurrent `ensure()` caller can be spinning on the sentinel while we
///    hold it.
/// 2. With the sentinel now in place (as if we were the real bootstrap
///    winner that hit OOM), it calls [`rollback_registry_sentinel`] — the
///    IDENTICAL function the production OOM-bailout calls before
///    `std::process::abort()`.
/// 3. It then verifies the anti-livelock postcondition directly: a
///    subsequent `compare_exchange(null, SENTINEL, ..)` must SUCCEED,
///    proving the rollback actually cleared the sentinel back to `null`
///    (if the rollback were a no-op, this CAS would fail with `Err(SENTINEL)`
///    and a real winner — or every future `ensure()` caller — would spin
///    forever, which is exactly bug #131).
/// 4. It immediately restores `REGISTRY_PTR` to the value observed on entry
///    (`null`, since step 1 only proceeds when the initial load was `null`),
///    leaving the process exactly as it found it.
///
/// Returns `Some(true)` if the rollback was proven to clear the sentinel,
/// `Some(false)` if the postcondition CAS unexpectedly failed (rollback is
/// broken — the counterfactual this test is designed to catch), or `None` if
/// the registry was already initialised by another test and this check could
/// not run (callers should treat that as "not applicable", never as failure).
///
/// Callers MUST hold the crate's registry-wide serial guard (as every other
/// `tests/registry_*` file already does) so no concurrent thread is calling
/// `ensure()`/`ensure_slow()` while this hook is mutating `REGISTRY_PTR`.
#[doc(hidden)]
pub fn dbg_rollback_sentinel_reenterable() -> Option<bool> {
    // See the identical construction (and its SAFETY/provenance rationale) in
    // `ensure_slow` above: a bare marker address, never dereferenced.
    let sentinel = core::ptr::without_provenance_mut::<Registry>(SENTINEL_INITIALIZING);

    // Step 1: only proceed if the registry is still UNINIT (null). If it is
    // already real (or, impossibly under the serial guard, mid-init), do
    // not touch it -- leave any live registry completely undisturbed.
    REGISTRY_PTR
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .ok()?;

    // Step 2: run the EXACT rollback the production OOM-bailout runs.
    rollback_registry_sentinel();

    // Step 3: prove the anti-livelock postcondition -- a fresh CAS(null,
    // SENTINEL) must now succeed, meaning a real bootstrap winner (or any
    // future `ensure_slow` caller) would NOT spin forever on a stuck
    // sentinel.
    let postcondition_holds = REGISTRY_PTR
        .compare_exchange(
            core::ptr::null_mut(),
            sentinel,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_ok();

    // Step 4: restore REGISTRY_PTR to null, exactly as observed on entry,
    // regardless of the postcondition outcome, so the process is left
    // exactly as this hook found it (no live registry was ever touched --
    // we only ever entered this function with REGISTRY_PTR == null).
    REGISTRY_PTR.store(core::ptr::null_mut(), Ordering::Release);

    Some(postcondition_holds)
}

/// The current high-water `count` (test introspection). Each test claims
/// fresh slots; because `count` is monotonic across the suite (we never
/// reset the slot array — that would leak the lazily-materialised
/// `HeapCore`s), a test derives its expected slot indices relative to the
/// count it observed at entry.
pub fn count_for_test() -> u32 {
    ensure().count.load(Ordering::Acquire)
}
