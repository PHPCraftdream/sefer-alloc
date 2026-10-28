#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals"
))]
//! U32: `try_carve_batch` issues its contiguous bump run per leaf
//! (`IssueRun::prepare_range` / `commit_range`) instead of per block. The
//! class code must still land on exactly every carved block's start granule,
//! across leaf boundaries and for every Small class, and the prepare fault
//! countdown must advance by exactly one per carved block.
//! Counterfactuals: storing at `first + j` instead of `first + j * stride` →
//! an interior granule is coded (interior publish accepted); a per-leaf block
//! count one too high → the run guard fatals; skipping the same-leaf
//! countdown → the follow-up carve is not refused.

use sefer_alloc::AllocCore;

const LEAF: usize = 4096;

fn leaves_spanned(blocks: &[*mut u8]) -> usize {
    let first = blocks[0].addr() / LEAF;
    let last = blocks[blocks.len() - 1].addr() / LEAF;
    last - first + 1
}

#[test]
fn carved_runs_code_every_block_start_and_no_interior_granule() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let mut multi_leaf_classes = 0;
    for class in 0..AllocCore::dbg_small_class_count() {
        let block = AllocCore::dbg_block_size(class);
        // Three leaves' worth, at least two blocks.
        let want = (3 * LEAF / block + 2).clamp(2, 600);
        let mut out = vec![core::ptr::null_mut::<u8>(); want];
        let n = core.dbg_carve_batch(class, &mut out);
        assert!(n > 0, "class {class}: carve refused");
        let carved = &out[..n];
        if leaves_spanned(carved) > 1 {
            multi_leaf_classes += 1;
        }
        for &p in carved {
            for interior in (16..block.min(LEAF)).step_by(16) {
                // SAFETY: an interior address of a live block; a correct
                // sidecar refuses it, so no ownership is transferred.
                assert!(
                    !unsafe { core.dbg_publish_small_sidecar_free(p.wrapping_add(interior)) },
                    "class {class}: interior granule +{interior} carries a class code",
                );
            }
        }
        for &p in carved {
            // SAFETY: each carved block is live and published exactly once,
            // reclaimed by the drain below.
            assert!(
                unsafe { core.dbg_publish_small_sidecar_free(p) },
                "class {class}: block start carries no class code",
            );
        }
        assert_eq!(core.dbg_drain_sidecar_ingress(), n, "class {class}: drain");
        assert!(
            carved.iter().all(|&p| core.dbg_is_free_for(p)),
            "class {class}: a drained block is not free",
        );
    }
    assert!(
        multi_leaf_classes >= 3,
        "vacuous: only {multi_leaf_classes} classes carved across a leaf boundary",
    );
}

#[cfg(feature = "bench-internals")]
#[test]
fn prepare_countdown_advances_once_per_carved_block() {
    use sefer_alloc::registry::segment_route::SmallSidecar;

    const N: usize = 600;
    const TAIL: usize = 5;
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    // Class 0 (16 B): 600 blocks span three leaves, most blocks same-leaf.
    let mut out = vec![core::ptr::null_mut::<u8>(); N];
    SmallSidecar::fail_prepare_after_for_test(N + TAIL);
    assert_eq!(
        core.dbg_carve_batch(0, &mut out),
        N,
        "countdown beyond the run"
    );
    assert!(leaves_spanned(&out) > 1, "vacuous: one-leaf run");
    let mut next = vec![core::ptr::null_mut::<u8>(); 2 * TAIL];
    assert_eq!(
        core.dbg_carve_batch(0, &mut next),
        0,
        "the countdown must have {TAIL} blocks left after {N}",
    );
    assert_eq!(core.dbg_carve_batch(0, &mut next), 2 * TAIL, "retry");

    // A refusal inside a later leaf aborts the whole run.
    for k in [1, 2, 300, N] {
        SmallSidecar::fail_prepare_after_for_test(k);
        assert_eq!(core.dbg_carve_batch(0, &mut out), 0, "refusal at block {k}");
        SmallSidecar::fail_prepare_after_for_test(0);
    }
    assert_eq!(core.dbg_carve_batch(0, &mut out), N, "clean carve");
}
