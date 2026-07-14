//! Differential property test for the Phase 8 segment substrate (`alloc-core`
//! feature). Models `AllocCore` against a reference (a `Vec` of live
//! allocations). Encodes invariants M1--M4 through the substrate layer.
//!
//! Per the short-scenario policy: ~64 cases, small sizes.
//!
//! (An earlier version drove this through the now-removed `Heap` wrapper;
//! `Heap` was a pure pass-through to `AllocCore` on the single-thread `alloc`
//! feature, so this is a faithful 1:1 substitution.)

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use proptest::prelude::*;
use sefer_alloc::AllocCore;

/// A live allocation in the model.
#[derive(Clone)]
struct Live {
    ptr: *mut u8,
    size: usize,
    align: usize,
}

unsafe impl Send for Live {}

/// Operations applied to both the model and `AllocCore`.
#[derive(Clone, Debug)]
enum Op {
    Alloc { size: usize, align: usize },
    Dealloc(usize),
    Realloc { i: usize, new_size: usize },
    AllocZeroed { size: usize, align: usize },
}

fn small_size() -> impl Strategy<Value = usize> {
    // Mostly small (the hot free-list path); occasionally large (> SMALL_MAX,
    // the dedicated-segment path). The large arm is weighted rare and capped at
    // 128 KiB — enough to exercise the large path (SMALL_MAX is ~94 KiB) while
    // keeping the suite fast per the short-scenario policy (no multi-MiB
    // byte-by-byte writes).
    prop_oneof![
        9 => (1usize..=4096).prop_map(|s| s.max(1)),
        1 => (4097usize..=128 * 1024).prop_map(|s| s.max(1)),
    ]
}

fn small_align() -> impl Strategy<Value = usize> {
    prop_oneof![
        Just(1usize),
        Just(2usize),
        Just(4usize),
        Just(8usize),
        Just(16usize),
        Just(4096usize),
    ]
}

fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a + asize <= b || b + bsize <= a)
}

proptest! {
    // `failure_persistence: None` — do not write a regressions file (avoids the
    // "SourceParallel failed to find lib.rs" abort under some run layouts and
    // keeps runs hermetic), matching the Phase 7d / Phase 8 differential tests.
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn heap_matches_reference_model(
        ops in prop::collection::vec(
            prop_oneof![
                (small_size(), small_align()).prop_map(|(s, a)| Op::Alloc { size: s, align: a }),
                any::<usize>().prop_map(Op::Dealloc),
                (any::<usize>(), small_size()).prop_map(|(i, ns)| Op::Realloc { i, new_size: ns }),
                (small_size(), small_align()).prop_map(|(s, a)| Op::AllocZeroed { size: s, align: a }),
            ],
            0..200,
        )
    ) {
        let mut heap = AllocCore::new().expect("heap bootstrap");
        let mut live: Vec<Live> = Vec::new();

        for op in ops {
            match op {
                Op::Alloc { size, align } => {
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let ptr = heap.alloc(layout);
                    prop_assert!(!ptr.is_null(), "alloc returned null");
                    prop_assert_eq!(
                        (ptr as usize) % align, 0,
                        "pointer not aligned"
                    );
                    // M3: no overlap with any live allocation.
                    for other in live.iter() {
                        prop_assert!(
                            !ranges_overlap(ptr as usize, size, other.ptr as usize, other.size),
                            "new alloc overlaps live"
                        );
                    }
                    // Write pattern + read back (M1 validity).
                    unsafe {
                        for b in 0..size {
                            ptr.add(b).write(0xA7);
                        }
                        for b in 0..size {
                            prop_assert_eq!(ptr.add(b).read(), 0xA7, "byte did not read back");
                        }
                    }
                    live.push(Live { ptr, size, align });
                }
                Op::AllocZeroed { size, align } => {
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let ptr = heap.alloc_zeroed(layout);
                    prop_assert!(!ptr.is_null());
                    prop_assert_eq!((ptr as usize) % align, 0);
                    unsafe {
                        for b in 0..size {
                            prop_assert_eq!(ptr.add(b).read(), 0, "byte not zeroed");
                        }
                    }
                    live.push(Live { ptr, size, align });
                }
                Op::Dealloc(i) => {
                    if !live.is_empty() {
                        let i = i % live.len();
                        let l = live.swap_remove(i);
                        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
                        unsafe { heap.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap()) };
                    }
                }
                Op::Realloc { i, new_size } => {
                    if !live.is_empty() {
                        let i = i % live.len();
                        let l = live[i].clone();
                        let old_layout = Layout::from_size_align(l.size, l.align).unwrap();
                        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
                        let new_ptr = unsafe { heap.realloc(l.ptr, old_layout, new_size) };
                        if !new_ptr.is_null() {
                            prop_assert_eq!((new_ptr as usize) % l.align, 0);
                            // M1/realloc: the preserved prefix (the `min(old,new)`
                            // bytes) must equal what was written before. NOTE: a
                            // GROWN realloc leaves the tail `[old..new)`
                            // UNINITIALISED — that is correct allocator behaviour
                            // (realloc does not zero the grown region). When the
                            // new block is reused from the free list, that tail
                            // holds stale bytes, NOT zero. So we (a) check only the
                            // preserved prefix here, then (b) re-establish the
                            // pattern over the FULL new size so a later realloc of
                            // this entry checks against known contents (not the
                            // uninitialised tail). Phase 8's substrate test passed
                            // this check only vacuously, because fresh OS pages are
                            // zeroed; the heap's free-list reuse exposes the real
                            // (correct) semantics.
                            let keep = l.size.min(new_size);
                            unsafe {
                                for b in 0..keep {
                                    let v = new_ptr.add(b).read();
                                    prop_assert!(
                                        v == 0xA7 || v == 0,
                                        "realloc did not preserve the prefix"
                                    );
                                }
                                for b in 0..new_size {
                                    new_ptr.add(b).write(0xA7);
                                }
                            }
                            live[i] = Live { ptr: new_ptr, size: new_size, align: l.align };
                        }
                    }
                }
            }
        }

        // Free all survivors.
        for l in &live {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { heap.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap()) };
        }
        drop(live);
        drop(heap);
    }
}
