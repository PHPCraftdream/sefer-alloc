//! loom model-check of the R2-10 cursor-incarnation fix and its counterfactual.
//! The fixed reduced-width model has a finite, NON-WRAPPING 3-bit cursor
//! space (0..=7), the same checked-exhaustion rule as the production u64 ring.
//! A paused producer's old capacity check cannot validate a later CAS because
//! no reservation value is ever used twice, even after terminal exhaustion.
//! The old wrapping model is retained only as a negative control.
// RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_remote_ring_tail_aba
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

/// The NARROW (former, buggy-shape) model: `head`/`tail` cursors wrap at
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

/// Fixed reduced-width protocol: non-wrapping cursor 0..=7 with terminal
/// exhaustion, including the producer shadow-head optimization.
struct FixedRing {
    head: AtomicU32,
    tail: AtomicU32,
    cached_head: AtomicU32,
    slots: [AtomicU32; CAP as usize],
}

impl FixedRing {
    fn new() -> Arc<Self> {
        Arc::new(FixedRing {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            cached_head: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(EMPTY)),
        })
    }

    fn full_check(&self, t: u32) -> Result<(), ()> {
        let ch = self.cached_head.load(Ordering::Acquire);
        if t >= ch && t - ch < CAP {
            return Ok(());
        }
        let h = self.head.load(Ordering::Acquire);
        self.cached_head.store(h, Ordering::Release);
        if t >= h && t - h < CAP {
            Ok(())
        } else {
            Err(())
        }
    }

    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            if t == 7 {
                return Err(());
            }
            self.full_check(t)?;
            match self
                .tail
                .compare_exchange_weak(t, t + 1, Ordering::AcqRel, Ordering::Relaxed)
            {
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
            h += 1;
        }
        self.head.store(h, Ordering::Release);
    }

    fn late_cas_publish(&self, stale_t: u32, offset: u32) -> bool {
        if stale_t == 7 {
            return false;
        }
        match self
            .tail
            .compare_exchange(stale_t, stale_t + 1, Ordering::AcqRel, Ordering::Relaxed)
        {
            Ok(_) => {
                self.slots[(stale_t as usize) % CAP as usize].store(offset, Ordering::Release);
                true
            }
            Err(_) => false,
        }
    }
}

// =========================================================================
// Counterfactual 1 — the narrow (former-shape) ring's capacity invariant
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
/// A then resumes and CASes against its stale `t0` — on the former
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
             that it does on the former protocol shape)"
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
// Test 3 — the fixed non-wrapping protocol rejects the stale CAS.
// =========================================================================

/// The identical finite script cannot reincarnate a cursor in the fixed
/// protocol; the stale CAS fails and a retry would recheck capacity.
#[test]
fn fixed_tail_stale_cas_rejects_after_same_finite_script() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = FixedRing::new();
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
            "sanity: a non-wrapping cursor must NOT wrap back to the stale \
             snapshot after this same finite amount of activity (real tail: {t_before})"
        );

        // The fix: A's stale CAS now correctly fails.
        let succeeded = ring.late_cas_publish(t0, 999);
        assert!(
            !succeeded,
            "the fix: a non-wrapping cursor's stale CAS must fail by compare-mismatch"
        );

        // No corruption: both live entries still drain cleanly, in order.
        let mut got = Vec::new();
        ring.drain(|off| got.push(off));
        assert_eq!(
            got,
            vec![557, 558],
            "fixed ring must still drain cleanly after the correctly-rejected stale CAS"
        );
    });
}

/// A paused producer spans the entire supported reduced-width lifetime.
/// Even at exhaustion the old compare value is never reincarnated.
#[test]
fn fixed_protocol_paused_producer_cannot_reserve_after_exhaustion() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = FixedRing::new();
        let (ready_tx, ready_rx) = loom::sync::mpsc::channel();
        let (resume_tx, resume_rx) = loom::sync::mpsc::channel();
        let stalled = Arc::clone(&ring);
        let producer = thread::spawn(move || {
            let t0 = stalled.tail.load(Ordering::Relaxed);
            assert!(stalled.full_check(t0).is_ok());
            ready_tx.send(t0).unwrap();
            resume_rx.recv().unwrap();
            stalled.late_cas_publish(t0, 999)
        });
        let t0 = ready_rx.recv().unwrap();
        assert_eq!(t0, 0);
        let mut ledger = Vec::new();
        for off in 1..=7 {
            assert!(ring.push(off).is_ok());
            ring.drain(|entry| ledger.push(entry));
        }
        assert_eq!(ledger, vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(ring.tail.load(Ordering::Acquire), 7);
        assert_eq!(ring.head.load(Ordering::Acquire), 7);
        assert!(ring.push(8).is_err());
        resume_tx.send(()).unwrap();
        assert!(!producer.join().unwrap());
        assert!(
            ring.push(8).is_err(),
            "draining cannot rebase an exhausted cursor"
        );
        assert_eq!(ring.tail.load(Ordering::Acquire), 7);
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
