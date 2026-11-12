//! J13b tail trim: `Always` (min 0, budget 0) advises exactly the 2 MiB floor
//! of the block's END (payload offset + requested size), never the floor of the
//! SEGMENT-rounded usable span. Sizes sit exactly on, one byte below and one
//! byte above 2 MiB multiples of the observed block end. Expected values come
//! from the observed header offset, not from guessed constants. Real advice is
//! asserted on Linux only. One scenario per binary (process-global counters).

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
const GRANULE: usize = 2 * MIB;

#[test]
fn advice_ends_at_the_two_mib_floor_of_the_block_end() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(0);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    let linux = cfg!(target_os = "linux");

    // The payload offset depends only on the alignment: learn it from a probe.
    let probe_layout = Layout::from_size_align(16 * MIB, 8).unwrap();
    let probe = core.alloc(probe_layout);
    assert!(!probe.is_null());
    let (_, _, _, offset, _) = core.large_geometry_for_test(probe).expect("large block");
    assert!(
        offset > 0 && offset < GRANULE,
        "header offset is sub-granule"
    );

    // Exact, one-below and one-above 2 MiB multiples of the block END, plus the
    // plain request sizes that straddle 8 MiB.
    let sizes = [
        16 * MIB,
        8 * MIB - 1,
        8 * MIB,
        8 * MIB + 1,
        16 * MIB - offset - 1,
        16 * MIB - offset,
        16 * MIB - offset + 1,
        3 * MIB,
        GRANULE - offset - 1,
        GRANULE - offset,
    ];

    let mut blocks = vec![(probe, probe_layout)];
    let mut total = 0u64;
    let mut saw_old_overreach = false;
    for (index, size) in sizes.into_iter().enumerate() {
        let layout = Layout::from_size_align(size, 8).unwrap();
        // The probe is the first entry; every other size is a fresh allocation.
        let p = if index == 0 {
            probe
        } else {
            let p = core.alloc(layout);
            assert!(!p.is_null());
            blocks.push((p, layout));
            p
        };
        let (_, _, base, off, _) = core.large_geometry_for_test(p).expect("large block");
        assert_eq!(off, offset);
        assert_eq!(p.addr(), base + off);
        let span = core.dbg_span_usable_of(p);
        let end = off + size;
        assert!(span >= end);

        let before = total;
        total = AllocCore::dbg_thp_advised_bytes_total();
        let advised = (total - before) as usize;
        let expected = if linux { end / GRANULE * GRANULE } else { 0 };
        assert_eq!(
            advised, expected,
            "size {size}: advice must end at the 2 MiB floor of {end}"
        );
        assert!(
            advised <= end,
            "size {size}: advice never passes the block end"
        );
        assert!(advised <= span);
        assert_eq!(advised % GRANULE, 0);
        if linux && size % GRANULE == 0 {
            assert_eq!(advised, size, "a 2 MiB-multiple request is advised exactly");
        }
        // The pre-trim extent was the floor of the usable span.
        if linux && span / GRANULE * GRANULE > expected {
            saw_old_overreach = true;
        }
        assert_eq!(core.large_thp_charge_for_test(p), Some(0));
    }
    if linux && !cfg!(feature = "exact-span-large") {
        assert!(
            saw_old_overreach,
            "the sizes must include blocks whose span floor exceeds the block end"
        );
    }
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);

    for (p, layout) in blocks {
        // SAFETY: each block is live with its original layout.
        unsafe { core.dealloc(p, layout) };
    }
    drop(core);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);
}
