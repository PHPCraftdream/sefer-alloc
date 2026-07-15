//! Regression / deliverable coverage for R6-OPT-P0-2 round 1 — chunking the
//! registry's slot array (`src/registry/registry_chunk.rs` +
//! `src/registry/bootstrap.rs`'s per-chunk `Registry::slot`).
//!
//! ## What this file proves
//!
//! 1. **The core deliverable — claiming touches only the chunks it needs.**
//!    Claiming a slot in chunk 0 (a low index) must NOT materialise any
//!    OTHER chunk. This is the whole point of the round: a process that only
//!    ever claims a few heaps should pay the OS reservation cost for a
//!    handful of 64-slot chunks, not the entire `MAX_HEAPS = 4096`-slot
//!    array. A true process-level RSS/commit-charge assertion is out of
//!    reach for an in-process `cargo test` (that is what
//!    `examples/first_alloc_process.rs` is for — see its module doc and this
//!    task's own before/after commit-charge measurement), so this test
//!    instead uses the `#[doc(hidden)]` chunk-introspection hook
//!    `Registry::dbg_chunk_is_materialised` to assert the STRUCTURAL
//!    invariant directly: after claiming a low-index slot, high-index chunks
//!    remain `UNINIT`.
//!
//! 2. **Slot-address stability survives chunking.** `HeapRegistry::claim`
//!    returns a `*mut HeapCore` pointing directly into the slot's
//!    `UnsafeCell` inside its (now chunk-resident) `HeapSlot`. Because a
//!    materialised chunk is NEVER freed or moved (the `mem::forget`d OS
//!    reservation lives for the process lifetime — see `bootstrap.rs`'s
//!    module doc and `RegistryChunk`'s doc comment), the address a slot
//!    resolves to before a recycle must be IDENTICAL to the address it
//!    resolves to after a later re-claim of the same slot index. This is the
//!    same whole-slot-reuse invariant `tests/registry_basic.rs`'s
//!    `recycle_then_claim_reuses_slot_and_bumps_generation` already checks
//!    via `id()` — this test extends it to check the raw POINTER identity as
//!    well, which is the property `heap_registry::bind_slot_counters`'s
//!    planted `&'static` references (`&slot.remote.thread_free`,
//!    `&slot.overflow`) actually depend on.
//!
//! Both properties would FAIL under a naive (broken) chunking implementation:
//! (1) would fail if `slot()` eagerly materialised every chunk up to
//! `count`/`MAX_HEAPS` instead of only the chunk containing the requested
//! index; (2) would fail if a chunk were, say, reallocated/moved on some
//! later access, or if `RegistryChunk`'s slot array were re-indexed
//! differently across calls.

#![cfg(feature = "alloc-global")]

use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against the other registry-touching test files in this crate
// (matches the discipline already used throughout `tests/`, e.g.
// `registry_basic.rs`) — claiming here advances `count`/materialises chunk
// 0, which other tests' absolute-slot-index assertions implicitly assume is
// stable for their own duration.
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

/// Core deliverable: claiming ONE heap (necessarily a low slot index, since
/// `count` is monotonic and the whole suite's cumulative claims across every
/// test file stay in the low hundreds at most — far short of the
/// `CHUNK_SLOTS`-sized range of even chunk 1) must not materialise the LAST
/// chunk. If chunking degenerated into "resolve the index, but eagerly
/// materialise every chunk anyway", this assertion would fail.
#[test]
fn claiming_one_heap_does_not_materialise_unrelated_chunks() {
    let _serial = SerialGuard::acquire();

    let last_chunk = bootstrap::dbg_num_chunks() - 1;
    let reg = bootstrap::ensure();

    // Precondition sanity: the last chunk must not ALREADY be materialised
    // by prior test-suite traffic (if it were, the assertion below would be
    // vacuously true regardless of whether this claim triggers it). Given
    // `MAX_HEAPS = 4096` and the suite's total claim volume, this chunk is
    // never reached by ordinary tests.
    assert!(
        !reg.dbg_chunk_is_materialised(last_chunk),
        "test precondition violated: the last chunk is already materialised \
         by earlier test-suite traffic — this test cannot prove anything \
         about lazy materialisation under this precondition; if the suite's \
         claim volume has grown enough to reach here, pick a still-higher \
         chunk index or otherwise assert this differently"
    );

    // Claim a heap — this touches count (a low index) and therefore, at
    // most, a low-numbered chunk.
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "claim must not return null");

    // The core assertion: the LAST chunk must still be untouched.
    assert!(
        !reg.dbg_chunk_is_materialised(last_chunk),
        "claiming a low-index slot must NOT materialise an unrelated \
         high-index chunk — chunking has degenerated into eager whole-array \
         materialisation"
    );

    // Clean up: recycle so this claim does not perturb other tests' index
    // arithmetic more than a normal claim already would.
    // SAFETY: `heap` was just returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(heap) };
}

/// Slot-address stability across recycle/re-claim, now specifically in the
/// presence of chunking: the chunk backing a slot is never freed or moved,
/// so the `*mut HeapCore` a slot resolves to must be byte-identical before
/// and after a recycle → re-claim cycle of that same slot index.
#[test]
fn slot_address_is_stable_across_recycle_under_chunking() {
    let _serial = SerialGuard::acquire();

    let a = HeapRegistry::claim();
    assert!(!a.is_null(), "first claim must not return null");
    // SAFETY: `a` is live, just claimed, not yet recycled.
    let id_a = unsafe { (*a).id() };
    let addr_before = a as usize;

    // SAFETY: `a` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };

    // Re-claim: with the free_slots LIFO stack empty of anything else in a
    // freshly-serialised test, this pops the SAME slot we just recycled.
    let b = HeapRegistry::claim();
    assert!(!b.is_null(), "re-claim must not return null");
    // SAFETY: `b` is live, just claimed, not yet recycled.
    let id_b = unsafe { (*b).id() };
    let addr_after = b as usize;

    assert_eq!(
        id_a, id_b,
        "re-claim after a single recycle must reuse the SAME slot index \
         (free_slots LIFO with nothing else pushed in between)"
    );
    assert_eq!(
        addr_before, addr_after,
        "the SAME slot index must resolve to the SAME memory address before \
         and after a recycle → re-claim cycle — chunking must not move or \
         reallocate a materialised chunk (bind_slot_counters plants \
         &'static references into slot fields that depend on exactly this \
         stability)"
    );

    // SAFETY: `b` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(b) };
}

/// Cross-thread variant of the address-stability property: a DIFFERENT
/// thread re-claiming the recycled slot must still observe the same address
/// as the original claimer — proving the stability is a property of the
/// chunk memory itself (published via Release/Acquire to any thread), not an
/// artefact of same-thread cache/TLB locality.
#[test]
fn slot_address_is_stable_across_recycle_from_different_thread() {
    let _serial = SerialGuard::acquire();

    let a = HeapRegistry::claim();
    assert!(!a.is_null());
    // SAFETY: `a` is live, just claimed, not yet recycled.
    let id_a = unsafe { (*a).id() };
    let addr_before = a as usize;
    // SAFETY: `a` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(a) };

    let (id_b, addr_after) = std::thread::spawn(|| {
        let b = HeapRegistry::claim();
        assert!(!b.is_null());
        // SAFETY: `b` is live, just claimed by this thread, not yet recycled.
        let id_b = unsafe { (*b).id() };
        let addr = b as usize;
        // SAFETY: `b` was returned by `claim` and not yet recycled.
        unsafe { HeapRegistry::recycle(b) };
        (id_b, addr)
    })
    .join()
    .expect("claimer thread panicked");

    assert_eq!(id_a, id_b, "the spawned thread must reuse the same slot");
    assert_eq!(
        addr_before, addr_after,
        "a DIFFERENT thread re-claiming the recycled slot must observe the \
         SAME address as the original claimer — the chunk's Release publish \
         must be visible to every thread, not just the one that touched it \
         first"
    );
}
