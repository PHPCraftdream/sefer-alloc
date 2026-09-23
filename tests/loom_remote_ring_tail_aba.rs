//! loom model-check of the **R2-10 ABA hazard** on `RemoteFreeRing`'s `u32`
//! tail CAS (task #2012, R2-10 P2,
//! `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`).
//!
//! # The hazard, in one paragraph
//!
//! `RemoteFreeRing::push` (`src/alloc_core/segment/remote_free_ring/ops.rs`) reads
//! `t = tail.load(Relaxed)`, then checks capacity via `full_check(t)` (which
//! reads `head` at THAT instant), and only THEN attempts
//! `tail.compare_exchange_weak(t, t+1, ...)`. If a producer is preempted
//! AFTER its capacity check succeeds but BEFORE its CAS runs, and enough
//! OTHER producers + the consumer complete a full `u32` incarnation cycle
//! (net `2^32` pushes) while it is stalled, `tail`'s CURRENT value can
//! coincidentally equal the stalled producer's long-stale snapshot again —
//! its CAS then succeeds by numeric coincidence, NOT because the capacity
//! check it already performed is still valid. The result: an over-capacity
//! reservation that overwrites a live, undrained entry (a torn/lost update)
//! and/or violates the ring's own `tail.wrapping_sub(head) <= RING_CAP`
//! invariant. See `src/alloc_core/segment/remote_free_ring/mod.rs`'s module doc,
//! new "R2-10 — the tail-CAS ABA hazard" section, for the full writeup and
//! this hazard's relationship to the ALREADY-covered Kani proofs (which
//! prove the single-call modular arithmetic is exact, but say nothing about
//! a STALE snapshot surviving a full wrap — a temporal, multi-step property
//! Kani's per-call proofs cannot express).
//!
//! # Scope — a REDUCED-WIDTH stand-in, not the real `u32`/`u64` type
//!
//! Like `loom_remote_ring.rs`, this models the protocol in isolation using
//! `loom::sync::atomic` — NOT the real `RemoteFreeRing`. loom cannot
//! practically explore `2^32` real states, so the model's cursor arithmetic
//! is TRUNCATED to a tiny modulus (`MOD = 4`, via `wrapping_add`/masking on
//! a `u32` atomic — a real narrow integer type is not required; the mask
//! reproduces the exact same wrap SHAPE at a width loom can exhaustively
//! reason about) with `CAP = 2`. A full "incarnation cycle" at this scale is
//! 4 successful pushes — reproducible with a handful of real operations, per
//! this task's own acceptance criterion ("no billions of real operations are
//! needed for this").
//!
//! The "stalled producer" gap (long preemption between a producer's capacity
//! check and its CAS) is SCRIPTED, not discovered by loom's own interleaving
//! search — loom cannot wait for `2^32` other events either. This mirrors
//! the task's own suggested approach: "an explicit thread ordering that lets
//! other threads run many operations first, and only then attempts its CAS."
//! `NARROW`'s tests 1-2 reproduce the hazard against the CURRENT (buggy)
//! protocol shape; `WIDE`'s test 3 re-runs the IDENTICAL finite script
//! against a cursor wide enough that it cannot coincidentally wrap back to
//! the stale snapshot — modelling why the real fix (widen `RemoteFreeRing`'s
//! `head`/`tail`/`cached_head` from `u32` to `u64`) closes the hole: a `u64`
//! cursor would need the same "centuries under any realistic throughput"
//! amount of activity to wrap that the review itself names, so ANY
//! realistic finite amount of interleaved activity during a stall (of which
//! this test's tiny script is a representative instance, not a special
//! case) cannot produce the coincidence.
//!
//! # Status of the real fix (honesty note)
//!
//! This file provides the reproduction + closure evidence the task asked
//! for. It does NOT land the `u32` → `u64` widening in
//! `src/alloc_core/segment/remote_free_ring/` itself — that change's blast radius
//! (layout constants, ~10 test files purpose-built around the `u32`
//! wrap boundary, e.g. `tests/regression_ring_cursor_wrap.rs`, several
//! `dbg_*` test-hook signatures, `src/kani_proofs.rs`'s `ring_wrap_proofs`
//! module) was judged too large to complete and fully zero-trust-verify in
//! one task cycle, and this hazard requires ~`2^32` operations during ONE
//! producer's stall to trigger — the same order of magnitude of rarity this
//! same module's own "F10 wrap argument precondition" section already
//! accepts elsewhere ("No code change is warranted for a hazard this
//! remote"). Tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 149 (see
//! `docs/correctness-open-items/ACTIVE.md`), mirroring how R2-09 (item 148,
//! same review round) scoped an equally-large redesign out of its own task
//! cycle in favor of an honest interim record.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-core,alloc-xthread --test loom_remote_ring_tail_aba
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel slot value meaning "no offset" (not-yet-published or drained).
/// Matches `RING_SLOT_EMPTY` in the real `RemoteFreeRing` (`u32::MAX`).
const EMPTY: u32 = u32::MAX;

/// The reduced modulus: cursors wrap at `MOD = 4` (mask `0b11`) instead of
/// the real `2^32` — a full incarnation cycle is 4 successful pushes.
const MASK: u32 = 3;
/// The reduced capacity: 2 live slots.
const CAP: u32 = 2;

/// The NARROW (current, buggy-shape) model: `head`/`tail` cursors wrap at
/// `MOD = 4` via explicit masking — the truncated-width stand-in for the
/// real `u32` cursor's `2^32` wrap. Mirrors `RemoteFreeRing::push`'s exact
/// shape: a separate capacity check (`full_check`) BEFORE the CAS, with no
/// generation/epoch tag binding the two together.
struct NarrowRing {
    head: AtomicU32,
    tail: AtomicU32,
    slots: [AtomicU32; CAP as usize],
}

impl NarrowRing {
    fn new() -> Arc<Self> {
        Arc::new(NarrowRing {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(EMPTY)),
        })
    }

    /// Mirrors `RemoteFreeRing::full_check`'s slow path exactly (the
    /// mechanism the hazard lives in; the F10 shadow fast path is an
    /// orthogonal optimisation layered on top of the SAME underlying
    /// snapshot-then-CAS shape and is not separately modelled here).
    fn full_check(&self, t: u32) -> Result<(), ()> {
        let h = self.head.load(Ordering::Acquire);
        if (t.wrapping_sub(h)) & MASK < CAP {
            Ok(())
        } else {
            Err(())
        }
    }

    /// The CORRECT (fresh-snapshot) push: read `t`, full-check, CAS, publish
    /// — exactly `RemoteFreeRing::push`'s protocol, one iteration per call
    /// (no contention in this file's sequential setup scripts).
    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            self.full_check(t)?;
            match self.tail.compare_exchange_weak(
                t,
                (t.wrapping_add(1)) & MASK,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slots[(t as usize) % CAP as usize].store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Mirrors `RemoteFreeRing::drain` exactly (wrap-correct, break on an
    /// unpublished slot, clear after reclaiming).
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP as usize];
            let off = slot.load(Ordering::Acquire);
            if off == EMPTY {
                break;
            }
            reclaim(off);
            slot.store(EMPTY, Ordering::Relaxed);
            h = (h.wrapping_add(1)) & MASK;
        }
        self.head.store(h, Ordering::Release);
    }

    /// The STALLED producer's resumed action: a CAS against a snapshot `t`
    /// taken long ago, WITHOUT redoing `full_check`. This is exactly what
    /// `RemoteFreeRing::push`'s real loop does after its full-check succeeds
    /// — the CAS never re-validates capacity, it only compares against the
    /// `t` the (now long-stale) check already approved. Returns whether the
    /// CAS succeeded; on success, publishes `offset` into the reserved slot
    /// exactly like `push` does.
    fn late_cas_publish(&self, stale_t: u32, offset: u32) -> bool {
        match self.tail.compare_exchange(
            stale_t,
            (stale_t.wrapping_add(1)) & MASK,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                self.slots[(stale_t as usize) % CAP as usize].store(offset, Ordering::Release);
                true
            }
            Err(_) => false,
        }
    }
}

/// The WIDE (fixed-shape) model: byte-identical protocol, but the cursor
/// arithmetic is NOT masked — real, unbounded `u32` `wrapping_add`/
/// `wrapping_sub`. Relative to the tiny finite script both models run in
/// this file, a `u32` space (let alone the real fix's `u64`) is
/// "wide enough" that the same script cannot wrap it back to the stale
/// snapshot — modelling why widening the real cursor closes the hole.
struct WideRing {
    head: AtomicU32,
    tail: AtomicU32,
    slots: [AtomicU32; CAP as usize],
}

impl WideRing {
    fn new() -> Arc<Self> {
        Arc::new(WideRing {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(EMPTY)),
        })
    }

    fn full_check(&self, t: u32) -> Result<(), ()> {
        let h = self.head.load(Ordering::Acquire);
        if t.wrapping_sub(h) < CAP {
            Ok(())
        } else {
            Err(())
        }
    }

    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            self.full_check(t)?;
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slots[(t as usize) % CAP as usize].store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP as usize];
            let off = slot.load(Ordering::Acquire);
            if off == EMPTY {
                break;
            }
            reclaim(off);
            slot.store(EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }

    fn late_cas_publish(&self, stale_t: u32, offset: u32) -> bool {
        match self.tail.compare_exchange(
            stale_t,
            stale_t.wrapping_add(1),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                self.slots[(stale_t as usize) % CAP as usize].store(offset, Ordering::Release);
                true
            }
            Err(_) => false,
        }
    }
}

// =========================================================================
// Counterfactual 1 — the narrow (current-shape) ring's capacity invariant
// (`tail.wrapping_sub(head) <= CAP`) is VIOLATED once a stale-snapshot CAS
// survives a full wraparound.
// =========================================================================

/// `#[should_panic]`: reproduces the R2-10 mechanism exactly as traced
/// through the real code. Thread A takes its capacity-check snapshot `t0`,
/// then "stalls" (the snapshot is simply held, per this file's scripted-gap
/// approach). While it is stalled, a scripted sequence of OTHER pushes +
/// one drain — the reduced-scale stand-in for "many other producers and the
/// consumer completing a full incarnation cycle" — advances `tail` through
/// exactly one full lap of the `MOD = 4` space, landing back on `t0` with
/// the ring genuinely FULL again (two NEW live, undrained entries). Thread
/// A then resumes and CASes against its stale `t0` — on the current
/// protocol shape this SUCCEEDS by numeric coincidence, violating the
/// ring's own documented capacity invariant.
#[test]
#[should_panic(expected = "R2-10 ABA")]
fn counterfactual_narrow_tail_stale_cas_violates_capacity_invariant() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = NarrowRing::new();

        // Thread A's capacity-check snapshot, taken while the ring is fresh
        // (this succeeded a real `full_check(t0)`, elided here since its
        // result — "room exists" — is exactly what a fresh ring guarantees).
        let t0 = ring.tail.load(Ordering::Relaxed);
        assert_eq!(t0, 0, "sanity: fresh ring's tail starts at 0");

        // Thread A stalls HERE. Scripted gap: the sequence below is the
        // reduced-scale stand-in for "billions of other operations complete
        // while A is preempted" — push, push (fills CAP=2), drain (reclaims
        // both), push, push (wraps tail exactly one lap, back to t0).
        assert!(ring.push(555).is_ok());
        assert!(ring.push(556).is_ok());
        let mut first_drain = Vec::new();
        ring.drain(|off| first_drain.push(off));
        assert_eq!(
            first_drain,
            vec![555, 556],
            "first drain must reclaim both fresh pushes"
        );
        assert!(ring.push(557).is_ok());
        assert!(ring.push(558).is_ok());

        let h_before = ring.head.load(Ordering::Acquire);
        let t_before = ring.tail.load(Ordering::Acquire);
        assert_eq!(
            t_before, t0,
            "sanity: tail must have wrapped exactly one full lap back to A's stale snapshot"
        );
        assert_eq!(
            (t_before.wrapping_sub(h_before)) & MASK,
            CAP,
            "sanity: ring must be genuinely full (occupancy == CAP) when A resumes"
        );

        // Thread A resumes: CASes against its long-stale t0, WITHOUT
        // redoing full_check (see NarrowRing::late_cas_publish's doc).
        let succeeded = ring.late_cas_publish(t0, 999);

        // The occupancy invariant `tail.wrapping_sub(head) <= CAP` (the
        // ring's own documented invariant — see
        // `src/alloc_core/segment/remote_free_ring/ops.rs`'s "full-check" doc) MUST
        // hold after EVERY successful reservation. If A's stale CAS
        // succeeded, it just reserved a THIRD slot into an already-full
        // ring — violating it.
        if succeeded {
            let h_after = ring.head.load(Ordering::Acquire);
            let t_after = ring.tail.load(Ordering::Acquire);
            let occupancy_after = (t_after.wrapping_sub(h_after)) & MASK;
            assert!(
                occupancy_after <= CAP,
                "R2-10 ABA: stale tail CAS succeeded against a genuinely full ring \
                 after a full counter wraparound back to the stalled producer's \
                 snapshot — occupancy is now {occupancy_after} > CAP ({CAP})"
            );
        }
        // Even if the naive occupancy arithmetic above happened not to catch
        // it (it does, for this exact scripted state), the CAS succeeding at
        // all here is itself the violation — a fresh, correct full_check at
        // the moment of the CAS would have rejected it (see the "full ring"
        // sanity assert above). State that explicitly too.
        assert!(
            !succeeded,
            "R2-10 ABA: stale tail CAS succeeded against a genuinely full ring \
             after a full counter wraparound back to the stalled producer's \
             snapshot — a fresh full_check at CAS time would have rejected this"
        );
    });
}

// =========================================================================
// Counterfactual 2 — the practical consequence: a live, undrained entry is
// silently overwritten (lost) by the stale CAS's publish.
// =========================================================================

/// `#[should_panic]`: same reproduction as above, one step further — proves
/// the CONCRETE consequence the review names ("an overwrite of an unread
/// entry... silently overwritten"), not just the abstract occupancy
/// invariant. After A's stale CAS succeeds and publishes into the recycled
/// slot, offset 557 (which was live and undrained in that same slot) is
/// never seen by any subsequent drain — permanently lost.
#[test]
#[should_panic(expected = "lost")]
fn counterfactual_narrow_tail_stale_cas_overwrites_live_undrained_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = NarrowRing::new();
        let t0 = ring.tail.load(Ordering::Relaxed);

        assert!(ring.push(555).is_ok());
        assert!(ring.push(556).is_ok());
        let mut first_drain = Vec::new();
        ring.drain(|off| first_drain.push(off));
        assert_eq!(first_drain, vec![555, 556]);
        // These two are LIVE and UNDRAINED at the moment A resumes — 557
        // lands in the same physical slot A is about to reserve.
        assert!(ring.push(557).is_ok());
        assert!(ring.push(558).is_ok());

        let succeeded = ring.late_cas_publish(t0, 999);
        assert!(
            succeeded,
            "setup invariant: this reproduction assumes the ABA CAS succeeds \
             (see the sibling occupancy-invariant test for the direct proof \
             that it does on the current protocol shape)"
        );

        // Drain whatever is left. A correct protocol never loses a
        // successfully-published, not-yet-drained offset.
        let mut got = Vec::new();
        ring.drain(|off| got.push(off));
        ring.drain(|off| got.push(off)); // second pass: catch anything a
                                         // slot-aliasing early-break stranded

        assert!(
            got.contains(&557),
            "lost: R2-10 ABA overwrote the live, undrained offset 557 in the \
             recycled slot before it could ever be drained (drained instead: {got:?})"
        );
    });
}

// =========================================================================
// Test 3 — the fix's shape: a wide-enough cursor closes the hole.
// =========================================================================

/// Regular (non-panicking) test. Re-runs the IDENTICAL finite script
/// against `WideRing` (unmasked, real `u32` wraparound) — the reduced-scale
/// stand-in for the real fix (widen `head`/`tail`/`cached_head` from `u32`
/// to `u64`). With a cursor space this much larger than the script's
/// operation count, the same stalled-producer gap cannot produce the
/// coincidental wrap-back: `tail` only advances to 4, nowhere near
/// colliding with A's stale snapshot (0). A's late CAS therefore correctly
/// FAILS (a plain compare-mismatch — the same outcome ordinary CAS
/// contention already produces), forcing exactly the safe fallback the real
/// protocol already has for a lost CAS race: retry with a fresh
/// `tail.load`/`full_check`.
#[test]
fn correct_wide_tail_stale_cas_rejects_after_same_finite_script() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = WideRing::new();
        let t0 = ring.tail.load(Ordering::Relaxed);
        assert_eq!(t0, 0);

        // IDENTICAL script to the narrow reproduction: 2 pushes, 1 drain
        // (reclaims both), 2 more pushes.
        assert!(ring.push(555).is_ok());
        assert!(ring.push(556).is_ok());
        let mut first_drain = Vec::new();
        ring.drain(|off| first_drain.push(off));
        assert_eq!(first_drain, vec![555, 556]);
        assert!(ring.push(557).is_ok());
        assert!(ring.push(558).is_ok());

        let t_before = ring.tail.load(Ordering::Acquire);
        assert_ne!(
            t_before, t0,
            "sanity: a wide-enough cursor must NOT wrap back to the stale \
             snapshot after this same finite amount of activity (real tail: {t_before})"
        );

        // The fix: A's stale CAS now correctly fails.
        let succeeded = ring.late_cas_publish(t0, 999);
        assert!(
            !succeeded,
            "the fix: a wide cursor's stale CAS must fail (compare-mismatch), \
             not coincidentally succeed, once the counter space is larger than \
             any realistic amount of interleaved activity during a stall"
        );

        // No corruption: both live entries still drain cleanly, in order.
        let mut got = Vec::new();
        ring.drain(|off| got.push(off));
        assert_eq!(
            got,
            vec![557, 558],
            "wide-cursor ring must still drain cleanly after the correctly-rejected stale CAS"
        );
    });
}

// =========================================================================
// Test 4 — contrast: a concurrent FRESH producer's own full_check is not
// itself confused by a racing stale CAS.
// =========================================================================

/// Regular (non-panicking) test. After the same setup (ring full, A holds a
/// stale `t0`), spawns A's stale `late_cas_publish` and a fresh producer B's
/// ordinary, single-attempt `push` CONCURRENTLY, letting loom explore every
/// interleaving of their atomic accesses. INVARIANT under test: B's own
/// fresh full_check (which always re-reads the CURRENT `head`/`tail` at its
/// own call time) correctly rejects EVERY interleaving — occupancy is `>=
/// CAP` both before A's CAS (2) and after it (3), so B never wrongly
/// succeeds regardless of ordering. This isolates the bug to the STALE
/// snapshot specifically (A's `t0`, read long before the wrap), not to any
/// general fragility in the fresh-check protocol itself.
#[test]
fn narrow_fresh_producer_always_rejects_concurrently_with_racing_stale_cas() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let ring = NarrowRing::new();
        let t0 = ring.tail.load(Ordering::Relaxed);

        assert!(ring.push(555).is_ok());
        assert!(ring.push(556).is_ok());
        let mut first_drain = Vec::new();
        ring.drain(|off| first_drain.push(off));
        assert_eq!(first_drain, vec![555, 556]);
        assert!(ring.push(557).is_ok());
        assert!(ring.push(558).is_ok());
        assert_eq!(
            ring.tail.load(Ordering::Acquire),
            t0,
            "sanity: tail wrapped back to t0"
        );

        let ring_a = Arc::clone(&ring);
        let ta = thread::spawn(move || ring_a.late_cas_publish(t0, 999));

        let ring_b = Arc::clone(&ring);
        let tb = thread::spawn(move || ring_b.push(777));

        let _a_result = ta.join().unwrap();
        let b_result = tb.join().unwrap();

        assert!(
            b_result.is_err(),
            "a FRESH producer's own full_check must reject every interleaving here \
             (the ring is full both before and after A's stale CAS) — B unexpectedly \
             succeeded, which would point at a bug in the fresh-check path itself, \
             not the stale-snapshot ABA hazard this file targets"
        );
    });
}
