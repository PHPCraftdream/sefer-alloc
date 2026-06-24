//! I6 — compaction-by-construction (Phase 2).
//!
//! These tests assert, through the *public* API alone, that the
//! compaction-by-construction property holds through our [`Region`] wrapper:
//! after a churn of inserts interleaved with removes, every surviving handle
//! still resolves to its value, the live values are densely accounted
//! (`len() == iter count`), and removing then re-inserting reuses the free
//! list so the backing capacity stays bounded by the high-water mark of live
//! entries — no fragmentation leaks through the membrane.
//!
//! `slotmap` owns the dense layout and the free list; these tests treat that
//! ownership as a *contract* observable from outside. Randomness comes from a
//! fixed-seed hand-rolled LCG (no `rand` dependency) so the churn is
//! deterministic and reproducible.

use sefer_alloc::{Handle, Region};

/// A tiny xorshift-style LCG: deterministic, seedable, no external crate.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        // Guard against the all-zero state, which xorshift would get stuck in.
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64 — fixed, cheap, good enough for deterministic churn.
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Drives a deterministic churn over a [`Region`]: inserts up to `target` live
/// entries, randomly removing survivors as it goes, and records the value each
/// live handle *should* resolve to so the test can check it afterwards.
fn churn(seed: u64, target: usize) -> Region<u64> {
    let mut region = Region::with_capacity(target);
    // Live handles paired with the value they must resolve to.
    let mut live: Vec<(Handle<u64>, u64)> = Vec::with_capacity(target);
    let mut rng = Lcg::new(seed);
    let mut next_value = 0u64;

    for _ in 0..(target * 4) {
        if live.len() < target {
            let value = next_value;
            next_value += 1;
            let handle = region.insert(value);
            live.push((handle, value));
        }
        // Randomly retire ~half as many entries as we have live, to force the
        // free list to churn and the dense store to compact.
        if !live.is_empty() && rng.next_u64().is_multiple_of(3) {
            // The cast is safe: `live.len()` is tiny (at most `target`), so the
            // modulo result fits in `usize` on every target we support.
            #[allow(clippy::cast_possible_truncation)]
            let idx = (rng.next_u64() as usize) % live.len();
            let (handle, value) = live.swap_remove(idx);
            assert_eq!(region.remove(handle), Some(value), "remove returned wrong value");
        }
    }
    // Final resolution check while we still hold the model.
    for (handle, value) in &live {
        assert_eq!(
            region.get(*handle),
            Some(value),
            "live handle failed to resolve its value after churn"
        );
    }
    region
}

/// I6 (a): after a churn, **every surviving handle still resolves to its
/// value** — compaction preserves live-handle resolution.
#[test]
fn surviving_handles_resolve_after_churn() {
    let region = churn(0xA11CE, 4096);
    // Re-walk the live entries and confirm none silently went missing.
    let resolved = region.iter().copied().collect::<Vec<_>>();
    assert!(!resolved.is_empty());
}

/// I6 (b): `len()` equals the number of survivors, and `iter()` yields exactly
/// `len()` items — the live values stay densely accounted through the wrapper,
/// with no fragmentation leaking past the typed boundary.
#[test]
fn len_matches_iter_after_churn() {
    let region = churn(0xB0B, 8192);
    let iter_count = region.iter().count();
    assert_eq!(iter_count, region.len(), "iter() yielded != len() entries");
    assert!(!region.is_empty());
}

/// I6 (c): removing entries and re-inserting the same number **reuses the free
/// list** — capacity stays bounded by the high-water mark of live entries and
/// does not grow unboundedly under steady-state churn.
#[test]
fn reinsert_reuses_capacity_without_unbounded_growth() {
    let mut region = Region::new();
    let mut live: Vec<Handle<u64>> = Vec::with_capacity(2048);

    // Grow to a high-water mark of 2048 live entries.
    for _ in 0..2048 {
        live.push(region.insert(0u64));
    }
    let high_water_capacity = region.capacity();
    assert!(high_water_capacity >= 2048, "capacity must cover live entries");

    // Steady-state churn: remove all, then re-insert the same count. Because
    // the freed slots return to slotmap's free list, this must not push
    // capacity higher than the high-water mark.
    for _ in 0..3 {
        for handle in live.drain(..) {
            region.remove(handle);
        }
        assert_eq!(region.len(), 0, "region not empty after draining live set");
        for _ in 0..2048 {
            live.push(region.insert(0u64));
        }
        assert_eq!(region.len(), 2048);
        assert!(
            region.capacity() <= high_water_capacity,
            "capacity grew past high-water mark under steady-state churn: {} > {}",
            region.capacity(),
            high_water_capacity,
        );
    }
}
