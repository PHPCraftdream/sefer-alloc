//! J13b: under `numa-aware` the advice mechanism is compiled out; a config
//! with `Always` still builds and allocates a Large block. Uses only ungated
//! public surface.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "numa-aware"
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode};

const MIB: usize = 1024 * 1024;

#[test]
fn always_config_is_inert_under_numa() {
    let cfg = LargeCacheConfig::new()
        .huge_page_mode(LargeThpMode::Always)
        .huge_page_block_min_bytes(0)
        .huge_page_budget_bytes(8 * MIB);
    let mut core = AllocCore::new_with_config(cfg).expect("primordial bootstrap");
    let l = Layout::from_size_align(16 * MIB, 8).unwrap();
    let p = core.alloc(l);
    assert!(!p.is_null());
    // SAFETY: `p` is live with its original layout.
    unsafe { core.dealloc(p, l) };
}
