//! Regression (R34-17/task #536, release-stabilization finding F-7 [low]):
//! `RemoteFreeRing::drain` must publish its `head` cursor via an RAII guard so
//! that a `reclaim` closure which **unwinds mid-drain** still commits the
//! progress actually made — instead of leaving `head` at its pre-drain value
//! and wedging the ring.
//!
//! ## The defect this covers
//!
//! Before R34-17, `drain` stored `head` (`Release`) ONLY in a single
//! `self.head().store(h, Release)` line AFTER the `while h != t` loop. If the
//! `reclaim` closure unwound (a `debug_assert!` in `dec_live_and_maybe_decommit`,
//! `sync_directory_for_segment_classes`, a magazine-residency predicate, etc.
//! — none of the real reclaim closures carries a no-panic contract), that
//! post-loop store was never reached, so `head` stayed at its last-published
//! value. The next `drain` re-read that stale `head` and, because the slots of
//! any FULLY-processed offsets (reclaim + `slot.store(EMPTY)` + `h += 1`) were
//! now `EMPTY`, hit the `if off == RING_SLOT_EMPTY { break; }` at the very first
//! cleared slot — reclaiming NOTHING and leaving every offset from the panicking
//! iteration onward permanently stuck in the ring (a stuck "false-empty" that
//! persists until the segment is recycled and the ring reset). No current
//! reachable production path triggers this (`AllocCore::reclaim_offset` is
//! panic-hardened), so this is hardening, not a live-bug fix.
//!
//! ## The fix under test
//!
//! `DrainHeadPublish` — a tiny RAII guard whose `Drop` does the sole
//! `head.store(h, Release)`. On the happy path its `Drop` runs at scope end; on
//! the unwind path its `Drop` runs during unwind — either way `h` holds the
//! most-recently-advanced value, so only real progress is published.
//!
//! ## How the unwind is forced
//!
//! The `reclaim` closure is a plain `FnMut(u32)` supplied by THIS test (not the
//! production reclaim path), so no test-injection hook is needed: the closure
//! counts its own calls and `panic!()`s on the 3rd call **before** recording
//! the 3rd offset. That deterministically exercises the unwind-out-of-reclaim
//! path the guard exists to defend.
//!
//! ## Non-vacuousness (counterfactual)
//!
//! The assertion `head == 2` after the panicking drain is the counterfactual:
//! without `DrainHeadPublish` (the post-loop store skipped by the unwind),
//! `head` would stay `0` (its pre-drain value), the test's `head == 2` check
//! would FAIL, and the follow-up second drain would reclaim zero offsets
//! (breaking at the cleared `slot[0]`). Verified by temporarily neutering the
//! guard's `Drop` (commenting out its `head.store`), confirming both failures,
//! then restoring — `git diff` showed zero residual changes.

#![cfg(all(
    all(feature = "alloc-core", feature = "alloc-xthread"),
    feature = "internals"
))]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT};

/// Allocate a FOOTPRINT-sized, 4-byte-aligned buffer for an isolated ring.
fn ring_buffer() -> Box<[u8]> {
    let mut buf: Vec<u8> = vec![0u8; FOOTPRINT];
    assert!(
        (buf.as_mut_ptr() as usize).is_multiple_of(core::mem::align_of::<u32>()),
        "ring buffer must be 4-byte aligned"
    );
    buf.into_boxed_slice()
}

/// A panicking `drain` reclaim closure must still publish the `head` cursor up
/// to the offsets FULLY processed before the panic (R34-17/task #536, F-7). The
/// guard's `Drop` is the sole `head.store`, so a mid-drain unwind publishes
/// partial progress instead of leaving `head` wedged at its pre-drain value.
#[test]
fn drain_panicking_closure_publishes_partial_head() {
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    // SAFETY: `buf` is a FOOTPRINT-sized, 4-byte-aligned, exclusively-owned
    // buffer live for the whole test.
    unsafe { RemoteFreeRing::init_test_buffer(base) };
    // SAFETY: same buffer, still live.
    let ring = unsafe { RemoteFreeRing::over_test_buffer(base) };

    // Four distinct, valid offsets (each < SEGMENT, != RING_SLOT_EMPTY).
    let offsets = [100u32, 200, 300, 400];
    for &off in &offsets {
        assert!(ring.push(off).is_ok(), "push must succeed on a fresh ring");
    }
    let (h, t) = ring.dbg_cursors();
    assert_eq!((h, t), (0, 4), "fresh ring: head=0, tail=4 after 4 pushes");

    // Reclaim closure: reclaim (record) offsets for calls 1 and 2; on call 3,
    // panic BEFORE recording — so offsets[0] and offsets[1] are FULLY processed
    // (reclaim + slot.clear + advance) and the 3rd iteration unwinds inside
    // `reclaim` with offsets[2]'s slot still occupied and h un-advanced.
    let reclaimed = Arc::new(Mutex::new(Vec::<u32>::new()));
    let call_count = Arc::new(AtomicU32::new(0));
    let reclaimed_c = Arc::clone(&reclaimed);
    let call_count_c = Arc::clone(&call_count);

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ring.drain(|off| {
            let n = call_count_c.fetch_add(1, Ordering::SeqCst) + 1;
            if n == 3 {
                panic!("deliberate panic inside reclaim (F-7 test, R34-17/task #536)");
            }
            reclaimed_c.lock().expect("reclaim lock poisoned").push(off);
        });
    }))
    .is_err();
    assert!(
        panicked,
        "reclaim closure was expected to panic on the 3rd call"
    );

    // Only the first two offsets were recorded (the closure panicked before
    // recording the 3rd).
    let recorded = reclaimed.lock().expect("reclaim lock poisoned").clone();
    assert_eq!(
        recorded,
        vec![100, 200],
        "only the first two offsets reclaimed"
    );

    // ── THE GUARD'S EFFECT ─────────────────────────────────────────────────
    // `head` was published to 2 — the two FULLY-processed offsets' progress is
    // committed. WITHOUT the guard, `head` would stay 0 (the post-loop store
    // skipped by the unwind), this assertion would FAIL, and the second drain
    // below would reclaim nothing (breaking at the now-EMPTY slot[0]).
    let (head, tail) = ring.dbg_cursors();
    assert_eq!(
        head, 2,
        "head must be published to the partial progress (2 offsets fully processed)"
    );
    assert_eq!(
        tail, 4,
        "tail is producer-owned and unaffected by the drain panic"
    );

    // ── NO LEAK: a second (non-panicking) drain reaches the remaining offsets
    // With the guard, head=2, so this drain starts at slot[2]=300 and reclaims
    // 300 and 400 cleanly. WITHOUT the guard (head=0), this drain would break
    // at slot[0]=EMPTY and reclaim nothing — 300 and 400 would be stuck forever.
    let mut second = Vec::new();
    let final_head = ring.drain(|off| {
        second.push(off);
    });
    assert_eq!(
        second,
        vec![300, 400],
        "second drain reclaims the two stuck offsets"
    );
    assert_eq!(
        final_head, 4,
        "second drain advances head to tail (ring drained)"
    );

    // Total across both drains: all four offsets, each exactly once — no loss,
    // no duplication.
    let mut all = recorded;
    all.extend(second);
    all.sort();
    assert_eq!(
        all,
        vec![100, 200, 300, 400],
        "no offset lost or duplicated"
    );
}

/// The head-publish guard publishes EXACTLY ONCE even when the drain completes
/// normally (no panic): the return value equals the published `head`, and a
/// follow-up `dbg_cursors` read agrees. Guards against a regression where the
/// guard's `Drop` and a leftover explicit store both fire (double-store).
#[test]
fn drain_normal_path_publishes_head_once() {
    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    unsafe { RemoteFreeRing::init_test_buffer(base) };
    let ring = unsafe { RemoteFreeRing::over_test_buffer(base) };

    for &off in &[10u32, 20, 30] {
        assert!(ring.push(off).is_ok());
    }

    let mut reclaimed = Vec::new();
    let returned_head = ring.drain(|off| reclaimed.push(off));
    assert_eq!(reclaimed, vec![10, 20, 30]);

    // The returned head must equal the persisted head (guard published once).
    let (head, tail) = ring.dbg_cursors();
    assert_eq!(returned_head, head, "drain return value == published head");
    assert_eq!(head, tail, "ring fully drained: head == tail");
    assert_eq!(head, 3);
}
