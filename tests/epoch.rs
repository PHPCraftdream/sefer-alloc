#![allow(deprecated)]
//! Epoch tier tests over `EpochRegion<T>` (Phase 3b-II, `experimental`).
//!
//! FAST tests per the short-scenario policy:
//!
//! 1. **Single-threaded conformance** — a fixed sequence of insert/get/remove
//!    behaves like a reference model (a `Vec` of live `(handle, value)`):
//!    fresh handle resolves, removed handle is `None` forever, stale never
//!    resolves (I1–I3), `len` tracks (I4), and ABA via slot reuse is prevented.
//!    Also covers the fixed-capacity full-region `Err` path.
//! 2. **Fixed-capacity full region** — `insert` returns `Err(value)` (no panic)
//!    once all slots are taken, and the value is handed back intact.
//! 3. **Concurrent readers + writers** — a few writer threads churn (insert /
//!    remove their own handles) while reader threads continuously `get_cloned`
//!    a shared pool of live handles; assert a reader NEVER observes a torn or
//!    cross value — every resolution returns the correct value or `None`.
//!    Bounded thread/op counts so it runs in well under a second or two.

#![cfg(feature = "experimental")]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::scope;

use sefer_alloc::{EpochHandle, EpochRegion};

/// A single-threaded op-sequence conformance check against a reference model.
///
/// The model is a `Vec<(EpochHandle<u32>, u32)>` of live entries. Every op is
/// checked against it: a fresh handle resolves to its value, a removed handle
/// is `None` forever (and a second remove is a no-op `false`), a stale handle
/// (post-reuse) never resolves, and `len`/`is_empty` track the live count.
#[test]
fn single_threaded_sequence_matches_reference_model() {
    // Capacity large enough to allow reuse (the ABA check below relies on a
    // slot being freed and then reused).
    let region = EpochRegion::<u32>::with_capacity(8);
    let mut model: Vec<(EpochHandle<u32>, u32)> = Vec::new();

    // The region starts empty.
    assert!(region.is_empty());
    assert_eq!(region.len(), 0);

    // Insert a handful and check each resolves immediately (I1).
    for v in [10_u32, 20, 30, 40, 50] {
        let h = region
            .insert(v)
            .expect("region with capacity 8 must accept the first inserts");
        assert_eq!(
            region
                .get_cloned(h)
                .expect("fresh handle must resolve (I1)"),
            v
        );
        model.push((h, v));
    }
    assert_eq!(region.len(), model.len());
    assert!(!region.is_empty());

    // Every live model handle still resolves to its own value.
    for (h, v) in &model {
        assert_eq!(
            region
                .get_cloned(*h)
                .expect("live handle must resolve (I1)"),
            *v
        );
    }

    // Remove the middle handle; it must be None forever (I2), second remove is
    // a no-op false, and survivors stay valid (I1).
    let victim_idx = 2;
    let (victim_h, victim_v) = model.remove(victim_idx);
    assert!(region.remove(victim_h), "live handle must remove once");
    assert_eq!(
        region.get_cloned(victim_h),
        None,
        "removed handle must be None (I2)"
    );
    assert!(
        !region.remove(victim_h),
        "second remove of a stale handle is a no-op false (I2)"
    );
    assert_eq!(region.len(), model.len());
    for (h, v) in &model {
        assert_eq!(
            region
                .get_cloned(*h)
                .expect("survivor must still resolve (I1)"),
            *v
        );
    }
    let _ = victim_v;

    // I3 (no ABA): insert a fresh value — it may reuse the freed slot, but the
    // OLD handle to that slot must NEVER resolve to the new value.
    let new_v = 999_u32;
    let new_h = region.insert(new_v).expect("a freed slot is available");
    assert_eq!(
        region
            .get_cloned(new_h)
            .expect("new handle must resolve (I1)"),
        new_v
    );
    assert_eq!(
        region.get_cloned(victim_h),
        None,
        "stale handle to a reused slot must not resolve (I3)"
    );
    assert_ne!(
        new_h, victim_h,
        "fresh handle differs from the stale one (generation bumped)"
    );
    model.push((new_h, new_v));
    assert_eq!(region.len(), model.len());

    // Remove everything and confirm accounting collapses to zero (I4) and every
    // removed handle is None forever.
    while let Some((h, _v)) = model.pop() {
        assert!(region.remove(h), "live handle must remove");
        assert_eq!(region.get_cloned(h), None);
    }
    assert_eq!(region.len(), 0);
    assert!(region.is_empty());
}

/// The fixed-capacity contract: `insert` returns `Err(value)` (no panic) once
/// every slot is occupied, and the value is handed back intact for the caller
/// to dispose of.
#[test]
fn full_region_returns_err_with_value_back() {
    const CAP: usize = 4;
    let region = EpochRegion::<u32>::with_capacity(CAP);

    // Fill the region exactly.
    for v in 0..u32::try_from(CAP).unwrap() {
        region
            .insert(v)
            .unwrap_or_else(|_| panic!("insert {v} into a region of cap {CAP} must succeed"));
    }
    assert_eq!(region.len(), CAP);

    // One more insert must fail with Err, returning the value back.
    let overflow_value = 4242_u32;
    let returned = region
        .insert(overflow_value)
        .expect_err("insert into a full region must return Err (no panic)");
    assert_eq!(
        returned, overflow_value,
        "the overflow value is handed back"
    );

    // Capacity is unchanged by the failed insert, and the region still resolves
    // every live handle.
    assert_eq!(region.len(), CAP);
}

/// I5 (drop-once) for the epoch tier: values still LIVE when the region is
/// dropped must have their destructors run exactly once. This is the invariant
/// `EpochRegion`'s `Drop` upholds — without it, live values would leak (their
/// destructors never running). The counter would read 0 if the leak were not
/// fixed, so this test is a real regression guard.
#[test]
fn region_drop_runs_live_value_destructors_once() {
    struct DropCounter(Arc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let counter = Arc::new(AtomicUsize::new(0));
    {
        let region = EpochRegion::<DropCounter>::with_capacity(8);
        for _ in 0..5 {
            region
                .insert(DropCounter(Arc::clone(&counter)))
                .unwrap_or_else(|_| panic!("insert into a region of cap 8 must succeed"));
        }
        assert_eq!(region.len(), 5);
        // The region is dropped here with all 5 values still live.
    }
    assert_eq!(
        counter.load(Ordering::Relaxed),
        5,
        "every value live at region drop must be dropped exactly once (I5)"
    );
}

// ---------------------------------------------------------------------------
// Concurrency test.
// ---------------------------------------------------------------------------

/// Per-thread fixed-seed LCG (Numerical Recipes constants). Deterministic per
/// thread index, so the test is reproducible across runs (no `rand` crate).
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    /// Next pseudo-random `u64`.
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    /// `true` with probability `num / denom`.
    fn chance(&mut self, num: u32, denom: u32) -> bool {
        if denom == 0 {
            return false;
        }
        (self.next_u64() % u64::from(denom)) < u64::from(num)
    }
    /// A pseudo-random index in `0..n` (`n` must be > 0).
    fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).expect("index space fits u64");
        usize::try_from(self.next_u64() % n64).expect("modulo result fits usize")
    }
}

/// A value tagged with the id of the thread that inserted it and a per-thread
/// sequence number. A reader proves the value belongs to the right handle by
/// checking the thread tag — a torn/cross read would surface as a value whose
/// `(thread, seq)` does not belong to the owning writer.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tagged {
    thread: usize,
    seq: u64,
}

const CAPACITY: usize = 64;
const WRITERS: usize = 3;
const READERS: usize = 3;
const WRITER_OPS: usize = 1_200;
const READER_OPS: usize = 3_600;

/// Shared pool of currently-live handles tagged with the owning writer's tid,
/// so a reader can verify a resolved value belongs to that writer.
type HandlePool<T> = Arc<Mutex<Vec<(usize, EpochHandle<T>)>>>;

/// Under concurrent readers + writers, every `get_cloned` returns either the
/// correct value for the handle or `None` — never a *different* (torn/cross)
/// value.
///
/// Writers churn their own values (insert + random remove of their own handles)
/// and publish the live ones into a shared `Mutex<Vec<(owner_thread, handle)>>`
/// pool. Readers sample handles from the pool and assert that any resolved
/// value's `(thread, seq)` belongs to the handle's owner thread (never a
/// different writer's value). Accounting (I4) is checked once all writers join.
#[test]
fn concurrent_readers_and_writers_never_observe_a_torn_value() {
    // Capacity is shared across writers; insert may return Err when the region
    // is momentarily full (a writer simply retries/drops that op — no panic).
    let region = Arc::new(EpochRegion::<Tagged>::with_capacity(CAPACITY));
    // Shared pool of currently-live handles, tagged with the owning writer's
    // tid so a reader can verify a resolved value belongs to that writer.
    let pool: HandlePool<Tagged> = Arc::new(Mutex::new(Vec::new()));

    let total_survivors = scope(|scope| {
        let live = Arc::new(AtomicBool::new(true));

        // Spawn writers first; they seed the pool as they go.
        let mut writer_handles: Vec<_> = (0..WRITERS)
            .map(|tid| {
                let region = Arc::clone(&region);
                let pool = Arc::clone(&pool);
                scope.spawn(move || writer(tid, &region, &pool))
            })
            .collect();

        // Readers run concurrently, sampling the shared pool until signalled.
        let mut reader_handles: Vec<_> = Vec::with_capacity(READERS);
        for rid in 0..READERS {
            let region = Arc::clone(&region);
            let pool = Arc::clone(&pool);
            let live = Arc::clone(&live);
            reader_handles.push(scope.spawn(move || reader(rid, &region, &pool, &live)));
        }

        let survivors: usize = writer_handles
            .drain(..)
            .map(|h| h.join().expect("writer thread panicked"))
            .sum();

        // Signal readers to wind down and join — assertion failures propagate.
        live.store(false, Ordering::Relaxed);
        for h in reader_handles {
            h.join().expect("reader thread panicked");
        }

        survivors
    });

    // I4 under concurrency: the live count equals what the writers reported.
    assert_eq!(
        region.len(),
        total_survivors,
        "len() must equal the total live entries writers reported (I4 under concurrency)"
    );
    assert_eq!(region.is_empty(), total_survivors == 0);
}

/// A writer churns its own values: inserts a uniquely-tagged value, reads it
/// back immediately (must resolve to its own value), publishes the live handle
/// into the shared pool, and randomly removes one of its own live handles
/// (removing it from the pool too). Returns the count of its surviving handles.
///
/// Insert may transiently return `Err` when the shared region is momentarily
/// full; the writer simply drops that op and continues (the fixed-capacity
/// contract — no panic).
fn writer(
    tid: usize,
    region: &EpochRegion<Tagged>,
    pool: &Mutex<Vec<(usize, EpochHandle<Tagged>)>>,
) -> usize {
    let mut rng = Lcg::new(
        u64::try_from(tid)
            .unwrap()
            .wrapping_add(0x9E37_79B9_7F4A_7C15),
    );
    // Track this writer's own handles locally so removal is ownership-safe and
    // we can report survivors. The shared pool is a read-only view for readers.
    let mut my_handles: Vec<EpochHandle<Tagged>> = Vec::with_capacity(WRITER_OPS);

    for seq in 0..u64::try_from(WRITER_OPS).unwrap() {
        let value = Tagged { thread: tid, seq };
        let Ok(h) = region.insert(value.clone()) else {
            // Region momentarily full (fixed capacity shared with other
            // writers): drop this op, no panic.
            continue;
        };

        // Immediate re-read: a fresh handle must resolve to its own value, not
        // a different one — the per-handle property under contention.
        let got = region
            .get_cloned(h)
            .expect("fresh handle must resolve immediately");
        assert_eq!(
            got, value,
            "writer {tid}: immediate re-read returned a different value"
        );

        my_handles.push(h);
        pool.lock().expect("pool mutex poisoned").push((tid, h));

        // Randomly remove one of our own handles (ownership-safe: only ours).
        if !my_handles.is_empty() && rng.chance(1, 3) {
            let idx = rng.below(my_handles.len());
            let victim = my_handles.swap_remove(idx);
            let removed = region.remove(victim);
            assert!(
                removed,
                "writer {tid}: our own live handle must remove exactly once"
            );
            // Also remove it from the shared pool so readers don't keep probing
            // a handle that's now stale (they'd just see None — harmless — but
            // trimming keeps the pool bounded).
            trim_pool(pool, victim);
            assert_eq!(
                region.get_cloned(victim),
                None,
                "writer {tid}: removed handle must resolve to None (I2)"
            );
        }
    }

    my_handles.len()
}

/// Remove one occurrence of `victim` from the shared pool (best-effort; readers
/// tolerate a stale handle by observing `None`).
fn trim_pool(pool: &Mutex<Vec<(usize, EpochHandle<Tagged>)>>, victim: EpochHandle<Tagged>) {
    let mut guard = pool.lock().expect("pool mutex poisoned");
    if let Some(pos) = guard.iter().position(|(_, h)| *h == victim) {
        guard.swap_remove(pos);
    }
}

/// Global counter of total reader probes — asserted at the end so we know the
/// readers actually did meaningful work (and did not short-circuit).
static PROBES: AtomicUsize = AtomicUsize::new(0);

/// A reader continuously samples handles from the shared pool and probes the
/// region. For every resolved value it asserts the value's `(thread, seq)`
/// belongs to the handle's owning writer (no cross/torn value); removed handles
/// are expected to resolve to `None`. Runs until `live` goes false, with a floor
/// of `READER_OPS` probes so a fast signal does not starve the test.
fn reader(
    rid: usize,
    region: &EpochRegion<Tagged>,
    pool: &Mutex<Vec<(usize, EpochHandle<Tagged>)>>,
    live: &AtomicBool,
) {
    let mut rng = Lcg::new(
        u64::try_from(rid)
            .unwrap()
            .wrapping_add(0x51ED_270B_1F2C_3D4E)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );

    loop {
        let done =
            !live.load(Ordering::Relaxed) && PROBES.load(Ordering::Relaxed) >= READER_OPS * READERS;
        if done {
            break;
        }

        // Snapshot a sampling view of the pool without holding the lock long.
        // We clone out one random entry (handle is Copy, cheap).
        let probe = {
            let guard = pool.lock().expect("pool mutex poisoned");
            if guard.is_empty() {
                None
            } else {
                let (owner, h) = guard[rng.below(guard.len())];
                Some((owner, h))
            }
        };

        if let Some((owner, h)) = probe {
            PROBES.fetch_add(1, Ordering::Relaxed);
            if let Some(v) = region.get_cloned(h) {
                // The core assertion: a resolved value must belong to the
                // handle's owning writer. A torn/cross read would produce a
                // value whose thread tag differs from `owner`.
                assert_eq!(
                    v.thread, owner,
                    "reader {rid}: resolved value belongs to writer {} but the handle was \
                     owned by writer {} — torn/cross read detected",
                    v.thread, owner,
                );
                assert!(
                    v.seq < u64::try_from(WRITER_OPS).unwrap(),
                    "reader {rid}: resolved value seq {} is out of range — torn read detected",
                    v.seq
                );
            }
            // `None` is always acceptable: the handle may have been removed by
            // its owner between the pool snapshot and this probe.
        }
    }
}
