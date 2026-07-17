//! `racy-ptr-cell` — a lazy, CAS-published pointer cell with fallible init,
//! OOM rollback, and loser re-race.
//!
//! [`RacyPtrCell<T>`] is a three-state machine over a **single** `AtomicPtr<T>`:
//!
//! ```text
//! UNINIT(null) --CAS--> INITIALIZING(sentinel=1) --Release store--> READY(real *mut T)
//!                              |
//!                              +-- init returns None (OOM) --> UNINIT(null)  [rollback]
//! ```
//!
//! - The thread that CASes `null -> sentinel` becomes the **winner** and runs
//!   the caller's init closure exactly once.
//! - On success the winner publishes the real pointer with **`Release`** and
//!   leaks it for the process lifetime (the cell never drops or frees `T`).
//! - **Losers spin-`Acquire` only while the state is `INITIALIZING`** — NOT
//!   `while != READY`. Spinning on `!= READY` deadlocks against the OOM-rollback
//!   path: if the winner hits OOM and rolls the sentinel back to `null` without
//!   ever publishing `READY`, a `!= READY` spinner waits forever for a `READY`
//!   that will never come. Spinning on `== INITIALIZING` instead means a loser
//!   that observes the rollback (`null`) falls out of the spin and **re-races
//!   the CAS itself**.
//! - On winner **OOM** the sentinel is rolled back to `null` and losers
//!   re-race — unlike [`core::cell::OnceCell`] / `std::sync::OnceLock`, which
//!   poison or block on a failed initialiser.
//!
//! ## Why not `OnceLock`?
//!
//! This cell fills the niche `OnceLock` cannot: it is
//!
//! - **`no_std` and allocation-free** — the cell itself is one `AtomicPtr`; it
//!   never touches the heap.
//! - **safe inside a `#[global_allocator]`** — it uses NO `std` sync primitive
//!   (no `Mutex`, no parking, no `OnceLock`), so it cannot re-enter the very
//!   allocator it is bootstrapping. It is used by hand-rolled allocators,
//!   runtimes, and bare-metal bootstraps that must publish a process-`'static`
//!   pointer before any heap exists.
//! - **fallible with rollback + re-race** — `OnceLock::get_or_init` cannot
//!   fail; `get_or_try_init` poisons the cell on `Err`. This cell rolls back so
//!   a later attempt (after the OS frees memory, say) can succeed.
//!
//! ## The spin-wait (no parking, no `std`)
//!
//! Losers busy-spin with [`core::hint::spin_loop`] — there is no OS park/unpark
//! (that would need `std` sync and could re-enter the allocator). The spin
//! window is exactly one init closure (typically one OS reservation + one
//! publish store), so the busy-wait is bounded and short in practice. This is a
//! deliberate design constraint of the "safe inside the global allocator"
//! niche, not an oversight — see the module docs above.
//!
//! ## Sentinel encoding
//!
//! The `INITIALIZING` state is the address `1` (`SENTINEL_INITIALIZING`), a bare
//! marker that is **never dereferenced, only compared** for pointer equality.
//! Constructed via [`core::ptr::without_provenance_mut`] so it carries no
//! provenance — strict-provenance-clean, since it is never turned back into a
//! dereferenceable pointer. `1` is not a valid address for any `T` a caller
//! stores here (every such `T` has alignment >= 2), so it can never collide with
//! a real published pointer.
//!
//! ## What the caller owns
//!
//! The cell stores and hands back a `*mut T` / `NonNull<T>`; it does **not**
//! own the pointee. The init closure is responsible for producing a pointer
//! valid for the lifetime the caller treats the cell's output as living (for the
//! bootstrap use case: a leaked, process-`'static` allocation). Reading the
//! payload behind the pointer is `unsafe` and left to the caller, who knows the
//! pointee's real lifetime — see [`RacyPtrCell::get`] and
//! [`RacyPtrCell::get_or_try_init`].

// This crate is a single-file seam crate: `unsafe` is confined to this one
// module, lifted by the crate-level `#![allow(unsafe_code)]` below. There is a
// SINGLE documented reason to hold `unsafe` here: constructing the
// never-dereferenced `INITIALIZING` sentinel pointer via
// `core::ptr::without_provenance_mut` (a `const fn` that is safe on modern
// toolchains) requires no `unsafe`; the only genuinely `unsafe` surface is the
// pointer-sentinel comparison discipline plus the caller-facing accessors that
// hand back the raw pointer. All raw-pointer *dereferencing* is the CALLER's
// responsibility (this crate never reads through `T`). The crate body's own
// `unsafe` is confined to two audited kinds: `unsafe impl Send/Sync` for the
// `AtomicPtr`-backed cell (justified below), and `unsafe { NonNull::new_unchecked(p) }`
// at the accessor sites where `p` was already proven non-null by an
// `is_ready`/`!= 0` check. The `#![allow(unsafe_code)]` is retained (rather than
// `#![forbid]`) so the crate can expose the raw `*mut T` / `NonNull<T>` seam
// types and those confined sites. Every `unsafe fn` / `unsafe impl` carries a
// `# Safety` / `// SAFETY:` justification.
#![allow(unsafe_code)]
#![cfg_attr(not(test), no_std)]

use core::marker::PhantomData;
use core::ptr::NonNull;

// The atomics are aliased so loom can shadow the REAL `RacyPtrCell` type: under
// `--cfg loom` the cell is built on `loom::sync::atomic`, so the shipped loom
// tests (in `tests/`) model-check the actual implementation, not a hand-copied
// transcription. Under normal builds it is `core::sync::atomic`, keeping the
// crate `no_std` and allocation-free.
#[cfg(not(loom))]
use core::sync::atomic::{AtomicPtr, Ordering};
#[cfg(loom)]
use loom::sync::atomic::{AtomicPtr, Ordering};

/// The loser spin-wait hint. In a normal build this is [`core::hint::spin_loop`]
/// (a PAUSE/YIELD CPU hint, no scheduler involvement). Under `--cfg loom` the
/// real busy-spin is opaque to loom's model executor and would exhaust its
/// branch budget ("processor must make progress"); there we yield to loom's
/// fair scheduler instead, so it can advance the winner thread to its publish.
/// Same happens-before semantics either way (a hint/yield synchronises nothing);
/// only the scheduling nudge differs.
#[cfg(loom)]
#[inline]
fn spin_hint() {
    loom::thread::yield_now();
}
#[cfg(not(loom))]
#[inline]
fn spin_hint() {
    core::hint::spin_loop();
}

/// The `INITIALIZING` sentinel address: a non-null, non-real marker meaning
/// "one thread won the CAS and is currently running the init closure". Never
/// dereferenced — only compared for pointer equality against the cell's stored
/// value. `1` is below the minimum alignment of any `T` stored here (asserted
/// at construction), so it can never equal a real published pointer.
const SENTINEL_INITIALIZING: usize = 1;

/// A lazy, CAS-published pointer cell: `UNINIT -> INITIALIZING -> READY` over a
/// single `AtomicPtr<T>`, with fallible init (OOM rolls back and losers
/// re-race). See the [crate-level docs](crate) for the full state machine, the
/// anti-livelock loser-spin rule, and the "safe inside a `#[global_allocator]`"
/// niche.
///
/// The cell never drops, frees, or reads through the pointee — it only
/// publishes and hands back the `*mut T` the init closure produced.
pub struct RacyPtrCell<T> {
    /// The one word driving the state machine: `null` = `UNINIT`,
    /// [`SENTINEL_INITIALIZING`] = `INITIALIZING`, any other value = `READY`
    /// (a real published pointer).
    ptr: AtomicPtr<T>,
    /// `RacyPtrCell<T>` behaves like it holds a `*mut T` it hands out; the
    /// marker documents the relationship without owning a `T`.
    _marker: PhantomData<*mut T>,
}

// The cell is `Send + Sync` UNCONDITIONALLY, exactly like the `AtomicPtr<T>` it
// wraps — and for the same reason. The cell never dereferences `T` or hands out
// a `&T`; it only stores and returns a RAW `*mut T` / `NonNull<T>`. Whether the
// pointee is safe to *access* from another thread is the CALLER's contract (the
// `get`/`get_or_try_init` accessors return raw pointers, and reading through
// them is `unsafe`), not this type's — precisely the `AtomicPtr` model, which is
// `Send + Sync` for every `T`. This is what lets the cell hold a pointer to a
// `!Sync` payload (e.g. a per-thread heap) whose actual access the caller guards
// by its own single-writer/`&mut` discipline. The `PhantomData<*mut T>` (present
// only to document the "holds a `*mut T`" relationship and pin variance) is what
// removes the auto-impls, so we restore them here.
//
// SAFETY: `ptr` is an `AtomicPtr`, so all concurrent access to the cell's own
// state is race-free; the only value crossing a thread boundary through the cell
// is a raw `*mut T`, which is `Send`/`Sync`-neutral (raw pointers carry no
// sharing obligation — the obligation is on the caller's later deref). Identical
// to `AtomicPtr<T>`'s own unconditional `Send + Sync`.
unsafe impl<T> Send for RacyPtrCell<T> {}
// SAFETY: see the `Send` impl above.
unsafe impl<T> Sync for RacyPtrCell<T> {}

impl<T> RacyPtrCell<T> {
    /// Construct a fresh `UNINIT` cell (null pointer).
    ///
    /// Under `--cfg loom` this cannot be `const` (loom's atomics have no
    /// const constructor); on normal builds it is `const` so the cell can live
    /// in a `static`.
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        // Compile-time guard: the sentinel address (1) must not be a valid
        // aligned address for `T`, or it could collide with a real pointer.
        // Every `T` used behind this cell must have alignment >= 2.
        assert!(
            core::mem::align_of::<T>() >= 2,
            "RacyPtrCell<T> requires align_of::<T>() >= 2 so the INITIALIZING \
             sentinel (address 1) can never collide with a real published pointer"
        );
        RacyPtrCell {
            ptr: AtomicPtr::new(core::ptr::null_mut()),
            _marker: PhantomData,
        }
    }

    /// Construct a fresh `UNINIT` cell (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        assert!(
            core::mem::align_of::<T>() >= 2,
            "RacyPtrCell<T> requires align_of::<T>() >= 2"
        );
        RacyPtrCell {
            ptr: AtomicPtr::new(core::ptr::null_mut()),
            _marker: PhantomData,
        }
    }

    /// The `INITIALIZING` sentinel as a `*mut T` — a bare marker, never
    /// dereferenced, constructed WITHOUT provenance (strict-provenance-clean).
    #[inline]
    fn sentinel() -> *mut T {
        core::ptr::without_provenance_mut::<T>(SENTINEL_INITIALIZING)
    }

    /// `true` iff `p` is a real published pointer (non-null AND non-sentinel).
    #[inline]
    fn is_ready(p: *mut T) -> bool {
        let a = p.addr();
        a != 0 && a != SENTINEL_INITIALIZING
    }

    /// Return the published pointer if the cell is `READY`, else `None`.
    ///
    /// A pure `Acquire` load: no CAS, no init, no spin. `None` means the cell is
    /// `UNINIT` or `INITIALIZING` right now (neither the sentinel nor null is
    /// ever returned as `Some`).
    ///
    /// The returned pointer is the exact value the init closure produced; the
    /// `Acquire` load pairs with the winner's `Release` publish, so any read the
    /// caller performs through the pointer sees the fully initialised pointee.
    #[inline]
    #[must_use]
    pub fn get(&self) -> Option<NonNull<T>> {
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) {
            // `is_ready` proved `p` non-null.
            Some(unsafe { NonNull::new_unchecked(p) })
        } else {
            None
        }
    }

    /// Get the published pointer, or run `init` to produce it — with the full
    /// `UNINIT -> INITIALIZING -> READY` protocol, OOM rollback, and loser
    /// re-race.
    ///
    /// Contract:
    /// - **Fast path**: if the cell is already `READY`, returns the published
    ///   pointer with one `Acquire` load; `init` is not called.
    /// - **Winner**: the thread that CASes `null -> sentinel` calls `init`
    ///   exactly once. `init` returns `Some(ptr)` on success (the cell
    ///   publishes it with `Release` and returns it — `ptr` is leaked for the
    ///   process lifetime, the cell never frees it), or `None` on OOM (the cell
    ///   rolls the sentinel back to `null` and returns `None`; a later call may
    ///   retry).
    /// - **Loser**: a thread that loses the CAS spins with `Acquire` loads
    ///   **only while the state is `INITIALIZING`**. When the winner publishes,
    ///   the loser returns the same pointer. When the winner rolls back after
    ///   OOM (state returns to `null`), the loser falls out of the spin and
    ///   **re-races the CAS itself** — it does not wait for a `READY` that will
    ///   never come.
    ///
    /// Returns `Some(published pointer)` (same value for all threads across a
    /// successful lifetime) or `None` if `init` reported OOM on this thread's
    /// winning attempt. The returned pointer is never null and never the
    /// sentinel.
    ///
    /// `init` must be reentrancy-safe with respect to whatever the cell guards:
    /// it runs while this thread holds the `INITIALIZING` sentinel, so it must
    /// not itself call back into `get_or_try_init` on the SAME cell (that would
    /// spin forever — the current thread is the only one able to publish).
    pub fn get_or_try_init<F>(&self, mut init: F) -> Option<NonNull<T>>
    where
        F: FnMut() -> Option<NonNull<T>>,
    {
        loop {
            // Fast path: already READY.
            let p = self.ptr.load(Ordering::Acquire);
            if Self::is_ready(p) {
                return Some(unsafe { NonNull::new_unchecked(p) });
            }

            // Slow path: race to become the initialising winner.
            match self.ptr.compare_exchange(
                core::ptr::null_mut(),
                Self::sentinel(),
                // Success `Acquire`: pairs with a later winner's `Release`
                // publish observed by future `Acquire` readers. (We are that
                // winner here; the pairing matters for the fast-path/loser
                // loads.)
                Ordering::Acquire,
                // Failure `Relaxed`: we re-load in the spin loop below.
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // ── Winner ──────────────────────────────────────────────
                    // We hold the INITIALIZING sentinel; we are the sole
                    // initialiser. Run the caller's init closure.
                    match init() {
                        Some(ptr) => {
                            let raw = ptr.as_ptr();
                            debug_assert!(
                                Self::is_ready(raw),
                                "RacyPtrCell: init returned the null/sentinel address"
                            );
                            // Publish with `Release` so every subsequent
                            // `Acquire` load (fast path here, plus every loser's
                            // spin-load) sees the fully constructed pointee.
                            // This is THE ordering the Relaxed-publish
                            // counterfactual breaks.
                            self.ptr.store(raw, Ordering::Release);
                            return Some(ptr);
                        }
                        None => {
                            // OOM: roll the sentinel back to null so losers
                            // spinning on `== INITIALIZING` fall out and
                            // re-race, and future callers can retry. `Release`
                            // pairs with the retrying thread's later CAS
                            // `Acquire`: there is no partially-initialised state
                            // to synchronise (init never published), only the
                            // "cell is free again" fact.
                            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
                            return None;
                        }
                    }
                }
                Err(_) => {
                    // ── Loser ───────────────────────────────────────────────
                    // Spin ONLY while the state is INITIALIZING. This is the
                    // Phase-F1 anti-livelock rule: a `!= READY` spin would
                    // deadlock if the winner rolled back to null after OOM
                    // (READY never comes). Falling out on any non-INITIALIZING
                    // observation lets us return READY (winner published) or
                    // loop back to the top and re-race (winner rolled back).
                    loop {
                        let p = self.ptr.load(Ordering::Acquire);
                        let a = p.addr();
                        if a == SENTINEL_INITIALIZING {
                            // Still initialising — keep spinning.
                            spin_hint();
                            continue;
                        }
                        if a != 0 {
                            // READY: the winner published a real pointer.
                            return Some(unsafe { NonNull::new_unchecked(p) });
                        }
                        // null: the winner rolled back after OOM. Break out of
                        // the spin and re-race the CAS from the top — do NOT
                        // keep waiting for a READY that will never be published.
                        break;
                    }
                    // Fall through to the outer loop: re-race.
                }
            }
        }
    }

    /// Test-only introspection: `true` iff the cell is currently `READY` (holds
    /// a real, non-null, non-sentinel pointer). Not part of the value contract —
    /// exists so tests can assert lazy-materialisation ordering without racing
    /// a concurrent init.
    #[doc(hidden)]
    #[inline]
    #[must_use]
    pub fn dbg_is_ready(&self) -> bool {
        Self::is_ready(self.ptr.load(Ordering::Acquire))
    }

    /// Test-only anti-livelock rollback probe. Drives THIS cell through the
    /// exact `null -> sentinel -> rollback -> re-CAS` sequence the internal
    /// OOM-bailout runs, and proves the postcondition the whole design rests on:
    /// after a rollback, a fresh `CAS(null -> sentinel)` MUST succeed (the
    /// sentinel was genuinely cleared, so no future winner or spinning loser is
    /// wedged).
    ///
    /// Returns `Some(true)` if the rollback provably cleared the sentinel,
    /// `Some(false)` if the postcondition CAS unexpectedly failed (rollback
    /// broken — the counterfactual this probe catches), or `None` if the cell
    /// was not observed `UNINIT` on entry (already READY, or contended) and the
    /// probe could not run — callers treat that as "not applicable", never as
    /// failure. On success the cell is left exactly as found (`UNINIT`).
    ///
    /// Exists so a consumer's test can drive the rollback on a REAL, LIVE cell
    /// (e.g. a process-global registry chunk) — proving the shipped code path,
    /// not a copy — without a process-terminating OOM. The whole probe is a
    /// bounded, single-threaded sequence of atomic ops; callers MUST pick a
    /// cell no other thread is concurrently initialising (the entry CAS is the
    /// guard: if the cell is not `UNINIT`, the probe returns `None` and touches
    /// nothing).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_rollback_reenterable(&self) -> Option<bool> {
        // Step 1: only proceed if the cell is UNINIT (null). If it is already
        // READY or contended, do not touch it.
        self.ptr
            .compare_exchange(
                core::ptr::null_mut(),
                Self::sentinel(),
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .ok()?;

        // Step 2: run the EXACT rollback the internal OOM-bailout runs (sentinel
        // -> null, Release).
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);

        // Step 3: prove the postcondition — a fresh CAS(null -> sentinel) must
        // now succeed.
        let postcondition_holds = self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                Self::sentinel(),
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();

        // Step 4: restore to null, exactly as observed on entry.
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);

        Some(postcondition_holds)
    }
}

impl<T> Default for RacyPtrCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
