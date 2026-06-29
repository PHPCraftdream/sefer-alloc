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
//! as the registry bootstrap. Under `alloc-xthread`, [`HeapCore::install_thread_free`]
//! allocates a `Box` (via `std::alloc`); this happens on the FIRST fallback
//! allocation, which is by definition not inside the registry bootstrap (it
//! is inside the alloc face's alloc path). If that `Box` fails (global OOM
//! during the first fallback alloc), the fallback proceeds without a TFS —
//! own-thread allocs still work, cross-thread frees are a safe no-op. M10
//! is preserved.
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
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::registry::HeapCore;

/// Bootstrap-state values (mirrors `registry::bootstrap`).
const STATE_UNINIT: u8 = 0;
const STATE_INITIALIZING: u8 = 1;
const STATE_READY: u8 = 2;

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
    // Fast path: already READY. Acquire to see the init thread's writes.
    if INIT_STATE.load(Ordering::Acquire) == STATE_READY {
        // SAFETY: we observed READY under Acquire, which synchronises with
        // the initialising thread's Release store of READY. `FALLBACK` is
        // fully initialised and never dropped; the pointer is valid for the
        // process lifetime. We use `addr_of_mut!` to obtain the pointer
        // WITHOUT creating a mutable reference to the `static mut` (which
        // is UB under Rust 2024's `static_mut_refs` rule).
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
        // We are the sole initialiser. Construct the HeapCore in place.
        // `HeapCore::new` uses the sentinel id `u32::MAX` ("not bound to a
        // registry slot") — the fallback is NOT a registry slot; it is a
        // standalone process-global heap.
        match HeapCore::new(u32::MAX) {
            Some(hc) => {
                // SAFETY: we won the init race (STATE_INITIALIZING); no
                // other thread can read `FALLBACK` until we publish READY.
                // Writing a `HeapCore` into the `MaybeUninit` initialises
                // it. `addr_of_mut!` gives us the `*mut MaybeUninit<HeapCore>`
                // destination without creating a `&mut` to the `static mut`
                // (Rust 2024 `static_mut_refs`); we cast to `*mut HeapCore`
                // for the `write`.
                unsafe { (addr_of_mut!(FALLBACK) as *mut HeapCore).write(hc) };
                INIT_STATE.store(STATE_READY, Ordering::Release);
            }
            None => {
                // Primordial OOM. Roll back to UNINIT so a later caller can
                // retry (the OS may have freed memory by then). Return null
                // — the alloc face will surface this as true OOM.
                INIT_STATE.store(STATE_UNINIT, Ordering::Release);
                return core::ptr::null_mut();
            }
        }
    } else {
        // Lost the race: spin until READY.
        while INIT_STATE.load(Ordering::Acquire) != STATE_READY {
            core::hint::spin_loop();
        }
    }
    // SAFETY: READY observed (we either just published it or spun until it
    // appeared). `FALLBACK` is initialised and lives for the process
    // lifetime. `addr_of_mut!` avoids creating a mutable reference to the
    // `static mut` (Rust 2024 `static_mut_refs`).
    addr_of_mut!(FALLBACK) as *mut HeapCore
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
    acquire_lock();
    // SAFETY: `heap` is non-null and points to the initialised `FALLBACK`
    // `HeapCore`. `LOCK` is held (we just acquired it), giving us exclusive
    // `&mut` access — no other thread can be inside `with_heap`. The
    // `HeapCore` is valid for the process lifetime (never dropped).
    let result = f(unsafe { &mut *heap });
    release_lock();
    Some(result)
}

/// Acquire the fallback spinlock. Spins until `LOCK` flips false → true.
fn acquire_lock() {
    while LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        // Spin with a PAUSE/YIELD hint. Contention on the fallback is
        // essentially impossible (the windows that route here are rare and
        // typically single-threaded), so the spin is academic.
        core::hint::spin_loop();
    }
}

/// Release the fallback spinlock.
fn release_lock() {
    LOCK.store(false, Ordering::Release);
}
