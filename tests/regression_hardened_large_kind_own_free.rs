//! F7 (task #25) — the HARDENED Large-segment kind guard on the OWN-THREAD
//! magazine free path (`HeapCore::dealloc_own_thread_with_base`).
//!
//! ## The hole this guards
//!
//! The own-thread fastbin free path keys entirely off the *layout*: if
//! `SizeClasses::class_for(layout)` returns `Some(c)`, it proceeds straight to
//! the magazine / bitmap M2 oracles WITHOUT ever consulting the segment's
//! `kind`. If a caller frees a pointer that actually lives in a LARGE segment
//! but passes a SMALL layout (a `GlobalAlloc`-contract violation — the real UB
//! is on the caller side), the oracles read the "bitmap"/magazine state out of
//! the bytes of the Large allocation's PAYLOAD. Those bytes can read as
//! "not-in-magazine, not-free" → the block is pushed into the small magazine
//! and a later small alloc hands out an address INSIDE the still-live Large
//! allocation → silent aliasing / double-issue.
//!
//! The substrate path (`AllocCore::dealloc`) routes by segment `kind` FIRST and
//! so degrades to a clean no-op on the same violation. The `hardened` feature
//! restores that symmetry on the magazine path: a `kind_at(base) == Large`
//! check before the oracles turns the mismatched free into a detected no-op.
//!
//! ## Counterfactual (RED without the guard)
//!
//! Comment out the `#[cfg(feature = "hardened")]` Large-kind block in
//! `heap_core.rs::dealloc_own_thread_with_base` and re-run under
//! `--features "production hardened"`: `large_ptr_small_layout_free_is_noop`
//! goes RED — the Large payload address is pushed into the small magazine and
//! re-issued by a later small alloc, appearing as a duplicate / an address
//! inside the live Large block. Restoring the guard → GREEN.
//!
//! Gated to `hardened` (which pulls `fastbin`): only that build compiles the
//! guard and the magazine path it defends.

#![cfg(all(feature = "hardened", feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// Free a LARGE allocation via a SMALL layout on the OWN thread. The hardened
/// Large-kind guard must make it a no-op: the Large payload address must NOT be
/// pushed into the small magazine and re-issued, and the live Large block must
/// stay intact (no aliasing).
#[test]
fn large_ptr_small_layout_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // 2 MiB, align 8 (< SEGMENT) — unambiguously routed to the dedicated
    // Large-segment path (class_for → None) in every feature combination
    // (including `medium-classes`, which raises SMALL_MAX to 1 MiB), yet
    // owned by THIS heap so the own-thread routing (`contains_base`) reaches
    // the magazine free body.
    const LARGE_SIZE: usize = 2 * 1024 * 1024;
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    // A small layout whose `class_for` is `Some(c)` — the mismatched size the
    // buggy caller uses to free the Large pointer.
    //
    // Size 64 (block_size 64) is chosen deliberately so this test ISOLATES the
    // F7 Large-kind guard from the sibling interior-pointer guard: the Large
    // payload starts at segment offset `hdr_aligned` = `align_up(SegmentHeader,
    // PAGE)` = 4096 (for align 8, PAGE 4096), and 4096 is a whole multiple of
    // the 64 B block size, so the interior-pointer guard's `off % block_size ==
    // 0` check PASSES the pointer through — only the F7 kind check stops it.
    // (A non-page-multiple block size like 48 would be caught by the interior
    // guard first, making this test vacuous for F7.)
    let small_layout = Layout::from_size_align(64, 8).unwrap();

    let large = unsafe { (*heap).alloc(large_layout) };
    assert!(!large.is_null(), "large alloc returned null");
    // Fill the payload so the oracles, if they (wrongly) ran, would read our
    // pattern out of the payload bytes rather than incidental zeros.
    unsafe { std::ptr::write_bytes(large, 0xCC, LARGE_SIZE) };

    // The hazardous own-thread free of the Large pointer with a SMALL layout —
    // must be a NO-OP under the hardened Large-kind guard.
    unsafe { (*heap).dealloc(large, small_layout) };

    // The Large payload must be untouched (the free must not have mutated it).
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload mutated by the mismatched small-layout free"
        );
    }

    // R19-1 (task #337): the payload-byte read above is a non-conclusive
    // UAF-read (freed memory typically still shows stale bytes immediately
    // after release). The REAL "was this segment actually freed" signal is
    // `dbg_owner_id_for`: it returns `None` iff the segment is no longer
    // registered in this heap's `SegmentTable`. A defensive no-op MUST leave
    // it `Some(_)`.
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_some(),
        "F7 GUARD BROKEN: the mismatched small-layout free actually \
         unregistered/freed the Large segment — it must be a detected no-op"
    );

    // Cold-storm of small allocations: none may alias any byte of the live
    // Large block. If the guard were missing, the Large payload address would
    // have entered the small magazine and been re-issued here.
    const N: usize = 8192;
    let large_lo = large as usize;
    let large_hi = large_lo + LARGE_SIZE;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(small_layout) };
        assert!(!p.is_null(), "cold-storm small alloc returned null");
        let a = p as usize;
        assert!(
            !(large_lo..large_hi).contains(&a),
            "LARGE-KIND GUARD BROKEN: a small alloc was handed an address \
             inside the live Large block — the mismatched free aliased it"
        );
        issued.push(p);
    }
    // Distinctness: no small pointer may repeat (a Large payload re-issue would
    // still be distinct from small blocks, but a general double-issue would
    // surface here as a duplicate).
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE small pointer during the cold-storm"
    );

    // R19-1 (task #337): re-confirm liveness AFTER the cold-storm too — a
    // deferred/lazy free triggered by the storm must not have reclaimed it.
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_some(),
        "F7 GUARD BROKEN: the Large segment was unregistered/freed sometime \
         during the cold-storm — the mismatched free must be a permanent no-op"
    );

    // The Large block is still ours to free legitimately — heap stays sound.
    unsafe {
        assert_eq!(
            large.read(),
            0xCC,
            "Large payload corrupted by the cold-storm"
        );
        (*heap).dealloc(large, large_layout);
    }
    for &p in &issued {
        unsafe { (*heap).dealloc(p, small_layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ─────────────────────────────────────────────────────────────────────────
// R19-1 (task #337) — branch (A) coverage.
//
// The two tests below are compiled ONLY under the SAME promotion-reachable
// predicate `heap_core_free.rs`'s branch (A) uses, layered on this file's
// `#![cfg(hardened, alloc-global, fastbin)]`. So they run under
// `--features "hardened medium-classes"` (where branch (A) compiles —
// `exact-span-large` is off, so `not(exact-span-large)` is true) and are
// absent under bare `--features "hardened"` (branch (B) only). This is the
// feature combination the original test could never compile, so it could not
// catch the branch-(A) bug where a fabricated small-layout free on a Large
// pointer was routed straight to `self.core.dealloc` (really freeing the
// segment) instead of being a detected no-op.
// ─────────────────────────────────────────────────────────────────────────

/// R19-1 (task #337), branch (A) sub-scenario (a): a LEGITIMATE promoted-and-
/// grown Large block freed with its MATCHING layout MUST actually free the
/// segment (proves the consistency-check gate does not break the
/// correctness-required path). The promoted block's dealloc layout classifies
/// "small" under `medium-classes` (300 KiB < 1 MiB SMALL_MAX) even though it
/// lives in a Large segment, so this is exactly the case branch (A) exists to
/// route to `self.core.dealloc`.
#[cfg(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
#[test]
fn promoted_large_matching_free_actually_frees() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Start below the promotion threshold (256 KiB) so the initial alloc is a
    // medium-classified block in a Small segment.
    let medium_layout = Layout::from_size_align(128 * 1024, 8).unwrap();
    // Grow past the threshold (>= 256 KiB) but still under `medium-classes`'
    // SMALL_MAX (1 MiB) so the post-promotion dealloc layout classifies
    // "small" — exercising branch (A)'s "classify-small but live-in-Large"
    // routing.
    const NEW_SIZE: usize = 300 * 1024;

    let medium = unsafe { (*heap).alloc(medium_layout) };
    assert!(!medium.is_null(), "medium alloc returned null");

    // Growing realloc fires `try_promote_to_large`: allocates a fresh Large
    // segment stamped with `large_size == NEW_SIZE`, copies, frees the old
    // medium block, returns the new Large pointer.
    let promoted = unsafe { (*heap).realloc(medium, medium_layout, NEW_SIZE) };
    assert!(!promoted.is_null(), "promoting realloc returned null");
    assert_ne!(
        promoted, medium,
        "promotion must move the block to a new Large segment"
    );

    let promoted_layout = Layout::from_size_align(NEW_SIZE, 8).unwrap();

    // BEFORE the free: the promoted segment is live and registered.
    assert!(
        unsafe { (*heap).dbg_owner_id_for(promoted) }.is_some(),
        "promoted Large segment must be registered before the free"
    );

    // The legitimate matching-layout free — under `hardened` + the fix, the
    // `large_layout_consistent` check passes (large_size == NEW_SIZE) so it
    // reaches `self.core.dealloc`'s Large branch and ACTUALLY frees/
    // unregisters the segment.
    unsafe { (*heap).dealloc(promoted, promoted_layout) };

    // AFTER the free: the segment really was unregistered. This is the real
    // "was it freed" signal (not a payload-byte read). A no-op here would be a
    // correctness BUG (the promoted block would leak its segment).
    assert!(
        unsafe { (*heap).dbg_owner_id_for(promoted) }.is_none(),
        "promoted Large segment was NOT freed by the matching-layout free — \
         the consistency gate must NOT turn a legitimate free into a no-op"
    );

    unsafe { HeapRegistry::recycle(heap) };
}

/// R19-1 (task #337), branch (A) sub-scenario (b): an ILLEGITIMATE free of a
/// genuine Large pointer (never promoted) with a fabricated small layout MUST
/// be a detected no-op under `hardened` — the actual bug this task fixes.
/// Before the fix, branch (A) unconditionally called `self.core.dealloc`,
/// which freed/unregistered the segment; this test goes RED. After the fix,
/// the `large_layout_consistent` check fails (64 B != 2 MiB) and the free
/// degrades to the same defensive no-op branch (B) uses.
#[cfg(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
#[test]
fn large_ptr_small_layout_free_is_noop_branch_a() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // 2 MiB, align 8: unambiguously Large (> 1 MiB SMALL_MAX under
    // `medium-classes`), so it was NEVER promoted — a genuine Large block.
    const LARGE_SIZE: usize = 2 * 1024 * 1024;
    let large_layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    // A small layout (64 B) whose `class_for` is `Some(c)` — the fabricated
    // mismatched layout the buggy caller uses. 64 B keeps the payload offset
    // page-multiple so the sibling interior-pointer guard does not catch it
    // first (same isolation rationale as the existing branch-B test).
    let small_layout = Layout::from_size_align(64, 8).unwrap();

    let large = unsafe { (*heap).alloc(large_layout) };
    assert!(!large.is_null(), "large alloc returned null");
    unsafe { std::ptr::write_bytes(large, 0xCC, LARGE_SIZE) };

    // Confirm live before the hazardous free.
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_some(),
        "Large segment must be registered before the free"
    );

    // The ILLEGITIMATE own-thread free of the Large pointer with a SMALL
    // layout — under `hardened` + branch (A) this MUST be a detected no-op,
    // NOT a real free (the bug being fixed).
    unsafe { (*heap).dealloc(large, small_layout) };

    // The REAL oracle (R19-1): the segment must STILL be registered. Before
    // the fix this returned `None` — branch (A) had actually freed it via
    // `self.core.dealloc` (the Large arm unconditionally `unregister`s).
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_some(),
        "BRANCH (A) BUG: the mismatched small-layout free actually \
         unregistered/freed the Large segment — under `hardened` it must be \
         a detected no-op (task #337)"
    );

    // Cold-storm of small allocations: none may alias the live Large block,
    // and the Large segment must stay registered throughout.
    const N: usize = 8192;
    let large_lo = large as usize;
    let large_hi = large_lo + LARGE_SIZE;
    let mut issued: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(small_layout) };
        assert!(!p.is_null(), "cold-storm small alloc returned null");
        let a = p as usize;
        assert!(
            !(large_lo..large_hi).contains(&a),
            "LARGE-KIND GUARD BROKEN: a small alloc was handed an address \
             inside the live Large block — the mismatched free aliased it"
        );
        issued.push(p);
    }
    let distinct: HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE small pointer during the cold-storm"
    );
    // Re-confirm liveness after the storm.
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_some(),
        "Large segment was unregistered/freed during the cold-storm — the \
         mismatched free must be a permanent no-op"
    );

    // The Large block is still ours to free legitimately — heap stays sound.
    unsafe { (*heap).dealloc(large, large_layout) };
    assert!(
        unsafe { (*heap).dbg_owner_id_for(large) }.is_none(),
        "Large segment must be unregistered after the legitimate matching free"
    );
    for &p in &issued {
        unsafe { (*heap).dealloc(p, small_layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
