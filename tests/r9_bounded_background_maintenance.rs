#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use core::sync::atomic::{AtomicU64, Ordering};
use sefer_alloc::global::MaintenanceService;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

const BLOCK: usize = 128 * 1024;
const WORD_BYTES: usize = 1024;

fn word(ptr: *mut u8) -> usize {
    (ptr.addr() & (SegmentLayout::SEGMENT - 1)) / WORD_BYTES
}

fn allocated(core: &mut AllocCore, count: usize) -> Vec<*mut u8> {
    let layout = Layout::from_size_align(BLOCK, 16).unwrap();
    assert!(SegmentLayout::class_for(BLOCK, 16).is_some());
    let pointers: Vec<_> = (0..count).map(|_| core.alloc(layout)).collect();
    assert!(pointers.iter().all(|ptr| !ptr.is_null()));
    assert!(pointers.iter().all(|ptr| {
        SegmentLayout::segment_base_of(ptr.addr())
            == SegmentLayout::segment_base_of(pointers[0].addr())
    }));
    pointers
}

#[test]
fn idle_work_is_budgeted_and_strict_drain_stays_full() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let pointers = allocated(&mut core, 20);
    assert!(pointers.iter().any(|ptr| word(*ptr) > 3_800));
    let mut cursor = (0, 0);
    for expected in 1..=5 {
        assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (0, 1));
        assert_eq!(cursor, (0, expected));
    }
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 64), (0, 64));
    assert_eq!(cursor, (0, 69));

    let victim = pointers[1];
    let before = core.dbg_live_count_for(victim).unwrap();
    // SAFETY: victim is one current issued allocation, transferred once.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    assert!(!core.dbg_is_free_for(victim));
    assert_eq!(core.dbg_drain_sidecar_ingress(), 1);
    assert!(core.dbg_is_free_for(victim));
    assert_eq!(core.dbg_live_count_for(pointers[0]), Some(before - 1));
}

#[test]
fn one_unit_cursor_retires_early_and_late_words_once() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let pointers = allocated(&mut core, 12);
    let (early, late) = (pointers[11], pointers[1]);
    let (early_word, late_word) = (word(early), word(late));
    assert!(early_word < late_word);
    let before = core.dbg_live_count_for(pointers[0]).unwrap();
    // SAFETY: each pointer is a distinct current issued allocation.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(early) });
    // SAFETY: late is a different current issued allocation.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(late) });
    let mut cursor = (0, 0);
    for index in 0..=late_word {
        let (retired, units) = core.dbg_bounded_sidecar_step(&mut cursor, 1);
        assert_eq!(units, 1);
        assert_eq!(
            retired,
            usize::from(index == early_word || index == late_word)
        );
        assert_eq!(core.dbg_is_free_for(early), index >= early_word);
        assert_eq!(core.dbg_is_free_for(late), index >= late_word);
    }
    assert_eq!(core.dbg_live_count_for(pointers[0]), Some(before - 2));
    assert!(!core.dbg_is_free_for(pointers[0]));
    assert_eq!(core.dbg_drain_sidecar_ingress(), 0);
}

#[test]
fn heap_worker_hook_uses_persistent_cursor_across_visits() {
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: claim grants this thread exclusive ownership through recycle.
    let heap = unsafe { &mut *heap_ptr };
    let layout = Layout::from_size_align(BLOCK, 16).unwrap();
    let sentinel = heap.alloc(layout);
    let victim = heap.alloc(layout);
    assert!(!sentinel.is_null() && !victim.is_null());
    // SAFETY: victim is a unique current issue and is not used after transfer.
    assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
    assert_eq!(heap.dbg_background_maintenance_step(1).1, 1);
    assert_eq!(heap.dbg_background_cursor(), (0, 1));
    assert!(!heap.dbg_is_free_for(victim));
    let mut visits = 1;
    while !heap.dbg_is_free_for(victim) {
        assert_eq!(heap.dbg_background_maintenance_step(1).1, 1);
        visits += 1;
        assert!(visits <= word(victim) + 2);
    }
    assert!(!heap.dbg_is_free_for(sentinel));
    // SAFETY: sentinel is the remaining unique issue; recycle retains none.
    unsafe {
        heap.dealloc(sentinel, layout);
        HeapRegistry::recycle(heap_ptr);
    }
}

#[test]
fn late_publication_after_idle_round_is_seen_without_a_hint() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let pointers = allocated(&mut core, 12);
    let victim = pointers[11];
    let mut cursor = (0, 0);
    let mut visits = 0;
    loop {
        let (retired, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert_eq!(retired, 0);
        assert!(units <= 64);
        visits += 1;
        if cursor == (0, 0) {
            break;
        }
        assert!(visits <= 65);
    }
    let before = core.dbg_live_count_for(pointers[0]).unwrap();
    // SAFETY: victim remains one live issue after the idle round.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    let mut retired = 0;
    for _ in 0..=65 {
        let (n, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert!(units <= 64);
        retired += n;
        if retired != 0 {
            break;
        }
    }
    assert_eq!(retired, 1);
    assert!(core.dbg_is_free_for(victim));
    assert_eq!(core.dbg_live_count_for(pointers[0]), Some(before - 1));
}

#[test]
fn null_slot_and_reused_large_route_charge_one_unit_each() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(2 * 1024 * 1024, 16).unwrap();
    let first = core.alloc(layout);
    assert!(!first.is_null());
    let index = core.dbg_segment_id_of(first) as usize;
    assert!(index > 0);
    let old = RouteDirectory::global().lookup(first).unwrap();
    let incarnation = old.incarnation();
    // SAFETY: first is a current unique Large issue, transferred once.
    assert!(unsafe { old.publish_large() });
    let mut cursor = (index, 0);
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (1, 1));
    assert!(RouteDirectory::global().lookup(first).is_none());
    cursor = (index, 0);
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (0, 1));

    let second = core.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(core.dbg_segment_id_of(second) as usize, index);
    let next = RouteDirectory::global().lookup(second).unwrap();
    assert_ne!(next.incarnation(), incarnation);
    cursor = (index, 3_999);
    // SAFETY: second is a fresh unique issue, not the retired first instance.
    assert!(unsafe { next.publish_large() });
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (1, 1));
    assert!(RouteDirectory::global().lookup(second).is_none());
}

#[test]
fn reused_table_index_with_smaller_small_high_water_skips_stale_word() {
    let small = Layout::from_size_align(32 * 1024, 16).unwrap();
    assert!(SegmentLayout::class_for(small.size(), small.align()).is_some());
    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut core = AllocCore::dbg_new_routed_with_config_for_test(config).unwrap();
    let mut still_live = Vec::new();
    let mut old_segment = Vec::new();
    let mut reached_second = false;
    for _ in 0..=300 {
        let ptr = core.alloc(small);
        assert!(!ptr.is_null());
        let id = core.dbg_segment_id_of(ptr) as usize;
        if id == 1 {
            old_segment.push(ptr);
        } else {
            still_live.push(ptr);
        }
        if id == 2 {
            reached_second = true;
            break;
        }
    }
    assert!(reached_second && !old_segment.is_empty());
    let old_incarnation = RouteDirectory::global()
        .lookup(old_segment[0])
        .unwrap()
        .incarnation();
    let mut cursor = (0, 0);
    for _ in 0..=200 {
        let (_, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert!(units <= 64);
        if cursor.0 == 1 && cursor.1 >= 1_500 {
            break;
        }
    }
    assert_eq!(cursor.0, 1);
    assert!(cursor.1 >= 1_500);

    // SAFETY: these are all distinct current issues in the old Small segment.
    unsafe {
        for &ptr in &old_segment {
            core.dealloc(ptr, small);
        }
    }
    assert!(RouteDirectory::global().lookup(old_segment[0]).is_none());
    let new_ptr = loop {
        let ptr = core.alloc(small);
        assert!(!ptr.is_null());
        let id = core.dbg_segment_id_of(ptr) as usize;
        still_live.push(ptr);
        if id == 1 {
            break ptr;
        }
        assert!(still_live.len() <= 450, "recycled slot was not reused");
    };
    assert_eq!(cursor.0, 1);
    let new_pin = RouteDirectory::global().lookup(new_ptr).unwrap();
    assert_ne!(new_pin.incarnation(), old_incarnation);
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (0, 1));
    assert_eq!(cursor, (2, 0));
    // SAFETY: new_ptr is one current issued Small allocation, transferred once.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(new_ptr) });
    let mut retired = 0;
    for _ in 0..=200 {
        let (n, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert!(units <= 64);
        retired += n;
        if retired != 0 {
            break;
        }
    }
    assert_eq!(retired, 1);
    assert!(core.dbg_is_free_for(new_ptr));
    // SAFETY: all remaining pointers are distinct current issued instances.
    unsafe {
        for ptr in still_live {
            if ptr != new_ptr {
                core.dealloc(ptr, small);
            }
        }
    }
}

#[test]
fn growing_high_water_after_a_round_is_visited_next_round() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(32 * 1024, 16).unwrap();
    let mut issued: Vec<_> = (0..31).map(|_| core.alloc(layout)).collect();
    assert!(issued.iter().all(|ptr| !ptr.is_null()));
    let old_max = issued.iter().map(|ptr| word(*ptr)).max().unwrap();
    let mut cursor = (0, 0);
    for _ in 0..=65 {
        let (_, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert!(units <= 64);
        if cursor == (0, 0) {
            break;
        }
    }
    assert_eq!(cursor, (0, 0));
    let more: Vec<_> = (0..31).map(|_| core.alloc(layout)).collect();
    assert!(more.iter().all(|ptr| !ptr.is_null()));
    let grown = more
        .iter()
        .copied()
        .find(|ptr| word(*ptr) > old_max)
        .expect("a second real refill must grow the high-water");
    issued.extend(more);
    // SAFETY: grown is a current issued block, transferred exactly once.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(grown) });
    let mut retired = 0;
    for _ in 0..=65 {
        let (n, units) = core.dbg_bounded_sidecar_step(&mut cursor, 64);
        assert!(units <= 64);
        retired += n;
        if retired != 0 {
            break;
        }
    }
    assert_eq!(retired, 1);
    assert!(core.dbg_is_free_for(grown));
    // SAFETY: all other pointers remain distinct current issued blocks.
    unsafe {
        for ptr in issued {
            if ptr != grown {
                core.dealloc(ptr, layout);
            }
        }
    }
}

#[test]
fn busy_fallback_skips_then_keeps_its_numeric_progress() {
    let layout = Layout::from_size_align(BLOCK, 16).unwrap();
    let victim = HeapCore::dbg_with_fallback_for_test(|heap| {
        let sentinel = heap.alloc(layout);
        let victim = heap.alloc(layout);
        assert!(!sentinel.is_null() && !victim.is_null());
        assert_eq!(heap.dbg_background_maintenance_step(1).1, 1);
        let position = heap.dbg_background_cursor();
        assert_eq!(MaintenanceService::try_fallback_step_for_test(1), None);
        assert_eq!(heap.dbg_background_cursor(), position);
        // SAFETY: victim is a current issue transferred once, while the
        // fallback lock prevents any concurrent owner access.
        assert!(unsafe { heap.dbg_publish_small_sidecar_free(victim) });
        (sentinel.expose_provenance(), victim.expose_provenance())
    })
    .expect("fallback initialized");
    let mut seen = false;
    for _ in 0..=4_096 {
        let Some((retired, units)) = MaintenanceService::try_fallback_step_for_test(1) else {
            panic!("uncontended fallback visit failed");
        };
        assert_eq!(units, 1);
        if retired == 1 {
            seen = true;
            break;
        }
    }
    assert!(seen);
    HeapCore::dbg_with_fallback_for_test(|heap| {
        // SAFETY: these addresses came from live fallback issues kept by the
        // sentinel credit; no deallocation or reissue intervened.
        let sentinel = core::ptr::with_exposed_provenance_mut(victim.0);
        let freed = core::ptr::with_exposed_provenance_mut(victim.1);
        assert!(heap.dbg_is_free_for(freed));
        assert!(!heap.dbg_is_free_for(sentinel));
        // SAFETY: sentinel is the remaining unique issue.
        unsafe { heap.dealloc(sentinel, layout) };
    });
}

#[test]
fn negative_control_preterminal_hint_loses_the_late_publication() {
    let hint = AtomicU64::new(0);
    let bitmap = AtomicU64::new(0);
    hint.fetch_or(1, Ordering::AcqRel);
    assert_eq!(hint.swap(0, Ordering::AcqRel), 1);
    assert_eq!(bitmap.swap(0, Ordering::AcqRel), 0);
    bitmap.fetch_or(1, Ordering::AcqRel);
    assert_eq!(hint.load(Ordering::Acquire), 0);
    assert_eq!(bitmap.load(Ordering::Acquire), 1);
    assert_eq!(bitmap.swap(0, Ordering::AcqRel), 1);
}

#[test]
fn one_real_small_word_cut_retires_once() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(16, 16).unwrap();
    let sentinel = core.alloc(layout);
    let victim = core.alloc(layout);
    assert!(!sentinel.is_null() && !victim.is_null());
    let before = core.dbg_live_count_for(sentinel).unwrap();
    let mut cursor = (0, word(victim));
    // SAFETY: victim is one current issue, transferred exactly once.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    assert_eq!(core.dbg_bounded_sidecar_step(&mut cursor, 1), (1, 1));
    assert_eq!(core.dbg_live_count_for(sentinel), Some(before - 1));
    assert!(core.dbg_is_free_for(victim));
    // SAFETY: sentinel is the remaining unique issue.
    unsafe { core.dealloc(sentinel, layout) };
}
