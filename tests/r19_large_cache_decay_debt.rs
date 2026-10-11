#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;
use std::time::Duration;

const MIB: usize = 1024 * 1024;
const H: usize = 16 * MIB;

fn layout() -> Layout {
    // One page of header slack gives a 4 MiB span in exact-page and segment modes.
    Layout::from_size_align(4 * MIB - aligned_vmem::page_size(), 16).unwrap()
}

fn alloc(ac: &mut AllocCore) -> *mut u8 {
    let p = ac.alloc(layout());
    assert!(!p.is_null(), "allocation must succeed; no vacuous OOM skip");
    p
}

fn free(ac: &mut AllocCore, p: *mut u8) {
    // SAFETY: callers pass a live allocation from this core with layout(), once.
    unsafe { ac.dealloc(p, layout()) };
}

fn fill(ac: &mut AllocCore, count: usize) {
    let live: Vec<_> = (0..count).map(|_| alloc(ac)).collect();
    for p in live {
        free(ac, p);
    }
}

#[test]
fn drained_batch_discards_debt() {
    let mut ac = AllocCore::new().expect("core creation must succeed");
    ac.dbg_set_large_cache_budget(None);
    let spare: Vec<_> = (0..4).map(|_| alloc(&mut ac)).collect();
    ac.dbg_set_decay_config(1000, 60_000, H);
    fill(&mut ac, 8);
    assert_eq!(ac.dbg_large_cache_used(), 32 * MIB);
    let burst: Vec<_> = (0..5).map(|_| alloc(&mut ac)).collect();
    assert_eq!(ac.dbg_large_cache_used(), 12 * MIB);
    // Backdating primes the stride. Require an eviction from above H, not
    // merely the first free's ordinary 12 -> 16 MiB deposit. Decay precedes
    // the 4 MiB free deposit (or allocation withdrawal), so account for it
    // when checking that the decay batch itself reached H.
    ac.dbg_backdate_decay_timer(Duration::from_secs(1200));
    let mut first_min = usize::MAX;
    let mut cleared = false;
    for p in burst {
        let before_used = ac.dbg_large_cache_used();
        free(&mut ac, p);
        let after_used = ac.dbg_large_cache_used();
        first_min = first_min.min(after_used);
        cleared |= before_used > H
            && after_used < before_used + 4 * MIB
            && after_used
                .checked_sub(4 * MIB)
                .is_some_and(|used| used <= H);
    }
    for _ in 0..64 {
        let before_used = ac.dbg_large_cache_used();
        let p = alloc(&mut ac);
        let after_used = ac.dbg_large_cache_used();
        first_min = first_min.min(after_used);
        cleared |=
            before_used > H && after_used + 4 * MIB < before_used && after_used + 4 * MIB <= H;
        let before_used = after_used;
        free(&mut ac, p);
        let after_used = ac.dbg_large_cache_used();
        first_min = first_min.min(after_used);
        cleared |= before_used > H
            && after_used < before_used + 4 * MIB
            && after_used
                .checked_sub(4 * MIB)
                .is_some_and(|used| used <= H);
        if cleared {
            break;
        }
    }
    assert!(
        cleared,
        "first catch-up must evict from above H and clear excess: min={first_min}"
    );
    let mut min_used = usize::MAX;
    for p in spare {
        free(&mut ac, p);
        min_used = min_used.min(ac.dbg_large_cache_used());
    }
    for _ in 0..64 {
        let p = alloc(&mut ac);
        min_used = min_used.min(ac.dbg_large_cache_used());
        free(&mut ac, p);
        min_used = min_used.min(ac.dbg_large_cache_used());
    }
    // Red without V2. Final used is insufficient: free hides a drained batch.
    // Real scheduler stalls of >=2 minutes can still cause legitimate decay.
    assert!(min_used > H, "second batch inherited debt: min={min_used}");
}

#[test]
fn zero_interval_still_decays_each_eligible_call() {
    let mut ac = AllocCore::new().expect("core creation must succeed");
    ac.dbg_set_large_cache_budget(None);
    let spare = alloc(&mut ac);
    fill(&mut ac, 8);
    ac.dbg_set_decay_config(1000, 0, H);
    let p = alloc(&mut ac);
    assert_eq!(ac.dbg_large_cache_used(), 24 * MIB);
    free(&mut ac, p);
    assert_eq!(ac.dbg_large_cache_used(), 24 * MIB);
    free(&mut ac, spare);
    assert_eq!(ac.dbg_large_cache_used(), 24 * MIB);
}

#[test]
fn eight_slots_drain_with_positive_release_but_rounding_can_stall() {
    let mut ac = AllocCore::new().expect("core creation must succeed");
    ac.dbg_set_large_cache_budget(None);
    fill(&mut ac, 8);
    assert_eq!(ac.dbg_large_cache_slot_sizes().iter().flatten().count(), 8);
    ac.dbg_set_decay_config(1000, 0, 0);
    for _ in 0..8 {
        let before = ac.dbg_large_cache_used();
        ac.dbg_force_decay_tick();
        assert!(ac.dbg_large_cache_used() < before);
    }
    assert_eq!(ac.dbg_large_cache_used(), 0);
    ac.dbg_set_decay_config(1000, 1000, usize::MAX);
    fill(&mut ac, 1);
    let used = ac.dbg_large_cache_used();
    ac.dbg_set_decay_config(1000, 0, used - 1);
    for _ in 0..8 {
        ac.dbg_force_decay_tick();
        assert_eq!(
            ac.dbg_large_cache_used(),
            used,
            "one-byte excess rounds release to zero"
        );
    }
}
