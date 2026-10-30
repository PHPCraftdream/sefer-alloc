#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

//! B4-29 (U31 trigger, chunk 1) growth16 discovery oracle: the PRF01
//! `growth16` workload shape (n blocks of 16 B @ align(8), all allocated
//! before ANY free, then all freed) exercises discovery
//! (`B4_FIND_SEGMENT_CALLS` > 0). Since SC-6 A-exact the installed heap is
//! armed: with local frees only no route is ever notified, so discovery
//! trusts its negative answer and `B4_SCAN_CALLS` / `B4_SCAN_WORDS` and the
//! U31 observers stay exactly 0 (before SC-6 they were strictly positive:
//! every miss drained the whole heap). Counters read via
//! `SeferAlloc::dbg_b4_collapse_counters`, order pinned there. Prints one
//! JSON line per n.

use sefer_alloc::{AllocCore, SeferAlloc};
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

fn counters() -> [u64; 7] {
    SeferAlloc::dbg_b4_collapse_counters()
}

/// U31 scan observers: [0] = routed scans past validation, ALL sites
/// (denominator of the holders==base fraction), [1] = of those,
/// `Entry.refs == 1` at scan time (secondary slice: CURRENT holders, base —
/// NOT "no publications since the previous scan"), [2] = discovery scans
/// whose whole segment scan found no nonzero pending word, [3] = discovery
/// scans (`drain_segment_sidecar`) — the exact same set as [2]; the U31
/// upper bound of skippable scans is [2]/[3].
fn u31_counters() -> [u64; 4] {
    SeferAlloc::dbg_b4_u31_scan_counters()
}

/// Process-wide B4_* counters are shared by every `AllocCore` in the test
/// process; the delta-based assertions below are only exact when no sibling
/// test scans concurrently. Serialize the tests in this file.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn growth16_body(n: usize) {
    let allocator = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut addresses: Vec<usize> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid nonzero layout; every allocation stays live until the
        // free loop below (the PRF01 all-live-batch shape), freed exactly once.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        addresses.push(block.expose_provenance());
    }
    black_box(&addresses);
    for address in addresses {
        let block = core::ptr::with_exposed_provenance_mut(address);
        // SAFETY: retained exposed provenance, uniquely owned live allocation,
        // exact original layout and allocator; obligation removed once.
        unsafe { allocator.dealloc(block, layout) };
    }
}

#[test]
fn growth16_discovery_counters_positive() {
    let _guard = serialize();
    for n in [32_768_usize, 65_536, 131_072] {
        let before = counters();
        let u31_before = u31_counters();
        growth16_body(n);
        let after = counters();
        let delta = |i: usize| after[i] - before[i];
        println!(
            "{{\"growth16_n\":{n},\"find_segment_calls\":{},\"scan_calls\":{},\"scan_words\":{},\"foreign_publish_ok\":{},\"directory_lookups\":{},\"shard_lock_spins\":{},\"shard_lock_yields\":{}}}",
            delta(0),
            delta(1),
            delta(2),
            delta(3),
            delta(4),
            delta(5),
            delta(6),
        );
        assert!(delta(0) > 0, "n={n}: no discovery calls");
        // SC-6 A-exact (P23): growth16 frees locally only, so no route is
        // ever notified and the armed heap trusts every negative discovery
        // answer -- no sidecar drain runs at all (before SC-6 every miss
        // drained the whole heap: scan calls and words were > 0 here).
        assert_eq!(delta(1), 0, "n={n}: an armed heap drained on discovery");
        assert_eq!(delta(2), 0, "n={n}: an armed heap scanned pending words");

        // The U31 observers count routed discovery scans; with no drain they
        // must stay at zero, and their ordering invariants hold trivially.
        let u31_after = u31_counters();
        let seg = u31_after[0] - u31_before[0];
        let base = u31_after[1] - u31_before[1];
        let nopending = u31_after[2] - u31_before[2];
        let discovery = u31_after[3] - u31_before[3];
        println!(
            "{{\"growth16_n\":{n},\"u31_scan_segments\":{seg},\"u31_holders_base\":{base},\"u31_scan_no_pending\":{nopending},\"u31_discovery_scans\":{discovery}}}"
        );
        assert_eq!(
            (seg, base, nopending, discovery),
            (0, 0, 0, 0),
            "n={n}: U31 observers saw a routed discovery scan on an armed heap"
        );
    }
}

/// Chunk-A positive + negative oracle for `B4_U31_SCAN_NO_PENDING`:
/// (1) NEGATIVE leg — a routed discovery scan over segments with NO pending
///     publication (all bins of the scanned class empty everywhere) must
///     count exactly one NO_PENDING per drained scan (upper bound of the U31
///     skip benefit: these scans could not have reclaimed anything).
/// (2) POSITIVE leg — after a REAL terminal publication into the sidecar
///     (`dbg_publish_small_sidecar_free`, the foreign/terminal publish path
///     that leaves a pending bit), the same scan must NOT count NO_PENDING
///     (delta == 0): the scan found a nonzero pending word and reclaimed it.
/// A mutant that always fires NO_PENDING goes red on leg (2); a mutant that
/// never fires it goes red on leg (1).
#[test]
fn u31_no_pending_counts_empty_scans_but_not_published_pending_scans() {
    let _guard = serialize();
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");

    // 32 class-0 allocations carve ONE Small segment (plus its refill batch
    // pushed back into the SAME segment) and leave its class-0 bin empty
    // again — exactly one drained segment per scan (mold of
    // `tests/b4_find_segment_bin_head_gates_drain.rs`).
    let class_zero = Layout::from_size_align(AllocCore::dbg_block_size(0), 8).unwrap();
    let mut live = Vec::new();
    for _ in 0..32 {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
    }

    // ── Leg (1): no pending anywhere. Pick a class whose bin is empty in
    // every segment (class 1 was never allocated). `find_segment_with_free`
    // must fall through to the linear scan, drain the (single) Small segment
    // with a zero-pending sidecar, and return None.
    let class_one = 1_usize;
    let before = u31_counters();
    let found = core.dbg_find_segment_with_free(class_one);
    let after = u31_counters();
    let seg = after[0] - before[0];
    let nopending = after[2] - before[2];
    let discovery = after[3] - before[3];
    println!(
        "{{\"leg\":\"empty\",\"class\":{class_one},\"found\":{},\"u31_scan_segments\":{seg},\"u31_scan_no_pending\":{nopending},\"u31_discovery_scans\":{discovery}}}",
        found.is_some()
    );
    assert!(
        seg >= 1,
        "leg empty: at least one routed scan must be counted (got {seg})"
    );
    assert!(
        discovery >= 1,
        "leg empty: at least one discovery scan must be counted (got {discovery})"
    );
    assert!(
        found.is_none(),
        "leg empty: class {class_one} was never allocated, no free block may exist"
    );
    assert_eq!(
        nopending, discovery,
        "leg empty: every drained discovery scan saw zero nonzero pending words"
    );

    // ── Leg (2): REAL terminal publication (pending bit in the sidecar).
    let victim = live.remove(0);
    let slot = core.dbg_segment_id_of(victim) as usize;
    // SAFETY: victim is a live class-zero block of `core`; its single free is
    // transferred to the sidecar here and never freed again directly.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });

    let before = u31_counters();
    let found = core.dbg_find_segment_with_free(0);
    let after = u31_counters();
    let seg = after[0] - before[0];
    let nopending = after[2] - before[2];
    let discovery = after[3] - before[3];
    println!(
        "{{\"leg\":\"published\",\"found\":{},\"u31_scan_segments\":{seg},\"u31_scan_no_pending\":{nopending},\"u31_discovery_scans\":{discovery}}}",
        found.is_some()
    );
    assert!(
        found.is_some(),
        "leg published: pending block must be found"
    );
    assert_eq!(
        core.dbg_segment_id_of(found.unwrap()) as usize,
        slot,
        "leg published: found segment must be the victim's"
    );
    assert!(seg >= 1, "leg published: the draining scan must be counted");
    assert!(
        discovery >= 1,
        "leg published: the draining discovery scan must be counted"
    );
    assert_eq!(
        nopending, 0,
        "leg published: the scan found a nonzero pending word — NO_PENDING must NOT fire (mutant \"always no-pending\" goes red here)"
    );

    // Cleanup: the drained victim was reissued into the bin; free everything
    // still live exactly once.
    let reused = core.alloc(class_zero);
    assert_eq!(reused, victim, "the drained block is the one reissued");
    // SAFETY: reused is a live class-zero block, freed exactly once here.
    unsafe { core.dealloc(reused, class_zero) };
    for ptr in live {
        // SAFETY: each pointer is a live, distinct class-zero block.
        unsafe { core.dealloc(ptr, class_zero) };
    }
}
