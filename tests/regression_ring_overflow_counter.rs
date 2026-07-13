//! Regression test — task D2 (Phase D, пределы/тюнинг).
//!
//! `RemoteFreeRing::push` already tracked a per-segment overflow counter
//! (the cursor-block `overflow` field, exposed via `overflow_count()`), but
//! there was no PROCESS-WIDE counter — a production process had no way to
//! observe "did any ring overflow happen anywhere" without first knowing
//! which segment to ask. This test verifies the new
//! `sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW` counter
//! (task D2) actually increments when a ring is pushed past `RING_CAP`
//! without being drained — i.e. that an overflow REALLY happened and was
//! counted, not a vacuous "counter exists but never fires" check.
//!
//! Uses the isolated-buffer test surface (`over_test_buffer` /
//! `init_test_buffer`), same technique as `tests/remote_ring_unit.rs` — no
//! allocator, no segment, just the ring's own push/drain protocol. This
//! keeps the test fast and avoids coupling to allocator internals while
//! still exercising the exact code path (`push`'s full-ring branch) that the
//! counter instruments.
//!
//! Counterfactual (verified manually — see task report): if the
//! `DBG_RING_OVERFLOW.fetch_add` in `RemoteFreeRing::push`'s overflow branch
//! is removed, this test's `assert!(DBG_RING_OVERFLOW... > 0)` fails even
//! though the ring itself still correctly returns `Err(PushOverflow)` for
//! the (RING_CAP+1)-th push — proving the assertion is not vacuously true
//! merely because pushes fail, but specifically checks the counter wiring.

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use sefer_alloc::alloc_core::remote_free_ring::{
    RemoteFreeRing, DBG_RING_OVERFLOW, FOOTPRINT, RING_CAP,
};
use std::sync::atomic::Ordering;

#[test]
fn ring_overflow_increments_process_wide_counter() {
    // Fresh isolated buffer — no segment, no allocator.
    let mut buf = vec![0u8; FOOTPRINT].into_boxed_slice();
    let base = buf.as_mut_ptr();
    // SAFETY: `base` is a FOOTPRINT-sized, 4-byte-aligned, owned buffer.
    let ring = unsafe {
        RemoteFreeRing::init_test_buffer(base);
        RemoteFreeRing::over_test_buffer(base)
    };

    let before = DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    // Fill the ring to exactly RING_CAP (all succeed — the consumer never
    // drains, simulating an owner thread that is busy and never reaches its
    // alloc-path drain).
    let mut pushed_ok = 0usize;
    for i in 0..RING_CAP {
        match ring.push(i as u32) {
            Ok(()) => pushed_ok += 1,
            Err(_) => panic!("push {i} should not overflow before reaching RING_CAP"),
        }
    }
    assert_eq!(
        pushed_ok, RING_CAP,
        "ring should accept exactly RING_CAP pushes"
    );

    // Now push well past capacity without ever draining. Every one of these
    // MUST overflow (the ring is full and nobody is consuming).
    const EXTRA_PUSHES: usize = 64;
    let mut overflow_results = 0usize;
    for i in 0..EXTRA_PUSHES {
        if ring.push((RING_CAP + i) as u32).is_err() {
            overflow_results += 1;
        }
    }
    assert_eq!(
        overflow_results, EXTRA_PUSHES,
        "every push past RING_CAP with no drain must overflow (bounded-leak contract)"
    );

    let after = DBG_RING_OVERFLOW.load(Ordering::Relaxed);
    let delta = after - before;

    // The process-wide counter must have recorded AT LEAST the overflows we
    // just caused (>= rather than == because the counter is process-global
    // and other tests running in the same binary/process may also push
    // overflows concurrently — but it must never be 0 here, which is the
    // failure mode this test guards against).
    assert!(
        delta >= EXTRA_PUSHES as u64,
        "DBG_RING_OVERFLOW must have incremented by at least {EXTRA_PUSHES} \
         (observed overflow events), got delta={delta} (before={before}, after={after})"
    );

    // Sanity: the ring's own per-segment overflow counter agrees (exact,
    // single-threaded, no cross-test interference possible for THIS ring).
    assert_eq!(
        ring.overflow_count() as usize,
        EXTRA_PUSHES,
        "per-segment overflow_count() must exactly equal the overflow attempts on this ring"
    );
}
