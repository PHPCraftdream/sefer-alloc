//! Bounded alloc-decommit lifecycle and diagnostic-provenance checks under
//! strict-provenance Miri with Stacked Borrows (the manifest's miri-core row).
//! Historical file/test names do not imply a retained-segment recommit oracle.
//!
//! The diagnostic test passes a live allocation address with no provenance to
//! dbg_is_decommitted_for. The accessor treats it only as a membership key and
//! reads metadata via the allocator's stored canonical base; null and unrelated
//! keys return None. Only the original live pointer is actually deallocated.
//!
//! The cycle test disables the Small pool and performs two rounds of 6000
//! 2-KiB allocations, payload endpoint write/readback, and local frees. Empty
//! non-current Small segments take release_or_pool_empty_segment's release leg:
//! decommit_empty_segment_for_release counts the event and resets bump/flag,
//! then the caller releases the whole reservation. Subsequent allocation is
//! not asserted to reuse that reservation or execute retained-page recommit.
//!
//! Under Miri aligned-vmem reserves/releases through System.alloc/dealloc;
//! its page decommit/recommit backends are no-ops. Readback and an increased
//! decommit-event counter check exercised memory accesses and bookkeeping, not
//! RSS, native page protection, a recommit syscall, or same-VA reuse. Explicit
//! retained-payload decommit/reset is a separate hook path not driven here.

#![cfg(all(
    all(feature = "alloc-core", feature = "alloc-decommit"),
    feature = "internals"
))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

#[test]
fn dbg_is_decommitted_for_sound_under_provenance_less_input() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let real = ac.alloc(layout);
    assert!(!real.is_null());
    assert_eq!(ac.dbg_is_decommitted_for(real), Some(false));

    // The Phase-2 Miri revert control observed a strict-provenance failure
    // when the old implementation read metadata through this address-only key.
    let forged = core::ptr::without_provenance_mut::<u8>(real.addr());
    assert_eq!(ac.dbg_is_decommitted_for(forged), Some(false));
    assert_eq!(ac.dbg_is_decommitted_for(core::ptr::null_mut()), None);
    assert_eq!(
        ac.dbg_is_decommitted_for(core::ptr::without_provenance_mut::<u8>(1)),
        None
    );

    // SAFETY: real is live with this layout and freed once; forged is never
    // dereferenced by the test.
    unsafe { ac.dealloc(real, layout) };
}

#[test]
fn decommit_recommit_cycle_bookkeeping() {
    let before = AllocCore::dbg_decommit_count();
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool for this
    // test. With the pool ON (the production default), the handful of segments
    // this small miri-sized workload empties are ABSORBED by the pool (retained
    // committed, no decommit) instead of decommitted+recommitted — so the
    // decommit hook would never fire and the reuse would go through the pool,
    // not the recommit path. Disabling the pool restores the deterministic
    // decommit→recommit cycle this test was written to cover (that path is still
    // fully live under `production`: it fires whenever the pool is full or
    // disabled). The pool's OWN reuse path is covered by
    // `tests/small_segment_pool.rs`.
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");
    // 2 KiB blocks: ~2K per 4 MiB payload, so a few thousand spills one fresh
    // segment. Small enough for miri to finish quickly.
    let layout = Layout::from_size_align(2048, 8).unwrap();

    // Two rounds: round 1 fills + empties (→ decommit); round 2 reuses.
    for round in 0..2 {
        let mut ptrs = Vec::new();
        // Enough to overflow the primordial into >= 2 fresh Small segments, so
        // at least one fresh `Small` segment is NON-current when emptied (the
        // current carve target is never decommitted). ~2K blocks/segment at 2 KiB,
        // so ~6000 spans the primordial + 2 fresh segments.
        for i in 0..6000usize {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null i={i} round={round}");
            // Touch a few bytes (recommitted pages on reuse must read back).
            unsafe {
                let b = (i & 0xFF) as u8;
                p.write(b);
                p.add(2047).write(b);
            }
            ptrs.push((p, (i & 0xFF) as u8));
        }
        for &(p, b) in &ptrs {
            unsafe {
                assert_eq!(p.read(), b, "byte0 readback round={round}");
                assert_eq!(p.add(2047).read(), b, "byteN readback round={round}");
            }
        }
        // Free all → empties the fresh non-current Small segment → decommit.
        for &(p, _) in &ptrs {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
    }

    let after = AllocCore::dbg_decommit_count();
    assert!(
        after > before,
        "decommit hook never fired (before={before}, after={after})"
    );
}
