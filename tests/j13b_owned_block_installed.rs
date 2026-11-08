//! Isolated installed-GlobalAlloc admission oracle for the actual owned buffer.
#![cfg(all(
    not(miri),
    any(target_os = "linux", target_os = "android"),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use owned::OwnedBlock;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};
use std::thread;
use workload::{touch_slots_with_page, Shape};

#[global_allocator]
static INSTALLED: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

#[test]
fn real_owner_and_foreign_frees_preserve_learning_boundary() {
    const MIB: usize = 1024 * 1024;
    const GROUP: usize = 1;
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && page < MIB);
    let bytes = 8 * MIB - page;
    drop(
        AllocCore::new_with_config(
            LargeCacheConfig::new()
                .huge_page_mode(LargeThpMode::Auto)
                .huge_page_block_min_bytes(bytes)
                .huge_page_budget_bytes(0),
        )
        .expect("explicit process bootstrap"),
    );
    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Auto);
    assert_eq!(tuple.min_bytes, bytes);
    assert_eq!(tuple.budget_bytes, 0);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    let mut small = OwnedBlock::allocate(&INSTALLED, 32).expect("installed small allocation");
    assert_eq!(touch_slots_with_page(small.slots(), Shape::Dense, page), 32);
    small.reallocate(64).expect("installed realloc boundary");
    assert_eq!(small.requested_bytes(), 64);
    drop(small);
    let before = AllocCore::dbg_thp_control_telemetry();
    let initial_learning = AllocCore::dbg_thp_learning_telemetry();
    let ordinal = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(ordinal % 8, 0);
    let advice = AllocCore::dbg_thp_advised_bytes_total();
    let mut blocks: Vec<_> = (0..16)
        .map(|_| OwnedBlock::allocate(&INSTALLED, bytes).expect("bounded fresh block"))
        .collect();
    assert_eq!(AllocCore::dbg_thp_control_ordinal() - ordinal, 16);
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total() - advice,
        u64::try_from(14 * 8 * MIB).expect("bounded advice")
    );

    let owner_blocks: Vec<_> = blocks.drain(..8).collect();
    let returned = thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut owner_blocks = owner_blocks;
                for block in &mut owner_blocks {
                    assert_eq!(touch_slots_with_page(block.slots(), Shape::Sparse, page), 1);
                }
                owner_blocks
            })
            .join()
            .expect("touch worker joined")
    });
    drop(returned);
    let learned = AllocCore::dbg_thp_learning_telemetry();
    let owner = AllocCore::dbg_thp_control_telemetry()[GROUP];
    assert_eq!(owner[2] - before[GROUP][2], 1, "one owner sample");
    assert!(owner[7] > before[GROUP][7]);
    assert!(learned[GROUP][0] < initial_learning[GROUP][0]);
    assert_eq!(learned[GROUP][3] - initial_learning[GROUP][3], 1);
    assert_eq!(learned[GROUP][4], initial_learning[GROUP][4]);

    thread::scope(|scope| {
        scope
            .spawn(move || {
                for mut block in blocks {
                    assert_eq!(touch_slots_with_page(block.slots(), Shape::Sparse, page), 1);
                    drop(block);
                }
            })
            .join()
            .expect("foreign free worker joined")
    });
    assert_eq!(AllocCore::dbg_thp_learning_telemetry(), learned);
    INSTALLED.dbg_trim_current_thread();
    let after = AllocCore::dbg_thp_control_telemetry();
    assert_eq!(after[GROUP][0] - before[GROUP][0], 2);
    assert_eq!(after[GROUP][2] - before[GROUP][2], 1);
    assert_eq!(
        after[GROUP][3] - before[GROUP][3],
        1,
        "remote UNKNOWN, never a sample"
    );
    for column in [1, 4, 5] {
        assert_eq!(after[GROUP][column], before[GROUP][column]);
    }
    for group in 0..6 {
        if group != GROUP {
            assert_eq!(after[group], before[group]);
            assert_eq!(learned[group], initial_learning[group]);
        }
    }
    assert_eq!(AllocCore::dbg_thp_learning_telemetry(), learned);
    INSTALLED.dbg_trim_current_thread();
    assert_eq!(AllocCore::dbg_thp_control_telemetry(), after);
    assert_eq!(AllocCore::dbg_thp_learning_telemetry(), learned);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
}
