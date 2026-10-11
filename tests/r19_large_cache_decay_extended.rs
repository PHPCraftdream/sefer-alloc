#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;
use std::time::Duration;

#[test]
fn catchup_debt_survives_while_excess_remains() {
    const MIB: usize = 1024 * 1024;
    const H: usize = 16 * MIB;
    let layout = Layout::from_size_align(4 * MIB - aligned_vmem::page_size(), 16).unwrap();
    let mut ac = AllocCore::new().expect("core creation must succeed");
    ac.dbg_set_large_cache_budget(Some(256 * MIB));
    let live: Vec<_> = (0..40)
        .map(|_| {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "no OOM skip");
            p
        })
        .collect();
    ac.dbg_set_decay_config(1000, 60_000, H);
    for p in live {
        // SAFETY: live pointer from this core/layout, freed exactly once.
        unsafe { ac.dealloc(p, layout) };
    }
    assert_eq!(ac.dbg_large_cache_used(), 160 * MIB);
    assert_eq!(ac.dbg_large_cache_occupied_bits().count_ones(), 40);
    ac.dbg_backdate_decay_timer(Duration::from_secs(1200));
    let mut baseline = ac.dbg_large_cache_used();
    let mut first_min = baseline;
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        first_min = first_min.min(ac.dbg_large_cache_used());
        // SAFETY: live pointer from this core/layout, freed exactly once.
        unsafe { ac.dealloc(p, layout) };
        first_min = first_min.min(ac.dbg_large_cache_used());
        if ac.dbg_large_cache_used() < baseline {
            break;
        }
    }
    assert!(
        first_min < baseline - 4 * MIB,
        "catch-up must evict, not just pop a hit"
    );
    assert!(first_min > H, "40-slot control must retain excess");
    baseline = ac.dbg_large_cache_used();
    let mut second_min = baseline;
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        second_min = second_min.min(ac.dbg_large_cache_used());
        // SAFETY: live pointer from this core/layout, freed exactly once.
        unsafe { ac.dealloc(p, layout) };
        second_min = second_min.min(ac.dbg_large_cache_used());
    }
    // Red for unconditional Some(now); a normal cache hit alone removes 4 MiB.
    assert!(second_min < baseline - 4 * MIB, "retained excess lost catch-up debt: first={first_min}, baseline={baseline}, second={second_min}");
}
