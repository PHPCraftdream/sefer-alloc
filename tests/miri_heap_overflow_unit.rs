//! Minimal, standalone miri UB-detection target for `HeapOverflow`'s
//! push/drain protocol (RAD-4b, task #72).
//!
//! ## Why this file exists, separate from `remote_fanin.rs`
//!
//! `HeapOverflow` (`src/registry/heap_overflow.rs`) is exercised end-to-end
//! by `tests/remote_fanin.rs::remote_fanin_miri_minimal_retry_ub_check` (the
//! existing RAD-4 miri harness, which routes cross-thread frees through the
//! FULL `bootstrap::ensure()` + `HeapRegistry::claim` + `MAX_HEAPS`-slot
//! registry). That path was measured, during this task's development, to be
//! impractically slow under miri's interpreter on THIS registry's size (the
//! `HeapOverflow` field added ~24 KiB/slot × `MAX_HEAPS` = 4096 slots to the
//! process-global registry struct miri's Stacked/Tree Borrows tracking must
//! model) — the run did not complete in a reasonable time even for the
//! already-deliberately-minimal 260-block harness.
//!
//! This file isolates JUST the new protocol: it constructs a single
//! standalone, heap-`Box`-allocated `HeapOverflow`
//! (`HeapOverflow::new_boxed_for_test`, a `#[doc(hidden)] pub` test surface —
//! mirrors `RemoteFreeRing::over_test_buffer`'s identical "isolated ring
//! test" pattern used by `tests/remote_ring_unit.rs`), with NO registry, NO
//! `bootstrap::ensure()`, NO `HeapCore`/`AllocCore` substrate at all — just
//! the ring's own `push`/`drain` methods, driven directly by real
//! cross-thread pushes and a single-consumer drain. This is small enough for
//! miri to finish in a reasonable time while still exercising the ONE
//! genuinely new detail this task's design adds beyond the already
//! miri-covered `RemoteFreeRing`/`push_with_overflow_retry` protocol: the
//! two-atomic (`base`, `packed`) entry and its publish-order requirement
//! (`packed` before `base`) — see `heap_overflow.rs`'s module doc and
//! `tests/loom_heap_overflow.rs` (which loom-proves the SAME publish-order
//! requirement exhaustively across interleavings; this file's job is
//! UB-detection — no data race, no invalid memory access, no provenance
//! violation — on the REAL `core::sync::atomic` implementation, not the loom
//! model).
//!
//! Deliberately small (`N = 32` pushes, comfortably under `HEAP_OVERFLOW_CAP
//! = 2048` — no overflow-of-the-overflow-ring branch needed here; that
//! branch is plain safe-Rust arithmetic with no new unsafe surface and is
//! already covered by the native `remote_fanin` harnesses) so miri completes
//! quickly. Uses REAL (non-null, but arbitrary and never dereferenced)
//! `*mut u8` values as the `base` half of each entry — `HeapOverflow` never
//! dereferences `base` itself (it is opaque payload, exactly like the
//! `packed` word — see the module doc's "no block-byte writes" discipline),
//! so a synthetic non-null pointer is sound to push/drain without ever
//! reading through it.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::sync::Arc;
use std::thread;

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// Synthetic, never-dereferenced "segment base" values — distinct non-null
/// addresses so pushed entries can be distinguished on drain. `HeapOverflow`
/// only ever compares/stores these as `usize`, never reads through them (see
/// the module doc's "no block-byte writes" discipline), so using arbitrary
/// non-null integers cast to `*mut u8` is sound here (this file never
/// dereferences them either).
fn synthetic_base(tag: usize) -> *mut u8 {
    // Non-zero, `HeapOverflow`'s `ENTRY_EMPTY_BASE` sentinel is `0` — any
    // non-zero value is a valid "real" base for this ring's contract.
    core::ptr::without_provenance_mut((tag + 1) * 64)
}

/// Two producer threads push disjoint `(base, packed)` pairs concurrently;
/// the main thread joins then drains once. Asserts every pushed pair is
/// reclaimed exactly once and UNTORN (drain's observed `packed` always
/// matches what was pushed alongside that `base`) — the real-atomics
/// counterpart of `loom_heap_overflow.rs`'s `correct_overflow_never_tears_
/// loses_or_duplicates`, run under miri for UB-detection rather than
/// exhaustive interleaving exploration.
#[test]
fn heap_overflow_concurrent_push_drain_no_ub() {
    const N: usize = 16; // per producer; 32 total, well under HEAP_OVERFLOW_CAP.
    let ring = Arc::new(HeapOverflow::new_boxed_for_test());

    let ring_a = Arc::clone(&ring);
    let ta = thread::spawn(move || {
        for i in 0..N {
            let base = synthetic_base(i);
            // packed encodes `i` so the drain can verify untorn pairing.
            let packed = i as u32;
            assert!(
                ring_a.push(base, packed),
                "producer A push must not overflow"
            );
        }
    });

    let ring_b = Arc::clone(&ring);
    let tb = thread::spawn(move || {
        for i in 0..N {
            let base = synthetic_base(N + i);
            let packed = (N + i) as u32;
            assert!(
                ring_b.push(base, packed),
                "producer B push must not overflow"
            );
        }
    });

    ta.join().unwrap();
    tb.join().unwrap();

    let mut seen = std::collections::HashMap::new();
    ring.drain(|base, packed| {
        // UNTORN check: `base`'s tag (recovered from the synthetic address)
        // must match `packed` exactly, by construction of how they were
        // pushed together above.
        let tag = (base as usize) / 64 - 1;
        assert_eq!(
            packed, tag as u32,
            "torn entry: base tag {tag} paired with packed {packed} (expected {tag})"
        );
        *seen.entry(tag).or_insert(0u32) += 1;
    });

    assert_eq!(
        seen.len(),
        2 * N,
        "expected {} distinct entries, got {}",
        2 * N,
        seen.len()
    );
    for (&tag, &count) in &seen {
        assert_eq!(
            count, 1,
            "entry {tag} reclaimed {count} times (expected exactly 1)"
        );
    }
}

/// Single-threaded push-then-drain-then-push-again (wrap-adjacent, though
/// far short of a real `HEAP_OVERFLOW_CAP` wrap): exercises the plain
/// sequential path (no concurrency) for a baseline UB check, and confirms an
/// empty drain after full reclaim is a genuine no-op (no phantom entries).
#[test]
fn heap_overflow_sequential_push_drain_no_ub() {
    let ring = HeapOverflow::new_boxed_for_test();

    for i in 0..8usize {
        assert!(ring.push(synthetic_base(i), i as u32));
    }
    let mut count = 0u32;
    ring.drain(|_base, _packed| {
        count += 1;
    });
    assert_eq!(
        count, 8,
        "first drain must reclaim exactly the 8 pushed entries"
    );

    // Second drain (nothing pushed since) must be a no-op.
    let mut second_count = 0u32;
    ring.drain(|_base, _packed| {
        second_count += 1;
    });
    assert_eq!(
        second_count, 0,
        "drain of an already-drained ring must reclaim nothing"
    );
}
