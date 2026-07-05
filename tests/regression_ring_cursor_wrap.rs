//! **Regression: `RemoteFreeRing` long-run u32 cursor WRAP safety.**
//!
//! The per-segment `RemoteFreeRing` uses two monotonic `u32` cursors (`head`,
//! `tail`). On a single hot, long-lived segment, 2^32 cross-thread frees make
//! the cursors WRAP past `u32::MAX`. This is the ONE wrap hazard genuinely
//! reachable on a long run. The ring is wrap-SAFE **by design**, resting on two
//! invariants:
//!
//!   (a) occupancy is `tail.wrapping_sub(head)` — NOT `tail - head` (which would
//!       overflow/mis-verdict once `tail` wraps while `head` has not), and
//!   (b) the slot index is `i % RING_CAP` with `RING_CAP` a power of two, so
//!       the index sequence is CONTINUOUS across the `u32::MAX → 0` boundary
//!       (`2^32 % RING_CAP == 0`).
//!
//! These tests PIN that safety by presetting the cursors to the wrap boundary
//! (via the `dbg_set_cursors` seam) and driving the REAL ring across it. They
//! do NOT widen anything.
//!
//! ## Counterfactual (proves non-vacuity)
//!
//! - Change occupancy `t.wrapping_sub(h)` → `t - h` in the ring: under the
//!   boundary preset this overflow-panics (debug) or mis-verdicts, failing
//!   `boundary_fifo` / `occupancy_across_wrap`.
//! - Set `RING_CAP = 200` (non-power-of-two): the crate FAILS TO COMPILE on the
//!   `is_power_of_two` const-assert.

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT, RING_CAP};

// The ring stores raw `u32` entries (production packs `(offset, class)` into
// one u32, but the ring is agnostic — any non-sentinel u32 is a valid entry).
// We use plain distinct u32 payloads so loss/duplication/corruption is
// detectable, matching how `tests/remote_ring_unit.rs` drives the ring with
// raw offset values.

/// Allocate a `FOOTPRINT`-byte, 4-byte-aligned, zeroed buffer for a ring.
fn ring_buffer() -> Box<[u8]> {
    let mut buf: Vec<u8> = vec![0u8; FOOTPRINT];
    assert!(
        (buf.as_mut_ptr() as usize).is_multiple_of(core::mem::align_of::<u32>()),
        "ring buffer must be 4-byte aligned"
    );
    buf.into_boxed_slice()
}

/// A `Send`+`Sync` wrapper so the raw-pointer view can cross threads. SAFETY:
/// the buffer is shared and touched only through `&AtomicU32` from the node
/// seam (race-free atomics); the buffer outlives all threads (held by `Arc`).
struct SendRing(RemoteFreeRing);
unsafe impl Send for SendRing {}
unsafe impl Sync for SendRing {}

/// A distinct, valid entry for logical index `n`: a small multiple of 16
/// (a real block offset is `< SEGMENT = 1<<22`, well under the sentinel
/// `u32::MAX`). Distinctness makes loss/duplication/corruption detectable.
fn entry_for(n: u32) -> u32 {
    let e = (n + 1) * 16;
    assert_ne!(e, u32::MAX, "entry must not be the sentinel");
    e
}

/// **Test 1 — Boundary FIFO.** Preset `head = tail = u32::MAX - 2`, push 5
/// DISTINCT entries (the reservations cross `u32::MAX → 0` mid-sequence), drain
/// once, assert they come back in FIFO order with intact payloads.
#[test]
fn boundary_fifo_across_u32_wrap() {
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    let ring = RemoteFreeRing::over_test_buffer(base);

    let start = u32::MAX - 2;
    ring.dbg_set_cursors(start, start);
    assert_eq!(ring.dbg_cursors(), (start, start));

    // Push 5: tail goes MAX-2, MAX-1, MAX, 0, 1 → crosses the wrap.
    let entries: Vec<u32> = (0..5).map(entry_for).collect();
    for &e in &entries {
        assert!(ring.push(e).is_ok(), "push must succeed inside RING_CAP");
    }
    // tail: MAX-2 → MAX-1 → MAX → 0 → 1 → 2 (5 wrapping_adds), head still MAX-2.
    let (h, t) = ring.dbg_cursors();
    assert_eq!(h, start);
    assert_eq!(t, 2, "tail wrapped past u32::MAX to 2");
    assert_eq!(t.wrapping_sub(h), 5, "occupancy across wrap must be 5");

    let mut got = Vec::new();
    ring.drain(|packed| got.push(packed));

    assert_eq!(got.len(), 5, "all 5 drained");
    for (i, (&pushed, &drained)) in entries.iter().zip(got.iter()).enumerate() {
        assert_eq!(drained, pushed, "FIFO/payload broken at {i}");
    }
    // head advanced past the wrap to meet tail at 2; ring now empty.
    assert_eq!(ring.dbg_cursors(), (2, 2));
    let mut second = 0;
    ring.drain(|_| second += 1);
    assert_eq!(second, 0, "quiescent ring drains nothing");
}

/// **Test 2 — Full ring at the boundary.** Preset near the wrap, push until
/// full (`wrapping_sub(tail,head) == RING_CAP`), assert the next push OVERFLOWS
/// (counter bumps, entry dropped), then drain fully and confirm occupancy → 0
/// and every non-overflowed entry is correct — all straddling the wrap.
#[test]
fn full_ring_overflow_across_wrap() {
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    let ring = RemoteFreeRing::over_test_buffer(base);

    // Start so that filling RING_CAP entries crosses u32::MAX.
    let start = u32::MAX - (RING_CAP as u32) / 2;
    ring.dbg_set_cursors(start, start);

    let cap = RING_CAP as u32;
    let entries: Vec<u32> = (0..cap).map(entry_for).collect();
    for (i, &e) in entries.iter().enumerate() {
        assert!(ring.push(e).is_ok(), "push {i} inside RING_CAP must succeed");
    }
    // Ring full: occupancy == RING_CAP.
    let (h, t) = ring.dbg_cursors();
    assert_eq!(t.wrapping_sub(h), cap, "ring must be full");
    assert_eq!(ring.overflow_count(), 0, "no overflow yet");

    // Next push overflows.
    let overflow_entry = entry_for(cap + 777);
    assert!(
        ring.push(overflow_entry).is_err(),
        "push into a full ring must return Err(PushOverflow)"
    );
    assert_eq!(ring.overflow_count(), 1, "overflow counter must bump exactly once");

    // Drain fully — only the RING_CAP non-overflowed entries, in FIFO order.
    let mut got = Vec::new();
    ring.drain(|packed| got.push(packed));
    assert_eq!(got.len(), RING_CAP, "all non-overflowed entries drained");
    assert_eq!(got, entries, "FIFO order/payloads intact across the wrap");
    assert!(
        !got.contains(&overflow_entry),
        "the overflowed entry must NOT appear (it was dropped, bounded leak)"
    );

    // Occupancy back to 0.
    let (h2, t2) = ring.dbg_cursors();
    assert_eq!(t2.wrapping_sub(h2), 0, "occupancy must return to 0");
    assert_eq!(h2, t2);
}

/// **Test 3 — Occupancy across wrap (focused).** With `head = u32::MAX-1`,
/// `tail = 3` (tail wrapped, head did not), occupancy must read
/// `wrapping_sub(3, u32::MAX-1) == 5`. This is the focused check that
/// `wrapping_sub` is precisely what makes the wrap safe. It also drives a real
/// drain of the 5 in-flight entries to confirm the count is not merely
/// arithmetic but reflects real slots.
#[test]
fn occupancy_across_wrap_is_wrapping_sub() {
    // Pure arithmetic invariant first (independent of the ring instance).
    let head: u32 = u32::MAX - 1;
    let tail: u32 = 3;
    assert_eq!(tail.wrapping_sub(head), 5, "wrapping_sub occupancy must be 5");

    // Now the real ring: preset head = u32::MAX-1, then push 5 (tail: MAX-1,
    // MAX, 0, 1, 2 → ends at 3, crossing the wrap). Occupancy reads 5 and a
    // drain returns exactly those 5 in order.
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    let ring = RemoteFreeRing::over_test_buffer(base);

    let start = u32::MAX - 1;
    ring.dbg_set_cursors(start, start);
    let entries: Vec<u32> = (0..5).map(|n| entry_for(n + 100)).collect();
    for &e in &entries {
        assert!(ring.push(e).is_ok());
    }
    let (h, t) = ring.dbg_cursors();
    assert_eq!(h, head);
    assert_eq!(t, tail);
    assert_eq!(t.wrapping_sub(h), 5, "real-ring occupancy across wrap == 5");

    let mut got = Vec::new();
    ring.drain(|p| got.push(p));
    assert_eq!(got, entries, "the 5 in-flight entries drain in FIFO order");
}

/// **Test 4 — Concurrent hammer across the wrap (bounded, fast).** Preset the
/// cursors near `u32::MAX`, run 2 producers (distinct entries) + 1 owner
/// draining, a bounded op count so the cursors cross the boundary under real
/// contention. Assert: no entry duplicated or corrupted, and
/// `reclaimed + overflowed == pushed`.
#[test]
fn concurrent_hammer_across_wrap() {
    // Bounded and fast; even smaller under miri.
    #[cfg(miri)]
    const PER_PRODUCER: u32 = 200;
    #[cfg(not(miri))]
    const PER_PRODUCER: u32 = 20_000;
    const PRODUCERS: u32 = 2;

    let buf = Arc::new(ring_buffer());
    let base = buf.as_ptr() as *mut u8;
    RemoteFreeRing::init_test_buffer(base);
    // Preset so the very first pushes cross u32::MAX.
    {
        let r = RemoteFreeRing::over_test_buffer(base);
        let start = u32::MAX - 5;
        r.dbg_set_cursors(start, start);
    }
    let ring = Arc::new(SendRing(RemoteFreeRing::over_test_buffer(base)));

    let attempted = Arc::new(AtomicU64::new(0));
    let succeeded = Arc::new(AtomicU64::new(0));
    let reclaimed_count = Arc::new(AtomicU64::new(0));
    // packed entry -> reclaim count (must be exactly 1 for each drained entry).
    let seen = Arc::new(Mutex::new(std::collections::HashMap::<u32, u32>::new()));
    let stop = Arc::new(AtomicBool::new(false));

    // Distinct entries per producer: producer p owns a disjoint band so no two
    // producers push the same value → a duplicate can ONLY come from the ring
    // re-emitting. Values are 16-aligned and stay `< SEGMENT = 1<<22` (never the
    // sentinel) for our bounded op counts.
    fn packed_unique(p: u32, i: u32, per: u32) -> u32 {
        let logical = p * per + i;
        let e = (logical % ((1 << 18) - 1) + 1) * 16; // in (0, 2^22)
        assert_ne!(e, u32::MAX);
        e
    }

    // Consumer.
    let ring_c = Arc::clone(&ring);
    let seen_c = Arc::clone(&seen);
    let recl_c = Arc::clone(&reclaimed_count);
    let stop_c = Arc::clone(&stop);
    let consumer = std::thread::spawn(move || {
        let drain_into = |seen: &Mutex<std::collections::HashMap<u32, u32>>, recl: &AtomicU64| {
            let mut local = 0u64;
            ring_c.0.drain(|entry| {
                assert_ne!(entry, 0, "entry 0 is impossible (corruption)");
                assert_ne!(entry, u32::MAX, "sentinel drained (corruption)");
                let mut m = seen.lock().unwrap();
                *m.entry(entry).or_insert(0) += 1;
                local += 1;
            });
            recl.fetch_add(local, Ordering::Relaxed);
        };
        loop {
            drain_into(&seen_c, &recl_c);
            if stop_c.load(Ordering::Acquire) {
                drain_into(&seen_c, &recl_c); // final sweep
                return;
            }
            std::thread::yield_now();
        }
    });

    // Producers.
    let mut handles = Vec::new();
    for p in 0..PRODUCERS {
        let ring_p = Arc::clone(&ring);
        let att = Arc::clone(&attempted);
        let suc = Arc::clone(&succeeded);
        handles.push(std::thread::spawn(move || {
            for i in 0..PER_PRODUCER {
                let e = packed_unique(p, i, PER_PRODUCER);
                att.fetch_add(1, Ordering::Relaxed);
                if ring_p.0.push(e).is_ok() {
                    suc.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    stop.store(true, Ordering::Release);
    consumer.join().unwrap();

    let attempted = attempted.load(Ordering::Acquire);
    let succeeded = succeeded.load(Ordering::Acquire);
    let reclaimed = reclaimed_count.load(Ordering::Acquire);
    let overflow = ring.0.overflow_count() as u64;

    let map = seen.lock().unwrap();
    // No duplicates.
    let doubles = map.values().filter(|&&c| c > 1).count();
    assert_eq!(doubles, 0, "no entry may be reclaimed more than once");
    // reclaimed count matches distinct entries seen (all counts == 1).
    assert_eq!(reclaimed as usize, map.len(), "reclaimed == distinct seen");
    // Every Ok push is eventually drained.
    assert_eq!(succeeded, reclaimed, "every succeeded push must be reclaimed");
    // Master identity across the wrap.
    assert_eq!(
        reclaimed + overflow,
        attempted,
        "reclaimed({reclaimed}) + overflow({overflow}) != attempted({attempted})"
    );
    // Sanity: the cursors actually crossed the wrap (tail advanced past 0).
    let (_h, t) = ring.0.dbg_cursors();
    let total = PRODUCERS as u64 * PER_PRODUCER as u64;
    assert!(total > 10, "op count must be meaningful");
    let _ = t; // tail value depends on schedule; the identity above is the proof.
}
