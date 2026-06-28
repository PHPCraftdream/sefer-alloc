//! Deterministic churn regression test for the Phase 9 per-thread heap.
//!
//! Complements the randomised `heap_differential.rs` proptest with a fast,
//! fully deterministic (LCG-driven) alloc/dealloc/realloc churn that verifies
//! the byte pattern of EVERY live block after EVERY operation. Each live entry
//! carries a unique fill byte, so any overlap, free-list aliasing, or lost-data
//! bug surfaces immediately as a mismatch — and reproducibly, run to run.
//!
//! This guards the free-list reuse path: blocks handed back from the per-heap
//! free list hold stale bytes (unlike fresh OS-zeroed pages), so a realloc that
//! mis-copies, or an alloc that returns an aliased/overlapping block, corrupts
//! a tracked fill and trips the check. Kept fast per the short-scenario policy
//! (modest sizes, bounded ops).
#![cfg(feature = "alloc")]

use sefer_alloc::Heap;
use std::alloc::Layout;

struct Live {
    ptr: *mut u8,
    size: usize,
    align: usize,
    fill: u8,
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 16
}

fn verify_all(live: &[Live], label: &str, op: usize) {
    for (k, l) in live.iter().enumerate() {
        // SAFETY: `l.ptr` is a live allocation of `l.size` bytes we own.
        unsafe {
            for b in 0..l.size {
                let v = l.ptr.add(b).read();
                assert_eq!(
                    v, l.fill,
                    "CORRUPT {label} op#{op}: live[{k}] ptr={:p} size={} byte {b}={v:#x} want {:#x}",
                    l.ptr, l.size, l.fill
                );
            }
        }
    }
}

const ALIGNS: [usize; 6] = [1, 2, 4, 8, 16, 4096];

fn run(seed: u64, with_large: bool, ops_n: usize, label: &str) {
    let mut st = seed;
    let mut heap = Heap::new().unwrap();
    let mut live: Vec<Live> = Vec::new();
    let mut next_fill: u8 = 1;
    let bump_fill = |f: &mut u8| {
        let v = *f;
        *f = f.wrapping_add(1);
        if *f == 0 {
            *f = 1;
        }
        v
    };

    for op in 0..ops_n {
        let choice = lcg(&mut st) % 100;
        if choice < 45 {
            let size = if with_large && lcg(&mut st).is_multiple_of(10) {
                1 + (lcg(&mut st) as usize % 200_000)
            } else {
                1 + (lcg(&mut st) as usize % 2048)
            };
            let align = ALIGNS[lcg(&mut st) as usize % ALIGNS.len()];
            let layout = Layout::from_size_align(size, align).unwrap();
            let ptr = heap.alloc(layout);
            assert!(!ptr.is_null(), "{label}: alloc null op#{op}");
            assert_eq!((ptr as usize) % align, 0, "{label}: misaligned op#{op}");
            let fill = bump_fill(&mut next_fill);
            // SAFETY: valid for `size` bytes.
            unsafe {
                for b in 0..size {
                    ptr.add(b).write(fill);
                }
            }
            live.push(Live {
                ptr,
                size,
                align,
                fill,
            });
        } else if choice < 70 && !live.is_empty() {
            let i = lcg(&mut st) as usize % live.len();
            let l = live.swap_remove(i);
            heap.dealloc(l.ptr, Layout::from_size_align(l.size, l.align).unwrap());
        } else if !live.is_empty() {
            let i = lcg(&mut st) as usize % live.len();
            let new_size = 1 + (lcg(&mut st) as usize % 4096);
            let old = Layout::from_size_align(live[i].size, live[i].align).unwrap();
            let np = heap.realloc(live[i].ptr, old, new_size);
            assert!(!np.is_null(), "{label}: realloc null op#{op}");
            assert_eq!(
                (np as usize) % live[i].align,
                0,
                "{label}: realloc misaligned op#{op}"
            );
            let keep = live[i].size.min(new_size);
            let fill = live[i].fill;
            // SAFETY: np valid for new_size; first `keep` bytes preserved from old.
            unsafe {
                for b in 0..keep {
                    let v = np.add(b).read();
                    assert_eq!(
                        v, fill,
                        "{label}: realloc lost byte op#{op} b{b}={v:#x} want {fill:#x}"
                    );
                }
                for b in 0..new_size {
                    np.add(b).write(fill);
                }
            }
            live[i].ptr = np;
            live[i].size = new_size;
        }
        verify_all(&live, label, op);
    }
}

#[test]
fn churn_small_only() {
    run(0x1234_5678_9abc_def0, false, 1500, "small_only");
}

#[test]
fn churn_with_large() {
    run(0x0fed_cba9_8765_4321, true, 600, "with_large");
}
