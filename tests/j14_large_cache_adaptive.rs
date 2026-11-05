//! J14 (`large-cache-adaptive`): the Large cache learns a free-and-reallocate
//! habit from its mistakes (a miss a recently given-back span would have
//! served), starts as the production cache (8 slots, no byte budget) and grows
//! past them up to the learned target, keeps the target under the heap's recent
//! peak of live Large bytes, and forgets the habit over quiet decay intervals.
#![cfg(all(
    feature = "alloc-core",
    feature = "large-cache-adaptive",
    feature = "internals",
    not(miri)
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

const MIB: usize = 1024 * 1024;

/// One 4 MiB span per block.
fn layout() -> Layout {
    Layout::from_size_align(4 * MIB - aligned_vmem::page_size(), 16).unwrap()
}

fn round(ac: &mut AllocCore, n: usize) -> u64 {
    let l = layout();
    let hits0 = ac.dbg_large_cache_hits();
    let v: Vec<_> = (0..n)
        .map(|_| {
            let p = ac.alloc(l);
            assert!(!p.is_null());
            p
        })
        .collect();
    let hits = ac.dbg_large_cache_hits() - hits0;
    for p in v.into_iter().rev() {
        // SAFETY: live pointer from this core and layout, freed once.
        unsafe { ac.dealloc(p, l) };
    }
    hits
}

#[test]
fn learns_the_habit_and_serves_the_whole_round_past_64_slots() {
    let mut ac = AllocCore::new().expect("core");
    let n = 80; // 320 MiB live: more spans than the 8 base slots and than one occupancy word
    assert_eq!(round(&mut ac, n), 0, "round 1 starts on an empty cache");
    let (extra1, _, _, mistakes1, active1) = ac.dbg_large_cache_adaptive().expect("sidecar");
    assert_eq!(mistakes1, 0, "no mistake before any reuse");
    assert_eq!(
        (extra1, active1),
        (0, 0),
        "nothing learned yet: the 8 base slots only"
    );
    assert_eq!(
        ac.dbg_large_cache_used(),
        8 * 4 * MIB,
        "only the 8 base slots are kept"
    );

    let hits2 = round(&mut ac, n);
    assert_eq!(hits2, 8, "round 2 hits what round 1 kept");
    let (extra2, live2, peak2, mistakes2, _) = ac.dbg_large_cache_adaptive().unwrap();
    assert!(mistakes2 >= 1, "the given-back spans were asked for again");
    let headroom = ac.dbg_decay_config().2;
    assert!(peak2 >= n * 4 * MIB, "peak follows the live round: {peak2}");
    assert!(
        extra2 <= peak2 - headroom,
        "the learned extra stays under the program's own peak"
    );
    assert_eq!(live2, 0, "everything was freed again");
    assert_eq!(
        ac.dbg_large_cache_used(),
        n * 4 * MIB,
        "after learning the whole round is kept"
    );

    assert_eq!(
        round(&mut ac, n) as usize,
        n,
        "round 3 is served entirely from the cache"
    );
}

#[test]
fn forgets_the_habit_over_quiet_intervals_and_trims() {
    let mut ac = AllocCore::new().expect("core");
    for _ in 0..3 {
        round(&mut ac, 80);
    }
    let used = ac.dbg_large_cache_used();
    assert!(
        ac.dbg_large_cache_adaptive().unwrap().0 > 0,
        "habit learned"
    );
    // A small headroom makes the forgotten retention visible as trimming.
    ac.dbg_set_decay_config(1000, 1000, 16 * MIB);
    for _ in 0..64 {
        ac.dbg_force_decay_tick();
    }
    let (extra, _, _, _, _) = ac.dbg_large_cache_adaptive().unwrap();
    assert_eq!(extra, 0, "quiet intervals unlearn the extra");
    assert!(
        ac.dbg_large_cache_used() < used,
        "the cache decays once the habit is gone"
    );
}

#[test]
fn one_span_cycle_learns_nothing() {
    let mut ac = AllocCore::new().expect("core");
    for _ in 0..50 {
        round(&mut ac, 1);
    }
    // The base 8 slots never overflow: no sidecar, no adaptive state at all.
    assert!(ac.dbg_large_cache_adaptive().is_none());
}

#[test]
fn switched_off_keeps_the_fixed_extension() {
    let mut ac = AllocCore::new().expect("core");
    round(&mut ac, 80);
    ac.dbg_set_large_cache_adaptive(false);
    assert_eq!(
        round(&mut ac, 80),
        8,
        "what the policy kept before the switch"
    );
    assert_eq!(round(&mut ac, 80), 40, "the fixed 40-slot extension");
    assert_eq!(
        round(&mut ac, 80),
        40,
        "without the policy only the fixed 40 slots keep serving"
    );
}

/// Campaign 1 of J14: spans of 36 MiB make 8 slots more than the 256 MiB
/// default budget of `large-cache-extended`; the policy must not cache less than
/// the production cache (8 spans) and must still learn (the ghost list and the
/// histogram live in the sidecar, which exists once the base slots are full).
#[test]
fn large_spans_keep_the_base_slots_and_still_learn() {
    let mut ac = AllocCore::new().expect("core");
    let l = Layout::from_size_align(36 * MIB - aligned_vmem::page_size(), 16).unwrap();
    let round_big = |ac: &mut AllocCore, n: usize| -> u64 {
        let hits0 = ac.dbg_large_cache_hits();
        let v: Vec<_> = (0..n)
            .map(|_| {
                let p = ac.alloc(l);
                assert!(!p.is_null());
                p
            })
            .collect();
        let hits = ac.dbg_large_cache_hits() - hits0;
        for p in v.into_iter().rev() {
            // SAFETY: live pointer from this core and layout, freed once.
            unsafe { ac.dealloc(p, l) };
        }
        hits
    };
    assert_eq!(round_big(&mut ac, 10), 0);
    let used = ac.dbg_large_cache_used();
    assert!(
        used >= 8 * 32 * MIB,
        "8 spans exceed the 256 MiB default budget: {used}"
    );
    assert_eq!(used % 8, 0);
    assert_eq!(
        round_big(&mut ac, 10),
        8,
        "the base slots serve as in production"
    );
    assert!(
        ac.dbg_large_cache_adaptive().unwrap().3 >= 1,
        "the 2 given-back spans were asked for again"
    );
    assert_eq!(
        round_big(&mut ac, 10),
        10,
        "the learned target holds the whole round"
    );
}

/// A quiet interval moves the learned extra down and the decay then releases
/// most of the excess within a few steps.
#[test]
fn quiet_intervals_shrink_the_extra_and_release_fast() {
    let mut ac = AllocCore::new().expect("core");
    for _ in 0..3 {
        round(&mut ac, 80);
    }
    assert!(
        ac.dbg_large_cache_adaptive().unwrap().0 > 0,
        "habit learned"
    );
    let used0 = ac.dbg_large_cache_used();
    ac.dbg_set_decay_config(1000, 1000, 16 * MIB);
    // The first interval still carries the hits and mistakes of the rounds.
    ac.dbg_force_decay_tick();
    let extra_a = ac.dbg_large_cache_adaptive().unwrap().0;
    ac.dbg_force_decay_tick();
    let extra_b = ac.dbg_large_cache_adaptive().unwrap().0;
    assert!(
        extra_b < extra_a,
        "the extra falls in a quiet interval: {extra_b} < {extra_a}"
    );
    for _ in 0..6 {
        ac.dbg_force_decay_tick();
    }
    assert!(
        ac.dbg_large_cache_used() < used0 / 2,
        "quiet steps release 3/4 of the excess each: {} of {used0}",
        ac.dbg_large_cache_used()
    );
}
