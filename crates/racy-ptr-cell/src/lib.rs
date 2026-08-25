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
//!   re-race the CAS themselves, rather than being parked and woken.
//!
//! ## Why not `OnceLock`?
//!
//! This cell fills the niche `OnceLock` cannot: it is
//!
//! - **`no_std` and allocation-free** — the cell itself is one `AtomicPtr`; it
//!   never touches the heap.
//! - **usable inside a `#[global_allocator]`** — the cell's own non-panicking
//!   operations use NO `std` sync primitive (no `Mutex`, no parking, no
//!   `OnceLock`) and allocate nothing, so the cell itself cannot re-enter the
//!   allocator being bootstrapped. That is a property of the CELL, not of a
//!   whole `get_or_try_init` call: the caller's `init` closure runs inside
//!   that call and carries hard obligations of its own — see
//!   ["Using this inside a `#[global_allocator]`"](#using-this-inside-a-global_allocator)
//!   below. Used by hand-rolled allocators, runtimes, and bare-metal
//!   bootstraps that must publish a process-`'static` pointer before any heap
//!   exists.
//! - **fallible without parking** — `OnceLock::get_or_init` cannot fail at
//!   all, and its `get_or_try_init` is still unstable (`once_cell_try`). Both
//!   also PARK the losing threads for the duration of the winner's init. This
//!   cell reports failure as a plain `None` per caller and lets losers re-race
//!   the CAS with no OS involvement, so a later attempt (after the OS frees
//!   memory, say) can succeed without a blocking primitive anywhere.
//!
//!   To be precise about what `OnceLock` does and does not do, since this
//!   crate's earlier docs got it wrong: `OnceLock::get_or_try_init` does NOT
//!   poison the cell on `Err`, and a failed or panicking initialiser leaves it
//!   uninitialised and retryable (`std` drives it through
//!   `Once::call_once_force`, which deliberately ignores poisoning). The real
//!   distinctions are the ones above — `no_std`, no parking, no internal
//!   allocation, and a raw-pointer/lifetime posture — not recoverability.
//!
//! ## The spin-wait (no parking, no `std`)
//!
//! Losers busy-spin with [`core::hint::spin_loop`] — there is no OS park/unpark
//! (that would need `std` sync and could re-enter the allocator). **There is
//! no bounded-latency guarantee**: a loser waits for exactly as long as the
//! winner's `init` closure takes — **provided a winner is currently running
//! at all**. `init` is arbitrary caller code — a closure that blocks on a
//! syscall, gets preempted, or simply runs long makes every loser wait that
//! long too. The intended usage (typically one OS reservation + one publish
//! store) keeps the spin short in practice, but that is a caller obligation,
//! not something this cell enforces: **`init` must be fast and
//! non-blocking**, on top of the re-entry restriction below. A cell whose
//! `INITIALIZING` owner has stopped running (see "Fork and signal safety"
//! below) is waited on forever, not merely for a long time. This is a
//! deliberate design constraint of the "usable inside the global allocator"
//! niche, not an oversight — see the module docs above.
//!
//! ## Using this inside a `#[global_allocator]`
//!
//! The cell is built for this niche, but the niche has hard rules that are the
//! CALLER's to keep — the cell can enforce none of them:
//!
//! - **`init` must not allocate**, directly or transitively, and must not
//!   otherwise re-enter the allocator being bootstrapped. `init` runs while
//!   this thread holds the `INITIALIZING` sentinel; an allocation from inside
//!   it re-enters an allocator whose own bootstrap is mid-flight.
//! - **`init` must not block** — every loser thread spins for exactly as long
//!   as `init` runs (see "The spin-wait" above).
//! - **`init` must not wait on another cell that can wait back** — the
//!   re-entry restriction is transitive, and several cells form a lock-order
//!   graph. An allocator bootstrap is exactly the shape that produces this
//!   (many per-chunk cells plus a sidecar path); see
//!   [`RacyPtrCell::get_or_try_init`]'s own docs for the two-cell deadlock.
//! - **`init` must not panic, and no panic may unwind through a `GlobalAlloc`
//!   method** — [unwinding out of a global allocator is undefined
//!   behaviour][ga]. This crate's rollback guard keeps the CELL consistent
//!   across an unwinding `init` (the sentinel is rolled back, not left wedged),
//!   but it cannot make the unwind itself sound once the frame below is
//!   `GlobalAlloc::alloc`.
//! - **An `init` that returns the sentinel address is a caller bug, not a
//!   recoverable error.** The release-active `assert!` documented under
//!   [`RacyPtrCell::get_or_try_init`]'s `# Panics` exists to make that bug
//!   loud — it is a violated precondition, not a condition an allocator is
//!   expected to survive.
//!
//! ### Fork and signal safety
//!
//! The rules above are all about what `init` *does*; two further hazards
//! break the cell from **outside** `init`, with no misbehaving closure
//! anywhere. **The cell is neither fork-safe nor async-signal-safe.**
//! `INITIALIZING` is owned by a specific thread:
//!
//! - **`fork()` in a multithreaded process.** If one thread holds the
//!   sentinel (running `init`) when another thread calls `fork()`, the child
//!   process inherits a cell that reads `INITIALIZING` but has no thread that
//!   can ever publish or roll it back — every subsequent caller in the child
//!   spins forever. There is no reset API: `dbg_rollback_reenterable`'s entry
//!   CAS requires the cell to already be `null` and is a no-op on a
//!   sentinel-holding cell, by design.
//! - **An allocating signal handler.** If a signal is delivered to the thread
//!   that holds the sentinel and the handler allocates (directly, or
//!   transitively — a `format!`, a `Vec`, a panic-hook path), the allocator
//!   reaches the same cell from inside the handler, the claim CAS fails, and
//!   the handler spins on a sentinel owned by the very thread it interrupted:
//!   an unrecoverable single-thread self-deadlock.
//!
//! In a forking process, either publish every cell before the first `fork()`,
//! or `fork()` only from a thread you know holds no cell and treat `exec()`
//! in the child as mandatory. Do not allocate in a signal handler.
//!
//! There are **three** panic sites in total: that sentinel-collision
//! `assert!`, an unwinding `init`, and the `align_of::<T>() >= 2` check in
//! [`RacyPtrCell::new`] / [`RacyPtrCell::default`] (a const-eval failure in
//! the documented `static` form — which never reaches a panic runtime at
//! all; a runtime panic when reached from a non-const context). The two
//! that reach the panic runtime allocate in a `std` build, and the two link
//! environments need genuinely different mitigations, **not a shared
//! recipe**:
//!
//! - A `no_std` binary supplies a non-allocating `#[panic_handler]` itself.
//!   This closes the hazard completely: no handler, no allocation.
//! - A `std` binary's `panic = "abort"` profile setting removes the
//!   **unwind** (the UB when the frame below is `GlobalAlloc::alloc`), but it
//!   does **not** stop the panic runtime from allocating: the default hook
//!   allocates before it can print anything (measured: 2 allocations under
//!   `panic = "abort"`, `RUST_BACKTRACE=0`, `--release`, rustc 1.97,
//!   x86_64-pc-windows-msvc). Inside a `#[global_allocator]` that allocation
//!   re-enters the very cell that is mid-`init`, and the thread deadlocks on
//!   its own sentinel instead of aborting — the diagnostic the release-active
//!   `assert!` above exists to print never reaches stderr, because the
//!   allocation that would have printed it is the one that deadlocked. A
//!   `std` consumer therefore needs `panic = "abort"` **and** a
//!   `std::panic::set_hook` that goes straight to `std::process::abort`
//!   without formatting — or, better, an `init` that cannot panic at all.
//!   **Residual limit: a hook cannot help if the panic message is
//!   formatted.** `std` materialises the message (`payload.get()`) as an
//!   *argument* to the hook call, so `unwrap`/`expect`/`assert_eq!`/
//!   `panic!("{}", …)` allocate before any hook runs, whether or not the hook
//!   itself allocates — measured: 2 allocations for `Result::unwrap`, 4 for
//!   `assert_eq!`, with the same non-allocating hook that reaches 0 for a
//!   bare-`&'static str` panic. Only a panic whose message is a bare
//!   `&'static str` is fully covered by the `set_hook` mitigation. The
//!   crate's own two `assert!`s (the sentinel-collision check and the
//!   `align_of::<T>() >= 2` check) are of that shape and measure 0
//!   allocations before the hook; **the third panic site above — an
//!   unwinding `init` — is your code, and its message is whatever you
//!   wrote**, so it is covered only if you keep it a bare literal. **The
//!   only mitigation that covers a formatted message is an `init` that
//!   cannot panic at all.** Note also
//!   that `panic = "abort"` compiles the crate's internal rollback guard out
//!   entirely (it is unwind-only) — under this profile the cell-consistency
//!   guarantee below comes from the process dying, not from the guard.
//!
//! [ga]: https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html#safety
//!
//! ## Sentinel encoding
//!
//! The `INITIALIZING` state is the address `1` (`SENTINEL_INITIALIZING`), a bare
//! marker that is **never dereferenced, only compared** for pointer equality.
//! Constructed via [`core::ptr::without_provenance_mut`] so it carries no
//! provenance — strict-provenance-clean, since it is never turned back into a
//! dereferenceable pointer. An *aligned* pointer to `T` can never have address
//! `1` (`align_of::<T>() >= 2` is asserted at construction) — but a
//! *misaligned or synthesised* pointer at address `1` IS reachable from safe
//! code (an `init` closure can construct and return one). That case is
//! rejected by a release-active `assert!` in
//! [`RacyPtrCell::get_or_try_init`] — see its `# Panics`.
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
//!
//! ## Portability limit — requires pointer-width atomic CAS
//!
//! The whole cell is one `AtomicPtr<T>` driven by `compare_exchange`; that is
//! not an incidental implementation choice, it is the entire mechanism. This
//! crate therefore needs `target_has_atomic = "ptr"` and will **not compile**
//! on a target without it — notably `thumbv6m-none-eabi` (Cortex-M0/M0+),
//! `riscv32imc-unknown-none-elf` (no `A` extension), and `msp430-none-elf`.
//! This crate is `no_std` and allocation-free, but neither property implies
//! pointer-width CAS: those targets have load/store atomics but no
//! `compare_exchange`. A build on an unsupported target fails fast with an
//! explicit [`compile_error!`] naming the requirement, rather than the more
//! cryptic "no method named `compare_exchange`" error a bare unresolved
//! import would otherwise produce.

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
#![deny(missing_docs)]
#![cfg_attr(not(test), no_std)]

// The whole cell is one AtomicPtr driven by compare_exchange (see the
// crate-doc "Portability limit" section above) — that requires pointer-width
// atomic CAS from the target. Fail fast with an explicit, named reason
// instead of the cryptic "no method named `compare_exchange`" unresolved-item
// error a naive use would otherwise produce on e.g.
// thumbv6m-none-eabi/riscv32imc/msp430.
#[cfg(not(target_has_atomic = "ptr"))]
compile_error!(
    "racy-ptr-cell requires a target with pointer-width atomic \
     compare-and-swap (target_has_atomic = \"ptr\"): the whole cell is one \
     AtomicPtr driven by compare_exchange. Targets with load/store atomics \
     but no CAS -- thumbv6m-none-eabi (Cortex-M0/M0+), \
     riscv32imc-unknown-none-elf (no `A` extension), msp430-none-elf -- are \
     NOT supported, despite this crate being no_std and allocation-free."
);

use core::marker::PhantomData;
use core::ptr::NonNull;

// The atomics are aliased so loom can shadow the REAL `RacyPtrCell` type: under
// `--cfg loom` the cell is built on `loom::sync::atomic`, so the shipped loom
// tests (in `tests/`) model-check the actual implementation, not a hand-copied
// transcription. Under normal builds it is `core::sync::atomic`, keeping the
// crate `no_std` and allocation-free.
//
// CONSUMER HAZARD: `--cfg loom` is a global `RUSTFLAGS` cfg — it applies to
// every crate in the build, not only the one whose loom suite you meant to
// run. Under it `RacyPtrCell::new` is NOT `const` (see its doc), so a
// `static CELL: RacyPtrCell<T> = RacyPtrCell::new();` anywhere in the build
// fails to compile. Scope the flag (`cargo test -p <crate> ...`), or supply a
// `#[cfg(loom)]` const-capable stand-in in your own crate — see
// `src/registry/bootstrap.rs`'s `loom_shim` in the `sefer-alloc` repository
// this crate is extracted from, for a worked example.
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
/// value. An *aligned* pointer to `T` can never equal this address
/// (`align_of::<T>() >= 2` is asserted at construction); a *misaligned or
/// synthesised* pointer at this address is reachable from safe code and is
/// rejected by a release-active `assert!` in
/// [`RacyPtrCell::get_or_try_init`], not by this constant alone.
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

/// RAII rollback guard held across the init closure (task #706): if `init`
/// unwinds instead of returning, the winner thread's stack unwinds through
/// this guard's `Drop`, which stores `null` with `Release` — exactly the
/// same rollback the explicit OOM path performs. Without this, an unwinding
/// `init` leaves the `INITIALIZING` sentinel stuck forever: every concurrent
/// loser busy-spins at 100% CPU indefinitely (they spin on `==
/// INITIALIZING`, which never changes), and every future
/// `get_or_try_init`/`get` caller observes permanent `INITIALIZING` — a
/// silent whole-process livelock, and a strictly worse outcome than the
/// `OnceLock` equivalent, which leaves its cell uninitialised and lets the
/// next caller retry.
///
/// Defused (via [`RollbackGuard::defuse`]) on both non-unwinding exits — the
/// successful publish and the explicit `None`/OOM rollback — so the normal
/// paths are unaffected; this guard only ever fires on the unwind path.
///
/// Test coverage note (task #774, finding F10): `tests/cell_unit.rs`'s
/// `panicking_init_rolls_back_and_subsequent_call_succeeds` proves a
/// strictly weaker property than the one described above — that a
/// SUBSEQUENT call on an already-quiescent cell succeeds after a panicking
/// init unwound and rolled back. It does not model a loser thread that was
/// ALREADY spinning inside `get_or_try_init` at the moment the winner's
/// `init` unwinds. The two properties coincide for the guard's current
/// implementation (both reduce to "the cell left `INITIALIZING`"), so this
/// is a coverage gap, not a known hole in the fix — a future change that
/// made the rollback conditional (e.g. skipping it when a loser is
/// observed waiting) could pass the existing test while reintroducing the
/// spin. Not closed by a loom test here: loom's deterministic scheduling
/// model and `std::panic::catch_unwind` do not compose cleanly (loom needs
/// to replay every interleaving of an unwind path, which its own docs do
/// not treat as a first-class supported pattern), and the ROI did not
/// justify building a bespoke harness for it in this round.
struct RollbackGuard<'a, T> {
    ptr: &'a AtomicPtr<T>,
    defused: bool,
}

impl<'a, T> RollbackGuard<'a, T> {
    #[inline]
    fn new(ptr: &'a AtomicPtr<T>) -> Self {
        Self {
            ptr,
            defused: false,
        }
    }

    /// Disarm the guard: its `Drop` becomes a no-op. Call once the caller has
    /// itself handled the `INITIALIZING` state (published `READY`, or
    /// performed the explicit `None`/OOM rollback).
    #[inline]
    fn defuse(&mut self) {
        self.defused = true;
    }
}

impl<T> Drop for RollbackGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if !self.defused {
            // Same ordering rationale as the explicit OOM rollback in
            // `get_or_try_init`: `Release` pairs with the retrying thread's
            // later CAS `Acquire`; there is no partially-initialised state to
            // synchronise (init never published), only the "cell is free
            // again" fact.
            self.ptr.store(core::ptr::null_mut(), Ordering::Release);
        }
    }
}

impl<T> RacyPtrCell<T> {
    /// Construct a fresh `UNINIT` cell (null pointer).
    ///
    /// **Not `const` under `--cfg loom`** (loom's atomics have no const
    /// constructor); on normal builds it is `const` so the cell can live in a
    /// `static`. Because `--cfg loom` is a global `RUSTFLAGS` cfg, this
    /// applies to every crate in a build that sets it, not only crates that
    /// mean to run loom against `RacyPtrCell` itself — a
    /// `static CELL: RacyPtrCell<T> = RacyPtrCell::new();` anywhere in such a
    /// build fails to compile. Scope the flag to this crate
    /// (`cargo test -p racy-ptr-cell ...`), or supply your own
    /// `#[cfg(loom)]` const-capable stand-in if you need the flag
    /// workspace-wide.
    ///
    /// # Panics
    ///
    /// Panics if `align_of::<T>() == 1`. The `INITIALIZING` sentinel is encoded
    /// as the address `1` (see the crate-level "Sentinel encoding" docs); that
    /// encoding needs a spare low bit, which requires every valid aligned
    /// address of `T` to be even — i.e. `align_of::<T>() >= 2`. In the
    /// documented `static CELL: RacyPtrCell<T> = RacyPtrCell::new();` usage
    /// this `assert!` is evaluated at compile time (a const-eval failure, not
    /// a runtime panic); called from a non-const context (e.g. inside a
    /// function, or via `RacyPtrCell::<T>::default()`) with a `T` whose
    /// alignment is 1, it panics at runtime instead.
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
    ///
    /// # Panics
    ///
    /// Panics if `align_of::<T>() == 1` — see the non-loom [`RacyPtrCell::new`]
    /// doc above for why (identical condition; this build cannot be `const` so
    /// the check always runs at runtime here).
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
            // SAFETY: `is_ready(p)` just proved `p` is non-null (neither null
            // nor the sentinel).
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
    ///
    /// The restriction is **transitive, and multiple cells form a lock-order
    /// graph**: `init` must not wait, through any chain of calls, on a cell
    /// whose own initialiser can wait on this one. Two cells are enough for a
    /// deadlock with no direct self-recursion anywhere — thread 1 wins `A` and
    /// its `init` initialises `B`, while thread 2 wins `B` and its `init`
    /// initialises `A`; both spin forever at 100% CPU. Acquire multiple cells
    /// in a fixed global order, exactly as you would locks.
    ///
    /// `init` must also be fast and non-blocking: every loser thread spins for
    /// exactly as long as the winner's `init` call takes (see the module docs'
    /// "spin-wait" section) — there is no bounded-latency guarantee from the
    /// cell itself, only from the caller keeping `init` short.
    ///
    /// Calling this from inside a `#[global_allocator]` adds further hard
    /// obligations on `init` (no allocation, no unwind) — see the crate docs'
    /// ["Using this inside a `#[global_allocator]`"](crate#using-this-inside-a-global_allocator)
    /// section.
    ///
    /// # Panics
    ///
    /// Panics if the winning `init` call returns `Some(ptr)` where `ptr`'s
    /// address is the reserved `INITIALIZING` sentinel (`1`) — a safe `init`
    /// closure can construct and return this exact address, and publishing it
    /// unguarded would make every reader (this thread's own fast path
    /// included) misclassify the cell as still-initializing forever. This
    /// check is release-active, not `debug_assert!`-gated.
    ///
    /// If `init` itself panics (unwinds) instead of returning, the panic
    /// propagates out of `get_or_try_init` and the cell is left in `UNINIT`
    /// (not wedged in `INITIALIZING`) — a later call, on any thread, may
    /// retry `init`. This mirrors the OOM/`None` rollback above; the only
    /// difference is how the winner exits. Note what this does and does not
    /// buy: it keeps the CELL consistent, but it does not make the unwind
    /// itself sound when the frame below is a `GlobalAlloc` method, where
    /// unwinding is undefined behaviour regardless of this cell's state.
    pub fn get_or_try_init<F>(&self, mut init: F) -> Option<NonNull<T>>
    where
        F: FnMut() -> Option<NonNull<T>>,
    {
        loop {
            // Fast path: already READY.
            let p = self.ptr.load(Ordering::Acquire);
            if Self::is_ready(p) {
                // SAFETY: `is_ready(p)` just proved `p` is non-null (neither
                // the `null` UNINIT value nor the `SENTINEL_INITIALIZING`
                // marker), so `p` is a real published pointer.
                return Some(unsafe { NonNull::new_unchecked(p) });
            }

            // Slow path: race to become the initialising winner.
            match self.ptr.compare_exchange(
                core::ptr::null_mut(),
                Self::sentinel(),
                // Success `Acquire`: synchronises-with the `Release` rollback
                // store (OOM at :544, or the unwind guard's `Drop`) of a
                // PREVIOUS winner that abandoned the cell — the only prior
                // `Release` stores that can leave it `null`. It says nothing
                // about the publish this thread is about to perform: an
                // acquire cannot pair with a release that has not happened
                // yet. The load-bearing pair for the pointee is this winner's
                // own `Release` publish below against every reader's
                // `Acquire` load. Whether `Relaxed` would suffice here (a
                // rollback leaves no payload state to acquire) is an open
                // perf question, not settled by this comment.
                Ordering::Acquire,
                // Failure `Relaxed`: we re-load in the spin loop below.
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // ── Winner ──────────────────────────────────────────────
                    // We hold the INITIALIZING sentinel; we are the sole
                    // initialiser. Hold a rollback guard across `init()` so an
                    // UNWINDING init (a panic in caller code, or the `assert!`
                    // below firing) also rolls the sentinel back — see
                    // `RollbackGuard`'s own doc for why this is load-bearing
                    // (task #706).
                    let mut guard = RollbackGuard::new(&self.ptr);
                    match init() {
                        Some(ptr) => {
                            let raw = ptr.as_ptr();
                            // Release-active `assert!`, not `debug_assert!`
                            // (task #707): a SAFE init closure can construct
                            // `NonNull::new(without_provenance_mut(1))` and
                            // hand back the very SENTINEL address this cell
                            // uses to mean "still initialising". In release,
                            // a `debug_assert!` here compiles out, so the
                            // sentinel would get published as if it were
                            // READY — every current loser and every future
                            // caller then spins forever, since the published
                            // value reads back as `INITIALIZING`, not `READY`
                            // (`is_ready`'s own definition), with no
                            // diagnostic anywhere. Two integer compares on a
                            // once-per-cell cold path is a negligible cost
                            // for closing a violation of this method's own
                            // documented "never null, never the sentinel"
                            // guarantee that is reachable from 100% safe
                            // code — exactly the class `debug_assert!` is
                            // NOT meant for. If this fires, the rollback
                            // guard above (task #706) unwinds it cleanly.
                            assert!(
                                Self::is_ready(raw),
                                "RacyPtrCell: init returned the null/sentinel address"
                            );
                            // Publish with `Release` so every subsequent
                            // `Acquire` load (fast path here, plus every loser's
                            // spin-load) sees the fully constructed pointee.
                            // This is THE ordering the Relaxed-publish
                            // counterfactual breaks.
                            self.ptr.store(raw, Ordering::Release);
                            // Defuse: the guard's rollback must NOT fire now
                            // that the real pointer is published.
                            guard.defuse();
                            return Some(ptr);
                        }
                        None => {
                            // OOM: roll the sentinel back to null so losers
                            // spinning on `== INITIALIZING` fall out and
                            // re-race, and future callers can retry. `Release`
                            // pairs with the retrying thread's later CAS
                            // `Acquire`: there is no partially-initialised state
                            // to synchronise (init never published), only the
                            // "cell is free again" fact. Explicit here (rather
                            // than relying on the guard's Drop) to keep this
                            // path's ordering self-documenting; defuse first so
                            // the guard does not redundantly store again.
                            guard.defuse();
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
                            // SAFETY: `a != 0` (checked above) and `a !=
                            // SENTINEL_INITIALIZING` (the `if` above this one
                            // already returned/continued on that value), so
                            // `p` is neither null nor the sentinel — a real
                            // published pointer.
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

    /// Test-probe introspection: `true` iff the cell is currently `READY`
    /// (holds a real, non-null, non-sentinel pointer). Says nothing about
    /// the published *value* itself (that is [`RacyPtrCell::get`]'s
    /// contract).
    ///
    /// This is functionally identical to `get().is_some()` — same single
    /// `Acquire` load, same predicate, no capability `get` lacks — it does
    /// **not** avoid racing a concurrent init any differently than `get`
    /// does (task #774, finding F4 corrected an earlier doc claiming
    /// otherwise). It exists as a named, self-documenting boolean
    /// introspection primitive: a caller writing `assert!(cell.dbg_is_ready())`
    /// reads as "assert the cell materialised" without an
    /// `.is_some()`/`.is_none()` match at the call site. A real in-repo
    /// consumer already relies on exactly this: the root `sefer-alloc`
    /// crate's `Registry::dbg_chunk_is_materialised`
    /// (`src/registry/bootstrap.rs`) forwards to this method to assert
    /// chunk-materialisation state in its own regression tests.
    ///
    /// # Stability
    ///
    /// This is a deliberate, STABLE part of the public API (task #710) — a
    /// `dbg_`-prefixed test-probe surface, not a hidden implementation
    /// detail. It carries the crate's normal semver guarantee like any
    /// other public item; a `#[doc(hidden)]` posture was rejected precisely
    /// because it would advertise this function to downstream consumers'
    /// tests (see [`RacyPtrCell::dbg_rollback_reenterable`]'s own doc) while
    /// hiding it from the rustdoc those consumers would need to discover it
    /// — see the crate README's "Test-probe API stability" section for the
    /// full rationale.
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
    /// Returns `Some(true)` if the rollback provably cleared the sentinel
    /// (the postcondition CAS re-won the cell; it is restored to `UNINIT`
    /// before returning). `None` covers TWO distinct "could not test"
    /// cases, deliberately conflated because neither is evidence rollback is
    /// broken: (a) the cell was not observed `UNINIT` on the entry CAS
    /// (already `READY`, or another thread owned it at that instant), or
    /// (b) the postcondition CAS in step 3 failed because a real
    /// `get_or_try_init` caller raced in and re-won the cell during the
    /// probe's own rollback-then-reCAS window — in that case the probe
    /// leaves the cell alone (does NOT touch the new owner's state) and
    /// reports "not applicable". `Some(false)` is intentionally
    /// unreachable: this probe cannot distinguish "rollback is broken" from
    /// "someone else legitimately owns the cell now" by construction (both
    /// look identical from here — the postcondition CAS just fails either
    /// way), so it never claims rollback failure, only "clean" or
    /// "inconclusive".
    ///
    /// Exists so a consumer's test can drive the rollback on a REAL, LIVE cell
    /// (e.g. a process-global registry chunk) — proving the shipped code path,
    /// not a copy — without a process-terminating OOM. The whole probe is a
    /// bounded, single-threaded sequence of atomic ops; callers MUST pick a
    /// cell no other thread is concurrently initialising. The entry CAS is
    /// only a POINT-IN-TIME check, not mutual exclusion across the whole
    /// probe: if the cell is not observed `UNINIT` at that instant, the probe
    /// returns `None` and touches nothing, but a concurrent
    /// [`RacyPtrCell::get_or_try_init`] racing in AFTER the entry CAS (during
    /// the probe's own rollback-then-reCAS window) is not excluded by it — the
    /// probe's final restore step accounts for that by only touching the cell
    /// when its own postcondition CAS actually re-won ownership (see the
    /// step-by-step comments in the body).
    ///
    /// # Stability
    ///
    /// This is a deliberate, STABLE part of the public API (task #710), not
    /// `#[doc(hidden)]`. This function is explicitly written to be called
    /// FROM a downstream consumer's own test suite ("a consumer's test can
    /// drive the rollback on a REAL, LIVE cell" above) — a `#[doc(hidden)]`
    /// posture would have advertised it to those consumers while hiding it
    /// from the rustdoc they would need to find it in the first place, an
    /// unresolvable contradiction the crate's rust-intel audit caught. See
    /// the crate README's "Test-probe API stability" section for the full
    /// rationale and the rejected feature-flag alternative.
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

        // Step 4: restore to null, exactly as observed on entry — but ONLY if
        // step 3's CAS actually re-won ownership of the cell (postcondition
        // held). If it failed, the cell is not ours any more: a real
        // `get_or_try_init` caller raced in during the window between step 2's
        // rollback and step 3's CAS, won the CAS itself, and may already be
        // running (or have finished) the caller's init closure. Storing null
        // unconditionally here would clobber that other owner's sentinel (or
        // its published pointer) out from under it — the exact clobber this
        // probe must not cause. When we did not re-win, leave the cell alone
        // and report "not applicable" rather than a false rollback failure: a
        // concurrent owner racing in is not evidence that rollback itself is
        // broken.
        if !postcondition_holds {
            return None;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);

        Some(postcondition_holds)
    }
}

impl<T> Default for RacyPtrCell<T> {
    fn default() -> Self {
        Self::new()
    }
}
