#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
//! B1-a: `flush_run` finds the run end in its own pass and retires
//! `run length − rejected` credits once. A same-segment batch holding a
//! rejected block (already FREE through the sidecar) and a null must retire
//! exactly the accepted blocks, and the freelist must hold each of them once.
//! Counterfactuals: credit the whole run (ignore `rejected`) → live drops one
//! too many; let the null not split the run → the block after it is lost or
//! linked twice.

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::{AllocCore, SegmentLayout};

fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

#[test]
fn run_with_rejected_block_and_null_retires_exactly_the_accepted() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let c = core.dbg_layout_class_for(layout).expect("small class");

    const N: usize = 8;
    let mut mag = vec![core::ptr::null_mut::<u8>(); N];
    assert_eq!(core.refill_class(c, N, &mut mag), N);
    let base = seg_base(mag[0]);
    assert!(
        mag.iter().all(|&p| seg_base(p) == base),
        "precondition: one segment, so the batch is one run split only by the null"
    );

    // mag[2] becomes FREE through the sidecar while still in the batch.
    let rejected = mag[2];
    // SAFETY (R6-MS-4): `rejected` is a live allocation owned by `core`; this
    // is its single logical remote free, reclaimed by the drain below.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(rejected) });
    core.dbg_drain_sidecar_ingress();

    // mag[5] stays live: a null in its place splits the run.
    let kept = mag[5];
    mag[5] = core::ptr::null_mut();

    let live_before = core.dbg_live_count_for(mag[0]).expect("segment live count");
    // SAFETY (R6-MS-3): every non-null entry except `rejected` is a live
    // class-`c` allocation of `core` freed exactly once here; `rejected` is
    // already FREE and must be skipped by the per-block state-byte guard.
    unsafe { core.flush_class(c, &mag) };
    let accepted = N - 2;
    assert_eq!(
        core.dbg_live_count_for(kept),
        Some(live_before - accepted as u32),
        "flush must retire exactly the {accepted} accepted blocks"
    );

    // Every flushed block and `rejected` are on the freelist exactly once.
    let mut out = vec![core::ptr::null_mut::<u8>(); N + 8];
    let n = core.refill_class(c, N + 8, &mut out);
    let got: Vec<usize> = out[..n].iter().map(|&p| p as usize).collect();
    let unique: HashSet<usize> = got.iter().copied().collect();
    assert_eq!(unique.len(), n, "refill returned a duplicate");
    for &p in mag.iter().filter(|p| !p.is_null()) {
        assert!(
            unique.contains(&(p as usize)),
            "flushed block lost from the freelist"
        );
    }
    assert!(
        !unique.contains(&(kept as usize)),
        "the live block must not be free"
    );

    for &p in &out[..n] {
        // SAFETY (R6-MS-1/2): returned by the refill above, live, freed once.
        unsafe { core.dealloc(p, layout) };
    }
    // SAFETY (R6-MS-1/2): `kept` is live and freed once.
    unsafe { core.dealloc(kept, layout) };
}
