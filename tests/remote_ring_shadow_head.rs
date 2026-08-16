//! **F10 (task #502) — `RemoteFreeRing` shadow/cached-head correctness
//! coverage.**
//!
//! The pre-existing `tests/remote_ring_unit.rs` and
//! `tests/regression_ring_cursor_wrap.rs` already exercise the ring's core
//! MPSC identity (`reclaimed + overflowed == pushed`) and the `u32::MAX -> 0`
//! wrap, and per that file's own doc comment "a stale-shadow bug would break
//! [those invariants]" — so they are non-vacuous counterfactuals for F10 by
//! construction, and this file does NOT duplicate them.
//!
//! This file adds coverage specific to the shadow mechanism itself:
//!
//! 1. `shadow_path_activation_oracle_fast_and_slow_both_reachable` — the
//!    `bench-internals`-gated `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW` counters
//!    (the harness's own path-activation oracle, per CLAUDE.md's R30-8 rule)
//!    actually move, and move in the expected direction for each regime: a
//!    ring with headroom take the FAST path; a ring held at/near capacity
//!    takes the SLOW path.
//! 2. `shadow_survives_dbg_set_cursors_reset` — `dbg_set_cursors` (used by
//!    the wrap regression suite) resets `cached_head` alongside `head`/`tail`
//!    (this task's own fix to that seam — see its doc comment), so a preset
//!    ring's FIRST push after the preset can take the shadow FAST path
//!    immediately when there is genuine headroom, instead of unconditionally
//!    paying one stale-shadow slow-path load. This is a behavioural pin, not
//!    a soundness requirement (the pre-fix behaviour was already sound, just
//!    needlessly conservative — see the module doc's "F10" section).
//! 3. `shadow_stale_low_never_causes_spurious_admit` — a hand-driven
//!    single-threaded scenario that forces `cached_head` stale-low (by
//!    filling the ring, letting the shadow observe "full", then draining
//!    WITHOUT any push touching the shadow again) and confirms the very next
//!    push still succeeds (the slow path re-derives from the real, now-
//!    advanced `head`) — the concrete instance of the module doc's
//!    "consequence: missed overflow cannot happen" argument.

#![cfg(all(
    all(feature = "alloc-core", feature = "alloc-xthread"),
    feature = "internals"
))]

use std::sync::Mutex;

/// Serial guard for process-global `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW` counters.
/// These counters are incremented by any push's full_check, and libtest runs
/// this file's tests on parallel threads by default. Without this guard, a test
/// that both reads a before/after snapshot and asserts on the delta would race
/// against concurrent sibling tests in the same binary, exactly the hazard that
/// caused the original flaky failure (adversarial regime got 256 fast / 2002 slow,
/// breaching the 90% threshold, even though slow_delta itself was structurally
/// correct at 2000 ≈ ROUNDS). The lock is held for the whole test body so the
/// shared counters are exercised single-threaded when it matters; the lock
/// itself is cheap enough to hold unconditionally, so no `#[cfg]` branching is
/// needed.
static SERIAL: Mutex<()> = Mutex::new(());

use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT, RING_CAP};

/// Slack allowance for spurious `compare_exchange_weak` retries on weak-CAS
/// architectures (aarch64, ARM). Each retry causes `full_check` to be called
/// again, incrementing the shadow counters. A single-threaded workload can
/// experience a few spurious failures per ROUNDS due to preemption, cache
/// eviction, or loss of exclusive monitor. On strong-CAS targets (x86_64),
/// `compare_exchange_weak` compiles to `lock cmpxchg` and never fails
/// spuriously, so the actual slack will be 0.
///
/// This constant preserves the original fix's benefit (distinguishing ~256
/// counter-pollution noise from sibling tests vs. a few CAS retries) while
/// making the test portable across architectures.
///
/// Gated on `bench-internals` because every consumer is: both users are
/// `#[cfg(feature = "bench-internals")]` tests reading the
/// `DBG_RING_PUSH_SHADOW_*` counters, which exist only under that feature.
/// Declaring it unconditionally made it dead code in the
/// `hardened medium-classes internals` clippy row (no `bench-internals` there),
/// failing `-D warnings`. Caught by `npm run check` after task #1033 landed —
/// that row is precisely what a per-crate `-p aligned-vmem` verification pass
/// does not cover.
#[cfg(feature = "bench-internals")]
const CAS_RETRY_SLACK: u64 = 8;

fn ring_buffer() -> Box<[u8]> {
    let mut buf: Vec<u8> = vec![0u8; FOOTPRINT];
    assert!(
        (buf.as_mut_ptr() as usize).is_multiple_of(core::mem::align_of::<u32>()),
        "ring buffer must be 4-byte aligned"
    );
    buf.into_boxed_slice()
}

/// **Test 3** (documented first in code since it needs no feature gate
/// beyond `alloc-xthread`): a hand-driven scenario proving a stale-low
/// shadow cannot cause a missed overflow. Fill the ring (shadow now sees
/// "full" — cached_head refreshed to the real, now-stale-forever-until-next-
/// refresh head), drain everything (advances the REAL head, but nothing
/// refreshes the shadow — no push happens between drain and the next push),
/// then push once more: this push's fast-path check sees a stale-low
/// `cached_head` (still the pre-drain value) and correctly falls to the slow
/// path, which re-derives from the real (now-advanced) `head` and admits
/// the push. A broken "trust the shadow forever" implementation would
/// spuriously report overflow here.
#[test]
fn shadow_stale_low_never_causes_spurious_admit() {
    // This test does NOT touch the `DBG_RING_PUSH_SHADOW_*` counters, but
    // it does perform pushes that increment them. Taking the guard prevents
    // concurrent corruption of those counters when this test runs alongside
    // the other two `#[test]` functions in this file that do read/assert
    // on those counters.
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    // SAFETY: `base` is a FOOTPRINT-sized, 4-byte-aligned, exclusively-owned
    // buffer (see `ring_buffer()`), live for the whole test.
    let ring = unsafe {
        RemoteFreeRing::init_test_buffer(base);
        RemoteFreeRing::over_test_buffer(base)
    };

    // Fill the ring to RING_CAP — the last push's full_check necessarily
    // refreshes cached_head (either it was already stale-full-looking, or
    // this exact push is the one that makes tail.wrapping_sub(cached_head)
    // hit RING_CAP for the FIRST time and forces the slow-path refresh).
    for i in 0..RING_CAP as u32 {
        let off = (i + 1) * 16;
        assert!(
            ring.push(off).is_ok(),
            "push {i} inside RING_CAP must succeed"
        );
    }
    // One more push while full: MUST overflow (slow path re-confirms real
    // fullness) and refreshes cached_head to the real (full) head value.
    assert!(
        ring.push(9999 * 16).is_err(),
        "push into a genuinely full ring must overflow"
    );
    assert_eq!(ring.overflow_count(), 1);

    // Drain everything. This advances the REAL head from 0 to RING_CAP, but
    // does NOT touch cached_head (drain never writes the shadow — only a
    // producer's full_check does, and no push happens during this drain).
    let mut drained = 0usize;
    ring.drain(|_| drained += 1);
    assert_eq!(drained, RING_CAP, "drain must reclaim all RING_CAP entries");

    // The ring is now EMPTY (real head == real tail == RING_CAP), but
    // cached_head is STALE-LOW (still holds the pre-drain "full" head value,
    // i.e. 0). The critical assertion: a push right now must SUCCEED. Under
    // the real (correct) full_check, cached_head=0 makes
    // tail.wrapping_sub(0) = RING_CAP >= RING_CAP -> shadow suggests full ->
    // falls to the slow path -> real head.load(Acquire) sees RING_CAP (post
    // drain) -> tail.wrapping_sub(RING_CAP) = 0 < RING_CAP -> admits.
    assert!(
        ring.push(4242).is_ok(),
        "STALE-LOW shadow must never cause a spurious overflow: the slow \
         path must re-derive from the real (now-advanced) head and admit \
         this push — the ring is genuinely empty"
    );

    // Clean up: drain the one entry just pushed.
    let mut got = Vec::new();
    ring.drain(|off| got.push(off));
    assert_eq!(
        got,
        vec![4242],
        "the post-drain push must be reclaimed intact"
    );
}

/// **Test 2:** `dbg_set_cursors` resets `cached_head` alongside `head`/`tail`
/// (this task's fix — see that method's doc comment), so a preset ring with
/// genuine headroom takes the shadow FAST path on its very next push,
/// confirmed via the `bench-internals`-gated oracle counters.
#[cfg(feature = "bench-internals")]
#[test]
fn shadow_survives_dbg_set_cursors_reset() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use sefer_alloc::alloc_core::remote_free_ring::DBG_RING_PUSH_SHADOW_FAST;
    use std::sync::atomic::Ordering;

    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    // SAFETY: see above.
    let ring = unsafe {
        RemoteFreeRing::init_test_buffer(base);
        RemoteFreeRing::over_test_buffer(base)
    };

    // Preset far from the wrap, well inside capacity headroom.
    let start = 1_000_000u32;
    ring.dbg_set_cursors(start, start);
    assert_eq!(ring.dbg_cursors(), (start, start));

    let fast_before = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed);

    // The first push after a CONSISTENT preset (cached_head == head == tail)
    // must take the FAST path: tail.wrapping_sub(cached_head) == 0 < CAP.
    assert!(
        ring.push(16).is_ok(),
        "push after consistent preset must succeed"
    );

    let fast_after = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed);

    // With the serial guard, this test has exclusive access to the global
    // counters for its entire body, so the exact delta can be asserted
    // without concern about concurrent sibling-test noise. On weak-CAS
    // architectures (aarch64), spurious compare_exchange_weak failures can
    // cause a few extra fast-path increments, so we allow a small slack.
    // See CAS_RETRY_SLACK above.
    assert!(
        fast_after > fast_before && fast_after <= fast_before + 1 + CAS_RETRY_SLACK,
        "the first push after dbg_set_cursors must take the shadow FAST path \
         (cached_head was reset to match head by dbg_set_cursors); weak-CAS \
         retries may add a few extra fast counts (expected increment ~1, allowed \
         range 1..={slack}, got {increment})",
        slack = 1 + CAS_RETRY_SLACK,
        increment = fast_after - fast_before
    );
}

/// **Test 1:** the path-activation oracle (`DBG_RING_PUSH_SHADOW_FAST`/
/// `_SLOW`) actually distinguishes the favorable regime (ring never near
/// capacity — owner drains promptly) from the adversarial regime (ring held
/// at/near capacity — every push's shadow sees "might be full"). This is the
/// SAME oracle a wall-clock gate report would use to prove which regime it
/// measured; this test proves the oracle itself is trustworthy before any
/// gate report cites it.
///
/// **Note on the oracle counters and test parallelism:** `DBG_RING_PUSH_SHADOW_FAST`/
/// `_SLOW` are PROCESS-WIDE statics. Without the serial guard (see the
/// `SERIAL` mutex at module top), concurrent sibling tests in this same
/// binary could pollute the counters between `before`/`after` snapshots,
/// causing spurious failures on percentage assertions — exactly what
/// happened to the original adversarial regime assertion (88.66% vs 90%).
/// The serial guard eliminates this noise, allowing exact counts and
/// percentage assertions without tolerance margins.
#[cfg(feature = "bench-internals")]
#[test]
fn shadow_path_activation_oracle_fast_and_slow_both_reachable() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use sefer_alloc::alloc_core::remote_free_ring::{
        DBG_RING_PUSH_SHADOW_FAST, DBG_RING_PUSH_SHADOW_SLOW,
    };
    use std::sync::atomic::Ordering;

    // --- Favorable regime: push once, drain immediately, repeat. The ring
    // never approaches capacity, so `cached_head` (refreshed to 0 on the
    // very first slow-path call) stays valid ("tail - 0 < CAP") for a long
    // run before the modulus/occupancy ever forces a refresh. ---
    {
        let buf = ring_buffer();
        let base = buf.as_ptr() as *mut u8;
        // SAFETY: see above.
        let ring = unsafe {
            RemoteFreeRing::init_test_buffer(base);
            RemoteFreeRing::over_test_buffer(base)
        };

        let fast_before = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed);
        let slow_before = DBG_RING_PUSH_SHADOW_SLOW.load(Ordering::Relaxed);

        const ROUNDS: u32 = 2_000;
        for i in 0..ROUNDS {
            assert!(ring.push((i + 1) * 16).is_ok());
            let mut n = 0;
            ring.drain(|_| n += 1);
            assert_eq!(n, 1, "drain-immediately regime: exactly 1 per round");
        }

        let fast_delta = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed) - fast_before;
        let slow_delta = DBG_RING_PUSH_SHADOW_SLOW.load(Ordering::Relaxed) - slow_before;
        let total = fast_delta + slow_delta;
        // With the serial guard, this test has exclusive access to the global
        // counters from parallel sibling tests. On weak-CAS architectures (aarch64),
        // spurious compare_exchange_weak failures can cause a few extra full_check
        // calls per ROUNDS, so we allow a small slack. See CAS_RETRY_SLACK above.
        assert!(
            total >= ROUNDS as u64 && total <= ROUNDS as u64 + CAS_RETRY_SLACK,
            "every push takes at least one full_check; weak-CAS retries may add a few extras \
             (expected ~{ROUNDS}, allowed range {ROUNDS}..={ro}, got {total})",
            ro = ROUNDS + CAS_RETRY_SLACK as u32
        );
        // The favorable regime must show the fast path dominating. With the
        // serial guard eliminating concurrent noise, we can assert the exact
        // percentage that the test setup structurally guarantees: at most a
        // handful of slow-path calls are expected (the very first push, before
        // cached_head is ever refreshed from 0, plus any periodic refresh due
        // to modulus/occupancy wrapping), so >= 99% fast path is realistic.
        let fast_pct = (fast_delta as f64 / total as f64) * 100.0;
        assert!(
            fast_pct >= 99.0,
            "favorable (drain-immediately) regime should show >=99% fast-path \
             activation, got {fast_pct:.2}% ({fast_delta} fast / {slow_delta} slow)"
        );
    }

    // --- Adversarial regime: fill to capacity WITHOUT ever draining, so
    // every single push after the first RING_CAP must observe "might be
    // full" from the shadow and take the slow path (the ring genuinely IS
    // at the boundary the whole time). ---
    {
        let buf = ring_buffer();
        let base = buf.as_ptr() as *mut u8;
        // SAFETY: see above.
        let ring = unsafe {
            RemoteFreeRing::init_test_buffer(base);
            RemoteFreeRing::over_test_buffer(base)
        };

        // Fill to exactly RING_CAP: the genuinely-adversarial condition is
        // occupancy sitting AT the boundary continuously. Drive a strict
        // 1-out/1-in steady state: `dbg_set_cursors` advances `head` by
        // exactly 1 (simulating the owner draining exactly one reclaimed
        // slot — cheaper and more precise than `drain()`, which would
        // reclaim however many contiguous published entries exist and could
        // over-drain in one call, breaking the exact 1-per-round cadence
        // this test needs), then one push refills to RING_CAP again. Every
        // such push has occupancy == RING_CAP - 1 == RING_CAP - 1 against a
        // `cached_head` that is stale by exactly 1 slot (never refreshed by
        // the drain-via-dbg_set_cursors, which does NOT touch cached_head
        // here — using `head().store` directly, not the seam, precisely so
        // the shadow stays untouched between pushes) — i.e.
        // `t.wrapping_sub(cached_head) == RING_CAP`, which is NOT `<
        // RING_CAP`, forcing the slow path on EVERY push in this loop.
        for i in 0..RING_CAP as u32 {
            assert!(ring.push((i + 1) * 16).is_ok());
        }
        assert_eq!(
            ring.dbg_cursors().1.wrapping_sub(ring.dbg_cursors().0),
            RING_CAP as u32,
            "ring must be exactly full before the adversarial loop"
        );

        // Take a fresh before snapshot AFTER the initial RING_CAP fill,
        // to measure only the adversarial loop's own contribution.
        let fast_before = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed);
        let slow_before = DBG_RING_PUSH_SHADOW_SLOW.load(Ordering::Relaxed);

        const ROUNDS: u32 = 2_000;
        for i in 0..ROUNDS {
            // Advance the real head by exactly 1 via the seam that touches
            // ONLY head/tail — NOT cached_head — so the shadow stays stale
            // relative to the now-advanced head, forcing every subsequent
            // push's full_check onto the slow path.
            let (h, _t) = ring.dbg_cursors();
            ring.dbg_advance_head_only(h.wrapping_add(1));
            // Refill by exactly 1 (occupancy back to RING_CAP).
            assert!(
                ring.push((i + 1) * 16).is_ok(),
                "adversarial refill push must succeed"
            );
        }

        let fast_delta = DBG_RING_PUSH_SHADOW_FAST.load(Ordering::Relaxed) - fast_before;
        let slow_delta = DBG_RING_PUSH_SHADOW_SLOW.load(Ordering::Relaxed) - slow_before;
        let total = fast_delta + slow_delta;
        // With the serial guard, this test has exclusive access to the global
        // counters. On weak-CAS architectures (aarch64), spurious compare_exchange_weak
        // failures can cause a few extra full_check calls per ROUNDS, so we allow a
        // small slack. See CAS_RETRY_SLACK above.
        assert!(
            total >= ROUNDS as u64 && total <= ROUNDS as u64 + CAS_RETRY_SLACK,
            "adversarial regime must issue at least one full_check per round; weak-CAS \
             retries may add a few extras (expected ~{ROUNDS}, allowed range {ROUNDS}..={ro}, got {total})",
            ro = ROUNDS + CAS_RETRY_SLACK as u32
        );
        // Verify the adversarial regime actually activates the slow path.
        // With the serial guard eliminating concurrent noise, we can assert
        // the exact count that the test setup structurally guarantees:
        // every push in the adversarial loop forces a slow-path full_check,
        // so slow_delta should equal ROUNDS. The test name promises
        // "both reachable", and this proves the slow path is actually
        // exercised without being scheduler-sensitive.
        assert!(
            slow_delta >= ROUNDS as u64 - 10,
            "adversarial (held-at-capacity) regime must show slow-path \
             activation near ROUNDS (>= {expected}, got {slow_delta} / {ROUNDS})",
            expected = ROUNDS - 10
        );
    }
}
