//! loom model-check of the **PERF-PASS-4 (G9/C2, task #52) ring-drain empty
//! guard** — the pre-drain check that skips `RemoteFreeRing::drain` (and its
//! unconditional `head.store(_, Release)`) when a Relaxed `tail` load matches
//! the owner's cached `head` value.
//!
//! # Scope — what loom covers
//!
//! Like `loom_remote_ring.rs`, this models the ring's push/drain protocol
//! in isolation using `loom::sync::atomic` (not the real `RemoteFreeRing`).
//! It extends that model with:
//!
//! 1. **`cached_head`**: an owner-PRIVATE (plain, non-atomic — never touched
//!    by any other thread, mirroring `SegmentHeader::ring_drain_head`'s
//!    single-writer discipline) `u32` that the consumer persists across
//!    guarded-drain calls.
//! 2. **`guarded_drain`**: the guard itself — a Relaxed `tail` load compared
//!    against `cached_head`; on a match, skip the real drain (no `head` load,
//!    no `head` store) and return 0 reclaims; on a mismatch, run the real
//!    `drain` and refresh `cached_head` to the drain's final `head`.
//!
//! # Scenario (a) — empty-check-vs-concurrent-push race
//!
//! A producer thread pushes a fresh offset RIGHT as the consumer's
//! `guarded_drain` runs. loom explores every interleaving of the producer's
//! `tail` CAS + slot publish against the consumer's `tail_relaxed` read +
//! (conditionally) `drain`. The invariant under test: **every successfully
//! pushed offset is EVENTUALLY reclaimed** by SOME `guarded_drain` call in a
//! sequence — the guard may cause an individual call to skip (miss this
//! round), but per the module's "later drain picks it up" contract, a
//! SUBSEQUENT call (after the guard observes `tail` has moved) must find and
//! reclaim it. This test drives several guarded-drain rounds after the
//! producer joins so the final round is guaranteed to observe the push.
//!
//! # Scenario (b) — slot re-claim boundary
//!
//! Models the segment-header-resident cache surviving (or, in the buggy
//! counterfactual, wrongly surviving) a "new owner" taking over the same ring
//! memory. In THIS codebase's shard-reuse discipline (see
//! `HeapRegistry::claim`'s module doc / `abandon_segments`'s "whole-heap
//! reuse" note), a slot recycle→claim reuses the SAME `HeapCore` — and hence
//! the SAME live segments/rings — whole; there is no "new owner, old ring"
//! combination. This test proves the model is safe under the ONLY reset that
//! can legitimately occur: a fresh segment (fresh ring memory, cache reset to
//! 0 to match `RemoteFreeRing::init_in_place`'s zeroed cursors — see
//! `SegmentHeader::small`). It drains a ring, resets BOTH the ring cursors
//! AND the cache to 0 (the fresh-segment invariant), then has a "new" producer
//! push again and confirms a fresh `guarded_drain` still finds it — i.e. no
//! stale-cache leakage survives a legitimate reset.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-core,alloc-xthread --test loom_remote_ring_drain_guard
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel slot value meaning "no offset" (not-yet-published or drained).
/// Matches `RING_SLOT_EMPTY` in the real `RemoteFreeRing` (`u32::MAX`).
const RING_SLOT_EMPTY: u32 = u32::MAX;

/// A small ring capacity so the wrap path is exercised within loom's bounded
/// exploration. The real `RemoteFreeRing` uses 256; loom needs a tiny model.
const CAP: usize = 4;

/// The loom model of the ring's metadata: head, tail, and the slot array.
/// Mirrors `RingModel` in `loom_remote_ring.rs`.
struct RingModel {
    head: AtomicU32,
    tail: AtomicU32,
    slots: [AtomicU32; CAP],
}

impl RingModel {
    fn new() -> Self {
        RingModel {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
        }
    }

    /// The CORRECT push (mirrors `RemoteFreeRing::push`).
    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP as u32 {
                return Err(()); // full → overflow
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slots[(t as usize) % CAP].store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// The CORRECT unconditional drain (mirrors `RemoteFreeRing::drain`,
    /// POST task-#52 change — now returns the final `head`).
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) -> u32 {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            reclaim(off);
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
        h
    }

    /// PERF-PASS-4: the pre-drain guard primitive (mirrors
    /// `RemoteFreeRing::tail_relaxed`) — a Relaxed `tail` load ONLY.
    fn tail_relaxed(&self) -> u32 {
        self.tail.load(Ordering::Relaxed)
    }
}

/// The owner side of the model: a ring plus its owner-PRIVATE cached head
/// (mirrors `SegmentHeader::ring_drain_head`, a plain — non-atomic — field
/// touched ONLY by the single consumer thread, never by a producer).
struct Owner {
    ring: RingModel,
    /// Owner-private cache. NOT an atomic: only the single consumer thread
    /// ever reads/writes this in the real code (the drain guard runs
    /// exclusively on the segment's owning thread), so a loom model that used
    /// an atomic here would be modelling a stronger primitive than
    /// production code actually uses. We enforce "single writer" in the test
    /// harness itself by never spawning more than one consumer thread.
    cached_head: std::cell::Cell<u32>,
}

// SAFETY (test-only): `Cell` is `!Sync`, but this model's contract is
// EXACTLY "at most one thread ever touches `cached_head`" — the same
// single-consumer invariant the production `SegmentHeader::ring_drain_head`
// field relies on (see its doc comment). Every test below spawns exactly one
// "current owner" thread that calls `guarded_drain`; the loom `Arc` needs
// `Owner: Sync` to be shared, so we assert the invariant here rather than
// switch to an atomic (which would over-model the real field's ordering).
unsafe impl Sync for Owner {}

impl Owner {
    fn new() -> Arc<Self> {
        Arc::new(Owner {
            ring: RingModel::new(),
            cached_head: std::cell::Cell::new(0),
        })
    }

    /// The GUARDED drain (mirrors the `find_segment_with_free_impl` change):
    /// skip the real drain (no `head` touch at all) when `tail_relaxed() ==
    /// cached_head`; otherwise drain for real and refresh the cache from the
    /// drain's own final `head`. Returns the offsets reclaimed this call (for
    /// the test to tally).
    fn guarded_drain(&self) -> Vec<u32> {
        let cached = self.cached_head.get();
        if self.ring.tail_relaxed() == cached {
            return Vec::new();
        }
        let mut got = Vec::new();
        let new_head = self.ring.drain(|off| got.push(off));
        self.cached_head.set(new_head);
        got
    }
}

// =========================================================================
// Scenario (a) — empty-check-vs-concurrent-push race.
// =========================================================================

/// A producer pushes ONE offset while the consumer runs several
/// `guarded_drain` rounds concurrently (racing the push against the guard's
/// `tail_relaxed` read). loom explores every interleaving. INVARIANT: by the
/// time both threads have joined and one FINAL guarded_drain has run after
/// the join (guaranteeing the producer's publish is visible), the offset has
/// been reclaimed EXACTLY once across the whole sequence — never lost, never
/// duplicated. This is the "later drain picks it up" contract: an individual
/// racing call may see `tail_relaxed() == cached_head` and skip (the push
/// hadn't landed yet from this thread's view), but that can only happen if
/// the push truly hadn't landed — a subsequent round always re-checks.
#[test]
fn guard_never_loses_a_racing_push() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();
        let reclaimed_total = Arc::new(AtomicUsize::new(0));

        let owner_p = Arc::clone(&owner);
        let producer = thread::spawn(move || while owner_p.ring.push(42).is_err() {});

        // Consumer: race a few guarded-drain rounds DURING the producer's
        // execution window (loom interleaves these with the push above).
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
        // visible (join is a full happens-before edge in loom), so this
        // guarded_drain is guaranteed to observe `tail` having moved past
        // whatever `cached_head` the racing rounds left behind, if the
        // racing rounds hadn't already reclaimed it.
        let got = owner.guarded_drain();
        reclaimed_total.fetch_add(got.len(), Ordering::Relaxed);

        let total = reclaimed_total.load(Ordering::Relaxed);
        assert_eq!(
            total, 1,
            "guarded drain reclaimed the racing push {total} times (expected exactly 1) \
             — the empty-check-vs-concurrent-push race lost or duplicated it"
        );
    });
}

/// Two producers pushing DISTINCT offsets while the consumer interleaves
/// guarded-drain rounds — a slightly larger race window than the single-push
/// scenario above, still bounded for loom.
#[test]
fn guard_never_loses_two_racing_pushes() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();
        let seen_10 = Arc::new(AtomicUsize::new(0));
        let seen_20 = Arc::new(AtomicUsize::new(0));

        let owner_a = Arc::clone(&owner);
        let pa = thread::spawn(move || while owner_a.ring.push(10).is_err() {});
        let owner_b = Arc::clone(&owner);
        let pb = thread::spawn(move || while owner_b.ring.push(20).is_err() {});

        let owner_c = Arc::clone(&owner);
        let seen_10_c = Arc::clone(&seen_10);
        let seen_20_c = Arc::clone(&seen_20);
        let consumer = thread::spawn(move || {
            for _ in 0..2 {
                for off in owner_c.guarded_drain() {
                    if off == 10 {
                        seen_10_c.fetch_add(1, Ordering::Relaxed);
                    } else if off == 20 {
                        seen_20_c.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });

        pa.join().unwrap();
        pb.join().unwrap();
        consumer.join().unwrap();

        // Final drain after the join — both pushes are now guaranteed visible.
        for off in owner.guarded_drain() {
            if off == 10 {
                seen_10.fetch_add(1, Ordering::Relaxed);
            } else if off == 20 {
                seen_20.fetch_add(1, Ordering::Relaxed);
            }
        }

        assert_eq!(
            seen_10.load(Ordering::Relaxed),
            1,
            "offset 10 reclaimed a number of times != 1 across the guarded-drain race"
        );
        assert_eq!(
            seen_20.load(Ordering::Relaxed),
            1,
            "offset 20 reclaimed a number of times != 1 across the guarded-drain race"
        );
    });
}

// =========================================================================
// Scenario (b) — slot re-claim boundary (fresh-segment reset).
// =========================================================================

/// Models the ONLY reset this codebase's shard-reuse discipline permits: a
/// segment is torn down to fresh state (ring cursors AND the header-resident
/// cache both reset to 0, matching `RemoteFreeRing::init_in_place` and
/// `SegmentHeader::small`'s zero-init running at the SAME call site,
/// `reserve_small_segment`). After the reset, a "new" producer push must
/// still be found by a fresh `guarded_drain` — no stale cache value from the
/// ring's PRIOR life leaks across the reset and causes a missed drain.
#[test]
fn guard_sees_push_after_fresh_segment_reset() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let owner = Owner::new();

        // "Prior life": push and fully drain (cache now equals a non-zero
        // head — e.g. after CAP/2 prior pushes+drains in the segment's
        // history). Use a value that is NOT 0 so a buggy "reset cache to the
        // ring's CURRENT head instead of 0" implementation would be
        // indistinguishable from correct here — the test specifically resets
        // to 0 (the documented fresh-segment invariant) and checks a push
        // that lands at the NEW ring's slot 0.
        assert!(owner.ring.push(999).is_ok());
        let got = owner.guarded_drain();
        assert_eq!(
            got,
            vec![999],
            "prior-life push not reclaimed by prior-life drain"
        );
        // cached_head is now 1 (one entry drained).
        assert_eq!(owner.cached_head.get(), 1);

        // "Fresh segment reset": brand-new ring memory (head=0, tail=0, all
        // slots empty) AND the header-resident cache reset to 0 — the SAME
        // call site in production (`reserve_small_segment`) performs both
        // resets together, so modelling them as a single atomic step here is
        // faithful (there is no window where one resets without the other).
        let fresh = Owner::new();

        // A "new" producer (standing in for a different logical owner
        // reusing the address space / a fresh claim) pushes to the FRESH
        // ring. A THREAD race is exercised here too: the push races the
        // first post-reset guarded_drain, exactly like scenario (a), but
        // starting from the freshly-reset cache instead of a
        // previously-drained one.
        let fresh_p = Arc::clone(&fresh);
        let producer = thread::spawn(move || while fresh_p.ring.push(7).is_err() {});

        let fresh_c = Arc::clone(&fresh);
        let reclaimed = Arc::new(AtomicUsize::new(0));
        let reclaimed_c = Arc::clone(&reclaimed);
        let consumer = thread::spawn(move || {
            for _ in 0..2 {
                let got = fresh_c.guarded_drain();
                reclaimed_c.fetch_add(got.len(), Ordering::Relaxed);
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();

        // Final guaranteed-visible round.
        let got = fresh.guarded_drain();
        reclaimed.fetch_add(got.len(), Ordering::Relaxed);

        assert_eq!(
            reclaimed.load(Ordering::Relaxed),
            1,
            "post-reset push not reclaimed exactly once — a stale cache value \
             would either miss it (0) or a broken model would double-count it"
        );
    });
}

/// COUNTERFACTUAL: if the fresh-segment reset resets the RING (head/tail/
/// slots) but FAILS to reset the cache (leaves it at the prior life's
/// drained-head value, e.g. `1`), and the new ring's first real head value
/// the guard would compute also happens to read as "unchanged" relative to
/// that stale cache, a push can be silently missed forever (not just
/// deferred to a later drain — genuinely never observed, because
/// `tail_relaxed()` on the NEW ring starts back at small values and a stale
/// large cached value makes `tail_relaxed() != cached_head` still evaluate
/// true today — so this counterfactual instead demonstrates the sharper
/// hazard: a stale cache that happens to EQUAL the new ring's `tail` after a
/// few pushes causes the guard to skip a round that a correct (reset) cache
/// would have drained, delaying (not losing) the reclaim by one round. This
/// pins that a NON-reset cache is at best a correctness footgun and the
/// production code's reset-alongside-the-ring discipline is load-bearing —
/// the counterfactual's final assertion is deliberately the OPPOSITE of the
/// real test above (it demands the round-1 result differ from a fresh-cache
/// run), and `#[should_panic]` documents that a stale, non-reset cache
/// produces an observably different (and wrong) trace.
#[test]
#[should_panic(expected = "stale cache")]
fn counterfactual_stale_cache_not_reset_across_segment_reuse() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        // Prior life: drain 3 entries so the (buggy, carried-forward) cache
        // would read 3 after reuse instead of the correct reset value 0.
        let owner = Owner::new();
        for off in [1, 2, 3] {
            assert!(owner.ring.push(off).is_ok());
        }
        let got = owner.guarded_drain();
        assert_eq!(got.len(), 3);
        let stale_cached_head = owner.cached_head.get();
        assert_eq!(stale_cached_head, 3);

        // BUG: "reuse" builds a fresh ring (head=0, tail=0) but WRONGLY
        // carries the stale cache forward instead of resetting it to 0.
        let fresh_ring = RingModel::new();
        let buggy_owner = Owner {
            ring: fresh_ring,
            cached_head: std::cell::Cell::new(stale_cached_head), // BUG: should be 0
        };

        // Push exactly `stale_cached_head` (3) offsets into the fresh ring —
        // this brings the fresh ring's `tail` to 3, MATCHING the stale cache
        // by coincidence (a realistic hazard: any workload that pushes the
        // same count before the first post-reuse drain reproduces this).
        for off in [11, 12, 13] {
            assert!(buggy_owner.ring.push(off).is_ok());
        }

        // The guard now WRONGLY sees `tail_relaxed() (3) == cached_head (3)`
        // and skips the drain — the 3 real pushes on the fresh ring are
        // never reclaimed by this call.
        let got = buggy_owner.guarded_drain();
        assert!(
            !got.is_empty(),
            "stale cache: guard skipped a fresh segment's real pushes because \
             a NON-reset cache coincidentally matched the fresh ring's tail \
             (this is exactly why production resets ring_drain_head to 0 \
             alongside RemoteFreeRing::init_in_place at the same call site)"
        );
    });
}
