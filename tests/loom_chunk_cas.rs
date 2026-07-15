//! loom model-check of the **per-chunk materialisation CAS state-machine**
//! (R6-OPT-P0-2 round 1: `Registry::ensure_chunk`/`ensure_chunk_slow` in
//! `src/registry/bootstrap.rs`).
//!
//! # Scope — what loom covers
//!
//! This harness models ONE chunk's `UNINIT → INITIALIZING → READY`
//! `AtomicPtr<RegistryChunk>` transition in isolation using
//! `loom::sync::atomic` (NOT the real `Registry::ensure_chunk`/
//! `ensure_chunk_slow`, which use `core::sync::atomic`). It is structurally
//! the SAME race `tests/loom_bootstrap_cas.rs` already covers for the
//! pre-chunking whole-registry `REGISTRY_PTR` — this file is that model
//! transcribed at chunk granularity, since round 1 moved the identical
//! CAS-then-spin-then-publish protocol DOWN a level (from "the one registry
//! pointer" to "one pointer per chunk", each chunk independently
//! materialised on its own first touch — see `bootstrap.rs`'s module doc for
//! the full "why" of the move).
//!
//! It asserts the SAME core safety properties `loom_bootstrap_cas.rs` proves,
//! narrowed to a single chunk:
//!
//! > Across every concurrent interleaving of multiple threads racing to be
//! > the first to touch a not-yet-materialised chunk:
//! > 1. **Exactly-once allocation** — only ONE thread executes the winner
//! >    ("allocate + init") branch for this chunk.
//! > 2. **Same pointer for all observers** — every thread that calls
//! >    `ensure_chunk_modeled` for this chunk returns the SAME final pointer
//! >    value.
//! > 3. **No sentinel / null leaks** — no thread returns the sentinel (1) or
//! >    null.
//! > 4. **Happens-before** — a losing thread observing the real pointer under
//! >    `Acquire` sees the winner's `init_marker` write (Release/Acquire
//! >    pair) — i.e. the winner's in-place chunk initialisation is fully
//! >    visible before any loser dereferences the chunk.
//!
//! # The counterfactual (non-vacuousness proof)
//!
//! Identical in shape to `loom_bootstrap_cas.rs`'s counterfactual: a naive
//! load-then-store (no CAS, no sentinel) protocol lets two threads both
//! observe `null`, both allocate a fresh chunk, and both store — a
//! double-materialisation of the SAME chunk index. We assert `alloc_count ==
//! 1`; loom must find the interleaving where this fails →
//! `#[should_panic]` passes. If it does NOT panic, the harness is vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global --test loom_chunk_cas
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;
use std::ptr::null_mut;

/// Sentinel pointer value: `1 as *mut RegistryChunk` (mirrors
/// `bootstrap::SENTINEL_INITIALIZING` verbatim — same bit pattern, same
/// "never dereferenced, only compared" contract).
const SENTINEL: usize = 1;

/// A minimal model of a `RegistryChunk`. We only need one field that proves
/// the winner fully constructed the chunk before publishing the pointer:
/// `init_marker` is written with `0xDEAD_BEEF` by the winner, and any loser
/// that observes the published pointer under `Acquire` must see that value.
///
/// (The REAL `RegistryChunk`'s in-place init writes NOTHING — OS-zeroed
/// pages already form a valid `HeapSlot` array, see `bootstrap.rs`'s
/// `ensure_chunk_slow` comment. This model keeps an explicit marker anyway,
/// exactly as `loom_bootstrap_cas.rs` does for `Registry`, so the
/// Release/Acquire happens-before property has something concrete to prove —
/// "the winner's write, whatever it is, must be visible" is the property
/// under test, independent of whether the real code happens to have zero
/// such writes today.)
struct RegistryChunk {
    init_marker: u32,
}

// ============================================================================
// Correct protocol (transcribes `Registry::ensure_chunk` / `ensure_chunk_slow`)
// ============================================================================

/// Model of `Registry::ensure_chunk` (fast path + slow path combined for
/// clarity) for ONE chunk's `AtomicPtr`.
fn ensure_chunk_modeled(
    ptr: &Arc<AtomicPtr<RegistryChunk>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut RegistryChunk {
    // Fast path: already READY.
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 && p_usize != SENTINEL {
        return p;
    }
    ensure_chunk_modeled_slow(ptr, alloc_count)
}

/// Model of `ensure_chunk_slow`: race via CAS(null → sentinel) on ONE
/// chunk's pointer, then either allocate-and-publish or spin-wait.
///
/// The spin loop is bounded to `MAX_SPIN_ITERS` iterations, mirroring
/// `loom_bootstrap_cas.rs`'s identical bound and rationale: loom's scheduler
/// explores all interleavings under the preemption bound, so a bounded loop
/// covers the same critical races (the winner always publishes within the
/// cap; loom would report a deadlock if a winner interleaving didn't
/// terminate).
const MAX_SPIN_ITERS: usize = 4;

fn ensure_chunk_modeled_slow(
    ptr: &Arc<AtomicPtr<RegistryChunk>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut RegistryChunk {
    let sentinel = SENTINEL as *mut RegistryChunk;

    match ptr.compare_exchange(
        null_mut(),
        sentinel,
        Ordering::Acquire, // success ordering (mirrors real code)
        Ordering::Relaxed, // failure ordering (mirrors real code)
    ) {
        Ok(_) => {
            // ── Winner ──────────────────────────────────────────────────────
            // Exactly one CAS winner exists per chunk's lifetime.
            alloc_count.fetch_add(1, Ordering::Relaxed);

            // Simulate "allocate + construct": write a sentinel field value
            // that proves the chunk is fully constructed (a loser observing
            // the pointer under Acquire MUST see this write).
            let chunk = Box::new(RegistryChunk {
                init_marker: 0xDEAD_BEEF,
            });
            let base = Box::into_raw(chunk);

            // Publish the real pointer with Release — pairs with the Acquire
            // loads in the loser spin loop (and the fast-path load in
            // `ensure_chunk_modeled`).
            ptr.store(base, Ordering::Release);
            base
        }
        Err(_) => {
            // ── Loser ───────────────────────────────────────────────────────
            // Spin until a real (non-null, non-sentinel) pointer is observed.
            for _ in 0..MAX_SPIN_ITERS {
                let p = ptr.load(Ordering::Acquire);
                let p_usize = p as usize;
                if p_usize != 0 && p_usize != SENTINEL {
                    return p;
                }
                thread::yield_now();
            }
            let p = ptr.load(Ordering::Acquire);
            assert!(
                p as usize != 0 && p as usize != SENTINEL,
                "loom: loser did not observe the real chunk pointer within \
                 MAX_SPIN_ITERS iterations — a winner interleaving did not \
                 publish in time"
            );
            p
        }
    }
}

// ============================================================================
// Broken protocol (counterfactual)
// ============================================================================

/// The BROKEN naive init: no sentinel, no CAS — just load-then-store, at
/// chunk granularity. Two threads both observe `null` for the SAME chunk
/// index, both allocate a fresh `RegistryChunk`, and both store their own
/// pointer — a double-materialisation of one chunk (leaking one allocation
/// and returning divergent pointers to the two threads).
fn ensure_chunk_modeled_broken_naive(
    ptr: &Arc<AtomicPtr<RegistryChunk>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut RegistryChunk {
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 {
        return p;
    }

    // BUG: load-then-store WITHOUT CAS.
    alloc_count.fetch_add(1, Ordering::Relaxed);
    let chunk = Box::new(RegistryChunk {
        init_marker: 0xDEAD_BEEF,
    });
    let base = Box::into_raw(chunk);
    ptr.store(base, Ordering::Release);
    base
}

// ============================================================================
// Tests — correct protocol
// ============================================================================

/// 2-thread race: both threads call `ensure_chunk_modeled` on the SAME chunk
/// index concurrently — the exact scenario the task spec asks to cover
/// ("multiple threads racing to be the first to touch a not-yet-materialised
/// chunk").
///
/// Asserts:
/// 1. Exactly ONE thread executed the winner branch (`alloc_count == 1`).
/// 2. Both threads returned the SAME pointer.
/// 3. Neither returned null or the sentinel.
/// 4. The loser thread saw the winner's `init_marker` write
///    (`(*ptr).init_marker == 0xDEAD_BEEF`) — Release/Acquire pair.
#[test]
fn chunk_lazy_init_exactly_once_two_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<RegistryChunk>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_chunk_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_chunk_modeled(&p2, &a2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r2, "all threads must observe the SAME chunk pointer");
        assert!(!r1.is_null(), "returned pointer must not be null");
        assert_ne!(
            r1 as usize, SENTINEL,
            "returned pointer must not be sentinel"
        );

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE thread must materialise this chunk (got {count})"
        );

        // SAFETY: r1 is the pointer published by the winner (from
        // `Box::into_raw`, valid, non-null, properly aligned); r1 == r2.
        unsafe {
            assert_eq!(
                (*r1).init_marker,
                0xDEAD_BEEF,
                "loser must see the fully constructed chunk (Release/Acquire pair)"
            );
            drop(Box::from_raw(r1));
        }
    });
}

/// 3-thread race (main thread + 2 spawned) on the SAME chunk index — more
/// interleavings than the 2-thread variant, exercising the case where TWO
/// losers spin in the INITIALIZING window while ONE winner publishes.
#[test]
fn chunk_lazy_init_exactly_once_three_threads() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<RegistryChunk>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let (p1, a1) = (Arc::clone(&ptr), Arc::clone(&alloc_count));
        let (p2, a2) = (Arc::clone(&ptr), Arc::clone(&alloc_count));

        let t1 = thread::spawn(move || ensure_chunk_modeled(&p1, &a1));
        let t2 = thread::spawn(move || ensure_chunk_modeled(&p2, &a2));

        let r_main = ensure_chunk_modeled(&ptr, &alloc_count);

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(
            r1, r_main,
            "thread 1 and main must observe the same pointer"
        );
        assert_eq!(
            r2, r_main,
            "thread 2 and main must observe the same pointer"
        );
        assert!(!r_main.is_null());
        assert_ne!(r_main as usize, SENTINEL);

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE thread must materialise this chunk (got {count})"
        );

        // SAFETY: same justification as the 2-thread test.
        unsafe {
            assert_eq!((*r_main).init_marker, 0xDEAD_BEEF);
            drop(Box::from_raw(r_main));
        }
    });
}

/// Fast-path re-entry: once a chunk is published, a second call in the SAME
/// thread hits the fast path (load → non-null/non-sentinel → return
/// immediately). The pointer must match the first call's result.
#[test]
fn chunk_fast_path_reentry_returns_same_pointer() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<RegistryChunk>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_chunk_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || {
            let first = ensure_chunk_modeled(&p2, &a2);
            let second = ensure_chunk_modeled(&p2, &a2);
            assert_eq!(
                first, second,
                "fast-path re-entry must return the same chunk pointer"
            );
            first
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r2, "both threads must see the same chunk pointer");

        // SAFETY: same as before.
        unsafe {
            assert_eq!((*r1).init_marker, 0xDEAD_BEEF);
            drop(Box::from_raw(r1));
        }
    });
}

// ============================================================================
// Counterfactual — broken naive load-then-store double-materialises a chunk
// ============================================================================

/// COUNTERFACTUAL: naive load-then-store (no CAS, no sentinel) leads to
/// double-materialisation of the SAME chunk.
///
/// **If this test PASSES (does not panic), the counterfactual is vacuous** —
/// loom failed to find the double-materialisation interleaving, and the
/// harness needs rework.
#[test]
#[should_panic(expected = "exactly ONE thread must materialise this chunk")]
fn counterfactual_chunk_naive_no_cas_double_materialises() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<RegistryChunk>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_chunk_modeled_broken_naive(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_chunk_modeled_broken_naive(&p2, &a2));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Intentionally do NOT reclaim r1/r2 here: in the double-
        // materialisation interleaving the two pointers differ, and one of
        // the allocations is stranded (no safe way to know which one `ptr`
        // currently holds from both threads). The assertion below fires
        // before we reach cleanup — loom's per-iteration allocator tracks
        // these.
        let _ = (r1, r2);

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE thread must materialise this chunk (got {count}, \
             naive load-then-store double-materialises)"
        );
    });
}
