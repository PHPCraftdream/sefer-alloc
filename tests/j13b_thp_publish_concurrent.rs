//! J13b C2: eight threads publish eight distinct tuples through heap
//! materialisation at once; exactly one wins whole and the other seven each
//! count one conflict. One test: process-global.
#![cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals",
    not(feature = "numa-aware")
))]

use std::sync::{Arc, Barrier};
use std::thread;

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpTuple;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, LargeThpGradient, LargeThpMode};

const MIB: usize = 1024 * 1024;
const N: usize = 8;

fn candidate(i: usize) -> LargeCacheConfig {
    let gradients = [
        LargeThpGradient::CAUTIOUS,
        LargeThpGradient::EAGER,
        LargeThpGradient::STEADY,
    ];
    LargeCacheConfig::new()
        .huge_page_mode(if i & 1 == 0 {
            LargeThpMode::Always
        } else {
            LargeThpMode::Auto
        })
        .huge_page_block_min_bytes((i + 1) * MIB)
        .huge_page_budget_bytes(i * 64 * MIB)
        .huge_page_gradient(gradients[i % 3])
}

#[test]
fn one_whole_tuple_wins_and_seven_conflict() {
    let tuples: Vec<ThpTuple> = (0..N)
        .map(|i| candidate(i).dbg_resolved_thp_tuple())
        .collect();
    for i in 0..N {
        for j in (i + 1)..N {
            assert_ne!(tuples[i], tuples[j], "candidates {i} and {j} must differ");
        }
    }
    assert_eq!(AllocCore::dbg_thp_tuple(), ThpTuple::DEFAULT);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);

    let barrier = Arc::new(Barrier::new(N));
    let handles: Vec<_> = (0..N)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let cfg = candidate(i);
                barrier.wait();
                let core = AllocCore::new_with_config(cfg).expect("primordial");
                drop(core);
            })
        })
        .collect();
    for h in handles {
        h.join().expect("publisher thread");
    }

    let won = AllocCore::dbg_thp_tuple();
    let winners = tuples.iter().filter(|t| **t == won).count();
    assert_eq!(winners, 1, "readback is exactly one candidate, never a mix");
    assert_ne!(won, ThpTuple::DEFAULT);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), (N - 1) as u64);
}
