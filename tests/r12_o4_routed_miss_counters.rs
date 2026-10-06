#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

//! Round 12 O-4 (data only): the routed miss-scan counters classify every
//! negative-directory scan into exactly one outcome — a drain-created free
//! block hidden behind the negative directory, directory lag, or nothing.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

#[derive(Clone, Copy)]
struct Snap {
    scans: u64,
    created: u64,
    already: u64,
    nothing: u64,
}

fn snap() -> Snap {
    Snap {
        scans: AllocCore::dbg_routed_miss_scans(),
        created: AllocCore::dbg_routed_miss_scan_drain_created_free(),
        already: AllocCore::dbg_routed_miss_scan_bin_already_nonempty(),
        nothing: AllocCore::dbg_routed_miss_scan_nothing(),
    }
}

fn since(before: Snap) -> Snap {
    let now = snap();
    Snap {
        scans: now.scans - before.scans,
        created: now.created - before.created,
        already: now.already - before.already,
        nothing: now.nothing - before.nothing,
    }
}

fn outcome(d: Snap) -> (u64, u64, u64, u64) {
    (d.scans, d.created, d.already, d.nothing)
}

#[test]
fn routed_miss_scan_outcomes_are_exclusive_and_classified() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let large_class = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let class_zero = Layout::from_size_align(AllocCore::dbg_block_size(0), 8).unwrap();

    // Materialise the directory with many distinct Small roots; class zero is
    // untouched, so its BinTables are empty everywhere.
    let mut large_live = Vec::new();
    for _ in 0..4096 {
        let ptr = core.alloc(large_class);
        assert!(!ptr.is_null(), "setup allocation failed");
        large_live.push(ptr);
        if core.dbg_directory_is_materialised() {
            break;
        }
    }
    assert!(
        core.dbg_directory_is_materialised(),
        "setup crosses directory threshold"
    );

    // The first class-zero allocation carves it plus a 31-block refill; 32
    // allocations leave every class-zero bin empty again.
    let mut live = Vec::new();
    for _ in 0..32 {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
    }

    // (c) Nothing: the directory is right, the scan finds no block.
    let before = snap();
    assert!(core.dbg_find_segment_with_free(0).is_none());
    assert_eq!(outcome(since(before)), (1, 0, 0, 1), "empty miss");

    // (b) A terminal publication hidden behind the negative directory: the
    // bin is empty and the bit clear until the scan drains the sidecar.
    let victim = live.remove(0);
    // SAFETY: victim is a live class-zero block of `core`; its single free is
    // transferred to the sidecar here and never freed again directly.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    let slot = core.dbg_segment_id_of(victim) as usize;
    assert_eq!(core.dbg_directory_get_bit(0, slot), Some(false));
    let before = snap();
    assert!(core.dbg_find_segment_with_free(0).is_some());
    assert_eq!(
        outcome(since(before)),
        (1, 1, 0, 0),
        "drain-created free block"
    );

    // (a) Directory lag: the bin is already non-empty, only the bit is lost.
    let reused = core.alloc(class_zero);
    assert_eq!(reused, victim, "the drained block is the one reissued");
    // SAFETY: reused is a live class-zero block, freed exactly once here.
    unsafe { core.dealloc(reused, class_zero) };
    assert_eq!(core.dbg_directory_get_bit(0, slot), Some(true));
    assert!(core.dbg_directory_force_clear_bit(0, slot));
    let before = snap();
    assert!(core.dbg_find_segment_with_free(0).is_some());
    assert_eq!(outcome(since(before)), (1, 0, 1, 0), "directory lag");
    assert_eq!(
        core.dbg_directory_get_bit(0, slot),
        Some(true),
        "scan self-healed the bit"
    );

    // Whole-run invariant: one outcome per scan.
    let total = snap();
    assert!(total.scans >= 3);
    assert_eq!(total.scans, total.created + total.already + total.nothing);

    for ptr in live {
        // SAFETY: each pointer is a live, distinct class-zero block.
        unsafe { core.dealloc(ptr, class_zero) };
    }
    for ptr in large_live {
        // SAFETY: each pointer is live, distinct, and freed once here.
        unsafe { core.dealloc(ptr, large_class) };
    }
}
