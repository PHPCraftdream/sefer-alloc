//! Default policy activation at the requested-size boundary. The historical
//! filename predates the supported-Linux Always default. One process scenario.
#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpTuple};
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare};

const MIB: usize = 1024 * 1024;

#[test]
fn default_advice_uses_requested_threshold_and_does_not_publish() {
    let mut core = AllocCore::new().expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(Some(0));
    assert_eq!(AllocCore::dbg_thp_tuple(), ThpTuple::DEFAULT);
    assert_eq!(
        AllocCore::dbg_thp_tuple().mode,
        if cfg!(target_os = "linux") {
            LargeThpMode::Always
        } else {
            LargeThpMode::Off
        }
    );

    let mut advised = 0;
    for size in [8 * MIB - 1, 8 * MIB, 8 * MIB + 1, 16 * MIB] {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let p = core.alloc(layout);
        assert!(!p.is_null());
        let span = core.dbg_span_usable_of(p);
        assert!(span >= size);
        if cfg!(target_os = "linux") && size >= 8 * MIB {
            // The span cap is the observed payload end (header offset plus request).
            let (_, _, base, offset, _) = core.large_geometry_for_test(p).expect("large block");
            assert_eq!(p.addr(), base + offset);
            let end = (offset + size).min(span);
            let expected =
                ThpAdvice::extent(end, LargeThpShare::ONE, None, aligned_vmem::page_size());
            assert!(expected <= end, "advice never passes the payload end");
            assert!(expected > 0, "eligible default must activate advice");
            advised += expected as u64;
        }
        assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), advised);
        assert_eq!(core.large_thp_charge_for_test(p), Some(0));
        assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
        // SAFETY: `p` is live and uniquely owned with its original layout.
        unsafe { core.dealloc(p, layout) };
    }
    drop(core);
    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_returned_total(), 0);

    #[cfg(feature = "alloc-global")]
    {
        use sefer_alloc::SeferAlloc;
        use std::alloc::GlobalAlloc;

        let allocator = SeferAlloc::new();
        let mut blocks = Vec::new();
        for size in [8 * MIB - 1, 8 * MIB] {
            let layout = Layout::from_size_align(size, 8).unwrap();
            // SAFETY: the layout is valid and nonzero.
            let p = unsafe { allocator.alloc(layout) };
            assert!(!p.is_null());
            let actual = AllocCore::dbg_thp_advised_bytes_total();
            if cfg!(target_os = "linux") && size >= 8 * MIB {
                assert!(actual > advised, "default GlobalAlloc must activate advice");
            } else {
                assert_eq!(actual, advised, "requested-size boundary must be preserved");
            }
            advised = actual;
            blocks.push((p, layout));
        }
        for (p, layout) in blocks {
            // SAFETY: `p` is uniquely owned and was allocated by this facade/layout.
            unsafe { allocator.dealloc(p, layout) };
        }
    }

    // Knobless use must not consume the first explicit publication.
    let cfg = LargeCacheConfig::new().huge_page_mode(LargeThpMode::Off);
    let mut off = AllocCore::new_with_config(cfg).expect("explicit Off bootstrap");
    off.dbg_set_large_cache_budget(Some(0));
    assert_eq!(AllocCore::dbg_thp_tuple().mode, LargeThpMode::Off);
    assert_eq!(AllocCore::dbg_thp_tuple_conflicts(), 0);
    let layout = Layout::from_size_align(16 * MIB, 8).unwrap();
    let p = off.alloc(layout);
    assert!(!p.is_null());
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), advised);
    // SAFETY: `p` is live and uniquely owned with its original layout.
    unsafe { off.dealloc(p, layout) };
    drop(off);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
}
