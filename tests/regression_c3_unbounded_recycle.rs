//! One terminal owner pass must recycle more than the former CAP=32.
//! With 39 segments, primordial + cursor + four pool entries leave at least
//! 33 immediate recycles. A 64-KiB class needs at most 2,496 issued blocks,
//! bounding each wave by 156 MiB of segment spans (312 MiB raw VA).
//! Route bitmap/class storage adds about 11 MiB at the 16-byte granule.
//! A second wave must reuse the released slots without growing high-water.
//! Verified negative control: limiting the actual owner pass to 32 recycles
//! fails the immediate-recycle floor of 33, before the pool drain.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use std::collections::BTreeMap;

use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

const OLD_RECYCLE_CAP: usize = 32;
const POOL_CAP: usize = 4;
const TARGET_SEGMENTS: usize = OLD_RECYCLE_CAP + 2 + POOL_CAP + 1;
const BLOCK_SIZE: usize = 64 * 1024;
const MAX_BLOCKS: usize = TARGET_SEGMENTS * (SegmentLayout::SEGMENT / BLOCK_SIZE);

fn allocate(core: &mut AllocCore, layout: Layout, index: usize, phase: &str) -> *mut u8 {
    let failures = AllocCore::dbg_segments_reserve_failed_total();
    let reserved = AllocCore::dbg_segments_reserved_total();
    let ptr = core.alloc(layout);
    assert!(
        !ptr.is_null(),
        "{phase} NULL at block={index}: slots={}/{} live_segments={} pooled={} reserve_delta={} constructor_failure_delta={}",
        core.dbg_table_count(), AllocCore::dbg_max_segments(), core.segment_bases().count(),
        core.dbg_pooled_count(), AllocCore::dbg_segments_reserved_total() - reserved,
        AllocCore::dbg_segments_reserve_failed_total() - failures,
    );
    ptr
}

#[cfg_attr(miri, ignore)] // Preserve the existing native-only traversal witness.
#[test]
fn unbounded_recycle_within_single_scan() {
    eprintln!(
        "C3 geometry: segments={TARGET_SEGMENTS} blocks<={MAX_BLOCKS} logical_peak<={} span_peak<={} raw_va<={} route_bitmap_bytes={}",
        MAX_BLOCKS * BLOCK_SIZE, TARGET_SEGMENTS * SegmentLayout::SEGMENT,
        TARGET_SEGMENTS * 2 * SegmentLayout::SEGMENT,
        TARGET_SEGMENTS * (SegmentLayout::SEGMENT / SegmentLayout::MIN_BLOCK) * 9 / 8,
    );
    let reserved_before = AllocCore::dbg_segments_reserved_total();
    let released_before = AllocCore::dbg_segments_released_total();
    let failed_before = AllocCore::dbg_segments_reserve_failed_total();
    let mut core = AllocCore::dbg_new_routed_with_config_for_test(
        LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(POOL_CAP)),
    )
    .expect("routed primordial");
    assert_eq!(core.dbg_pool_cap(), POOL_CAP);
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();
    assert!(
        core.dbg_layout_class_for(layout).is_some(),
        "fixture must use Small blocks"
    );

    let mut survivors = BTreeMap::new();
    let mut issued = Vec::with_capacity(MAX_BLOCKS);
    for index in 0..MAX_BLOCKS {
        let ptr = allocate(&mut core, layout, index, "first wave");
        let key = ptr.addr() & !(SegmentLayout::SEGMENT - 1);
        survivors.entry(key).or_insert(ptr);
        issued.push(ptr);
        if survivors.len() == TARGET_SEGMENTS {
            break;
        }
    }
    assert_eq!(
        survivors.len(),
        TARGET_SEGMENTS,
        "bounded fixture did not reach its segment target"
    );
    let high_water = core.dbg_table_count();
    assert_eq!(high_water as usize, TARGET_SEGMENTS);
    let first_wave_blocks = issued.len();
    for ptr in issued {
        let key = ptr.addr() & !(SegmentLayout::SEGMENT - 1);
        if survivors[&key] != ptr {
            // SAFETY: each non-survivor is live and freed once with its allocation Layout.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    for &ptr in survivors.values() {
        assert_eq!(core.dbg_live_count_for(ptr), Some(1));
        // SAFETY: each survivor is uniquely transferred once to its terminal descriptor.
        assert!(unsafe { core.dbg_publish_small_sidecar_free(ptr) });
    }

    let decommit_before = AllocCore::dbg_decommit_count();
    assert_eq!(
        core.dbg_drain_sidecar_ingress(),
        TARGET_SEGMENTS,
        "one pass must consume every survivor"
    );
    let decommits = AllocCore::dbg_decommit_count() - decommit_before;
    let recycled = survivors
        .values()
        .filter(|&&ptr| core.dbg_live_count_for(ptr).is_none())
        .count();
    let minimum = TARGET_SEGMENTS - 2 - POOL_CAP;
    assert!(minimum > OLD_RECYCLE_CAP);
    assert!(
        decommits > 0,
        "one pass did not activate the decommit mechanism"
    );
    assert!(
        core.dbg_pooled_count() <= POOL_CAP,
        "pool retention exceeded its configured bound"
    );
    assert!(
        recycled >= minimum,
        "one pass recycled {recycled}, expected >= {minimum} (> old CAP={OLD_RECYCLE_CAP}); decommits={decommits}"
    );

    let pooled = core.dbg_pooled_count();
    assert_eq!(core.dbg_drain_small_pool(), pooled);
    assert_eq!(core.dbg_pooled_count(), 0);
    let after_pool_drain = survivors
        .values()
        .filter(|&&ptr| core.dbg_live_count_for(ptr).is_none())
        .count();
    assert!(
        after_pool_drain >= TARGET_SEGMENTS - 2,
        "a drained pool retained a segment slot"
    );

    let mut second_segments = BTreeMap::new();
    let mut second_wave = Vec::with_capacity(MAX_BLOCKS);
    for index in 0..MAX_BLOCKS {
        let ptr = allocate(&mut core, layout, index, "reuse wave");
        second_segments
            .entry(ptr.addr() & !(SegmentLayout::SEGMENT - 1))
            .or_insert(ptr);
        second_wave.push(ptr);
        assert_eq!(
            core.dbg_table_count(),
            high_water,
            "recycled slots must be reused before extending high-water"
        );
        if second_segments.len() == TARGET_SEGMENTS {
            break;
        }
    }
    assert_eq!(
        second_segments.len(),
        TARGET_SEGMENTS,
        "reuse wave did not span the same number of segments"
    );
    let second_wave_blocks = second_wave.len();
    for ptr in second_wave {
        // SAFETY: each second-wave allocation is live and freed once with its original Layout.
        unsafe { core.dealloc(ptr, layout) };
    }
    core.dbg_drain_small_pool();
    eprintln!(
        "C3 result: first_wave_blocks={first_wave_blocks} second_wave_blocks={second_wave_blocks} first_wave_recycled={recycled} after_pool_drain={after_pool_drain} high_water={high_water}/{} reserve_delta={} release_delta={} constructor_failure_delta={}",
        AllocCore::dbg_max_segments(),
        AllocCore::dbg_segments_reserved_total() - reserved_before,
        AllocCore::dbg_segments_released_total() - released_before,
        AllocCore::dbg_segments_reserve_failed_total() - failed_before,
    );
    drop(core);
    assert_eq!(
        AllocCore::dbg_segments_reserved_total() - reserved_before,
        AllocCore::dbg_segments_released_total() - released_before,
        "test teardown must release every successfully handed-up segment reservation"
    );
}
