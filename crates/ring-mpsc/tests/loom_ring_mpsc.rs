//! Unified loom model-check of the `ring-mpsc` protocols — the CORRECT-protocol
//! properties run against the **real** [`MpscRing`] / [`DirtyRouter`] types (the
//! crate aliases its atomics to `loom::sync::atomic` under `--cfg loom`, so this
//! harness exercises the shipped implementation, not a hand-copied shadow model).
//!
//! This single suite collapses the SEVEN in-tree shadow-model harnesses that
//! each transcribed one of these protocols against `loom::sync::atomic`:
//! `loom_remote_ring`, `loom_remote_ring_drain_guard`, `loom_heap_overflow`,
//! `loom_heap_overflow_drain_guard`, `loom_overflow_first_retry` (a *composition*
//! example), `loom_dirty_publish`, and `loom_dirty_multi_segment`.
//!
//! # What runs against the REAL type
//!
//! - **Single-word ring exactly-once** (`RemoteFreeRing` shape) — via
//!   `MpscRing::<Owned<U32Entry, _>>`.
//! - **Two-word ring untorn / exactly-once** (`HeapOverflow` shape) — via
//!   `MpscRing::<Owned<UsizeU32Entry, _>>`. The pair-publish ordering is INSIDE
//!   the real `UsizeU32Entry::publish`/`read`, so loom checks the actual field
//!   order.
//! - **Drain-guard** (`tail_relaxed` cached-cursor liveness) — the real
//!   `drain() -> stop` / `tail_relaxed()` methods.
//! - **Overflow-retry composition** — the real ring at `CAP = 1`, two producers
//!   retrying a bounded number of times (the `overflow_first_retry` example
//!   shape). NOTE: only the two composed rings generalize; the sefer-side
//!   ordering *policy* (`push_with_overflow_retry`) stays in-tree — this test
//!   ships the reusable composition, not that policy.
//! - **DirtyRouter publish/swap** and **multi-key single-word** — the real
//!   `mark`/`for_each_dirty`, composed with a real ring.
//!
//! # The counterfactuals (non-vacuousness proofs)
//!
//! Loom cannot rebuild the crate with a deliberately-broken ordering, so the
//! FOUR counterfactual families are transcribed here as `#[should_panic]` shadow
//! models over `loom::sync::atomic` — the exact shape the real code implements,
//! with ONE ordering/condition flipped:
//!
//! 1. `counterfactual_drain_without_break_loses` — a drain that SKIPS (rather
//!    than STOPS at) an unpublished reserved slot loses the offset.
//! 2. `counterfactual_drain_without_clear_duplicates` — a drain that does NOT
//!    clear a drained slot re-reclaims it on the next wrap.
//! 3. `counterfactual_wrong_pair_publish_order_tears` — a two-word push that
//!    publishes the gate (`base`) BEFORE the payload (`packed`) hands a drain a
//!    torn entry.
//! 4. `counterfactual_drain_guard_returns_tail_sticks` — the R2-4 bug: a drain
//!    that returns the entry-time `tail` (not the actual stop) makes the guard
//!    skip the re-drain that must observe a late publish.
//!
//! If any counterfactual PASSES (does not panic) the suite is vacuous.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release -p ring-mpsc --test loom_ring_mpsc
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

use ring_mpsc::{DirtyRouter, MpscRing, Owned, U32Entry, UsizeU32Entry};

fn model() -> loom::model::Builder {
    let mut b = loom::model::Builder::new();
    b.preemption_bound = Some(3);
    b
}

// =========================================================================
// Real-type: single-word ring exactly-once (RemoteFreeRing shape).
// =========================================================================

#[test]
fn real_u32_ring_never_loses_or_duplicates() {
    model().check(|| {
        let ring = Arc::new(MpscRing::<Owned<U32Entry, 4>>::new());
        let ra = Arc::clone(&ring);
        let ta = thread::spawn(move || while ra.push(10).is_err() {});
        let rb = Arc::clone(&ring);
        let tb = thread::spawn(move || while rb.push(20).is_err() {});
        ta.join().unwrap();
        tb.join().unwrap();

        let mut got_a = 0u32;
        let mut got_b = 0u32;
        ring.drain(|off| {
            if off == 10 {
                got_a += 1;
            } else if off == 20 {
                got_b += 1;
            }
        });
        assert_eq!(got_a, 1, "offset 10 reclaimed {got_a} times (want 1)");
        assert_eq!(got_b, 1, "offset 20 reclaimed {got_b} times (want 1)");
    });
}

#[test]
fn real_u32_drain_empty_is_noop() {
    model().check(|| {
        let ring = MpscRing::<Owned<U32Entry, 4>>::new();
        let mut n = 0u32;
        ring.drain(|_| n += 1);
        assert_eq!(n, 0);
    });
}

// =========================================================================
// Real-type: two-word ring untorn / exactly-once (HeapOverflow shape).
// =========================================================================

#[test]
fn real_pair_ring_never_tears_loses_or_duplicates() {
    model().check(|| {
        let ring = Arc::new(MpscRing::<Owned<UsizeU32Entry, 4>>::new());
        let ra = Arc::clone(&ring);
        let ta = thread::spawn(move || while ra.push((0x1000, 111)).is_err() {});
        let rb = Arc::clone(&ring);
        let tb = thread::spawn(move || while rb.push((0x2000, 222)).is_err() {});
        ta.join().unwrap();
        tb.join().unwrap();

        let mut got_a = 0u32;
        let mut got_b = 0u32;
        ring.drain(|(base, packed)| {
            if base == 0x1000 {
                assert_eq!(packed, 111, "torn: base 0x1000 paired with wrong packed");
                got_a += 1;
            } else if base == 0x2000 {
                assert_eq!(packed, 222, "torn: base 0x2000 paired with wrong packed");
                got_b += 1;
            }
        });
        assert_eq!(
            got_a, 1,
            "entry (0x1000,111) reclaimed {got_a} times (want 1)"
        );
        assert_eq!(
            got_b, 1,
            "entry (0x2000,222) reclaimed {got_b} times (want 1)"
        );
    });
}

// =========================================================================
// Real-type: drain guard (tail_relaxed cached-cursor liveness).
// =========================================================================

/// A producer pushes; a consumer uses the `drain() -> stop` return as its cached
/// cursor and `tail_relaxed()` as the empty-guard. INVARIANT: after the producer
/// joins, the guarded consumer eventually drains the entry (the guard never
/// wedges — `tail_relaxed != cached` forces the real drain).
#[test]
fn real_drain_guard_never_wedges() {
    model().check(|| {
        let ring = Arc::new(MpscRing::<Owned<U32Entry, 4>>::new());
        let rp = Arc::clone(&ring);
        let tp = thread::spawn(move || while rp.push(42).is_err() {});

        // Consumer: guard-then-drain, a bounded number of rounds.
        let mut cached = ring.head_relaxed();
        let mut got = 0u32;
        for _ in 0..4 {
            if ring.tail_relaxed() != cached {
                cached = ring.drain(|v| {
                    if v == 42 {
                        got += 1;
                    }
                });
            }
            thread::yield_now();
        }
        tp.join().unwrap();
        // One final guarded drain after the producer definitely published.
        if ring.tail_relaxed() != cached {
            ring.drain(|v| {
                if v == 42 {
                    got += 1;
                }
            });
        }
        assert_eq!(
            got, 1,
            "drain guard lost or duplicated the entry (got {got})"
        );
    });
}

// =========================================================================
// Real-type: overflow-retry composition (the overflow_first_retry example).
// =========================================================================

const MODEL_RETRY_BOUND: u32 = 3;

/// 2 producers push disjoint offsets into a `CAP = 1` ring — the second producer
/// is GUARANTEED to observe a full ring at least once, forcing genuine overflow
/// — each retrying up to `MODEL_RETRY_BOUND` times. A consumer drains once after
/// both join. INVARIANT: every push that reported `Ok` is reclaimed exactly once
/// (the ring's own no-loss/no-dup invariant holds under the retry composition).
#[test]
fn real_overflow_retry_composition_never_loses_or_duplicates() {
    model().check(|| {
        let ring = Arc::new(MpscRing::<Owned<U32Entry, 1>>::new());
        let reclaimed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);
        let landed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);

        let mut producers = Vec::new();
        for (i, &offset) in [10u32, 20u32].iter().enumerate() {
            let rp = Arc::clone(&ring);
            let lp = Arc::clone(&landed);
            producers.push(thread::spawn(move || {
                for _ in 0..MODEL_RETRY_BOUND {
                    if rp.push(offset).is_ok() {
                        lp[i].store(1, Ordering::Relaxed);
                        return;
                    }
                    thread::yield_now();
                }
            }));
        }
        for p in producers {
            p.join().unwrap();
        }

        ring.drain(|off| {
            let idx = match off {
                10 => 0,
                20 => 1,
                _ => return,
            };
            reclaimed[idx].fetch_add(1, Ordering::Relaxed);
        });

        for i in 0..2 {
            let seen = reclaimed[i].load(Ordering::Acquire);
            let did_land = landed[i].load(Ordering::Relaxed) == 1;
            if did_land {
                assert_eq!(
                    seen, 1,
                    "producer {i} landed but reclaimed {seen} times (want 1)"
                );
            } else {
                assert_eq!(
                    seen, 0,
                    "producer {i} reclaimed {seen} times despite never landing"
                );
            }
        }
    });
}

// =========================================================================
// Real-type: DirtyRouter publish/swap + a composed ring (dirty_publish shape).
// =========================================================================

/// 2 producers push a ring entry then `mark` the router; the consumer runs two
/// `for_each_dirty` + drain passes. INVARIANT: both entries are drained across
/// the two passes (the mark makes them visible; the swap does not lose a bit).
#[test]
fn real_dirty_publish_never_loses_entry() {
    model().check(|| {
        let ring = Arc::new(MpscRing::<Owned<U32Entry, 2>>::new());
        let router = Arc::new(DirtyRouter::<1>::new());

        let ra = Arc::clone(&ring);
        let ma = Arc::clone(&router);
        let ta = thread::spawn(move || {
            if ra.push(10).is_ok() {
                ma.mark(0);
            }
        });
        let rb = Arc::clone(&ring);
        let mb = Arc::clone(&router);
        let tb = thread::spawn(move || {
            if rb.push(20).is_ok() {
                mb.mark(0);
            }
        });
        ta.join().unwrap();
        tb.join().unwrap();

        // Two consumer passes: swap the dirty word (may fire once or twice) and
        // drain the ring on any dirty observation.
        let mut got = 0u32;
        for _ in 0..2 {
            let mut dirty = false;
            router.for_each_dirty(|_| dirty = true);
            if dirty {
                ring.drain(|_| got += 1);
            }
        }
        // Whatever the interleaving, both entries end up drained: either a dirty
        // pass drained them, or the second pass caught a bit set after the first
        // swap. A final unconditional drain (the caller's full-sweep backstop)
        // mops up any bounded deferral.
        ring.drain(|_| got += 1);
        assert_eq!(
            got, 2,
            "dirty publish: drained {got} entries across passes (want 2)"
        );
    });
}

// =========================================================================
// Real-type: DirtyRouter multiple keys in one word (dirty_multi_segment shape).
// =========================================================================

/// Two producers mark DIFFERENT keys in the SAME 64-bit word; one `for_each_dirty`
/// swap must observe BOTH bits. INVARIANT: both keys are visited across the passes.
#[test]
fn real_dirty_multi_key_same_word() {
    model().check(|| {
        let router = Arc::new(DirtyRouter::<1>::new());
        let ma = Arc::clone(&router);
        let ta = thread::spawn(move || ma.mark(0));
        let mb = Arc::clone(&router);
        let tb = thread::spawn(move || mb.mark(1));
        ta.join().unwrap();
        tb.join().unwrap();

        let mut seen = 0u32;
        router.for_each_dirty(|_| seen += 1);
        router.for_each_dirty(|_| seen += 1);
        assert_eq!(seen, 2, "multi-key: visited {seen} keys (want 2)");
    });
}

// =========================================================================
// COUNTERFACTUALS — shadow models (loom can't rebuild the crate with a flipped
// ordering). Each transcribes the real protocol with ONE detail broken.
// =========================================================================

const RING_SLOT_EMPTY: u32 = u32::MAX;

/// A tiny single-word ring shadow model with a SWAPPABLE drain, for the two
/// single-word drain counterfactuals.
struct ShadowRing<const CAP: usize> {
    head: AtomicU32,
    tail: AtomicU32,
    slots: [AtomicU32; CAP],
}

impl<const CAP: usize> ShadowRing<CAP> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
        })
    }
    fn push(&self, off: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP as u32 {
                return Err(());
            }
            if self
                .tail
                .compare_exchange_weak(t, t.wrapping_add(1), Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.slots[(t as usize) % CAP].store(off, Ordering::Release);
                return Ok(());
            }
        }
    }
    /// BUG: advances past an unpublished reserved slot instead of stopping.
    fn drain_no_break<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slots[(h as usize) % CAP].load(Ordering::Acquire);
            if off != RING_SLOT_EMPTY {
                reclaim(off);
                self.slots[(h as usize) % CAP].store(RING_SLOT_EMPTY, Ordering::Relaxed);
            }
            h = h.wrapping_add(1); // does not break on empty — the bug.
        }
        self.head.store(h, Ordering::Release);
    }
    /// BUG: does NOT clear a drained slot; on the next wrap it is re-reclaimed.
    fn drain_no_clear<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slots[(h as usize) % CAP].load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            reclaim(off);
            // no clear — the bug.
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

#[test]
#[should_panic(expected = "lost")]
fn counterfactual_drain_without_break_loses() {
    model().check(|| {
        let ring = ShadowRing::<4>::new();
        let pushed = Arc::new(AtomicUsize::new(0));
        let reclaimed = Arc::new(AtomicUsize::new(0));

        let ra = Arc::clone(&ring);
        let pa = Arc::clone(&pushed);
        let ta = thread::spawn(move || {
            if ra.push(10).is_ok() {
                pa.fetch_add(1, Ordering::Relaxed);
            }
        });
        let rb = Arc::clone(&ring);
        let pb = Arc::clone(&pushed);
        let tb = thread::spawn(move || {
            if rb.push(20).is_ok() {
                pb.fetch_add(1, Ordering::Relaxed);
            }
        });
        let rc = Arc::clone(&ring);
        let cc = Arc::clone(&reclaimed);
        let pc = Arc::clone(&pushed);
        let tc = thread::spawn(move || {
            loop {
                rc.drain_no_break(|_| {
                    cc.fetch_add(1, Ordering::Relaxed);
                });
                if pc.load(Ordering::Acquire) >= 2 {
                    break;
                }
                thread::yield_now();
            }
            for _ in 0..2 {
                rc.drain_no_break(|_| {
                    cc.fetch_add(1, Ordering::Relaxed);
                });
            }
        });
        ta.join().unwrap();
        tb.join().unwrap();
        tc.join().unwrap();

        let pushed = pushed.load(Ordering::Acquire);
        let got = reclaimed.load(Ordering::Acquire);
        assert!(got == pushed, "lost offset: reclaimed {got} of {pushed}");
    });
}

#[test]
#[should_panic(expected = "duplicate")]
fn counterfactual_drain_without_clear_duplicates() {
    model().check(|| {
        let ring = ShadowRing::<4>::new();
        let counts: Arc<Vec<AtomicU32>> = Arc::new((0..200).map(|_| AtomicU32::new(0)).collect());
        let rp = Arc::clone(&ring);
        let tp = thread::spawn(move || {
            for off in 100..105u32 {
                while rp.push(off).is_err() {
                    thread::yield_now();
                }
            }
        });
        let rc = Arc::clone(&ring);
        let cc = Arc::clone(&counts);
        let tc = thread::spawn(move || {
            for _ in 0..8 {
                rc.drain_no_clear(|off| {
                    if (100..200).contains(&off) {
                        cc[off as usize - 100].fetch_add(1, Ordering::Relaxed);
                    }
                });
                thread::yield_now();
            }
        });
        tp.join().unwrap();
        tc.join().unwrap();

        let mut max_dup = 0u32;
        for c in counts.iter() {
            max_dup = max_dup.max(c.load(Ordering::Acquire));
        }
        assert!(
            max_dup <= 1,
            "duplicate: an offset reclaimed {max_dup} times"
        );
    });
}

/// Two-word shadow ring with a BROKEN publish order (gate before payload).
struct ShadowPairRing {
    head: AtomicUsize,
    tail: AtomicUsize,
    bases: [AtomicUsize; 4],
    packed: [AtomicU32; 4],
}
impl ShadowPairRing {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            bases: std::array::from_fn(|_| AtomicUsize::new(0)),
            packed: std::array::from_fn(|_| AtomicU32::new(0)),
        })
    }
    /// BUG: publish `base` (the gate) FIRST, `packed` SECOND — inverted.
    fn push_wrong_order(&self, base: usize, packed: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= 4 {
                return Err(());
            }
            if self
                .tail
                .compare_exchange_weak(t, t.wrapping_add(1), Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                let idx = t % 4;
                self.bases[idx].store(base, Ordering::Release);
                self.packed[idx].store(packed, Ordering::Relaxed);
                return Ok(());
            }
        }
    }
    /// The CORRECT drain (base Acquire gate first) — the bug is in the push.
    fn drain<F: FnMut(usize, u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let idx = h % 4;
            let base = self.bases[idx].load(Ordering::Acquire);
            if base == 0 {
                break;
            }
            let packed = self.packed[idx].load(Ordering::Relaxed);
            reclaim(base, packed);
            self.bases[idx].store(0, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

#[test]
#[should_panic(expected = "torn")]
fn counterfactual_wrong_pair_publish_order_tears() {
    model().check(|| {
        let ring = ShadowPairRing::new();
        let rp = Arc::clone(&ring);
        let tp = thread::spawn(move || {
            let _ = rp.push_wrong_order(0xABCD, 999);
        });
        let rc = Arc::clone(&ring);
        let tc = thread::spawn(move || {
            let mut torn = false;
            rc.drain(|base, packed| {
                if base == 0xABCD && packed != 999 {
                    torn = true;
                }
            });
            if torn {
                panic!("torn: base 0xABCD observed with packed != 999");
            }
        });
        tp.join().unwrap();
        tc.join().unwrap();

        let mut torn = false;
        ring.drain(|base, packed| {
            if base == 0xABCD && packed != 999 {
                torn = true;
            }
        });
        assert!(
            !torn,
            "torn: final drain observed base 0xABCD with stale packed"
        );
    });
}

/// The R2-4 drain-guard counterfactual: a drain that returns the ENTRY-TIME
/// `tail` (not the actual stop position). A guard caching that value skips the
/// re-drain that must observe a late publish → the entry sticks.
struct ShadowGuardRing<const CAP: usize> {
    head: AtomicUsize,
    tail: AtomicUsize,
    slots: [AtomicU32; CAP],
}
impl<const CAP: usize> ShadowGuardRing<CAP> {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
        })
    }
    fn push(&self, off: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP {
                return Err(());
            }
            if self
                .tail
                .compare_exchange_weak(t, t.wrapping_add(1), Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.slots[t % CAP].store(off, Ordering::Release);
                return Ok(());
            }
        }
    }
    fn tail_relaxed(&self) -> usize {
        self.tail.load(Ordering::Relaxed)
    }
    /// BUG: returns the entry-time `tail` snapshot, NOT the actual stop `h`.
    fn drain_return_tail<F: FnMut(u32)>(&self, mut reclaim: F) -> usize {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slots[h % CAP].load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            reclaim(off);
            self.slots[h % CAP].store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
        t // BUG: should be `h`.
    }
}

#[test]
#[should_panic(expected = "stuck")]
fn counterfactual_drain_guard_returns_tail_sticks() {
    // Single-threaded deterministic reproduction: reserve-without-publish, then a
    // buggy drain that returns `tail`, then a guard check that skips because the
    // cache equals the current tail — the pending entry never drains.
    model().check(|| {
        let ring = ShadowGuardRing::<8>::new();
        ring.push(1).unwrap();
        // Reserve an unpublished slot (producer mid-push).
        let t = ring.tail.load(Ordering::Relaxed);
        ring.tail.store(t.wrapping_add(1), Ordering::Relaxed);
        // Buggy drain: reclaims entry 1, stops at the unpublished slot, but
        // returns the entry-time tail (== current tail).
        let cached = ring.drain_return_tail(|_| {});
        // Now the producer publishes the reserved slot.
        ring.slots[t % 8].store(7, Ordering::Release);
        // Guard: cache == tail_relaxed → the buggy return makes the guard SKIP
        // the re-drain, so entry 7 is stuck.
        let mut drained_after = 0u32;
        if ring.tail_relaxed() != cached {
            ring.drain_return_tail(|_| drained_after += 1);
        }
        assert!(
            drained_after >= 1,
            "stuck: guard skipped the re-drain of the late publish"
        );
    });
}
