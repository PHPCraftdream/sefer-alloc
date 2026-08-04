//! The primordial fallback heap (Phase 12.3, §2.3 of
//! `ALLOC_PLAN_PHASE12-13.md`).
//!
//! A process-global, always-live [`HeapCore`] for the **pre-TLS** (very
//! early runtime init, before any thread's TLS is set up) and **post-TLS-
//! teardown** windows. The alloc face routes here when [`tls_heap::current`]
//! cannot serve a thread-local heap; the fallback is therefore the
//! embodiment of **M10 — never-null when serviceable**: the alloc face
//! returns null only on true OOM, never because "no heap is bound right
//! now".
//!
//! ## Correctness, not speed
//!
//! These windows are rare (early runtime init before the first thread's TLS
//! is wired; the dying moments of thread teardown). A simple spinlock-guarded
//! heap is fine — contention is essentially impossible (the runtime is
//! single-threaded in the pre-TLS window, and a tearing-down thread is
//! exiting). We do NOT use `std::sync::Mutex` (it may allocate on some
//! platforms on contention); a hand-rolled `AtomicBool` spinlock is M5-clean
//! (no `std::alloc` reachable).
//!
//! ## Never dropped
//!
//! The fallback [`HeapCore`] lives in a `static MaybeUninit` and is NEVER
//! dropped. Its segments stay mapped for the process lifetime. This is
//! intentional and correct: the fallback serves allocations that may
//! outlive any single thread, and dropping it would unmap memory a late
//! `dealloc` may still target. The bounded leak (one heap's footprint) is
//! the price of the never-null guarantee.
//!
//! ## Blocks are normal segment blocks
//!
//! Blocks allocated from the fallback are normal segment blocks — their
//! owning segment's header carries `owner_thread_free` set to the fallback
//! heap's TFS head (under `alloc-xthread`). So a later cross-thread free
//! routes correctly via `segment_base_of` → header owner, no special-casing
//! on the free path. Under plain `alloc-global` (no `alloc-xthread`) the
//! blocks are own-thread-only (the fallback is single-threaded anyway in
//! that config).
//!
//! ## M5-clean bootstrap
//!
//! The fallback's [`HeapCore::new`] goes through the OS aperture
//! (`mmap`/`VirtualAlloc`) and never `std::alloc` — same M5-clean property
//! as the registry bootstrap. Under `alloc-xthread`, the cross-thread
//! free-stack head is NOT an inline `HeapCore` field and is NOT `Box`-allocated:
//! task H1 hoisted it into the standalone process-`'static` [`FALLBACK_TFS`]
//! atomic (the fallback's analogue of a registry slot's `thread_free` word),
//! whose stable address is bound into the fallback `HeapCore` via
//! [`HeapCore::bind_thread_free`] once, at init under the bootstrap race,
//! BEFORE the `READY` publish. So cross-thread-free routing is wired purely
//! from that already-bound `'static` word — no allocation on any fallback
//! path, M5-clean and M10-preserving. (A `Box`-via-`std::alloc` here would
//! self-deadlock — the first fallback alloc runs under the fallback spinlock,
//! and re-entering the global allocator to grow a `Box` would recurse back
//! into it; hoisting the head to a `'static` avoids that entirely, with no
//! first-alloc `Box` and no OOM-on-install case to handle.)
//!
//! [`tls_heap::current`]: super::tls_heap::current

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented fallback-heap seam (Phase 12.3).
// `allow` lifts the crate-level `deny` for this file only. The `unsafe`
// surface here is:
//   * the hand-rolled atomic-init state-machine over a `static mut
//     MaybeUninit<HeapCore>` (mirrors the registry's bootstrap discipline),
//     and
//   * the spinlock-guarded `&mut HeapCore` handout (sound under the
//     spinlock's mutual exclusion).
// Every `unsafe` block carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

use crate::registry::HeapCore;

/// task H1: the fallback heap's cross-thread free-stack head / identity stamp,
/// hoisted OUT of the fallback `HeapCore` into this process-`'static` atomic —
/// the fallback's analogue of a registry slot's
/// [`HeapSlot::thread_free`](crate::registry::HeapSlot::thread_free).
///
/// The fallback `HeapCore` lives in a `static mut` handed out as `&mut` under
/// the [`LOCK`] spinlock; a REMOTE thread cross-thread-freeing a Large segment
/// owned by the fallback CASes its free-stack head through EXPOSED provenance.
/// Were that head an inline `HeapCore` field, the remote write would land
/// inside the range of the owner's `&mut *FALLBACK` — the same H1 aliasing
/// conflict fixed for registry heaps by moving the head into the `Sync` slot.
/// This standalone `'static` atomic (never inside any `&mut HeapCore`) is the
/// fallback's equivalent slot word: its stable address is planted into the
/// fallback `HeapCore` (via `HeapCore::bind_thread_free`) at init, and stamped
/// into the fallback's segment headers, so remote freers CAS THIS word, never
/// a byte inside `FALLBACK`.
///
/// `AtomicPtr` is `Sync`, so shared cross-thread atomic access is race-free;
/// null-initialised (empty stack). Only present under `alloc-xthread`.
#[cfg(feature = "alloc-xthread")]
static FALLBACK_TFS: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());

/// Bootstrap-state values (mirrors `registry::bootstrap`). `pub` so the F-8
/// counterfactual test (`tests/regression_fallback_init_unwind_guard.rs`) can
/// name them by intent rather than by raw `0`/`1`/`2` literals.
pub const STATE_UNINIT: u8 = 0;
pub const STATE_INITIALIZING: u8 = 1;
pub const STATE_READY: u8 = 2;

/// The fallback heap storage. `MaybeUninit` until [`ensure`] runs; once
/// `READY`, holds a live `HeapCore` for the process lifetime (never
/// dropped).
///
/// `static mut` because we hand out `&mut` through it under the spinlock;
/// the aliasing is governed by [`LOCK`] (mutual exclusion) and the atomic
/// init state-machine. The `HeapCore` is `!Sync` (it owns `AllocCore` which
/// is single-threaded), so we cannot use a plain `static`; the
/// `static mut` and spinlock together are the M5-clean equivalent of
/// `Mutex<HeapCore>` without the `std::alloc` reach.
static mut FALLBACK: MaybeUninit<HeapCore> = MaybeUninit::uninit();

/// The bootstrap state-machine word: `UNINIT → INITIALIZING → READY`.
static INIT_STATE: AtomicU8 = AtomicU8::new(STATE_UNINIT);

/// R6-OPT-P0-1: process-wide count of [`LOCK`] acquisitions (i.e. of
/// [`with_heap`] calls that got past the null check and entered the guarded
/// section). Diagnostic-only, `Relaxed` — mirrors the crate's existing
/// cold-path counter discipline (e.g. `CONFIG_CONFLICTS` in
/// `registry::heap_registry`): always compiled in (not gated behind
/// `alloc-stats`), since acquiring the fallback lock is definitionally a
/// cold/rare path already, not a hot-path tax this counter would visibly add
/// to.
///
/// Exists so a test can prove a negative — "the fallback spinlock was NOT
/// taken for this dealloc" — which is otherwise unobservable from outside
/// this module (`LOCK` itself is private). See
/// [`dbg_fallback_lock_acquisitions`] and
/// `tests/dealloc_only_no_bind_torn.rs`'s TORN-thread test, which snapshots
/// this counter immediately before and after a TORN-thread dealloc and
/// asserts it did not move (R6-OPT-P0-1's `current_for_dealloc` routes a
/// TORN thread's dealloc directly through `HeapCore::dealloc_foreign_routing`
/// without ever calling `with_heap`). `u64` (not `u8`/`u32`) so a long-running
/// process/test-binary that legitimately calls `with_heap` many times cannot
/// wrap this counter around and produce a false "unchanged" reading.
static LOCK_ACQUISITIONS: AtomicU64 = AtomicU64::new(0);

/// The fallback-heap spinlock. Held while a thread is performing an
/// alloc/dealloc/realloc on the fallback; ensures mutual exclusion so the
/// `&mut HeapCore` handout is sound. `AtomicBool` (not `std::sync::Mutex`)
/// to keep the path `std::alloc`-free (M5).
static LOCK: AtomicBool = AtomicBool::new(false);

/// Ensure the fallback heap is initialised, then return a `*mut HeapCore`
/// into it. The first call constructs the `HeapCore` in place (OS aperture,
/// no `std::alloc`); concurrent/later calls observe `READY` and return
/// immediately.
///
/// Returns a non-null pointer unless the OS refuses the primordial
/// reservation (true OOM) — in which case the caller (the alloc face) is
/// genuinely out of memory and returns null. This is the ONLY path that can
/// yield null, and it is the correct M10 outcome (true OOM, not a missing
/// heap).
#[must_use]
pub fn heap_ptr() -> *mut HeapCore {
    loop {
        // Fast path: already READY. Acquire to see the init thread's writes.
        if INIT_STATE.load(Ordering::Acquire) == STATE_READY {
            // SAFETY: we observed READY under Acquire, which synchronises
            // with the initialising thread's Release store of READY.
            // `FALLBACK` is fully initialised and never dropped; the
            // pointer is valid for the process lifetime. We use
            // `addr_of_mut!` to obtain the pointer WITHOUT creating a
            // mutable reference to the `static mut` (which is UB under
            // Rust 2024's `static_mut_refs` rule).
            return addr_of_mut!(FALLBACK) as *mut HeapCore;
        }
        // Slow path: race to initialise.
        let won = INIT_STATE
            .compare_exchange(
                STATE_UNINIT,
                STATE_INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok();
        if won {
            // F-8 (R34-17/task #536): arm the init-state rollback guard BEFORE
            // the fallible init. If anything between here and the READY publish
            // unwinds (`HeapCore::new`, the in-place `write`, `bind_thread_free`,
            // or the test-injection panic below), the guard's `Drop` rolls
            // `INIT_STATE` back to `UNINIT` so loser threads stop spinning in
            // the loop below and re-race the CAS themselves. WITHOUT this guard
            // a single unwind leaves `INIT_STATE` stuck at `INITIALIZING`
            // forever — every other thread that reaches `heap_ptr` spins
            // unbounded in the `while ... == STATE_INITIALIZING` loop (process-
            // wide livelock), the exact failure mode `LockGuard` already
            // eliminated one function down (`with_heap`) for the spinlock. Same
            // RAII form, one level up.
            let mut init_guard = InitStateGuard::armed();
            // R34-17/task #536: test-only panic injection. When the flag is set,
            // panic BEFORE `HeapCore::new` — simulating an unwind out of the
            // guarded region so a test can prove the guard rolls `INIT_STATE`
            // back to `UNINIT`. A plain `AtomicBool` read: does NOT touch
            // allocator metadata through a raw pointer, so it is a safe
            // injection point (not an `unsafe fn`), `internals`-gated so it
            // does not widen the surface of a `production` build.
            #[cfg(feature = "internals")]
            if DBG_INJECT_FALLBACK_INIT_PANIC.load(Ordering::Relaxed) {
                panic!(
                    "deliberate panic during fallback init (F-8 test injection, R34-17/task #536)"
                );
            }
            // We are the sole initialiser. Construct the HeapCore in place.
            // `HeapCore::new` uses the sentinel id `u32::MAX` ("not bound to
            // a registry slot") — the fallback is NOT a registry slot; it is
            // a standalone process-global heap.
            match HeapCore::new(u32::MAX) {
                Some(hc) => {
                    // SAFETY: we won the init race (STATE_INITIALIZING); no
                    // other thread can read `FALLBACK` until we publish
                    // READY. Writing a `HeapCore` into the `MaybeUninit`
                    // initialises it. `addr_of_mut!` gives us the
                    // `*mut MaybeUninit<HeapCore>` destination without
                    // creating a `&mut` to the `static mut` (Rust 2024
                    // `static_mut_refs`); we cast to `*mut HeapCore` for the
                    // `write`.
                    unsafe { (addr_of_mut!(FALLBACK) as *mut HeapCore).write(hc) };
                    // task H1: plant the fallback heap's stable handle to its
                    // out-of-struct free-stack head (`FALLBACK_TFS`), the
                    // fallback analogue of `bind_slot_counters` binding a
                    // registry heap to its slot's `thread_free`. Done here,
                    // under the init race (we are the sole initialiser; no
                    // other thread can read `FALLBACK` until we publish READY),
                    // BEFORE the Release store — so the first `with_heap`
                    // alloc/free already sees a bound handle. Skipped when
                    // `alloc-xthread` is off (the fallback is single-threaded
                    // in that config and has no cross-thread head).
                    #[cfg(feature = "alloc-xthread")]
                    {
                        // SAFETY: we won the init race (STATE_INITIALIZING) and
                        // just `write`(hc) into `FALLBACK`; we are its sole
                        // writer and no other thread can reference it until we
                        // publish READY. This exclusive `&mut` lives only for
                        // the `bind_thread_free` call. `FALLBACK_TFS` is a
                        // process-`'static` atomic, so `&FALLBACK_TFS` is a
                        // sound `&'static`.
                        let heap_ref: &mut HeapCore =
                            unsafe { &mut *(addr_of_mut!(FALLBACK) as *mut HeapCore) };
                        heap_ref.bind_thread_free(&FALLBACK_TFS);
                    }
                    INIT_STATE.store(STATE_READY, Ordering::Release);
                    // Happy path: READY just published — disarm the guard so
                    // its Drop does NOT clobber READY back to UNINIT.
                    init_guard.disarm();
                    // SAFETY: READY just published by us.
                    return addr_of_mut!(FALLBACK) as *mut HeapCore;
                }
                None => {
                    // Primordial OOM. Roll back to UNINIT so a later caller
                    // can retry (the OS may have freed memory by then).
                    // Return null — the alloc face will surface this as true
                    // OOM. Crucially, the rollback to UNINIT (rather than
                    // leaving the state stuck at INITIALIZING) is what lets
                    // any thread currently spinning in the loser branch
                    // below observe a state change and re-race the CAS
                    // itself, instead of spinning forever waiting for a
                    // READY that this failed winner will never publish.
                    INIT_STATE.store(STATE_UNINIT, Ordering::Release);
                    // OOM path: already rolled back ourselves — disarm the
                    // guard so its Drop does not store UNINIT a second time
                    // (harmless under the single-writer invariant, but
                    // needlessly redundant).
                    init_guard.disarm();
                    return core::ptr::null_mut();
                }
            }
        }
        // Lost the race: spin until the state leaves INITIALIZING. It may
        // land on READY (winner published successfully — loop will hit the
        // fast path) or UNINIT (winner hit primordial OOM and rolled back —
        // loop back to the top and re-race the CAS ourselves, rather than
        // spinning forever waiting for a READY that will never come).
        while INIT_STATE.load(Ordering::Acquire) == STATE_INITIALIZING {
            core::hint::spin_loop();
        }
    }
}

/// Execute `f` with `&mut` access to the fallback heap, under the spinlock.
/// Used by the alloc face when it routes to the fallback (TLS unavailable,
/// registry exhausted). The spinlock makes the `&mut` handout sound — at
/// most one thread is inside `with_heap` at a time.
///
/// Returns `f`'s result, or `None` if the fallback heap could not be
/// initialised (true OOM — the alloc face surfaces this as null).
pub fn with_heap<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut HeapCore) -> R,
{
    let heap = heap_ptr();
    if heap.is_null() {
        return None; // True OOM (the OS refused the primordial reservation).
    }
    // task L4: acquire the spinlock through a RAII guard so a panic inside `f`
    // still releases `LOCK` (its `Drop` runs while unwinding). Without the
    // guard, a panic in `f` would leave `LOCK == true` forever, wedging every
    // subsequent pre-TLS / teardown allocation in `acquire_lock`'s spin
    // (deadlock instead of a clean abort). No-panic is a project invariant
    // (HeapCore never panics), but the guard makes `with_heap` panic-safe at
    // zero cost — it also removes the explicit `release_lock()` call.
    let _guard = LockGuard::acquire();
    // SAFETY: `heap` is non-null and points to the initialised `FALLBACK`
    // `HeapCore`. `LOCK` is held (the guard just acquired it), giving us
    // exclusive `&mut` access — no other thread can be inside `with_heap`. The
    // `HeapCore` is valid for the process lifetime (never dropped).
    Some(f(unsafe { &mut *heap }))
}

/// RAII guard over the fallback [`LOCK`] spinlock (task L4). Acquiring it spins
/// `false → true`; dropping it stores `false`. The `Drop` guarantees the lock
/// is released even if the closure passed to [`with_heap`] panics — turning a
/// would-be permanent deadlock into a clean unwind (or process abort under
/// `panic = "abort"`), without which a single panic in the fallback would wedge
/// all later pre-TLS / teardown allocations forever.
struct LockGuard;

impl LockGuard {
    /// Acquire the fallback spinlock, returning the guard that will release it.
    fn acquire() -> Self {
        while LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Spin with a PAUSE/YIELD hint. Contention on the fallback is
            // essentially impossible (the windows that route here are rare and
            // typically single-threaded), so the spin is academic.
            core::hint::spin_loop();
        }
        // R6-OPT-P0-1: diagnostic-only, `Relaxed` — see `LOCK_ACQUISITIONS`'s
        // doc comment. Bumped once per successful acquisition (this point is
        // reached only after the CAS loop above wins).
        LOCK_ACQUISITIONS.fetch_add(1, Ordering::Relaxed);
        LockGuard
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        LOCK.store(false, Ordering::Release);
    }
}

/// F-8 (R34-17/task #536) — RAII guard over the fallback [`INIT_STATE`] bootstrap
/// word. Arming it declares "I have won the UNINIT→INITIALIZING CAS and am about
/// to do fallible init"; its [`Drop`] rolls `INIT_STATE` back to `UNINIT` IF it
/// is still armed when the guard goes out of scope (the unwind path), so loser
/// threads observing `INITIALIZING` stop spinning and re-race the CAS themselves
/// instead of livelocking forever. On both normal exit paths (READY published, or
/// primordial-OOM rollback to UNINIT) the winner calls [`disarm`](Self::disarm)
/// so the guard's `Drop` is a no-op — it must NOT clobber a just-published READY.
///
/// Mirrors the panic-safety form of [`LockGuard`] (one function down, for the
/// spinlock) and `ConflictRollback` (`registry::heap_registry`, for a slot's
/// FREE→LIVE CAS): same shape, applied to the bootstrap state-machine one level
/// above where `LockGuard` already guards the spinlock. Before this guard, an
/// unwind out of `HeapCore::new` / the in-place `write` / `bind_thread_free`
/// left `INIT_STATE` stuck at `INITIALIZING` permanently — every subsequent
/// `heap_ptr` loser spun unbounded in the `while ... == STATE_INITIALIZING`
/// loop (process-wide livelock). No-panic is a project invariant (`HeapCore`
/// never panics in production), but the guard makes `heap_ptr` panic-safe at
/// zero cost on the happy path (the `disarm` is a plain bool store; the armed
/// `Drop` runs only on unwind).
struct InitStateGuard {
    armed: bool,
}

impl InitStateGuard {
    /// Construct an armed guard. The caller MUST have just won the
    /// `UNINIT → INITIALIZING` CAS on [`INIT_STATE`].
    fn armed() -> Self {
        InitStateGuard { armed: true }
    }

    /// Disarm the guard so its `Drop` does NOT roll `INIT_STATE` back. Called on
    /// BOTH normal exit paths after the winner has itself published the final
    /// state (READY on success, UNINIT on primordial OOM).
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for InitStateGuard {
    fn drop(&mut self) {
        if self.armed {
            // Unwind path: we never published READY (nor rolled back to UNINIT
            // ourselves). Restore UNINIT so losers re-race the CAS instead of
            // spinning forever on a READY that will never come. We are the sole
            // INITIALIZING holder (single CAS winner), so this store cannot
            // race another writer. Release mirrors the explicit OOM-rollback
            // store's ordering.
            INIT_STATE.store(STATE_UNINIT, Ordering::Release);
        }
    }
}

/// `#[doc(hidden)]` test hook (task L4) — not part of the public API. Exercises
/// the panic-safety of the fallback spinlock: it invokes [`with_heap`] with a
/// closure that PANICS (caught here via `catch_unwind`), then reports whether a
/// SUBSEQUENT [`with_heap`] can still acquire the lock. Before the RAII
/// `LockGuard`, the panicking closure left `LOCK == true` forever, so the second
/// `with_heap` would spin in `acquire_lock` indefinitely (deadlock); the test
/// verifies that instead it returns promptly (guard released the lock while
/// unwinding). Returns `true` iff the second `with_heap` completed — i.e. the
/// lock was NOT wedged.
///
/// The whole call runs on the current thread with no `sleep`/polling: if the
/// lock were wedged the second `with_heap` would hang here (the test wraps this
/// in its own bounded watchdog thread, so a regression surfaces as a timeout
/// rather than a literal forever-hang).
#[cfg(feature = "std")]
#[doc(hidden)]
#[must_use]
pub fn dbg_panic_in_with_heap_releases_lock() -> bool {
    // First: a panicking closure. `catch_unwind` swallows the unwind so the
    // test process survives; the `LockGuard` inside `with_heap` must release
    // `LOCK` on the way out.
    let panicked = std::panic::catch_unwind(|| {
        let _ = with_heap(|_heap| {
            panic!("deliberate panic inside with_heap (task L4 test hook)");
        });
    })
    .is_err();
    if !panicked {
        // The closure was expected to panic; if it did not, the hook is broken.
        return false;
    }
    // Second: a NON-panicking closure. If the lock is still held (regression),
    // this spins forever; if the guard released it, this returns immediately.
    with_heap(|_heap| ()).is_some()
}

/// `#[doc(hidden)]` test hook (R6-OPT-P0-1) — not part of the public API.
/// Reads [`LOCK_ACQUISITIONS`], the process-wide count of successful fallback
/// spinlock acquisitions since process start. A test proves "this dealloc did
/// NOT take the fallback lock" by snapshotting this value immediately before
/// and after the call under test and asserting it did not move.
#[doc(hidden)]
#[must_use]
pub fn dbg_fallback_lock_acquisitions() -> u64 {
    LOCK_ACQUISITIONS.load(Ordering::Relaxed)
}

// ===========================================================================
// F-8 (R34-17/task #536) — test-only panic-injection + introspection for the
// fallback init-state unwind guard (`InitStateGuard` above). These let a test
// deterministically force an unwind out of the guarded region of `heap_ptr`
// (between the UNINIT→INITIALIZING CAS and the READY publish) and prove the
// guard rolls `INIT_STATE` back to `UNINIT`, so a subsequent `heap_ptr` re-races
// the CAS instead of livelocking forever on a stuck `INITIALIZING`. Gated on
// `internals` (the `AtomicBool`/`AtomicU8` reads here do NOT touch allocator
// metadata through a raw pointer, so they are safe injection points, not
// `unsafe fn` — same surface as R34-15's `DBG_INJECT_CHUNK_OOM`).
// ===========================================================================

/// R34-17/task #536: test-only panic injection for the fallback init path. When
/// set, `heap_ptr` panics immediately after winning the UNINIT→INITIALIZING CAS
/// and BEFORE `HeapCore::new` — simulating an unwind out of the guarded region
/// so a test can exercise the `InitStateGuard` rollback. Process-global; a test
/// that sets it MUST clear it before returning (use [`dbg_init_state`] to assert
/// the post-panic state, and a Drop guard to clear the flag).
#[cfg(feature = "internals")]
static DBG_INJECT_FALLBACK_INIT_PANIC: AtomicBool = AtomicBool::new(false);

/// `#[doc(hidden)]` test hook (R34-17/task #536) — not part of the public API.
/// Sets/clears the fallback-init panic-injection flag. When set, the next
/// `heap_ptr` call that wins the init CAS panics before constructing the
/// `HeapCore`. The flag is process-global; a test that sets it MUST clear it
/// before returning (use a Drop guard) so subsequent tests are unaffected.
#[cfg(feature = "internals")]
#[doc(hidden)]
pub fn dbg_set_inject_fallback_init_panic(on: bool) {
    DBG_INJECT_FALLBACK_INIT_PANIC.store(on, Ordering::SeqCst);
}

/// `#[doc(hidden)]` test hook (R34-17/task #536) — not part of the public API.
/// Reads the current [`INIT_STATE`] value (`STATE_UNINIT` / `STATE_INITIALIZING`
/// / `STATE_READY`). Lets a test assert the precondition (UNINIT, before any
/// fallback init has run) and the post-panic/post-recovery state.
#[cfg(feature = "internals")]
#[doc(hidden)]
#[must_use]
pub fn dbg_init_state() -> u8 {
    INIT_STATE.load(Ordering::Acquire)
}

/// `#[doc(hidden)]` test hook (R34-17/task #536) — not part of the public API.
/// Exercises the panic-safety of the fallback init-state guard: it arms the
/// injection flag, invokes [`heap_ptr`] (which panics inside the guarded
/// region, caught here via `catch_unwind`), then — with the flag CLEARED —
/// invokes [`heap_ptr`] a SECOND time and reports whether that second call
/// completed. Before the `InitStateGuard`, the panicking first call left
/// `INIT_STATE` stuck at `INITIALIZING` forever, so the second `heap_ptr` would
/// spin in the loser loop indefinitely (process-wide livelock); the test
/// verifies that instead it returns promptly (guard rolled the state back to
/// `UNINIT` while unwinding). Returns `true` iff the second `heap_ptr` returned
/// a non-null pointer — i.e. the init state was NOT wedged and re-init succeeded.
///
/// ## Precondition
///
/// The hook REQUIRES [`INIT_STATE`] to be `UNINIT` on entry (the fallback must
/// not already be initialised — otherwise `heap_ptr`'s fast path returns before
/// the guarded region and the injection is never reached, making the test
/// vacuous). In a dedicated test binary that does NOT install `SeferAlloc` as
/// the global allocator, the fallback is never hit by the harness, so this holds
/// trivially. If it does not hold, the hook returns `true` vacuously (the test
/// should additionally assert the precondition via [`dbg_init_state`] to catch a
/// vacuous run).
///
/// The whole call runs on the current thread with no `sleep`/polling: if the
/// state were wedged the second `heap_ptr` would hang here (the test wraps this
/// in its own bounded watchdog thread, so a regression surfaces as a timeout
/// rather than a literal forever-hang) — exactly mirroring
/// [`dbg_panic_in_with_heap_releases_lock`]'s structure one function above.
#[cfg(all(feature = "std", feature = "internals"))]
#[doc(hidden)]
#[must_use]
pub fn dbg_panic_in_fallback_init_rolls_back() -> bool {
    // Precondition: the fallback must not already be initialised, or the
    // injection is unreachable (the fast path returns before the CAS).
    if INIT_STATE.load(Ordering::Acquire) != STATE_UNINIT {
        // Vacuously "ok" — the test should independently assert the
        // precondition via `dbg_init_state()` so a vacuous run is caught.
        return true;
    }
    // Arm the injection: the next `heap_ptr` winner panics inside the guard.
    DBG_INJECT_FALLBACK_INIT_PANIC.store(true, Ordering::SeqCst);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Discard the pointer — we only care that the call panics (and that the
        // guard rolls the state back). The pointer is never dereferenced.
        let _ = heap_ptr();
    }))
    .is_err();
    // Clear the injection BEFORE the second call so the re-init can succeed.
    DBG_INJECT_FALLBACK_INIT_PANIC.store(false, Ordering::SeqCst);
    if !panicked {
        // The init was expected to panic; if it did not, the hook is broken.
        return false;
    }
    // Second call: if the guard rolled `INIT_STATE` back to `UNINIT` (the fix),
    // this re-races the CAS and completes (non-null). If the state is wedged at
    // `INITIALIZING` (regression), this spins forever — the test's watchdog
    // aborts the process after a timeout.
    !heap_ptr().is_null()
}
