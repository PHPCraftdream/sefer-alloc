//! J13b tail trim with a budget: the charge reserved before advice and stored
//! in the header equals the TRIMMED advised length (the 2 MiB floor of the block
//! end), the ledger and totals follow it, a later block finds exactly the
//! remaining budget, and every terminal release returns the charge once so the
//! ledger ends at zero. Real advice is asserted on Linux only. One scenario per
//! binary (process-global ledger).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;
const BUDGET: usize = 16 * MIB;
const SIZE: usize = 8 * MIB;

#[test]
fn charge_equals_trimmed_length_and_returns_to_zero() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(BUDGET);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    // Park decay far away so only the explicit steps below evict.
    core.dbg_set_decay_config(1000, 3_600_000, 1 << 40);
    let linux = cfg!(target_os = "linux");

    let layout = Layout::from_size_align(SIZE, 8).unwrap();
    let a = core.alloc(layout);
    let b = core.alloc(layout);
    let c = core.alloc(layout);
    assert!(!a.is_null() && !b.is_null() && !c.is_null());

    // Observed geometry: the end of an 8 MiB request sits just past 8 MiB, so
    // its trimmed advice is exactly 8 MiB while its span rounds up to 12 MiB.
    let (_, _, _, offset, _) = core.large_geometry_for_test(a).expect("large block");
    assert!(offset > 0 && offset < 2 * MIB);
    let trimmed = if linux {
        (offset + SIZE) / (2 * MIB) * (2 * MIB)
    } else {
        0
    };
    if linux {
        assert_eq!(trimmed, SIZE);
        assert!(core.dbg_span_usable_of(a) > trimmed);
    }

    // Two blocks take 8 MiB each (the whole budget); the third finds none. The
    // pre-trim extent (the 12 MiB span) would have charged 12 + 4 + 0.
    let charges = [a, b, c].map(|p| {
        core.large_thp_charge_for_test(p)
            .expect("live Large header")
    });
    assert_eq!(charges, [trimmed, trimmed, 0]);
    let used = charges.iter().sum::<usize>();
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used);
    assert_eq!(AllocCore::dbg_thp_charged_total(), used as u64);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), used as u64);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
    assert!(used <= BUDGET);

    // A rejected cache deposit releases at once and returns the charge once.
    core.dbg_set_large_cache_budget(Some(0));
    // SAFETY: `a` is live with its original layout.
    unsafe { core.dealloc(a, layout) };
    assert_eq!(AllocCore::dbg_thp_ledger_used(), used - charges[0]);
    assert_eq!(AllocCore::dbg_thp_returned_total(), charges[0] as u64);

    // The freed budget is exactly reusable: a fresh block is charged the same.
    let d = core.alloc(layout);
    assert!(!d.is_null());
    assert_eq!(core.large_thp_charge_for_test(d), Some(trimmed));
    assert_eq!(
        AllocCore::dbg_thp_ledger_used(),
        used - charges[0] + trimmed
    );
    assert!(AllocCore::dbg_thp_ledger_used() <= BUDGET);

    // SAFETY: all three are live with their original layouts.
    unsafe {
        core.dealloc(b, layout);
        core.dealloc(c, layout);
        core.dealloc(d, layout);
    }
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(
        AllocCore::dbg_thp_charged_total(),
        AllocCore::dbg_thp_returned_total()
    );
    assert_eq!(AllocCore::dbg_thp_charged_total(), (used + trimmed) as u64);
}
