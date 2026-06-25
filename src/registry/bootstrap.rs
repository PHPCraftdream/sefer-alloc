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
//! an atomic state-machine — but stores its slot array in a process-global
//! `static` rather than inside a segment.
//!
//! Why a `static` (not a primordial segment)?
//! - The slot array holds Rust values with drop glue and atomic fields
//!   (`AtomicU8`, `AtomicU32`, `UnsafeCell<MaybeUninit<HeapCore>>`). Laying
//!   those down via raw `Node::write_struct` byte writes (as the segment
//!   substrate does for its plain-data `SegmentHeader`) is unsound for types
//!   with non-trivial validity invariants. A `static` initialiser constructs
//!   them properly.
//! - The `static` is still **M5-clean**: it is a linker-provided `.bss`
//!   slot, allocated by the loader, NOT by `std::alloc`/`Vec`/`Box`. The
//!   registry path can never recurse into the global allocator via its
//!   storage — same property the segment path provides, achieved differently.
//! - The slot array is `pub(crate)` and lives for the process lifetime
//!   (never dropped), matching the "the primordial segment is never freed"
//!   invariant the segment substrate relies on.
//!
//! ## The state-machine
//!
//! `AtomicU8` `STATE_UNINIT → STATE_INITIALIZING → STATE_READY`:
//!
//! 1. The first caller of [`ensure`] observes `UNINIT` and CASes to
//!    `INITIALIZING`. The winner constructs the `Registry` value in place
//!    (a `const`-initialisable array — no allocation), then publishes `READY`
//!    with `Release`.
//! 2. Concurrent losers observe `INITIALIZING` (or `UNINIT` then fail the
//!    CAS) and `spin_loop` until they observe `READY` (loaded `Acquire`).
//! 3. After `READY`, every subsequent call is a single `Acquire` load +
//!    return — branch-light, allocation-free.
//!
//! `Release`/`Acquire` pair on the UNINIT→READY transition establishes
//! happens-before from the initialising thread's writes (the slot array
//! fields) to every reader that observes `READY`, so readers see a fully
//! constructed registry.
//!
//! ## This file is PURE SAFE COMPOSITION
//!
//! No `unsafe`: the `static` is constructed by a `const fn`, atomics are
//! safe to use, and `spin_loop` is a safe intrinsic. The `unsafe` (`Sync`
//! impl on `HeapSlot`, the `*mut HeapCore` handout) lives in
//! [`heap_slot`](super::heap_slot) and [`heap_registry`](super::heap_registry)
//! respectively.

use core::hint::spin_loop;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::heap_slot::HeapSlot;
use super::tagged_ptr::TaggedPtr;

/// Maximum number of heaps the registry can hold. Each live thread claims one
/// slot for its heap; `recycle` returns it. 4096 is generous for realistic
/// thread counts (a process with > 4096 simultaneous threads is pathological
/// for an allocator; the cap can be raised if a measured workload needs it).
/// At ~64 B per slot this is ~256 KiB of `.bss` — negligible.
pub const MAX_HEAPS: usize = 4096;

/// Bootstrap-state values stored in the init atomic.
const STATE_UNINIT: u8 = 0;
const STATE_INITIALIZING: u8 = 1;
const STATE_READY: u8 = 2;

/// The bootstrap outcome: the fixed slot array plus the dynamic atomics that
/// drive `claim`/`recycle`/`abandon`. Lives in a process-global `static`
/// (allocated by the loader in `.bss`, never by `std::alloc`), initialised
/// once via [`ensure`].
pub struct Registry {
    /// The fixed slot array. Indexed by slot id; `MAX_HEAPS` entries. Lives
    /// for the process lifetime (the `static` is never dropped).
    pub slots: [HeapSlot; MAX_HEAPS],
    /// High-water mark of allocated slots (the next unused slot index). A
    /// `claim` that finds `free_slots` empty `fetch_add`s this to mint a new
    /// slot. Capped at `MAX_HEAPS`.
    pub count: AtomicU32,
    /// Tagged-Treiber head of the `free_slots` stack: low 32 = slot index,
    /// high 32 = tag (bumped per push). Initialised empty.
    pub free_slots: AtomicU64,
    /// Tagged-Treiber head of the `abandoned_segs` stack: low 32 = segment
    /// base, high 32 = tag. Initialised empty.
    pub abandoned_segs: AtomicU64,
}

impl Registry {
    /// Construct the registry in its bootstrap state: every slot `FREE`,
    /// generation 0, `next_free = NEXT_FREE_TAIL`, heap uninitialised;
    /// `count = 0`, both stacks empty. `const` so it can initialise a
    /// `static` (no runtime code, no allocation — the loader zeros `.bss`).
    const fn new_zeroed() -> Self {
        Self {
            // `[expr; N]` for a non-`Copy` type requires a `const` block on
            // stable Rust (1.88). `HeapSlot::new_uninit` is `const fn`, so the
            // block is a compile-time array literal — the loader materialises
            // it in `.bss`.
            slots: [const { HeapSlot::new_uninit() }; MAX_HEAPS],
            count: AtomicU32::new(0),
            free_slots: AtomicU64::new(TaggedPtr::empty()),
            abandoned_segs: AtomicU64::new(TaggedPtr::empty()),
        }
    }
}

/// The process-global registry. Allocated by the loader in `.bss` (NOT by
/// `std::alloc` — M5-clean). Constructed once via its `const` initialiser,
/// never dropped.
static REGISTRY: Registry = Registry::new_zeroed();

/// The bootstrap state-machine word: `UNINIT → INITIALIZING → READY`.
static INIT_STATE: AtomicU8 = AtomicU8::new(STATE_UNINIT);

/// Ensure the registry is initialised, then return a `&'static` reference to
/// it. The first call performs the (allocation-free) construction
/// publication; concurrent and later calls observe `READY` and return
/// immediately.
///
/// This is the analogue of `AllocCore::new`'s call to
/// `bootstrap::primordial()`: a one-shot, OS-allocation-free init guarded by
/// an atomic state-machine. Unlike `std::sync::Once`, it touches NO
/// `std::alloc` path (Once's internal `Mutex` may allocate on some platforms).
pub fn ensure() -> &'static Registry {
    // Fast path: already READY. Acquire to see the registry's constructed
    // state (the initialising thread's Release store).
    if INIT_STATE.load(Ordering::Acquire) == STATE_READY {
        // SAFETY: we observed READY under Acquire, which synchronises with the
        // initialiser's Release store of READY. The `REGISTRY` static is
        // fully constructed (its `const` initialiser ran at load time) and
        // lives for the process lifetime; returning a `&'static` reference is
        // sound.
        return &REGISTRY;
    }
    // Slow path: race to initialise.
    let won = INIT_STATE.compare_exchange(
        STATE_UNINIT,
        STATE_INITIALIZING,
        // Acquire on success: pairs with our later Release store of READY so
        // later Acquire readers see the constructed state.
        Ordering::Acquire,
        // Relaxed on failure: we re-load below; no side-effect on failure.
        Ordering::Relaxed,
    ) == Ok(STATE_UNINIT);
    if won {
        // We are the sole initialiser. `REGISTRY` is already constructed by
        // its `const` initialiser (the loader placed it in `.bss`); there is
        // nothing to write except to publish READY. (Future hook: if the
        // registry ever needs runtime init — e.g. seeding `free_slots` with
        // the initial slot chain — it goes here, before the Release store.)
        INIT_STATE.store(STATE_READY, Ordering::Release);
    } else {
        // We lost the race. Spin until the winner publishes READY. `spin_loop`
        // emits a PAUSE/YIELD hint; the window is tiny (the initialiser does
        // no allocation, so READY follows within microseconds).
        while INIT_STATE.load(Ordering::Acquire) != STATE_READY {
            spin_loop();
        }
    }
    // SAFETY: as above — READY observed under Acquire synchronises with the
    // initialiser's Release store.
    &REGISTRY
}

/// Reset the bootstrap state back to `UNINIT`. **Test-only**: lets
/// `registry_basic` exercise the bootstrap idempotently (re-run `ensure` and
/// assert it does not re-initialise). Production code NEVER calls this.
pub fn reset_for_test() {
    // Safe ONLY because the tests are single-threaded and the registry is
    // otherwise quiescent when called. Lets a test observe the
    // UNINIT→READY transition more than once across the suite.
    INIT_STATE.store(STATE_UNINIT, Ordering::Release);
}

/// The current high-water `count` (test introspection). Each test claims
/// fresh slots; because `count` is monotonic across the suite (we never
/// reset the slot array — that would leak the lazily-materialised
/// `HeapCore`s), a test derives its expected slot indices relative to the
/// count it observed at entry.
pub fn count_for_test() -> u32 {
    REGISTRY.count.load(Ordering::Acquire)
}
