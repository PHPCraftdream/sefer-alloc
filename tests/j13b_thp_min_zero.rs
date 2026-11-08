//! J13b: `Always` with min 0 makes every fresh Large block eligible; the
//! advised bytes are exactly the 2 MiB-floored prefix of the REAL span. A
//! request of 3 MiB spans 4 MiB (one SEGMENT) by default, so 4 MiB is advised;
//! under `exact-span-large` the span is page-exact and the floor applies. The
//! expected value is derived from the observed span, never guessed. Large
//! classification depends on the active size-class table; exact-span builds
//! can produce spans below the advice granule. Pure extent tests cover that
//! zero-advice boundary. One scenario per binary.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;

use sefer_alloc::alloc_core::policy::large_thp_runtime::ThpAdvice;
use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, LargeThpShare};

const MIB: usize = 1024 * 1024;

fn expected(span: usize) -> u64 {
    if cfg!(target_os = "linux") {
        ThpAdvice::extent(span, LargeThpShare::ONE, None, aligned_vmem::page_size()) as u64
    } else {
        0
    }
}

#[test]
fn min_zero_advises_floored_span_of_small_large_blocks() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    core.dbg_set_large_cache_budget(None);
    assert_eq!(AllocCore::dbg_thp_tuple().min_bytes, 0);

    let l3 = Layout::from_size_align(3 * MIB, 8).unwrap();
    let l9 = Layout::from_size_align(9 * MIB, 8).unwrap();

    let p3 = core.alloc(l3);
    assert!(!p3.is_null());
    let s3 = core.dbg_span_usable_of(p3);
    if !cfg!(feature = "exact-span-large") {
        assert_eq!(s3 % (4 * MIB), 0);
        assert_eq!(
            expected(s3),
            if cfg!(target_os = "linux") {
                s3 as u64
            } else {
                0
            }
        );
    }
    let after3 = expected(s3);
    assert_eq!(AllocCore::dbg_thp_advised_bytes_total(), after3);

    let p9 = core.alloc(l9);
    assert!(!p9.is_null());
    let s9 = core.dbg_span_usable_of(p9);
    assert_eq!(
        AllocCore::dbg_thp_advised_bytes_total(),
        after3 + expected(s9)
    );

    assert_eq!(AllocCore::dbg_thp_charged_total(), 0);
    assert_eq!(AllocCore::dbg_thp_ledger_used(), 0);
    assert_eq!(core.large_thp_charge_for_test(p3), Some(0));
    assert_eq!(core.large_thp_charge_for_test(p9), Some(0));

    // SAFETY: both are live with their original layouts.
    unsafe {
        core.dealloc(p3, l3);
        core.dealloc(p9, l9);
    }
}
