//! CI-only behavioral oracle for the separately overlaid static reference.
#![cfg(all(
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare, SeferAlloc};
use std::alloc::{GlobalAlloc, Layout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

struct OwnedCohorts {
    blocks: [*mut u8; 16],
    layout: Layout,
}

impl Drop for OwnedCohorts {
    fn drop(&mut self) {
        for block in self.blocks {
            if !block.is_null() {
                // SAFETY: every nonnull entry is uniquely owned and retains its original layout.
                unsafe { GLOBAL.dealloc(block, self.layout) };
            }
        }
    }
}

#[test]
#[ignore = "requires the hash-pinned static-reference overlay in a disposable CI source copy"]
fn static_reference_samples_real_controls_without_learning() {
    const MIB: usize = 1024 * 1024;
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && page < MIB);
    // Header/alignment consumes one runtime page; the total useful span is exactly 8 MiB.
    let bytes = 8 * MIB - page;
    const GROUP: usize = 1;
    drop(
        AllocCore::new_with_config(
            LargeCacheConfig::new()
                .huge_page_mode(LargeThpMode::Always)
                .huge_page_block_min_bytes(bytes)
                .huge_page_budget_bytes(0),
        )
        .expect("explicit process tuple bootstrap"),
    );
    let tuple = AllocCore::dbg_thp_tuple();
    assert_eq!(tuple.mode, LargeThpMode::Always);
    assert_eq!(tuple.min_bytes, bytes);
    assert_eq!(tuple.budget_bytes, 0);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    let initial = AllocCore::dbg_thp_learning_telemetry();
    let one = u64::from(LargeThpShare::SCALE);
    assert_eq!(initial, [[one, one, one, 0, 0]; 6]);
    let before = AllocCore::dbg_thp_control_telemetry();
    let ordinal = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(ordinal % 8, 0, "two complete aligned cohorts");
    let advice = AllocCore::dbg_thp_advised_bytes_total();
    let layout = Layout::from_size_align(bytes, 8).expect("valid exact payload layout");
    let mut owned = OwnedCohorts {
        blocks: [core::ptr::null_mut(); 16],
        layout,
    };
    for block in &mut owned.blocks {
        // SAFETY: valid nonzero layout; every returned pointer is retained until its unique free.
        *block = unsafe { GLOBAL.alloc(layout) };
        assert!(!block.is_null());
        // SAFETY: this is a live owned nonempty payload; its first byte is in bounds.
        unsafe { block.write_volatile(0x5a) };
    }
    assert_eq!(AllocCore::dbg_thp_control_ordinal() - ordinal, 16);
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total() - advice,
        u64::try_from(14 * 8 * MIB).expect("bounded expected advice"),
        "fourteen non-controls must retain full fixed-one advice"
    );
    drop(owned);
    let after = AllocCore::dbg_thp_control_telemetry();
    let row = after[GROUP];
    assert_eq!(row[0] - before[GROUP][0], 2, "actual Always controls");
    assert_eq!(row[2] - before[GROUP][2], 2, "actual owner samples");
    for column in [1, 3, 4, 5] {
        assert_eq!(
            row[column], before[GROUP][column],
            "no failures or invalidations"
        );
    }
    let resident = row[6] - before[GROUP][6];
    let sampled = row[7] - before[GROUP][7];
    assert!(sampled > 0);
    assert!(u128::from(resident) * 100 < u128::from(sampled));
    for group in 0..6 {
        if group != GROUP {
            assert_eq!(after[group], before[group]);
        }
    }
    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry(),
        initial,
        "clean sparse observations must not update e/m/target or applied/dropped"
    );
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
}
