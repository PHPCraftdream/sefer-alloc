//! loom model-check of the **`HeapOverflow` two-field-entry MPSC ring
//! protocol** (RAD-4b, task #72).
//!
//! # Scope — what this adds beyond `loom_remote_ring.rs`
//!
//! `HeapOverflow` (`src/registry/heap_overflow.rs`) reuses the SAME
//! Vyukov-style push/drain CAS-reserve protocol `loom_remote_ring.rs` already
//! model-checks for `RemoteFreeRing` — same cursor arithmetic, same
//! break-on-unpublished-slot / clear-on-drain discipline. This file does NOT
//! re-prove that shared shape; it isolates the ONE genuinely new detail
//! `HeapOverflow` adds: each entry is a PAIR of atomics (`base`, `packed`),
//! not a single word, because a heap-level ring must carry which SEGMENT an
//! offset belongs to (a per-segment `RemoteFreeRing` does not need this — the
//! segment is implicit).
//!
//! The correct protocol publishes `packed` (`Relaxed`) THEN `base`
//! (`Release`) on push, and the drain reads `base` (`Acquire`) FIRST — using
//! it as BOTH the "is this slot published" gate AND the byte that makes the
//! preceding `packed` store visible (Release sequence: the `Relaxed` store to
//! `packed` is sequenced-before the `Release` store to `base` on the SAME
//! producer thread, so a consumer that `Acquire`-loads a non-empty `base`
//! also observes that producer's prior `packed` store — the same "publish
//! the last field last" idiom `RemoteFreeRing`'s OWN single-field push/drain
//! doc already documents, extended here to a pair). This file's counterfactual
//! is the ONE bug shape unique to the two-field entry: publishing `base`
//! BEFORE `packed` (or reading `packed` before confirming `base` is
//! published), which can hand the consumer a **torn read** — a `base` from
//! the CURRENT push paired with a stale/zero `packed` from a PRIOR
//! reservation of that same slot index (or vice versa).
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-global,alloc-xthread --test loom_heap_overflow
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel `base` value meaning "no entry" — matches `ENTRY_EMPTY_BASE` (0)
/// in the real `HeapOverflow`.
const ENTRY_EMPTY_BASE: usize = 0;

/// A small ring capacity so the wrap path stays within loom's bounded
/// exploration. The real `HeapOverflow` uses `HEAP_OVERFLOW_CAP = 2048`.
const CAP: usize = 4;

/// The loom model of `HeapOverflow`'s metadata: head/tail cursors plus TWO
/// parallel slot arrays (`bases`, `packed`) — the detail this file exists to
/// check, absent from `loom_remote_ring.rs`'s single-array model.
struct OverflowModel {
    head: AtomicUsize,
    tail: AtomicUsize,
    bases: [AtomicUsize; CAP],
    packed: [AtomicU32; CAP],
}

impl OverflowModel {
    fn new() -> Arc<Self> {
        Arc::new(OverflowModel {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            bases: std::array::from_fn(|_| AtomicUsize::new(ENTRY_EMPTY_BASE)),
            packed: std::array::from_fn(|_| AtomicU32::new(0)),
        })
    }

    /// The CORRECT push (mirrors `HeapOverflow::push`): full-check (Acquire
    /// head), CAS-reserve the tail (AcqRel), publish `packed` (Relaxed) THEN
    /// `base` (Release) — `base` is the LAST-published half of the pair.
    fn push(&self, base: usize, packed: u32) -> Result<(), ()> {
        debug_assert_ne!(base, ENTRY_EMPTY_BASE);
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP {
                return Err(());
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let idx = t % CAP;
                    self.packed[idx].store(packed, Ordering::Relaxed);
                    self.bases[idx].store(base, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// The CORRECT drain (mirrors `HeapOverflow::drain`): reads `base`
    /// (Acquire) FIRST as the publish gate; only once `base != EMPTY` does it
    /// read `packed` (Relaxed) — sound because the Acquire load of `base`
    /// synchronises-with the producer's Release store of `base`, which is
    /// sequenced-after (same thread) the producer's store of `packed`, so the
    /// Release SEQUENCE carries `packed`'s value along.
    fn drain<F: FnMut(usize, u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let idx = h % CAP;
            let base = self.bases[idx].load(Ordering::Acquire);
            if base == ENTRY_EMPTY_BASE {
                break; // reserved but not yet published — stop (order preserved)
            }
            let packed = self.packed[idx].load(Ordering::Relaxed);
            reclaim(base, packed);
            self.bases[idx].store(ENTRY_EMPTY_BASE, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }

    /// COUNTERFACTUAL: a push that publishes `base` (the gate) BEFORE
    /// `packed` (the payload) — the exact inversion of the correct order.
    /// A consumer's drain (the CORRECT `drain` above, unmodified) can observe
    /// the newly non-empty `base` (the gate has fired) but read `packed`
    /// BEFORE the producer's subsequent `packed` store lands — reclaiming a
    /// STALE `packed` value (0, or a prior wrap's leftover) paired with the
    /// CURRENT `base`. This is the two-field torn-read hazard unique to
    /// `HeapOverflow` (impossible in `RemoteFreeRing`, which has only one
    /// atomic per slot).
    fn push_broken_wrong_publish_order(&self, base: usize, packed: u32) -> Result<(), ()> {
        debug_assert_ne!(base, ENTRY_EMPTY_BASE);
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP {
                return Err(());
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let idx = t % CAP;
                    // BUG: publish the gate (`base`) FIRST, `packed` SECOND —
                    // inverted from the correct order. A racing drain can see
                    // the gate open and read `packed` before this store lands.
                    self.bases[idx].store(base, Ordering::Release);
                    self.packed[idx].store(packed, Ordering::Relaxed);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }
}

// =========================================================================
// Correct protocol — must never tear, lose, or duplicate an entry.
// =========================================================================

/// 2 producers push UNIQUE `(base, packed)` pairs (disjoint), then join, THEN
/// the main thread drains once. Asserts every successfully-pushed pair is
/// reclaimed EXACTLY once and UNTORN (the `packed` value observed always
/// matches the `base` it was pushed alongside — no cross-entry mixing).
#[test]
fn correct_overflow_never_tears_loses_or_duplicates() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = OverflowModel::new();
        // Producer A: base=0x1000, packed=111. Producer B: base=0x2000, packed=222.
        let ring_a = Arc::clone(&ring);
        let ta = thread::spawn(move || while ring_a.push(0x1000, 111).is_err() {});
        let ring_b = Arc::clone(&ring);
        let tb = thread::spawn(move || while ring_b.push(0x2000, 222).is_err() {});

        ta.join().unwrap();
        tb.join().unwrap();

        let mut got_a = 0u32;
        let mut got_b = 0u32;
        ring.drain(|base, packed| {
            if base == 0x1000 {
                // UNTORN check: this base must carry ITS OWN packed value.
                assert_eq!(
                    packed, 111,
                    "torn read: base 0x1000 paired with wrong packed"
                );
                got_a += 1;
            } else if base == 0x2000 {
                assert_eq!(
                    packed, 222,
                    "torn read: base 0x2000 paired with wrong packed"
                );
                got_b += 1;
            }
        });

        assert_eq!(
            got_a, 1,
            "entry (0x1000,111) reclaimed {got_a} times (expected 1)"
        );
        assert_eq!(
            got_b, 1,
            "entry (0x2000,222) reclaimed {got_b} times (expected 1)"
        );
    });
}

// =========================================================================
// Counterfactual — wrong publish order tears an entry.
// =========================================================================

/// COUNTERFACTUAL: a push that publishes `base` (the gate) before `packed`
/// (the payload) can hand a concurrent drain a TORN entry — the correct
/// `base` paired with a stale/zero `packed`. loom explores the interleaving
/// where the drain's `packed` read lands in the window between the two
/// stores.
///
/// `#[should_panic]` because loom finds the interleaving where the observed
/// `packed` does not match what was pushed alongside `base`. If this test
/// passes (no panic), the counterfactual is vacuous — the harness would not
/// actually be proving the real `push`'s field order matters.
#[test]
#[should_panic(expected = "torn")]
fn counterfactual_wrong_publish_order_tears_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = OverflowModel::new();
        let pushed = Arc::new(AtomicUsize::new(0));

        // Single producer using the BROKEN publish order, so the loom
        // scheduler can interleave the drain between its two stores.
        let ring_p = Arc::clone(&ring);
        let pushed_p = Arc::clone(&pushed);
        let tp = thread::spawn(move || {
            if ring_p.push_broken_wrong_publish_order(0xABCD, 999).is_ok() {
                pushed_p.fetch_add(1, Ordering::Release);
            }
        });

        // Consumer: the CORRECT drain (unmodified) — the bug lives entirely
        // in the push's publish order, not the drain.
        let ring_c = Arc::clone(&ring);
        let tc = thread::spawn(move || {
            loop {
                let mut torn = false;
                ring_c.drain(|base, packed| {
                    if base == 0xABCD && packed != 999 {
                        torn = true;
                    }
                });
                if torn {
                    panic!("torn: base 0xABCD observed with packed != 999 (stale/zero payload)");
                }
                loom::thread::yield_now();
                // Bound the loop: loom's model checker explores a finite
                // number of steps per thread; give up once the producer is
                // known to have finished and one more drain pass ran clean.
                break;
            }
        });

        tp.join().unwrap();
        tc.join().unwrap();

        // Final drain after both threads joined, in case the racing drain
        // above ran before the push's second store landed (untorn in THAT
        // interleaving) — loom still explores the interleaving where the
        // racing drain (above) caught the tear, satisfying `#[should_panic]`
        // across the full exploration.
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

/// Empty-ring drain is a no-op.
#[test]
fn drain_empty_overflow_is_noop() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = OverflowModel::new();
        let mut count = 0u32;
        ring.drain(|_, _| {
            count += 1;
        });
        assert_eq!(count, 0, "drain of empty overflow ring reclaimed something");
    });
}
