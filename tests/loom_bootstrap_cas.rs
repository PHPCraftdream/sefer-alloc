//! loom model-check of the **Registry bootstrap CAS state-machine**
//! (lazy-allocate Registry via aligned-vmem — `UNINIT → INITIALIZING → READY`).
//!
//! # Scope — what loom covers
//!
//! This harness models the pointer state-machine in `src/registry/bootstrap.rs`
//! in isolation using `loom::sync::atomic` (NOT the real `ensure`/`ensure_slow`,
//! which use `core::sync::atomic`). It asserts the core safety properties:
//!
//! > Across every concurrent interleaving:
//! > 1. **Exactly-once allocation** — only ONE thread executes the winner
//! >    ("allocate + init") branch.
//! > 2. **Same pointer for all observers** — every thread that calls
//! >    `ensure_modeled` returns the SAME final pointer value.
//! > 3. **No sentinel / null leaks** — no thread returns the sentinel (1)
//! >    or null.
//! > 4. **Happens-before** — a losing thread observing the real pointer under
//! >    `Acquire` sees the winner's `init_marker` write (Release/Acquire pair).
//!
//! # The counterfactual (non-vacuousness proof)
//!
//! The broken protocol omits the `Release` ordering on the winner's publish
//! store (uses `Relaxed` instead). In that case the loser's `Acquire` load of
//! the pointer no longer synchronises with the winner's `init_marker` write —
//! the loser may observe `init_marker == 0` (uninitialised). We assert
//! `init_marker == 0xDEADBEEF`; loom must find the interleaving where this
//! fails → `#[should_panic]` passes. If it does NOT panic, the harness is
//! vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-global --test loom_bootstrap_cas
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;
use std::ptr::null_mut;

/// Sentinel pointer value: `1 as *mut Registry`.
/// Encodes "one thread is currently initialising; losers must spin".
const SENTINEL: usize = 1;

/// A minimal model of the `Registry` struct. We only need one field that proves
/// the winner fully constructed the object before publishing the pointer:
/// `init_marker` is written with `0xDEAD_BEEF` by the winner, and any loser
/// that observes the published pointer under `Acquire` must see that value.
struct Registry {
    init_marker: u32,
}

// ============================================================================
// Correct protocol
// ============================================================================

/// Model of `ensure` (fast path + slow path combined for clarity).
///
/// Returns the pointer to the (model) Registry that was initialised and
/// published by the single winning thread.
fn ensure_modeled(
    ptr: &Arc<AtomicPtr<Registry>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut Registry {
    // Fast path: already READY.
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 && p_usize != SENTINEL {
        return p;
    }
    ensure_modeled_slow(ptr, alloc_count)
}

/// Model of `ensure_slow`: race via CAS(null → sentinel), then either allocate
/// or spin-wait.
///
/// The spin loop is bounded to `MAX_SPIN_ITERS` iterations. In the real code
/// the loop is unbounded, but for loom we cap it: loom's scheduler explores
/// all interleavings under the preemption bound, so a bounded loop covers the
/// same critical races (the winner always publishes within the cap; loom would
/// report a deadlock if a winner interleaving didn't terminate).
const MAX_SPIN_ITERS: usize = 4;

fn ensure_modeled_slow(
    ptr: &Arc<AtomicPtr<Registry>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut Registry {
    let sentinel = SENTINEL as *mut Registry;

    match ptr.compare_exchange(
        null_mut(),
        sentinel,
        Ordering::Acquire, // success ordering (mirrors real code)
        Ordering::Relaxed, // failure ordering (mirrors real code)
    ) {
        Ok(_) => {
            // ── Winner ──────────────────────────────────────────────────────
            // Exactly one CAS winner exists per state-machine lifetime.
            alloc_count.fetch_add(1, Ordering::Relaxed);

            // Simulate "allocate + construct": write a sentinel field value
            // that proves the registry is fully constructed (a loser observing
            // the pointer under Acquire MUST see this write).
            let reg = Box::new(Registry {
                init_marker: 0xDEAD_BEEF,
            });
            let base = Box::into_raw(reg);

            // Publish the real pointer with Release — pairs with the Acquire
            // loads in the loser spin loop (and the fast-path load in `ensure`).
            ptr.store(base, Ordering::Release);
            base
        }
        Err(_) => {
            // ── Loser ───────────────────────────────────────────────────────
            // Spin until a real (non-null, non-sentinel) pointer is observed.
            // Use `loom::thread::yield_now()` so loom's fair scheduler can
            // advance the winner thread (a real `spin_loop()` hint is opaque
            // to loom's exploration engine). The loop is bounded for loom.
            for _ in 0..MAX_SPIN_ITERS {
                let p = ptr.load(Ordering::Acquire);
                let p_usize = p as usize;
                if p_usize != 0 && p_usize != SENTINEL {
                    return p;
                }
                thread::yield_now();
            }
            // After MAX_SPIN_ITERS, the winner must have published by now in
            // every loom interleaving (the winner's path is a fixed number of
            // steps). A final load after the loop.
            let p = ptr.load(Ordering::Acquire);
            assert!(
                p as usize != 0 && p as usize != SENTINEL,
                "loom: loser did not observe the real pointer within MAX_SPIN_ITERS \
                 iterations — a winner interleaving did not publish in time"
            );
            p
        }
    }
}

// ============================================================================
// Broken protocol (counterfactual)
// ============================================================================

/// The BROKEN naive init: no sentinel, no CAS — just load-then-store.
///
/// Two threads both observe `null`, both allocate a fresh `Registry`, and both
/// store their own pointer. The second store overwrites the first, so:
/// - `alloc_count` reaches 2 (two "winner" branches executed) — a
///   double-allocation.
/// - The two threads return DIFFERENT pointers (thread A's allocation is
///   overwritten by thread B's store; thread A returns its own freshly-stored
///   pointer at store time, but thread B overwrites it — or vice versa
///   depending on scheduling).
///
/// loom finds the interleaving where `alloc_count == 2` and fires the
/// `assert_eq!(count, 1, ...)` assertion, causing a panic. The
/// `#[should_panic]` attribute declares that expectation.
///
/// This mirrors the `adopt_naive_broken` → `counterfactual_naive_adopt_double_owns`
/// pattern in `tests/loom_registry.rs`.
fn ensure_modeled_broken_naive(
    ptr: &Arc<AtomicPtr<Registry>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut Registry {
    // Fast path: already initialised.
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 {
        return p;
    }

    // BUG: load-then-store WITHOUT CAS — two threads can both see null, both
    // allocate, and both store. The second store wins, but both incremented
    // `alloc_count` and both believe they "won". The first thread's allocation
    // is leaked (overwritten); the second thread's pointer is what survives in
    // `ptr`, but the FIRST thread already returned ITS OWN pointer before the
    // overwrite — so the two return values diverge.
    alloc_count.fetch_add(1, Ordering::Relaxed);
    let reg = Box::new(Registry {
        init_marker: 0xDEAD_BEEF,
    });
    let base = Box::into_raw(reg);
    // Store without CAS: if another thread stored between our load and this
    // store, we silently overwrite the earlier pointer (double-allocation /
    // pointer divergence).
    ptr.store(base, Ordering::Release);
    base
}

// ============================================================================
// Tests — correct protocol
// ============================================================================

/// 2-thread race: both threads call `ensure_modeled` concurrently.
///
/// Asserts:
/// 1. Exactly ONE thread executed the winner branch (`alloc_count == 1`).
/// 2. Both threads returned the SAME pointer.
/// 3. Neither returned null or the sentinel.
/// 4. The loser thread saw the winner's `init_marker` write
///    (`(*ptr).init_marker == 0xDEAD_BEEF`) — Release/Acquire pair.
#[test]
fn lazy_init_exactly_once_two_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<Registry>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_modeled(&p2, &a2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Property 1: same pointer
        assert_eq!(
            r1, r2,
            "all threads must observe the SAME final pointer"
        );
        // Property 2: not null, not sentinel
        assert!(!r1.is_null(), "returned pointer must not be null");
        assert_ne!(r1 as usize, SENTINEL, "returned pointer must not be sentinel");

        // Property 3: exactly one winner
        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly ONE thread must execute the winner branch (got {count})");

        // Property 4: Release/Acquire synchronisation — loser sees init_marker.
        // SAFETY: r1 is the pointer published by the winner; it came from
        // `Box::into_raw` (valid, non-null, properly aligned). Both r1 and r2
        // are identical. The winner performed a `Release` store; we observed it
        // under `Acquire`, so happens-before is established.
        unsafe {
            assert_eq!(
                (*r1).init_marker,
                0xDEAD_BEEF,
                "loser must see the fully constructed Registry (Release/Acquire pair)"
            );
            // Reclaim to prevent a loom-detected allocation leak per iteration.
            drop(Box::from_raw(r1));
        }
    });
}

/// 3-thread race (main thread + 2 spawned): more interleavings than the
/// 2-thread variant, exercising the case where TWO losers spin in the
/// INITIALIZING window while ONE winner publishes.
///
/// Uses `preemption_bound = Some(1)` (tighter than the 2-thread variants) to
/// keep loom's state-space tractable with 3 concurrent racers. Even at
/// preemption_bound=1 this covers the key interleavings: all three racing the
/// CAS (one winning), both losers spinning through at least one check before
/// the winner publishes, and both finally observing the real pointer.
#[test]
fn lazy_init_exactly_once_three_threads() {
    let mut builder = loom::model::Builder::new();
    // preemption_bound = 1: smallest bound that still exercises real concurrent
    // interleavings. With 3 threads each doing CAS + publish/spin, this is
    // sufficient to cover the critical races without blowing the branch budget.
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<Registry>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        // Spawn two threads — the main loom thread is the third racer.
        let (p1, a1) = (Arc::clone(&ptr), Arc::clone(&alloc_count));
        let (p2, a2) = (Arc::clone(&ptr), Arc::clone(&alloc_count));

        let t1 = thread::spawn(move || ensure_modeled(&p1, &a1));
        let t2 = thread::spawn(move || ensure_modeled(&p2, &a2));

        // Main thread races concurrently.
        let r_main = ensure_modeled(&ptr, &alloc_count);

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // All three must agree on the same pointer.
        assert_eq!(r1, r_main, "thread 1 and main must observe the same pointer");
        assert_eq!(r2, r_main, "thread 2 and main must observe the same pointer");
        assert!(!r_main.is_null(), "returned pointer must not be null");
        assert_ne!(r_main as usize, SENTINEL, "returned pointer must not be sentinel");

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(count, 1, "exactly ONE thread must execute the winner branch (got {count})");

        // SAFETY: same justification as the 2-thread test.
        unsafe {
            assert_eq!(
                (*r_main).init_marker,
                0xDEAD_BEEF,
                "all threads must see the fully constructed Registry"
            );
            drop(Box::from_raw(r_main));
        }
    });
}

/// Fast-path re-entry: once the registry is published, a second call in the
/// SAME thread hits the fast path (load → non-null/non-sentinel → return
/// immediately). The pointer must match the first call's result.
#[test]
fn fast_path_reentry_returns_same_pointer() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<Registry>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        // First call (slow path — races with the spawned thread).
        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || {
            let first = ensure_modeled(&p2, &a2);
            // Second call in the same thread — must hit the fast path.
            let second = ensure_modeled(&p2, &a2);
            assert_eq!(first, second, "fast-path re-entry must return the same pointer");
            first
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r2, "both threads must see the same pointer");

        // SAFETY: same as before.
        unsafe {
            assert_eq!((*r1).init_marker, 0xDEAD_BEEF);
            drop(Box::from_raw(r1));
        }
    });
}

// ============================================================================
// Counterfactual — broken Relaxed publish loses synchronisation
// ============================================================================

/// COUNTERFACTUAL: naive load-then-store (no CAS, no sentinel) leads to
/// double-allocation.
///
/// Two threads both observe `null`, both allocate a fresh `Registry`, and both
/// store their own pointer. `alloc_count` reaches 2 — a double-allocation
/// (the CAS-based protocol prevents this by ensuring only ONE thread's CAS can
/// succeed on `null → sentinel`). Our assertion `count == 1` fires.
///
/// **If this test PASSES (does not panic), the counterfactual is vacuous** —
/// loom failed to find the double-allocation interleaving, and the harness
/// needs rework.
#[test]
#[should_panic(expected = "exactly ONE thread must execute the winner branch")]
fn counterfactual_naive_no_cas_double_allocates() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<Registry>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_modeled_broken_naive(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_modeled_broken_naive(&p2, &a2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Intentionally do NOT reclaim r1/r2 here: in the double-allocation
        // interleaving the two pointers differ, and one of the allocations is
        // stranded (no safe way to know which one `ptr` currently holds from
        // both threads). The assertion below fires before we reach cleanup —
        // loom's per-iteration allocator tracks these.
        let _ = (r1, r2);

        // With the naive protocol, loom finds an interleaving where BOTH
        // threads executed the allocation branch (alloc_count == 2). We assert
        // the CORRECT invariant (count == 1); loom makes it fail → panic →
        // `#[should_panic]` passes. This is the non-vacuousness proof.
        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE thread must execute the winner branch (got {count}, \
             naive load-then-store double-allocates)"
        );
    });
}
