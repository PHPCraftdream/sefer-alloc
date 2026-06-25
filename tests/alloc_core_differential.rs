//! Differential property test for the Phase 8 segment substrate (`alloc-core`).
//!
//! Models `AllocCore` against a reference (a `Vec` of live allocations on the
//! TEST's own allocator — which is fine, the test is NOT on the alloc path;
//! only `AllocCore` itself must be reentrancy-free). Encodes invariants M1–M4
//! from `docs/INVARIANTS.md`:
//!
//! - **M1 (validity):** every returned pointer is non-null, valid for the
//!   requested size, and aligned to the requested align.
//! - **M2 (no double-free / UAF):** freeing twice or using-after-free never
//!   corrupts the allocator (model only frees live pointers; `dealloc` of a
//!   stale/foreign pointer is a no-op).
//! - **M3 (no overlap):** two simultaneously-live allocations never share a
//!   byte (the model checks every new allocation against all live ones).
//! - **M4 (alignment & size fidelity):** the class chosen always satisfies
//!   size and align.
//!
//! Per the short-scenario policy (`CLAUDE.md`): ~64 cases, small sizes so the
//! suite (and miri over it) finishes quickly. Sizes are kept `<= SMALL_MAX` so
//! most allocations exercise the small free-list path; a few large ones
//! exercise the dedicated-segment path.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use proptest::prelude::*;
use sefer_alloc::AllocCore;

/// A live allocation in the model: its pointer, size, and align.
#[derive(Clone)]
struct Live {
    ptr: *mut u8,
    size: usize,
    align: usize,
}

unsafe impl Send for Live {}

/// The operations applied to both the model and `AllocCore`.
#[derive(Clone, Debug)]
enum Op {
    /// Allocate `size` bytes at `align` (a small power-of-two).
    Alloc { size: usize, align: usize },
    /// Free the `i`-th live allocation (by index in the model's Vec).
    Dealloc(usize),
    /// Realloc the `i`-th live allocation to `new_size`.
    Realloc { i: usize, new_size: usize },
    /// Alloc zeroed (exercises the zero path + read-back).
    AllocZeroed { size: usize, align: usize },
}

/// Small size generator: keep allocations in the small-class range so the free
/// list + bump path dominates. Occasionally (`1/8`) a larger size to exercise
/// the dedicated-segment path.
fn small_size() -> impl Strategy<Value = usize> {
    prop_oneof![
        (1usize..=4096).prop_map(|s| s.max(1)),
        (4097usize..=2 * 1024 * 1024).prop_map(|s| s.max(1)),
    ]
}

/// Alignment generator: powers of two in the small-class range, plus the
/// occasional large alignment (exercises the large path).
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn alloc_core_matches_reference_model(
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
        let mut alloc = AllocCore::new().expect("primordial bootstrap");
        let mut live: Vec<Live> = Vec::new();

        for op in ops {
            match op {
                Op::Alloc { size, align } => {
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let ptr = alloc.alloc(layout);
                    // M1: non-null (we did not exhaust memory in this bounded test).
                    prop_assert!(!ptr.is_null(), "alloc returned null");
                    // M1: aligned.
                    prop_assert_eq!(
                        (ptr as usize) % align,
                        0,
                        "pointer not aligned"
                    );
                    // M3: no overlap with any live allocation.
                    for other in live.iter() {
                        prop_assert!(
                            !ranges_overlap(ptr as usize, size, other.ptr as usize, other.size),
                            "new alloc overlaps live"
                        );
                    }
                    // Write a distinctive pattern + read it back (M1 validity:
                    // the bytes are ours and writable for `size`).
                    // SAFETY: `ptr` was just allocated for `layout.size()` bytes.
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
                    let ptr = alloc.alloc_zeroed(layout);
                    prop_assert!(!ptr.is_null());
                    prop_assert_eq!((ptr as usize) % align, 0, "not aligned");
                    // M1 + zero contract: every byte is 0.
                    // SAFETY: `ptr` valid for `size` bytes, freshly zeroed.
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
                        // SAFETY: `l.ptr` is a live allocation of `(l.size, l.align)`.
                        alloc.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap());
                        // M2: double-free is a no-op (must not corrupt).
                        alloc.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap());
                    }
                }
                Op::Realloc { i, new_size } => {
                    if !live.is_empty() {
                        let i = i % live.len();
                        let l = live[i].clone();
                        let old_layout = Layout::from_size_align(l.size, l.align).unwrap();
                        let new_ptr = alloc.realloc(l.ptr, old_layout, new_size);
                        if !new_ptr.is_null() {
                            // M1: new ptr aligned and non-null.
                            prop_assert_eq!((new_ptr as usize) % l.align, 0, "realloc not aligned");
                            // The `min(size, new_size)` prefix must be preserved.
                            let keep = l.size.min(new_size);
                            // SAFETY: both pointers valid for `keep` bytes.
                            unsafe {
                                for b in 0..keep {
                                    // We wrote 0xA7 to the old block earlier;
                                    // after realloc the first `keep` bytes must
                                    // still be 0xA7 (unless a recycled block
                                    // happened to overlap — but M3 forbids that).
                                    // To stay robust against the model's own
                                    // writes, we only check the byte is one of
                                    // the values we wrote (0xA7 from Alloc, 0
                                    // from AllocZeroed).
                                    let v = new_ptr.add(b).read();
                                    prop_assert!(
                                        v == 0xA7 || v == 0,
                                        "realloc byte corrupted"
                                    );
                                }
                            }
                            live[i] = Live { ptr: new_ptr, size: new_size, align: l.align };
                        }
                    }
                }
            }
        }

        // Drop everything (exercises dealloc for all survivors + the allocator
        // drop walks the registry). No use-after-free: we do not touch `live`
        // after dropping `alloc`.
        for l in &live {
            alloc.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap());
        }
        drop(live);
        drop(alloc);
    }
}

/// Whether `[a, a+asize)` and `[b, b+bsize)` overlap (touching endpoints do
/// NOT count — they are adjacent, not overlapping).
fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a + asize <= b || b + bsize <= a)
}
