//! R6-T06 (`docs/agent_reviews_round6/SUMMARY.md`; review
//! `api_security_review.md` R6-SEC-3): free-list `next` continuations are
//! validated on the NON-hardened pop paths. `Node::continuation_offset_or_fatal`
//! now rejects — with a `fatal` process abort, never a reissue — a poisoned
//! continuation that is below the segment base, inside the metadata region,
//! at/past the segment's `bump` cursor, at/after `SEGMENT` (including the
//! former silent `as u32` truncation for a poisoned pointer ≥ 4 GiB past the
//! base), or not a whole multiple of the class's block size.
//!
//! This is the production-floor sibling of
//! `tests/regression_freelist_next_validation.rs` (which covers the stronger
//! hardened oracle): a poisoned chain ABORTS in a spawned child process
//! (parent asserts the abort signature and the `fatal` diagnostic), while the
//! healthy control proves the same setup pops/drains normally. Pre-fix
//! counterfactual: every poisoned case returned from the trigger and printed
//! SURVIVED (the truncated-4-GiB case silently became head offset ~0), so the
//! parent's `status.success() == healthy` and SURVIVED asserts went red.

#![cfg(all(
    feature = "alloc-core",
    feature = "internals",
    not(feature = "hardened")
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};
use std::io::Write;

const BELOW_BASE: &str = "node: free-list continuation precedes segment base";
const OUT_OF_RANGE: &str = "node: free-list continuation out of range or misaligned";

fn oracle(case: &str, diagnostic: &str) {
    for healthy in [true, false] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bounds_child", "--nocapture"])
            .env("SEFER_R6_T06_CASE", case)
            .env("SEFER_R6_T06_HEALTHY", if healthy { "1" } else { "0" })
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains("ARMED"), "{case}: {stdout} {stderr}");
        assert!(!stderr.contains("panicked"), "{case}: {stderr}");
        assert_eq!(
            output.status.success(),
            healthy,
            "{case}: {stdout} {stderr}"
        );
        assert_eq!(stdout.contains("SURVIVED"), healthy, "{case}: {stdout}");
        if healthy {
            assert!(
                !stderr.contains("node: free-list continuation"),
                "{case}: healthy chain must not trip the validator: {stderr}"
            );
        } else {
            assert_ne!(output.status.code(), Some(101), "{case}: {stderr}");
            assert!(
                stderr.contains(diagnostic),
                "{case}: missing diagnostic '{diagnostic}': {stderr}"
            );
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(output.status.signal(), Some(6), "{case}: {stderr}");
            }
            #[cfg(windows)]
            assert!(
                matches!(output.status.code(), Some(3) | Some(-1073740791)),
                "{case}: expected the UCRT abort signature, got {:?}: {stderr}",
                output.status.code()
            );
        }
    }
}

/// Env-gated child: builds the c -> b -> a free chain, optionally poisons the
/// head node's intrusive `next` word, then pops/drains. The poisoned trigger
/// must abort inside the pop path; the healthy control must succeed.
#[test]
fn bounds_child() {
    let Ok(case) = std::env::var("SEFER_R6_T06_CASE") else {
        return;
    };
    let healthy = std::env::var("SEFER_R6_T06_HEALTHY").unwrap() == "1";
    let drain = case.starts_with("drain");
    let mut ac = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut blocks = [core::ptr::null_mut::<u8>(); 4];
    assert_eq!(ac.dbg_carve_batch(0, &mut blocks), 4);
    let [anchor, a, b, c] = blocks;
    let base = SegmentLayout::segment_base_of(anchor.addr());
    assert!(
        blocks
            .iter()
            .all(|&p| SegmentLayout::segment_base_of(p.addr()) == base),
        "setup: all four blocks must share one segment"
    );
    // SAFETY: allocator-issued live blocks, matching layout, freed exactly
    // once each — builds the LIFO chain c(head) -> b -> a.
    unsafe {
        ac.dealloc(a, layout);
        ac.dealloc(b, layout);
        ac.dealloc(c, layout);
    }
    assert_eq!(
        ac.dbg_freelist_head_for(anchor, 0),
        (c.addr() - base) as u32
    );
    let credits = ac.dbg_live_count_for(anchor).unwrap();
    if !healthy {
        let poison = match case.as_str() {
            "pop_out_of_segment" | "drain_out_of_segment" => {
                core::ptr::without_provenance_mut(c.addr() + SegmentLayout::SEGMENT)
            }
            "pop_below_base" => core::ptr::without_provenance_mut(base.wrapping_sub(8)),
            "pop_metadata" => core::ptr::without_provenance_mut(base),
            "pop_misaligned" | "drain_misaligned" => b.wrapping_add(8),
            "pop_beyond_bump" => {
                core::ptr::without_provenance_mut(base.wrapping_add(SegmentLayout::SEGMENT - 8))
            }
            "pop_truncated_4gib" => {
                core::ptr::without_provenance_mut(base.wrapping_add(0x1_0000_0000))
            }
            other => panic!("unknown case {other}"),
        };
        // SAFETY: this standalone, exclusively owned core retains allocator
        // provenance for the whole mapped segment; `c` is a free-list node
        // whose first word is the intrusive next link. Storing the
        // unvalidated poison there is the deliberate R6-T06 UAF scenario;
        // the value is only ever stored, never dereferenced by this test.
        unsafe { c.cast::<*mut u8>().write_unaligned(poison) };
    }
    println!("ARMED");
    std::io::stdout().flush().unwrap();
    if drain {
        let mut out = [core::ptr::null_mut::<u8>(); 8];
        // SAFETY: anchor names a segment owned by this exclusively owned core.
        let got = unsafe { ac.dbg_drain_freelist_batch(anchor, 0, &mut out[..3]) };
        if healthy {
            assert_eq!(got, 3);
            assert_eq!(&out[..3], &[c, b, a]);
            assert_eq!(ac.dbg_live_count_for(anchor), Some(credits + 3));
        }
    } else {
        let p = ac.alloc(layout);
        if healthy {
            assert_eq!(p, c);
            assert_eq!(ac.dbg_live_count_for(anchor), Some(credits + 1));
        }
    }
    // Printed only when the trigger RETURNED — the pre-fix behaviour the
    // parent treats as the red counterfactual.
    println!("SURVIVED");
    std::io::stdout().flush().unwrap();
}

#[test]
fn pop_out_of_segment_aborts() {
    oracle("pop_out_of_segment", OUT_OF_RANGE);
}

#[test]
fn pop_below_base_aborts() {
    oracle("pop_below_base", BELOW_BASE);
}

#[test]
fn pop_metadata_offset_aborts() {
    oracle("pop_metadata", OUT_OF_RANGE);
}

#[test]
fn pop_misaligned_aborts() {
    oracle("pop_misaligned", OUT_OF_RANGE);
}

#[test]
fn pop_beyond_bump_aborts() {
    oracle("pop_beyond_bump", OUT_OF_RANGE);
}

#[test]
fn pop_truncated_4gib_aborts() {
    if !cfg!(target_pointer_width = "64") {
        return;
    }
    oracle("pop_truncated_4gib", OUT_OF_RANGE);
}

#[test]
fn drain_out_of_segment_aborts() {
    oracle("drain_out_of_segment", OUT_OF_RANGE);
}

#[test]
fn drain_misaligned_aborts() {
    oracle("drain_misaligned", OUT_OF_RANGE);
}
