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
//!   re-race the CAS themselves, rather than being blocked and woken.
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
//! - **fallible without blocking** — `OnceLock::get_or_init` cannot fail at
//!   all, and its `get_or_try_init` is still unstable (`once_cell_try`). Both
//!   may also BLOCK the losing threads for the duration of the winner's init
//!   (the documented contract is that losers block; the mechanism `std` uses
//!   to do so is an implementation detail, not a stable promise). This
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
//! ### Panic sites and the two link environments
//!
//! The panic sites, independently:
//!
//! | # | Panic site | Whose code | Reaches the panic runtime? | Message shape | Allocations before a non-allocating hook (measured — see below) |
//! |---|---|---|---|---|---|
//! | 1 | sentinel-collision `assert!` in `get_or_try_init` | this crate | yes | bare `&'static str` | 0 |
//! | 2 | an unwinding `init` closure | **yours** | yes | whatever you wrote | 0 if a bare literal, ≥ 2 if formatted |
//! | 3 | `align_of::<T>() >= 2` in `new`, `static` form | this crate | **no** — const-eval failure, compile time | n/a | n/a |
//! | 3 | `align_of::<T>() >= 2` in `new`/`default`, non-const form | this crate | yes | bare `&'static str` | 0 |
//!
//! **Normative contract, separate from the measurement below: `init` must
//! not panic, full stop.** The `std` panic path *may* allocate before any
//! hook runs — especially for a formatted message — so the absence of an
//! allocation is never something to rely on. The numbers in this table and
//! the paragraph below are a measurement (rustc 1.97,
//! `x86_64-pc-windows-msvc`, `--release`, `RUST_BACKTRACE=0`, one specific
//! non-allocating hook), not an API guarantee about the panic runtime, this
//! crate's MSRV, other `std` implementations/targets, or future toolchains.
//!
//! The two link environments need genuinely different mitigations, **not a
//! shared recipe**:
//!
//! - A `no_std` binary supplies its own `#[panic_handler]`. Written not to
//!   allocate, it closes the hazard completely: the whole panic path is
//!   yours, so nothing on it can re-enter the allocator.
//! - A `std` binary's `panic = "abort"` profile setting removes the
//!   **unwind** (the UB when the frame below is `GlobalAlloc::alloc`), but it
//!   does **not** stop the panic runtime from allocating: with the DEFAULT
//!   hook, every panic sampled here allocated before it could print anything
//!   (measured: 2 allocations under `panic = "abort"`, `RUST_BACKTRACE=0`,
//!   `--release`, rustc 1.97, x86_64-pc-windows-msvc). Inside a
//!   `#[global_allocator]` that allocation re-enters the very cell that is
//!   mid-`init`, and the thread deadlocks on its own sentinel instead of
//!   aborting — the diagnostic the release-active `assert!` above exists to
//!   print never reaches stderr, because the allocation that would have
//!   printed it is the one that deadlocked. A `std` consumer therefore needs
//!   `panic = "abort"` **and** a `std::panic::set_hook` that goes straight to
//!   `std::process::abort` without formatting — or, better, an `init` that
//!   cannot panic at all. **Residual limit: a hook cannot help if the panic
//!   message is formatted.** `std` materialises the message (`payload.get()`)
//!   as an *argument* to the hook call, so `unwrap`/`expect`/`assert_eq!`/
//!   `panic!("{}", …)` allocate before any hook runs, whether or not the hook
//!   itself allocates — measured: 2 allocations for `Result::unwrap`, 4 for
//!   `assert_eq!`, with the same non-allocating hook that reaches 0 for a
//!   bare-`&'static str` panic. Only a panic whose message is a bare
//!   `&'static str` was measured allocation-free under that hook. The
//!   crate's own two `assert!`s (the sentinel-collision check and the
//!   `align_of::<T>() >= 2` check) are of that shape and measure 0
//!   allocations before the hook; **an unwinding `init` is your code, and
//!   its message is whatever you wrote**, so it is covered only if you
//!   keep it a bare literal — and even then only as a measured observation
//!   on one toolchain, not a promise. **The only mitigation the contract
//!   actually rests on is an `init` that cannot panic at all.** Note also
//!   that `panic = "abort"` compiles the crate's internal rollback guard out
//!   entirely (it is unwind-only) — under this profile the cell-consistency
//!   guarantee above comes from the process dying, not from the guard.
//!
//! ### Fork and signal safety
//!
//! Everything above is about `init` and the panic path it can reach; two
//! further hazards break the cell from **outside** any of that, with no
//! misbehaving closure and no panic anywhere. **The cell is neither
//! fork-safe nor async-signal-safe.**
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
//! **The rule for a multithreaded POSIX process is the POSIX rule, and this
//! crate adds nothing to it: after `fork()`, the child may call only
//! async-signal-safe functions until a successful `exec()`; if `exec()`
//! fails, terminate through an async-signal-safe path such as `_exit`.**
//! That means no Rust allocator, no `get_or_try_init`, no `init` closure, no
//! panic path, and no other ordinary Rust code in the child before `exec()`
//! — the child inherits the whole address space, including every lock and
//! resource state left behind by threads that do not exist in it, and POSIX
//! specifies that a function is not async-signal-safe unless it is
//! explicitly documented to be ([POSIX `fork()`], [async-signal-safety]).
//!
//! There is a narrower, cell-local invariant worth stating separately,
//! because it is the part this crate can speak to at all: **`fork()` must
//! not race any thread's `init`, anywhere in the process** — not just once,
//! before some notional "first" fork; every subsequent `fork()`, and every
//! cell created or reset afterward, is bound by it. A process-wide barrier
//! establishes it: every initializer holds the barrier's shared side for the
//! whole duration of its `init`, and the forking thread takes it
//! exclusively — which by construction both waits for quiescence and blocks
//! new inits — calls `fork()` **while still holding it**, and releases it
//! only after `fork()` returns. (Acquiring, observing quiescence, releasing,
//! and only then forking leaves a window in which a fresh `init` starts
//! before the fork; holding across the call is the load-bearing part.)
//!
//! **That barrier prevents exactly one thing: a child snapshotting a cell
//! wedged at `INITIALIZING` with no thread alive to finish it. It does NOT
//! make the allocator, this cell, or Rust runtime code callable in the child
//! before `exec()`** — inherited allocator and runtime locks are untouched
//! by it, and a `get_or_try_init` call in the child is a non-async-signal-safe
//! call regardless of what any cell's state word says. Anything broader than
//! the POSIX rule above is an environment-specific contract you own, and owes
//! a fully proven `atfork` protocol covering every affected resource, not
//! just these cells.
//!
//! Do not allocate in a signal handler.
//!
//! [ga]: https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html#safety
//! [POSIX `fork()`]: https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html
//! [async-signal-safety]: https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html
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
//! on a target without it. `thumbv6m-none-eabi` (Cortex-M0/M0+) and
//! `riscv32imc-unknown-none-elf` (no `A` extension) have load/store atomics
//! but no CAS; `msp430-none-elf` has no atomics at all. This crate is
//! `no_std` and allocation-free, but neither property implies pointer-width
//! CAS. A build on an unsupported target fails with an explicit
//! [`compile_error!`] naming the requirement, and with **nothing else**:
//! the implementation carries the positive `#[cfg(target_has_atomic =
//! "ptr")]`, so its body is not compiled there at all. That replaces the
//! "no method named `compare_exchange`" cascade an unguarded build would
//! produce on `thumbv6m-none-eabi`/`riscv32imc-unknown-none-elf`, and the
//! unresolved `AtomicPtr` import on `msp430-none-elf` (which has no atomics
//! for `core` to define it from), with one sentence naming the real
//! requirement.

// This crate is a two-file seam crate: `lib.rs` is a documentation +
// portability-guard facade with no code of its own, and ALL `unsafe` is
// confined to `imp.rs`, lifted by the crate-level `#![allow(unsafe_code)]`
// below. ONE
// documented reason holds `unsafe` here — handing a raw `*mut T` /
// `NonNull<T>` back to the caller — and it materialises at exactly two
// audited kinds of site:
//
//   1. `unsafe impl Send/Sync` for the `AtomicPtr`-backed cell (justified
//      below at the impls themselves);
//   2. `unsafe { NonNull::new_unchecked(p) }` at the accessor sites where
//      `p` was already proven non-null by an `is_ready`/`!= 0` check.
//
// Note what is NOT on that list: constructing the never-dereferenced
// `INITIALIZING` sentinel via `core::ptr::without_provenance_mut` needs no
// `unsafe` at all (it is a safe `const fn` on modern toolchains) — the
// sentinel-comparison discipline is a correctness invariant, not an `unsafe`
// one. All raw-pointer *dereferencing* is the CALLER's responsibility; this
// crate never reads through `T`. The `#![allow(unsafe_code)]` is retained
// (rather than `#![forbid]`) so the crate can expose the raw seam types and
// those two site kinds. Every `unsafe fn` / `unsafe impl` carries a
// `# Safety` / `// SAFETY:` justification.
#![allow(unsafe_code)]
#![deny(missing_docs)]
#![no_std]

// The whole cell is one AtomicPtr driven by compare_exchange (see the
// crate-doc "Portability limit" section above) — that requires pointer-width
// atomic CAS from the target. The implementation module below carries the
// POSITIVE form of this same cfg, so on an unsupported target the body is
// not compiled at all and this named diagnostic is the ONLY error the user
// sees — no follow-on E0599 (thumbv6m-none-eabi/riscv32imc) or E0432
// (msp430) cascade from code that could never have compiled there.
#[cfg(not(target_has_atomic = "ptr"))]
compile_error!(
    "racy-ptr-cell requires a target with pointer-width atomic \
     compare-and-swap (target_has_atomic = \"ptr\"): the whole cell is one \
     AtomicPtr driven by compare_exchange. thumbv6m-none-eabi \
     (Cortex-M0/M0+) and riscv32imc-unknown-none-elf (no `A` extension) have \
     load/store atomics but no CAS; msp430-none-elf has no atomics at all. \
     None of these are supported, despite this crate being no_std and \
     allocation-free."
);

/// The implementation. Split out of `lib.rs` so the whole body can carry the
/// POSITIVE `target_has_atomic = "ptr"` cfg in one place — on an unsupported
/// target this module is not compiled, leaving the `compile_error!` above as
/// the single diagnostic. Not a public module: its two items are re-exported
/// below and are the crate's entire public API.
#[cfg(target_has_atomic = "ptr")]
mod imp;

#[cfg(target_has_atomic = "ptr")]
pub use imp::{RacyPtrCell, RollbackProbe};
