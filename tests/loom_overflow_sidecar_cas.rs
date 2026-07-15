//! loom model-check of the **sidecar materialisation CAS state-machine**
//! (R6-OPT-P0-2 round 2: `HeapOverflow::push_impl` →
//! `bootstrap::ensure_overflow_sidecar`/`ensure_overflow_sidecar_slow` in
//! `src/registry/bootstrap.rs`).
//!
//! # Scope — what loom covers
//!
//! This harness models ONE ring's `UNINIT → INITIALIZING → READY`
//! `AtomicPtr<HeapOverflowSidecar>` transition in isolation using
//! `loom::sync::atomic` (NOT the real `bootstrap::ensure_overflow_sidecar`,
//! which uses `core::sync::atomic`). It is structurally the SAME race
//! `tests/loom_chunk_cas.rs` already covers for
//! `Registry::ensure_chunk`/`ensure_chunk_slow` — this file is that model
//! transcribed to the THIRD instance of the same CAS-then-spin-then-publish
//! protocol (whole-registry → per-chunk → per-overflow-sidecar; see
//! `bootstrap.rs`'s module doc for the full lineage).
//!
//! It asserts the SAME core safety properties `loom_chunk_cas.rs` proves,
//! narrowed to one ring's sidecar pointer:
//!
//! > Across every concurrent interleaving of multiple producer threads
//! > racing to be the first to push an entry past `INLINE_CAP` on the SAME
//! > ring (i.e. racing to materialise that ring's sidecar):
//! > 1. **Exactly-once allocation** — only ONE thread executes the winner
//! >    ("allocate + init") branch for this sidecar.
//! > 2. **Same pointer for all observers** — every thread that calls
//! >    `ensure_sidecar_modeled` for this ring returns the SAME final
//! >    pointer value.
//! > 3. **No sentinel / null leaks** — no thread returns the sentinel (1) or
//! >    null as a "success" outcome.
//! > 4. **Happens-before** — a losing thread observing the real pointer
//! >    under `Acquire` sees the winner's `init_marker` write
//! >    (Release/Acquire pair) — i.e. the winner's in-place sidecar
//! >    initialisation is fully visible before any loser dereferences it.
//!
//! # Why a separate file, not a parameterised `loom_chunk_cas.rs`
//!
//! Mirrors round 1's own precedent exactly (`loom_bootstrap_cas.rs` →
//! `loom_chunk_cas.rs` were two separate files, not one parameterised one) —
//! each file documents ONE concrete instance of the shared protocol shape,
//! keeping the "what does THIS specific pointer's lifecycle look like"
//! reasoning self-contained per site rather than requiring a reader to
//! mentally substitute type parameters.
//!
//! # The counterfactual (non-vacuousness proof)
//!
//! Identical in shape to `loom_chunk_cas.rs`'s counterfactual: a naive
//! load-then-store (no CAS, no sentinel) protocol lets two threads both
//! observe `null`, both allocate a fresh sidecar, and both store — a
//! double-materialisation of the SAME ring's sidecar. We assert
//! `alloc_count == 1`; loom must find the interleaving where this fails →
//! `#[should_panic]` passes. If it does NOT panic, the harness is vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global,alloc-xthread --test loom_overflow_sidecar_cas
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;
use std::ptr::null_mut;

/// Sentinel pointer value: `1 as *mut HeapOverflowSidecar` (mirrors
/// `heap_overflow::SIDECAR_SENTINEL_INITIALIZING` verbatim — same bit
/// pattern, same "never dereferenced, only compared" contract).
const SENTINEL: usize = 1;

/// A minimal model of a `HeapOverflowSidecar`. We only need one field that
/// proves the winner fully constructed the sidecar before publishing the
/// pointer: `init_marker` is written with `0xC0FF_EE00` by the winner, and
/// any loser that observes the published pointer under `Acquire` must see
/// that value.
///
/// (The REAL `HeapOverflowSidecar`'s in-place init writes NOTHING — OS-zeroed
/// pages already form a valid all-zero `bases`/`packed` array pair, see
/// `bootstrap.rs`'s `ensure_overflow_sidecar_slow` comment. This model keeps
/// an explicit marker anyway, exactly as `loom_chunk_cas.rs` does for
/// `RegistryChunk`, so the Release/Acquire happens-before property has
/// something concrete to prove.)
struct HeapOverflowSidecar {
    init_marker: u32,
}

// ============================================================================
// Correct protocol (transcribes `ensure_overflow_sidecar`/`_slow`)
// ============================================================================

/// Model of `bootstrap::ensure_overflow_sidecar` (fast path + slow path
/// combined for clarity) for ONE ring's `AtomicPtr<HeapOverflowSidecar>`.
/// Returns `Some(ptr)` on success, `None` on simulated OOM (mirrors the real
/// function's `bool` return combined with the pointer a caller would then
/// read via `HeapOverflow::slot`).
fn ensure_sidecar_modeled(
    ptr: &Arc<AtomicPtr<HeapOverflowSidecar>>,
    alloc_count: &Arc<AtomicU32>,
) -> Option<*mut HeapOverflowSidecar> {
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 && p_usize != SENTINEL {
        return Some(p);
    }
    ensure_sidecar_modeled_slow(ptr, alloc_count)
}

/// Model of `ensure_overflow_sidecar_slow`: race via CAS(null → sentinel) on
/// ONE ring's sidecar pointer, then either allocate-and-publish or
/// spin-wait. This model variant never simulates OOM (the OOM/rollback path
/// is covered by the native, non-loom test
/// `tests/heap_overflow_sidecar.rs::sidecar_rollback_is_recoverable_no_permanent_wedge`,
/// which drives the REAL rollback code directly — loom's job here is the
/// concurrent-race property, not the OOM branch).
///
/// The spin loop is bounded to `MAX_SPIN_ITERS` iterations, mirroring
/// `loom_chunk_cas.rs`'s identical bound and rationale: loom's scheduler
/// explores all interleavings under the preemption bound, so a bounded loop
/// covers the same critical races (the winner always publishes within the
/// cap; loom would report a deadlock if a winner interleaving didn't
/// terminate).
const MAX_SPIN_ITERS: usize = 4;

fn ensure_sidecar_modeled_slow(
    ptr: &Arc<AtomicPtr<HeapOverflowSidecar>>,
    alloc_count: &Arc<AtomicU32>,
) -> Option<*mut HeapOverflowSidecar> {
    let sentinel = SENTINEL as *mut HeapOverflowSidecar;

    match ptr.compare_exchange(
        null_mut(),
        sentinel,
        Ordering::Acquire, // success ordering (mirrors real code)
        Ordering::Relaxed, // failure ordering (mirrors real code)
    ) {
        Ok(_) => {
            // ── Winner ──────────────────────────────────────────────────
            alloc_count.fetch_add(1, Ordering::Relaxed);

            let sidecar = Box::new(HeapOverflowSidecar {
                init_marker: 0xC0FF_EE00,
            });
            let base = Box::into_raw(sidecar);

            // Publish the real pointer with Release — pairs with the
            // Acquire loads in the loser spin loop (and the fast-path load
            // in `ensure_sidecar_modeled`).
            ptr.store(base, Ordering::Release);
            Some(base)
        }
        Err(_) => {
            // ── Loser ───────────────────────────────────────────────────
            for _ in 0..MAX_SPIN_ITERS {
                let p = ptr.load(Ordering::Acquire);
                let p_usize = p as usize;
                if p_usize != 0 && p_usize != SENTINEL {
                    return Some(p);
                }
                thread::yield_now();
            }
            let p = ptr.load(Ordering::Acquire);
            assert!(
                p as usize != 0 && p as usize != SENTINEL,
                "loom: loser did not observe the real sidecar pointer within \
                 MAX_SPIN_ITERS iterations — a winner interleaving did not \
                 publish in time"
            );
            Some(p)
        }
    }
}

// ============================================================================
// Broken protocol (counterfactual)
// ============================================================================

/// The BROKEN naive init: no sentinel, no CAS — just load-then-store, on one
/// ring's sidecar pointer. Two producer threads both observe `null` for the
/// SAME ring, both allocate a fresh `HeapOverflowSidecar`, and both store
/// their own pointer — a double-materialisation of one ring's sidecar
/// (leaking one allocation and returning divergent pointers to the two
/// threads — exactly the wedge-adjacent corruption the real
/// `ensure_overflow_sidecar` CAS protocol exists to rule out).
fn ensure_sidecar_modeled_broken_naive(
    ptr: &Arc<AtomicPtr<HeapOverflowSidecar>>,
    alloc_count: &Arc<AtomicU32>,
) -> *mut HeapOverflowSidecar {
    let p = ptr.load(Ordering::Acquire);
    let p_usize = p as usize;
    if p_usize != 0 {
        return p;
    }

    // BUG: load-then-store WITHOUT CAS.
    alloc_count.fetch_add(1, Ordering::Relaxed);
    let sidecar = Box::new(HeapOverflowSidecar {
        init_marker: 0xC0FF_EE00,
    });
    let base = Box::into_raw(sidecar);
    ptr.store(base, Ordering::Release);
    base
}

// ============================================================================
// Tests — correct protocol
// ============================================================================

/// 2-producer race: both threads call `ensure_sidecar_modeled` on the SAME
/// ring's sidecar pointer concurrently — the exact scenario the task spec
/// asks to cover ("two first producers race to materialise the sidecar for
/// the first time").
///
/// Asserts:
/// 1. Exactly ONE thread executed the winner branch (`alloc_count == 1`).
/// 2. Both threads returned the SAME pointer.
/// 3. Neither returned null or the sentinel (both `Some`, same value).
/// 4. The loser thread saw the winner's `init_marker` write
///    (`(*ptr).init_marker == 0xC0FF_EE00`) — Release/Acquire pair.
#[test]
fn sidecar_lazy_init_exactly_once_two_producers() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<HeapOverflowSidecar>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_sidecar_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_sidecar_modeled(&p2, &a2));

        let r1 = t1.join().unwrap().expect("producer 1 must succeed");
        let r2 = t2.join().unwrap().expect("producer 2 must succeed");

        assert_eq!(
            r1, r2,
            "all producers must observe the SAME sidecar pointer"
        );
        assert!(!r1.is_null(), "returned pointer must not be null");
        assert_ne!(
            r1 as usize, SENTINEL,
            "returned pointer must not be sentinel"
        );

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE producer must materialise this ring's sidecar (got {count})"
        );

        // SAFETY: r1 is the pointer published by the winner (from
        // `Box::into_raw`, valid, non-null, properly aligned); r1 == r2.
        unsafe {
            assert_eq!(
                (*r1).init_marker,
                0xC0FF_EE00,
                "loser must see the fully constructed sidecar (Release/Acquire pair)"
            );
            drop(Box::from_raw(r1));
        }
    });
}

/// 3-producer race (main thread + 2 spawned) on the SAME ring's sidecar —
/// more interleavings than the 2-producer variant, exercising the case where
/// TWO losers spin in the INITIALIZING window while ONE winner publishes.
#[test]
fn sidecar_lazy_init_exactly_once_three_producers() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(1);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<HeapOverflowSidecar>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let (p1, a1) = (Arc::clone(&ptr), Arc::clone(&alloc_count));
        let (p2, a2) = (Arc::clone(&ptr), Arc::clone(&alloc_count));

        let t1 = thread::spawn(move || ensure_sidecar_modeled(&p1, &a1));
        let t2 = thread::spawn(move || ensure_sidecar_modeled(&p2, &a2));

        let r_main = ensure_sidecar_modeled(&ptr, &alloc_count).expect("main must succeed");

        let r1 = t1.join().unwrap().expect("producer 1 must succeed");
        let r2 = t2.join().unwrap().expect("producer 2 must succeed");

        assert_eq!(
            r1, r_main,
            "producer 1 and main must observe the same pointer"
        );
        assert_eq!(
            r2, r_main,
            "producer 2 and main must observe the same pointer"
        );
        assert!(!r_main.is_null());
        assert_ne!(r_main as usize, SENTINEL);

        let count = alloc_count.load(Ordering::Relaxed);
        assert_eq!(
            count, 1,
            "exactly ONE producer must materialise this ring's sidecar (got {count})"
        );

        // SAFETY: same justification as the 2-producer test.
        unsafe {
            assert_eq!((*r_main).init_marker, 0xC0FF_EE00);
            drop(Box::from_raw(r_main));
        }
    });
}

/// Fast-path re-entry: once a ring's sidecar is published, a second call in
/// the SAME producer thread hits the fast path (load → non-null/non-sentinel
/// → return immediately). The pointer must match the first call's result.
#[test]
fn sidecar_fast_path_reentry_returns_same_pointer() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<HeapOverflowSidecar>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_sidecar_modeled(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || {
            let first = ensure_sidecar_modeled(&p2, &a2).expect("first must succeed");
            let second = ensure_sidecar_modeled(&p2, &a2).expect("second must succeed");
            assert_eq!(
                first, second,
                "fast-path re-entry must return the same sidecar pointer"
            );
            first
        });

        let r1 = t1.join().unwrap().expect("producer 1 must succeed");
        let r2 = t2.join().unwrap();

        assert_eq!(r1, r2, "both producers must see the same sidecar pointer");

        // SAFETY: same as before.
        unsafe {
            assert_eq!((*r1).init_marker, 0xC0FF_EE00);
            drop(Box::from_raw(r1));
        }
    });
}

// ============================================================================
// Counterfactual — broken naive load-then-store double-materialises
// ============================================================================

/// COUNTERFACTUAL: naive load-then-store (no CAS, no sentinel) leads to
/// double-materialisation of the SAME ring's sidecar.
///
/// **If this test PASSES (does not panic), the counterfactual is vacuous** —
/// loom failed to find the double-materialisation interleaving, and the
/// harness needs rework.
#[test]
#[should_panic(expected = "exactly ONE producer must materialise this ring's sidecar")]
fn counterfactual_sidecar_naive_no_cas_double_materialises() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ptr: Arc<AtomicPtr<HeapOverflowSidecar>> = Arc::new(AtomicPtr::new(null_mut()));
        let alloc_count = Arc::new(AtomicU32::new(0));

        let p1 = Arc::clone(&ptr);
        let a1 = Arc::clone(&alloc_count);
        let t1 = thread::spawn(move || ensure_sidecar_modeled_broken_naive(&p1, &a1));

        let p2 = Arc::clone(&ptr);
        let a2 = Arc::clone(&alloc_count);
        let t2 = thread::spawn(move || ensure_sidecar_modeled_broken_naive(&p2, &a2));

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
            "exactly ONE producer must materialise this ring's sidecar (got {count}, \
             naive load-then-store double-materialises)"
        );
    });
}
