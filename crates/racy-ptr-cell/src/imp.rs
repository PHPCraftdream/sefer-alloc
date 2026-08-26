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

/// The outcome of [`RacyPtrCell::dbg_rollback_reenterable`] — exactly the two
/// answers that probe can give, and no third one it could never produce.
///
/// In particular there is no "rollback is broken" variant: the probe cannot
/// distinguish that from "another thread legitimately owns the cell now",
/// because both make its postcondition CAS fail identically. See the
/// method's own docs for the full argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackProbe {
    /// The rollback provably cleared the sentinel: the probe's postcondition
    /// CAS re-won the cell afterwards, so no future winner or spinning loser
    /// can be wedged by it. The cell is restored to `UNINIT` before
    /// returning.
    Proven,
    /// The probe could not run its check, and this is NOT evidence that
    /// rollback is broken. Either the cell was not `UNINIT` when the probe
    /// entered (already `READY`, or owned by another thread at that
    /// instant) — in which case the probe never touched it at all — or a
    /// real `get_or_try_init` caller re-won the cell during the probe's own
    /// rollback-then-reCAS window, in which case the probe still does not
    /// touch it, but the cell is no longer necessarily `UNINIT`: the real
    /// caller may already be running `init`, or may have published `READY`,
    /// by the time this returns. Either way the probe never clobbers a
    /// state it does not own.
    NotApplicable,
}

/// A lazy, CAS-published pointer cell: `UNINIT -> INITIALIZING -> READY` over a
/// single `AtomicPtr<T>`, with fallible init (OOM rolls back and losers
/// re-race). See the [crate-level docs](crate) for the full state machine, the
/// anti-livelock loser-spin rule, and the "usable inside a
/// `#[global_allocator]`" niche.
///
/// The cell never drops, frees, or reads through the pointee — it only
/// publishes and hands back the `*mut T` the init closure produced.
///
/// `#[repr(transparent)]`: the "one `AtomicPtr`"/"one word" claims made
/// throughout this crate's docs are a LAYOUT GUARANTEE, not an
/// implementation detail that happens to be true on the current compiler.
/// `PhantomData<*mut T>` is the only other field; it is always zero-sized
/// with alignment 1, which is exactly what `repr(transparent)` requires of
/// every field beyond the one real one.
#[repr(transparent)]
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

/// RAII rollback guard held across the init closure: if `init`
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
/// Test coverage note: `tests/cell_unit.rs`'s
/// `panicking_init_rolls_back_and_subsequent_call_succeeds` proves a
/// strictly weaker property than the one described above — that a
/// SUBSEQUENT call on an already-quiescent cell succeeds after a panicking
/// init unwound and rolled back. The same file's
/// `concurrent_get_or_try_init_started_before_unwind_completes_still_succeeds`
/// goes further: a real concurrent caller, whose own `get_or_try_init` call
/// is issued no later than the point where it observes the winner already
/// holds the sentinel, is never lost — either it observes the live sentinel
/// and spins until the rollback wakes it, or it observes the already-rolled-
/// back cell and wins the CAS itself directly. Both interleavings are
/// possible depending on scheduling, and the test only guarantees success
/// across whichever one actually happens; it does NOT deterministically
/// force the spin-and-wake path specifically (that would need a hook inside
/// the CAS/spin loop itself, which this crate does not have). A future
/// change that made the rollback conditional (e.g. skipping it when no
/// loser is observed waiting) would still very likely reintroduce a
/// livelock this test would time out on, just not with airtight certainty
/// that the spin branch itself was exercised on every run. Not closed by a
/// loom test: loom's deterministic scheduling model and
/// `std::panic::catch_unwind` do not compose cleanly (loom needs to replay
/// every interleaving of an unwind path, which its own docs do not treat as
/// a first-class supported pattern).
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
    /// `init` is [`FnOnce`], not `FnMut`, because ONE call to this method
    /// invokes it **at most once**: whichever way the winner arm exits
    /// (publish, OOM rollback, or unwind) it leaves the method, and the
    /// loser arm never calls `init` at all — a loser that falls out of the
    /// spin on a rollback re-races the CAS and, if it wins, is making its
    /// own first and only call. `FnOnce` is therefore the accurate bound,
    /// and it lets you pass a closure that consumes what it captures.
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
    #[must_use = "`None` means `init` reported OOM and the cell was rolled \
                  back to UNINIT — it is NOT initialised, and discarding \
                  this hides the failure"]
    #[inline]
    pub fn get_or_try_init<F>(&self, init: F) -> Option<NonNull<T>>
    where
        F: FnOnce() -> Option<NonNull<T>>,
    {
        // Fast path, and nothing else: one `Acquire` load plus the readiness
        // test. Everything the already-published case does NOT need — the
        // claim CAS, the rollback guard, the release-active `assert!`, the
        // loser spin, the re-race loop — lives in `init_slow`, which is
        // `#[cold] #[inline(never)]` so none of it is inlined into a
        // caller that only ever hits this branch.
        let p = self.ptr.load(Ordering::Acquire);
        if Self::is_ready(p) {
            // SAFETY: `is_ready(p)` just proved `p` is non-null (neither the
            // `null` UNINIT value nor the `SENTINEL_INITIALIZING` marker), so
            // `p` is a real published pointer.
            return Some(unsafe { NonNull::new_unchecked(p) });
        }
        self.init_slow(init)
    }

    /// The full `UNINIT -> INITIALIZING -> READY` protocol: claim CAS, init
    /// closure, publish/rollback, loser spin, re-race. Split out of
    /// [`RacyPtrCell::get_or_try_init`] so the already-READY fast path stays
    /// small enough to inline on its own; correctness is unchanged, and the
    /// re-checked fast path at the top of the loop below is still needed
    /// here (a re-racing loser re-enters it after a rollback).
    ///
    /// Note this does NOT deduplicate monomorphised code: the slow path is
    /// still generic over `F`, so one copy exists per closure type. It only
    /// keeps that copy out of the caller's hot path.
    #[cold]
    #[inline(never)]
    fn init_slow<F>(&self, init: F) -> Option<NonNull<T>>
    where
        F: FnOnce() -> Option<NonNull<T>>,
    {
        loop {
            // Re-checked fast path: a loser that fell out of the spin on a
            // rollback, or lost the CAS to a winner that has since
            // published, lands here.
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
                // Success `Acquire`: synchronises-with whichever `Release`
                // store last returned the cell to `null` — the explicit OOM
                // rollback in this function's own winner arm, the unwind
                // guard's `Drop`, or either of `dbg_rollback_reenterable`'s
                // two null-stores. It says nothing
                // about the publish this thread is about to perform: an
                // acquire cannot pair with a release that has not happened
                // yet. The load-bearing pair for the pointee is this winner's
                // own `Release` publish below against every reader's
                // `Acquire` load.
                //
                // Whether `Relaxed` would suffice here — a rollback leaves no
                // payload state for a new winner to acquire — remains an open
                // question, and is DELIBERATELY not acted on. Weakening it
                // needs BOTH a loom counterfactual proving the weaker form
                // sound and a measurement showing it is worth anything, and
                // the second half is unobtainable on the hardware this crate
                // is developed on: `Acquire` on x86-64 is a plain load, so a
                // local A/B can only ever report noise. The same applies to
                // the loser spin's per-iteration `Acquire` below. Both stay
                // as they are — over-strong, never under-strong — until
                // someone can measure them on a weakly-ordered target
                // (AArch64/ARM) with a model to back the change.
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
                    // `RollbackGuard`'s own doc for why this is load-bearing.
                    let mut guard = RollbackGuard::new(&self.ptr);
                    match init() {
                        Some(ptr) => {
                            let raw = ptr.as_ptr();
                            // Release-active `assert!`, not `debug_assert!`
                            // a SAFE init closure can construct
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
                            // guard above unwinds it cleanly.
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
                    // anti-livelock rule: a `!= READY` spin would
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
    /// does (an earlier version of this doc claimed
    /// otherwise). It exists as a named, self-documenting boolean
    /// introspection primitive: a caller writing `assert!(cell.dbg_is_ready())`
    /// reads as "assert the cell materialised" without an
    /// `.is_some()`/`.is_none()` match at the call site. The `sefer-alloc`
    /// allocator this crate was extracted from relies on exactly that: its
    /// own `Registry::dbg_chunk_is_materialised` forwards here to assert
    /// chunk-materialisation state in its regression tests.
    ///
    /// # Stability
    ///
    /// This is a deliberate, STABLE part of the public API — a
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
    /// Returns [`RollbackProbe::Proven`] if the rollback provably cleared the
    /// sentinel (the postcondition CAS re-won the cell; it is restored to
    /// `UNINIT` before returning). [`RollbackProbe::NotApplicable`] covers
    /// TWO distinct "could not test" cases, deliberately conflated because
    /// neither is evidence rollback is broken: (a) the cell was not observed
    /// `UNINIT` on the entry CAS (already `READY`, or another thread owned it
    /// at that instant), or (b) the postcondition CAS in step 3 failed
    /// because a real `get_or_try_init` caller raced in and re-won the cell
    /// during the probe's own rollback-then-reCAS window — in that case the
    /// probe leaves the cell alone (does NOT touch the new owner's state).
    ///
    /// **There is deliberately no "rollback is broken" variant.** This probe
    /// cannot distinguish that from "someone else legitimately owns the cell
    /// now" by construction — both look identical from here, the
    /// postcondition CAS simply fails either way — so the return type
    /// encodes exactly the two answers it can actually give, and no third
    /// one it could never produce.
    ///
    /// Exists so a consumer's test can drive the rollback on a REAL, LIVE cell
    /// (e.g. a process-global registry chunk) — proving the shipped code path,
    /// not a copy — without a process-terminating OOM. The whole probe is a
    /// bounded, single-threaded sequence of atomic ops; callers MUST pick a
    /// cell no other thread is concurrently initialising. The entry CAS is
    /// only a POINT-IN-TIME check, not mutual exclusion across the whole
    /// probe: if the cell is not observed `UNINIT` at that instant, the probe
    /// returns [`RollbackProbe::NotApplicable`] and touches nothing, but a
    /// concurrent
    /// [`RacyPtrCell::get_or_try_init`] racing in AFTER the entry CAS (during
    /// the probe's own rollback-then-reCAS window) is not excluded by it — the
    /// probe's final restore step accounts for that by only touching the cell
    /// when its own postcondition CAS actually re-won ownership (see the
    /// step-by-step comments in the body).
    ///
    /// # Stability
    ///
    /// This is a deliberate, STABLE part of the public API, not
    /// `#[doc(hidden)]`. This function is explicitly written to be called
    /// FROM a downstream consumer's own test suite ("a consumer's test can
    /// drive the rollback on a REAL, LIVE cell" above) — a `#[doc(hidden)]`
    /// posture would have advertised it to those consumers while hiding it
    /// from the rustdoc they would need to find it in the first place, an
    /// unresolvable contradiction the crate's rust-intel audit caught. See
    /// the crate README's "Test-probe API stability" section for the full
    /// rationale and the rejected feature-flag alternative.
    #[must_use]
    pub fn dbg_rollback_reenterable(&self) -> RollbackProbe {
        // Step 1: only proceed if the cell is UNINIT (null). If it is already
        // READY or contended, do not touch it.
        if self
            .ptr
            .compare_exchange(
                core::ptr::null_mut(),
                Self::sentinel(),
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return RollbackProbe::NotApplicable;
        }

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
            return RollbackProbe::NotApplicable;
        }
        self.ptr.store(core::ptr::null_mut(), Ordering::Release);

        RollbackProbe::Proven
    }
}

impl<T> Default for RacyPtrCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> core::fmt::Debug for RacyPtrCell<T> {
    /// Diagnostic-only classification of the cell's current state — never
    /// dereferences the pointee, so no `T: Debug` bound is needed (`T` never
    /// appears in the output). `Relaxed` is enough here: unlike `get`, this
    /// never hands the pointer back to the caller to dereference, so there is
    /// no happens-before edge to establish. Like any concurrent type's
    /// `Debug` impl (`OnceLock`'s included), the state printed can be stale
    /// the instant after this call returns.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let p = self.ptr.load(Ordering::Relaxed);
        f.write_str("RacyPtrCell(")?;
        match p.addr() {
            0 => f.write_str("Uninit")?,
            SENTINEL_INITIALIZING => f.write_str("Initializing")?,
            _ => write!(f, "Ready({p:p})")?,
        }
        f.write_str(")")
    }
}
