//! Concurrent stress test over `Arc<SyncRegion<T>>` (Phase 3a).
//!
//! The core property asserted here, beyond "didn't panic":
//!
//! > **No cross-thread corruption:** under random interleavings of insert /
//! > read / remove across several threads, a handle *always* resolves to the
//! > value its own thread inserted, or to `None` — never to a *different*
//! > value. The `RwLock` serialises every mutation against every read.
//!
//! Plus, after all threads join, `len()` equals the total survivors the threads
//! collectively reported (I4 — accounting holds under concurrency).
//!
//! Uses a per-thread fixed-seed LCG (no `rand` crate). Bounded to stay fast per
//! the short-scenario policy.

use std::sync::Arc;
use std::thread::scope;

use sefer_alloc::SyncRegion;

/// Per-thread fixed-seed LCG (Numerical Recipes constants). Deterministic per
/// thread index, so the test is reproducible across runs.
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
    /// A pseudo-random index in `0..n` (`n` must be > 0). Cast-free so the test
    /// stays clippy-pedantic clean.
    fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).expect("index space fits u64");
        usize::try_from(self.next_u64() % n64).expect("modulo result fits usize")
    }
}

/// A value tagged with the id of the thread that inserted it, so a read can
/// prove it belongs to the right handle and was not crossed with another.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tagged {
    thread: usize,
    seq: u64,
}

const THREADS: usize = 4;
const OPS_PER_THREAD: usize = 2_000;

#[test]
fn concurrent_insert_read_remove_never_corrupts_and_accounting_holds() {
    let region = Arc::new(SyncRegion::<Tagged>::with_capacity(
        OPS_PER_THREAD * THREADS,
    ));

    let total_survivors = scope(|scope| {
        let handles: Vec<_> = (0..THREADS)
            .map(|tid| {
                let region = Arc::clone(&region);
                // Each thread returns how many of its own handles it leaves alive.
                scope.spawn(move || worker(tid, &region))
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().expect("worker thread panicked"))
            .sum::<usize>()
    });

    // The core accounting property (I4) under concurrency.
    assert_eq!(
        region.len(),
        total_survivors,
        "len() must equal the total live entries threads reported"
    );

    // And is_empty only when truly empty — sanity, the region has survivors here
    // unless all threads happened to remove everything they inserted.
    assert_eq!(region.is_empty(), total_survivors == 0);
}

fn worker(tid: usize, region: &SyncRegion<Tagged>) -> usize {
    let mut rng = Lcg::new(
        u64::try_from(tid)
            .unwrap()
            .wrapping_add(0x9E37_79B9_7F4A_7C15),
    );
    let mut my_handles: Vec<sefer_alloc::Handle<Tagged>> = Vec::with_capacity(OPS_PER_THREAD);

    for seq in 0..u64::try_from(OPS_PER_THREAD).unwrap() {
        // Insert a value uniquely identifiable by (thread, seq).
        let value = Tagged { thread: tid, seq };
        let h = region.insert(value.clone());

        // Read it back IMMEDIATELY and assert it resolves to its own value, not
        // a different one — the per-handle property under contention.
        let got = region.get_cloned(h).expect("fresh handle must resolve");
        assert_eq!(
            got, value,
            "immediate re-read of a fresh handle returned a different value"
        );

        my_handles.push(h);

        // Randomly remove one of our own handles this iteration, chosen at
        // random and POPPED from our tracking vec so every probe hits a still
        // -live handle and the removes actually churn. We only ever touch
        // handles our own thread created, so this is ownership-safe.
        if !my_handles.is_empty() && rng.chance(1, 4) {
            let idx = rng.below(my_handles.len());
            let victim = my_handles.swap_remove(idx);
            let removed = region
                .remove(victim)
                .expect("our own live handle must remove exactly once");
            assert_eq!(
                removed.thread, tid,
                "removed a value from a different thread"
            );
            // After remove, the handle must be None forever (I2).
            assert_eq!(
                region.get_cloned(victim),
                None,
                "removed handle must resolve to None"
            );
        }
    }

    // After the churn, `my_handles` holds exactly our still-live handles (we
    // popped every removed one). Re-check each resolves to *our* value under a
    // single read guard — a final cross-thread re-assert of the no-corruption
    // property — and report the survivor count for the accounting check.
    let guard = region.read();
    for h in &my_handles {
        let v = guard
            .get(*h)
            .expect("a tracked (un-removed) handle must still resolve");
        assert_eq!(
            v.thread, tid,
            "surviving handle resolved to a value from a different thread"
        );
    }
    my_handles.len()
}
