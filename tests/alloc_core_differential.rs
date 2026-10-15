//! Differential property test for the Phase 8 segment substrate (`alloc-core`).
//!
//! Models `AllocCore` against the reference model + M1–M4 oracles that now live
//! in the `globalalloc-model` crate (the single shared harness for all three
//! in-repo differential copies — this test, `tests/heap_differential.rs`, and
//! `fuzz/fuzz_targets/global_alloc_ops.rs`). See `docs/INVARIANTS.md` M1–M4 and
//! the crate's docs for the oracle definitions. This file keeps ONLY the
//! sefer-specific wiring: the `AllocCore`-under-test adapter and the size
//! distribution / case count.
//!
//! The random fixture has at most 63 operations, sizes <=128 KiB and a 9:1
//! small/large mix: logical live bytes are bounded by 7.875 MiB. Even giving
//! every allocation/realloc a new production 4-MiB span bounds cumulative spans by
//! 256 MiB (512 MiB raw VA), including primordial. M1 still rejects NULL.
//! Persisted native failures keep their original 2-MiB/199-op generator in a
//! separate deterministic replay. The unsafe allocator contract permits each
//! issued allocation to be freed once.
//! Native coverage observes actual Large allocations from bounded streams and
//! an explicit two-block witness above the configured Small ceiling.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use std::alloc::Layout;
#[cfg(feature = "internals")]
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::Mutex;

#[cfg(not(miri))]
use globalalloc_model::Op;
use globalalloc_model::{drive, op_strategy, Config, RawAllocator};
use proptest::prelude::*;
#[cfg(not(miri))]
use proptest::strategy::ValueTree;
#[cfg(not(miri))]
use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};
use sefer_alloc::AllocCore;

/// `segment_bases` exists only under `alloc-global`/`alloc-xthread`; the
/// diagnostic line must still compile under plain `alloc-core internals`.
#[cfg(feature = "internals")]
fn live_segments(core: &AllocCore) -> String {
    #[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
    return core.segment_bases().count().to_string();
    #[cfg(not(any(feature = "alloc-global", feature = "alloc-xthread")))]
    return "n/a".to_string();
}

/// The allocator-under-test adapter: `AllocCore`'s methods take `&mut self`, so
/// wrap it in a `RefCell` to present the shared harness's `&self` `RawAllocator`
/// surface. Single-threaded, no reentrancy — the borrow never overlaps.
struct CoreUnderTest {
    core: RefCell<AllocCore>,
    #[cfg(feature = "internals")]
    large_allocations: Cell<usize>,
}
// Each case owns a fresh core; the lock isolates process-wide counter windows.
static TEST_LOCK: Mutex<()> = Mutex::new(());

impl CoreUnderTest {
    fn new() -> Self {
        Self {
            core: RefCell::new(AllocCore::new().expect("primordial bootstrap")),
            #[cfg(feature = "internals")]
            large_allocations: Cell::new(0),
        }
    }

    fn allocate(&self, layout: Layout, zeroed: bool) -> *mut u8 {
        let mut core = self.core.borrow_mut();
        #[cfg(feature = "internals")]
        let before = (
            AllocCore::dbg_segments_reserved_total(),
            AllocCore::dbg_segments_released_total(),
            AllocCore::dbg_segments_reserve_failed_total(),
        );
        let ptr = if zeroed {
            core.alloc_zeroed(layout)
        } else {
            core.alloc(layout)
        };
        #[cfg(feature = "internals")]
        if !ptr.is_null() && core.dbg_kind_at_tag(ptr) == 2 {
            self.large_allocations.set(self.large_allocations.get() + 1);
        }
        #[cfg(feature = "internals")]
        if ptr.is_null() {
            eprintln!(
                "NULL size={} align={} slots={}/{} live_segments={} reserve_delta={} release_delta={} constructor_failure_delta={}",
                layout.size(), layout.align(), core.dbg_table_count(),
                AllocCore::dbg_max_segments(), live_segments(&core),
                AllocCore::dbg_segments_reserved_total() - before.0,
                AllocCore::dbg_segments_released_total() - before.1,
                AllocCore::dbg_segments_reserve_failed_total() - before.2,
            );
            #[cfg(feature = "alloc-decommit")]
            eprintln!(
                "NULL retained: large_cache_bytes={} pool_segments={}",
                core.dbg_large_cache_used(),
                core.dbg_pooled_count()
            );
        }
        ptr
    }
}

// SAFETY: each method takes `&mut` on the inner `AllocCore` for the duration of
// one non-reentrant call and forwards to the matching inherent method, honoring
// the `RawAllocator` contract (valid/aligned pointers, unique frees and realloc
// prefix preservation). The model never asks this adapter to double-free.
unsafe impl RawAllocator for CoreUnderTest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout, false)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout, true)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding the caller's `dealloc` contract to `AllocCore`.
        unsafe { self.core.borrow_mut().dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding the caller's `realloc` contract to `AllocCore`.
        unsafe { self.core.borrow_mut().realloc(ptr, old_layout, new_size) }
    }
}

#[cfg(not(miri))]
#[test]
fn replay_native_failure_seed_without_shrinking() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let seeds: Vec<_> = include_str!("support/alloc_core_differential_native_seeds.txt")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split_whitespace();
            assert_eq!(
                words.next(),
                Some("cc"),
                "persisted fixture must use ChaCha seeds"
            );
            words.next().expect("persisted seed")
        })
        .collect();
    assert_eq!(
        seeds.len(),
        6,
        "historical fixture must preserve all six seeds"
    );
    assert!(seeds.contains(&"956defdc76c19f87ac2c556b81792f810287db9f43752f4887bc30a1619a50ce"));
    // Replay the latest failure first; each older seed retains its old shape too.
    for label in seeds.into_iter().rev() {
        assert_eq!(label.len(), 64);
        let mut seed = [0; 32];
        for (index, byte) in seed.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&label[index * 2..index * 2 + 2], 16).expect("seed byte");
        }
        replay_native_seed(&seed, label);
    }
}

#[cfg(not(miri))]
fn replay_native_seed(seed: &[u8; 32], label: &str) {
    let mut runner = TestRunner::new_with_rng(
        ProptestConfig {
            failure_persistence: None,
            max_shrink_iters: 0,
            ..ProptestConfig::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, seed),
    );
    let historical = Config {
        large_max: 2 * 1024 * 1024,
        small_weight: 1,
        large_weight: 1,
        ..config()
    };
    let ops = op_strategy(historical, 0..200)
        .new_tree(&mut runner)
        .expect("seeded stream")
        .current();
    if label == "956defdc76c19f87ac2c556b81792f810287db9f43752f4887bc30a1619a50ce" {
        assert_eq!(
            ops[122],
            Op::Alloc {
                size: 2_090_841,
                align: 128
            }
        );
    }
    let creators = ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                Op::Alloc { .. } | Op::AllocZeroed { .. } | Op::Realloc { .. }
            )
        })
        .count();
    let requested: usize = ops
        .iter()
        .map(|op| match op {
            Op::Alloc { size, .. } | Op::AllocZeroed { size, .. } => *size,
            Op::Realloc { new_size, .. } => *new_size,
            Op::Dealloc(_) => 0,
        })
        .sum();
    eprintln!(
        "seed replay {label}: ops={} logical_peak_bound={} cumulative_span_bound={} raw_va_bound={}",
        ops.len(),
        requested,
        (creators + 1) * sefer_alloc::SegmentLayout::SEGMENT,
        (creators + 1) * 2 * sefer_alloc::SegmentLayout::SEGMENT
    );
    #[cfg(feature = "internals")]
    let before = (
        AllocCore::dbg_segments_reserved_total(),
        AllocCore::dbg_segments_released_total(),
        AllocCore::dbg_segments_reserve_failed_total(),
    );
    let alloc = CoreUnderTest::new();
    drive(&alloc, historical, &ops);
    #[cfg(feature = "internals")]
    {
        let core = alloc.core.borrow();
        eprintln!("replay end: slots={}/{} live_segments={} reserve_delta={} release_delta={} constructor_failure_delta={}",
            core.dbg_table_count(), AllocCore::dbg_max_segments(), live_segments(&core),
            AllocCore::dbg_segments_reserved_total() - before.0,
            AllocCore::dbg_segments_released_total() - before.1,
            AllocCore::dbg_segments_reserve_failed_total() - before.2);
        #[cfg(feature = "alloc-decommit")]
        eprintln!(
            "replay retained: large_cache_bytes={} pool_segments={}",
            core.dbg_large_cache_used(),
            core.dbg_pooled_count()
        );
    }
    drop(alloc);
    #[cfg(feature = "internals")]
    {
        let reserved = AllocCore::dbg_segments_reserved_total() - before.0;
        let released = AllocCore::dbg_segments_released_total() - before.1;
        eprintln!("replay teardown: reserved={reserved} released={released}");
        assert_eq!(
            reserved, released,
            "replay teardown must release every successfully handed-up reservation"
        );
    }
}

#[cfg(all(not(miri), feature = "internals"))]
#[test]
fn bounded_streams_and_small_witness_exercise_large_allocations() {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reserved_before = AllocCore::dbg_segments_reserved_total();
    let released_before = AllocCore::dbg_segments_released_total();
    let mut runner = TestRunner::new_with_rng(
        ProptestConfig {
            failure_persistence: None,
            max_shrink_iters: 0,
            ..ProptestConfig::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, &[0x42; 32]),
    );
    let strategy = op_strategy(config(), 0..64);
    let mut random_large = 0;
    for _ in 0..16 {
        let ops = strategy
            .new_tree(&mut runner)
            .expect("bounded coverage stream")
            .current();
        let alloc = CoreUnderTest::new();
        drive(&alloc, config(), &ops);
        random_large += alloc.large_allocations.get();
    }
    // Alignment fallbacks reach Large even below SMALL_MAX without medium classes.
    #[cfg(not(feature = "medium-classes"))]
    assert!(
        random_large > 0,
        "bounded random streams never entered an actual Large segment"
    );

    let size = sefer_alloc::SegmentLayout::SMALL_MAX + sefer_alloc::SegmentLayout::PAGE;
    assert!(
        size <= 2 * 1024 * 1024,
        "explicit Large witness exceeded its resource bound"
    );
    let ops = [
        Op::Alloc {
            size,
            align: sefer_alloc::SegmentLayout::PAGE,
        },
        Op::AllocZeroed {
            size,
            align: sefer_alloc::SegmentLayout::PAGE,
        },
        Op::Dealloc(0),
        Op::Dealloc(0),
    ];
    let alloc = CoreUnderTest::new();
    drive(&alloc, config(), &ops);
    assert_eq!(
        alloc.large_allocations.get(),
        2,
        "explicit witness must allocate two actual Large blocks"
    );
    eprintln!(
        "Large coverage: bounded_random={random_large} explicit=2 explicit_live_bytes={}",
        2 * size
    );
    drop(alloc);
    assert_eq!(
        AllocCore::dbg_segments_reserved_total() - reserved_before,
        AllocCore::dbg_segments_released_total() - released_before,
        "coverage teardown must release every successfully handed-up reservation"
    );
}

/// Bounded random fixture; the dedicated replay preserves the historical seed.
fn config() -> Config {
    Config {
        small_max: 4096,
        large_max: 128 * 1024,
        small_weight: 9,
        large_weight: 1,
        max_align: 4096,
        double_free: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, max_shrink_iters: 64, ..ProptestConfig::default() })]
    #[test]
    fn alloc_core_matches_reference_model(ops in op_strategy(config(), 0..64)) {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let alloc = CoreUnderTest::new();
        drive(&alloc, config(), &ops);
    }
}
