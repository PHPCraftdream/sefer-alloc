//! loom model-check of the **`HeapOverflow` drain-guard protocol** — the
//! `is_likely_empty`-gated skip that lets `HeapCore::drain_heap_overflow`
//! avoid the full Acquire-pair drain when the owner's cached `tail` matches
//! the ring's current `tail`. This is the R2-4 finding's exact surface: the
//! guard's correctness depends on `HeapOverflow::drain` returning its ACTUAL
//! stop position (`head`), not the entry-time `tail` snapshot.
//!
//! # Scope — what this adds beyond `loom_heap_overflow.rs`
//!
//! `loom_heap_overflow.rs` model-checks the two-field publish-order protocol
//! (publish `packed` then `base`, drain reads `base` first as the gate) — it
//! does NOT model the guard/cache interaction at all (its `drain` returns
//! unit, no `is_likely_empty`, no caller cache). R2-4 is a bug in exactly that
//! unmodelled interaction: the return value of `drain` feeds the caller's
//! `overflow_tail_cache`, and `is_likely_empty(cache)` gates whether the next
//! drain runs. This file fills that gap, mirroring
//! `loom_remote_ring_drain_guard.rs`'s structure (the analogous — and
//! NON-buggy — `RemoteFreeRing` guard) adapted to `HeapOverflow`'s two-field
//! entry and `tail`-caching idiom.
//!
//! # The bug, restated for the model
//!
//! `drain` reserves `t = tail.load(Acquire)` at entry, drains until it hits an
//! unpublished slot at position `h` (`h < t`), publishes `h` to `self.head`,
//! and returns. The CORRECT return is `h`; the R2-4 bug returned `t`. The
//! caller caches the return; `is_likely_empty(cached)` is `tail.load(Relaxed)
//! == cached`. A producer that reserved the gap slot does NOT move `tail`
//! again when it publishes (the CAS already happened) — so a cache poisoned to
//! `t` equals the still-current `tail`, and every later guard check skips the
//! re-drain that must observe the now-published entry: a stuck free.
//!
//! loom explores the interleaving where a `drain` runs in the producer's
//! reserve→publish gap (between the tail CAS and the `base` store — separate
//! atomics, so loom interleaves between them). With the correct `return h`
//! the entry is always eventually reclaimed; with the buggy `return t` it is
//! stuck (never reclaimed across the whole guard sequence) in that
//! interleaving.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global,alloc-xthread --test loom_heap_overflow_drain_guard
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

/// The loom model of `HeapOverflow`'s metadata: `head`/`tail` cursors plus
/// the two parallel slot arrays (`bases`, `packed`). Mirrors `OverflowModel`
/// in `loom_heap_overflow.rs`.
struct OverflowModel {
    head: AtomicUsize,
    tail: AtomicUsize,
    bases: [AtomicUsize; CAP],
    packed: [AtomicU32; CAP],
}

impl OverflowModel {
    fn new() -> Self {
        OverflowModel {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            bases: std::array::from_fn(|_| AtomicUsize::new(ENTRY_EMPTY_BASE)),
            packed: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    /// The CORRECT push (mirrors `HeapOverflow::push`): full-check (Acquire
    /// head), CAS-reserve the tail (AcqRel), publish `packed` (Relaxed) THEN
    /// `base` (Release). Crucially the CAS and the `base` store are SEPARATE
    /// atomics, so loom can interleave a concurrent `drain` between them —
    /// exactly the R2-4 reserve→publish gap.
    fn push(&self, base: usize, packed: u32) -> Result<(), ()> {
        debug_assert_ne!(base, ENTRY_EMPTY_BASE);
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP {
                return Err(()); // full
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

    /// The CORRECT drain (mirrors the FIXED `HeapOverflow::drain`): returns
    /// `h`, the actual stop position published to `head` — NOT the entry-time
    /// `tail` snapshot.
    fn drain<F: FnMut(usize, u32)>(&self, mut reclaim: F) -> usize {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let idx = h % CAP;
            let base = self.bases[idx].load(Ordering::Acquire);
            if base == ENTRY_EMPTY_BASE {
                break; // reserved but not yet published — stop
            }
            let packed = self.packed[idx].load(Ordering::Relaxed);
            reclaim(base, packed);
            self.bases[idx].store(ENTRY_EMPTY_BASE, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
        h
    }

    /// COUNTERFACTUAL drain — the R2-4 bug: returns the entry-time `tail`
    /// snapshot `t` instead of the actual stop position `h`. Used ONLY by the
    /// `#[should_panic]` counterfactual test below to demonstrate that
    /// `return t` sticks the entry under the guard.
    fn drain_buggy_return_tail<F: FnMut(usize, u32)>(&self, mut reclaim: F) -> usize {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let idx = h % CAP;
            let base = self.bases[idx].load(Ordering::Acquire);
            if base == ENTRY_EMPTY_BASE {
                break;
            }
            let packed = self.packed[idx].load(Ordering::Relaxed);
            reclaim(base, packed);
            self.bases[idx].store(ENTRY_EMPTY_BASE, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
        t // BUG (R2-4): stale tail snapshot, not the stop position.
    }

    /// Mirrors `HeapOverflow::is_likely_empty`: a Relaxed `tail` load
    /// compared against the caller-cached value.
    fn is_likely_empty(&self, cached_tail: usize) -> bool {
        self.tail.load(Ordering::Relaxed) == cached_tail
    }
}

/// The owner side of the model: a ring plus its owner-PRIVATE cached `tail`
/// (mirrors `HeapCore::overflow_tail_cache`, a plain — non-atomic — field
/// touched ONLY by the single consumer thread, never by a producer).
struct Owner {
    ring: OverflowModel,
    cached_tail: std::cell::Cell<usize>,
}

// SAFETY (test-only): `Cell` is `!Sync`, but this model's contract is EXACTLY
// "at most one thread ever touches `cached_tail`" — the same single-consumer
// invariant the production `overflow_tail_cache` field relies on. Every test
// below spawns exactly one consumer thread that calls `guarded_drain`; the
// loom `Arc` needs `Owner: Sync` to be shared, so we assert the invariant
// here rather than switch to an atomic (which would over-model the real
// field's ordering).
unsafe impl Sync for Owner {}

impl Owner {
    fn new() -> Arc<Self> {
        Arc::new(Owner {
            ring: OverflowModel::new(),
            cached_tail: std::cell::Cell::new(0),
        })
    }

    /// The GUARDED drain (mirrors `HeapCore::drain_heap_overflow` with the
    /// FIXED `drain`): skip the real drain when `is_likely_empty(cached)`;
    /// otherwise drain for real and refresh the cache from the drain's own
    /// return value (the actual stop position).
    fn guarded_drain(&self) -> Vec<(usize, u32)> {
        if self.ring.is_likely_empty(self.cached_tail.get()) {
            return Vec::new();
        }
        let mut got = Vec::new();
        let new_cache = self.ring.drain(|b, p| got.push((b, p)));
        self.cached_tail.set(new_cache);
        got
    }

    /// COUNTERFACTUAL guarded drain: same as `guarded_drain` but refreshes the
    /// cache from the BUGGY `drain_buggy_return_tail` (returns `t`). Used only
    /// by the `#[should_panic]` test.
    fn guarded_drain_buggy(&self) -> Vec<(usize, u32)> {
        if self.ring.is_likely_empty(self.cached_tail.get()) {
            return Vec::new();
        }
        let mut got = Vec::new();
        let new_cache = self.ring.drain_buggy_return_tail(|b, p| got.push((b, p)));
        self.cached_tail.set(new_cache);
        got
    }
}

// =========================================================================
// Correct protocol — the entry is always eventually reclaimed despite the
// drain racing the producer's reserve→publish gap.
// =========================================================================

/// A producer pushes ONE entry while the consumer runs several `guarded_drain`
/// rounds concurrently. loom explores every interleaving — including the one
/// where the consumer's `drain` runs in the producer's reserve→publish gap
/// (between the tail CAS and the `base` store), observes the slot as
/// reserved-but-not-yet-published, and stops at `h < t`. INVARIANT: by the
/// time both threads have joined and one FINAL `guarded_drain` has run, the
/// entry has been reclaimed EXACTLY once. With the FIXED `return h`, the
/// cache stays strictly below `tail` while the reservation is unpublished, so
/// the guard keeps re-draining until the publish lands; with the R2-4
/// `return t`, the cache would equal the still-current `tail` and the guard
/// would skip forever (the counterfactual below proves that).
#[test]
fn guard_reclaims_entry_despite_unpublished_slot_gap() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();
        let reclaimed_total = Arc::new(AtomicUsize::new(0));

        let owner_p = Arc::clone(&owner);
        let producer = thread::spawn(move || while owner_p.ring.push(0x1000, 111).is_err() {});

        let owner_c = Arc::clone(&owner);
        let reclaimed_c = Arc::clone(&reclaimed_total);
        let consumer = thread::spawn(move || {
            for _ in 0..2 {
                let got = owner_c.guarded_drain();
                reclaimed_c.fetch_add(got.len(), Ordering::Relaxed);
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        // FINAL round after both threads joined: the push is now globally
        // visible (join is a full happens-before edge in loom), so a correct
        // guard observes `tail` has moved past whatever `cached_tail` the
        // racing rounds left behind and reclaims the entry if the racing
        // rounds hadn't already.
        let got = owner.guarded_drain();
        reclaimed_total.fetch_add(got.len(), Ordering::Relaxed);

        let total = reclaimed_total.load(Ordering::Relaxed);
        assert_eq!(
            total, 1,
            "the pushed entry must be reclaimed exactly once across the \
             guarded-drain sequence — returning the actual stop position h \
             keeps the cache below tail while a reservation is unpublished, \
             so the guard re-drains until the publish lands"
        );
    });
}

/// Empty-ring guarded drain is a no-op (the guard skips, nothing reclaimed).
#[test]
fn guarded_drain_empty_overflow_is_noop() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();
        let got = owner.guarded_drain();
        assert!(
            got.is_empty(),
            "guarded drain of empty ring reclaimed something"
        );
    });
}

// =========================================================================
// Counterfactual — the R2-4 `return t` sticks the entry.
// =========================================================================

/// COUNTERFACTUAL: the pre-fix drain that returns the stale tail snapshot `t`
/// instead of the actual stop position `h`. `#[should_panic]` because loom
/// finds the interleaving where the consumer's `drain` runs in the producer's
/// reserve→publish gap, caches `t` (== the still-current `tail`), and every
/// subsequent guard check then skips — the entry is never reclaimed across the
/// whole sequence (total == 0, not 1), failing the assertion. If this test
/// ever passes (no panic), the counterfactual is vacuous and no longer proves
/// the `return h` fix matters.
#[test]
#[should_panic(expected = "stuck")]
fn counterfactual_return_tail_sticks_the_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();
        let reclaimed_total = Arc::new(AtomicUsize::new(0));

        let owner_p = Arc::clone(&owner);
        let producer = thread::spawn(move || while owner_p.ring.push(0x1000, 111).is_err() {});

        let owner_c = Arc::clone(&owner);
        let reclaimed_c = Arc::clone(&reclaimed_total);
        let consumer = thread::spawn(move || {
            for _ in 0..2 {
                let got = owner_c.guarded_drain_buggy();
                reclaimed_c.fetch_add(got.len(), Ordering::Relaxed);
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        let got = owner.guarded_drain_buggy();
        reclaimed_total.fetch_add(got.len(), Ordering::Relaxed);

        let total = reclaimed_total.load(Ordering::Relaxed);
        assert_eq!(
            total, 1,
            "stuck: the buggy return-t poisoned the cache to equal tail, so \
             the guard skipped every re-drain and the entry was never \
             reclaimed (R2-4)"
        );
    });
}
