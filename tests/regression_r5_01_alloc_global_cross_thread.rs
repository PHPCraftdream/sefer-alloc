//! xxs R5-01 (`docs/reviews/2026-09-29-091221-src-review-xxs-sol-round-5.md`):
//! `alloc-global` alone (public API, no `internals`) must route cross-thread
//! frees and reallocs. Before the fix it dropped a cross-thread free (leaking
//! the block and pinning its segment) and returned null from a cross-thread
//! `realloc` (an abort for a moved-then-grown `Vec`).
//!
//! `AllocStats` counters are process-wide, so the tests here serialise on
//! `SerialGuard` (as `tests/global_alloc_mt.rs` does).

#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

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

fn live_segments() -> u64 {
    let s = GLOBAL.stats();
    s.segments_reserved_total
        .saturating_sub(s.segments_released_total)
}

/// Either `src/lib.rs`'s `compile_error!` fires (this file does not build) or
/// feature unification enabled `alloc-xthread`.
#[test]
#[allow(clippy::assertions_on_constants)]
fn alloc_global_implies_alloc_xthread() {
    assert!(
        cfg!(feature = "alloc-xthread"),
        "alloc-global built without alloc-xthread (R5-01 regression)"
    );
}

/// A block moved to another thread and grown via the raw `GlobalAlloc::realloc`
/// must be non-null with its prefix preserved. (Growing a real `Vec` would
/// abort the whole harness on the old code, so the raw entry point is used.)
#[test]
fn cross_thread_realloc_succeeds_and_preserves_prefix() {
    let _guard = SerialGuard::acquire();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let (tx, rx) = sync_channel::<usize>(0);
    let producer = thread::spawn(move || {
        // SAFETY: `layout` is a valid non-zero-size `Layout`.
        let ptr = unsafe { GLOBAL.alloc(layout) };
        assert!(!ptr.is_null(), "producer allocation failed");
        // SAFETY: `ptr` is a fresh live 64-byte allocation.
        unsafe { std::ptr::write_bytes(ptr, 0x5A, 64) };
        tx.send(ptr as usize)
            .expect("consumer channel closed early");
    });
    let ptr = rx.recv().expect("producer did not send a pointer") as *mut u8;
    producer.join().expect("producer thread panicked");

    let new_size = 4096usize;
    // SAFETY: `ptr` is the live start pointer of a 64-byte allocation made by
    // `GLOBAL` with exactly `layout`.
    let grown = unsafe { GLOBAL.realloc(ptr, layout, new_size) };
    assert!(
        !grown.is_null(),
        "R5-01 regression: cross-thread realloc returned null"
    );
    // SAFETY: `grown` is non-null and keeps at least the first 64 bytes.
    let prefix = unsafe { std::slice::from_raw_parts(grown, 64) };
    assert!(prefix.iter().all(|&b| b == 0x5A), "prefix corrupted");
    let new_layout = Layout::from_size_align(new_size, 8).unwrap();
    // SAFETY: `grown` is the live allocation `realloc` returned, freed once.
    unsafe { GLOBAL.dealloc(grown, new_layout) };
}

/// 256 x 4 MiB `Vec`s (Large under every `medium-classes*` setting) handed
/// producer -> consumer and dropped there must be reclaimed, not leaked.
#[test]
fn cross_thread_large_free_reclaims_segments() {
    let _guard = SerialGuard::acquire();
    const ROUNDS: usize = 256;
    const SIZE: usize = 4 * 1024 * 1024;

    let before = GLOBAL.stats();
    let segments_before = live_segments();

    let (tx, rx) = sync_channel::<Vec<u8>>(1);
    let producer = thread::spawn(move || {
        for _ in 0..ROUNDS {
            if tx.send(vec![1u8; SIZE]).is_err() {
                break;
            }
        }
    });
    let mut received = 0usize;
    while let Ok(v) = rx.recv() {
        assert_eq!(v.len(), SIZE);
        drop(v);
        received += 1;
    }
    producer.join().expect("producer thread panicked");
    assert_eq!(received, ROUNDS, "consumer missed rounds");

    let reclaimed = GLOBAL
        .stats()
        .large_xthread_reclaimed
        .saturating_sub(before.large_xthread_reclaimed);
    let segments_delta = live_segments().saturating_sub(segments_before);

    assert!(
        reclaimed >= 128,
        "R5-01 regression: only {reclaimed} of {ROUNDS} cross-thread Large \
         frees were reclaimed"
    );
    assert!(
        segments_delta <= 16,
        "R5-01 regression: {segments_delta} live segments accumulated"
    );
}

/// 400 small `Box`es handed producer -> consumer: coarse always-on bound on
/// live-segment growth. It is not RED on the old tree at this size (a segment
/// holds tens of thousands of 64 B blocks); the per-event check below is.
#[test]
fn cross_thread_small_free_bounds_segment_growth() {
    let _guard = SerialGuard::acquire();
    const ROUNDS: usize = 400;
    let segments_before = live_segments();
    small_cross_thread_round_trip(ROUNDS);
    let segments_delta = live_segments().saturating_sub(segments_before);
    assert!(
        segments_delta <= 8,
        "live segments grew by {segments_delta} from {ROUNDS} cross-thread \
         Small frees"
    );
}

/// Per-event check: no cross-thread small free may hit the foreign-or-
/// unroutable drop branch (its counter needs `alloc-stats`).
#[cfg(feature = "alloc-stats")]
#[test]
fn cross_thread_small_free_does_not_hit_foreign_or_unroutable_frees() {
    let _guard = SerialGuard::acquire();
    const ROUNDS: usize = 400;
    let before = GLOBAL.stats().foreign_or_unroutable_frees;
    small_cross_thread_round_trip(ROUNDS);
    let dropped = GLOBAL
        .stats()
        .foreign_or_unroutable_frees
        .saturating_sub(before);
    assert_eq!(
        dropped, 0,
        "R5-01 regression: {dropped} of {ROUNDS} cross-thread Small frees \
         were dropped instead of routed"
    );
}

/// One producer allocates `rounds` boxes, the consumer frees them.
fn small_cross_thread_round_trip(rounds: usize) {
    let (tx, rx) = sync_channel::<Box<[u8; 64]>>(1);
    let producer = thread::spawn(move || {
        for _ in 0..rounds {
            if tx.send(Box::new([7u8; 64])).is_err() {
                break;
            }
        }
    });
    let mut received = 0usize;
    while let Ok(b) = rx.recv() {
        assert_eq!(b[0], 7);
        drop(b);
        received += 1;
    }
    producer.join().expect("producer thread panicked");
    assert_eq!(received, rounds, "consumer missed rounds");
}
