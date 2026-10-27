#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]
// Path-mounted production modules keep crate-private evict_all accessible in
// this test crate without widening the library API or adding a HeapCore hook.
// The same source is compiled under exactly the target's Cargo feature set.
#![allow(dead_code, unused_imports)]
extern crate alloc;
#[path = "../src/alloc_core/mod.rs"]
pub mod alloc_core;
#[cfg(feature = "experimental")]
#[path = "../src/concurrent/mod.rs"]
mod concurrent;
#[path = "../src/ghost/mod.rs"]
mod ghost;
#[path = "../src/global/mod.rs"]
pub mod global;
#[path = "../src/registry/mod.rs"]
pub mod registry;
pub use alloc_core::{AllocCore, LargeCacheConfig, SegmentLayout};
pub use global::{AllocStats, SeferAlloc};

use core::alloc::Layout;
use std::time::Duration;

const MIB: usize = 1024 * 1024;
const H: usize = 16 * MIB;
fn layout() -> Layout {
    Layout::from_size_align(4 * MIB - aligned_vmem::page_size(), 16).unwrap()
}
fn fill(ac: &mut AllocCore) {
    let live: Vec<_> = (0..8)
        .map(|_| {
            let p = ac.alloc(layout());
            assert!(!p.is_null());
            p
        })
        .collect();
    for (i, p) in live.into_iter().enumerate() {
        // SAFETY: live pointer allocated by this core/layout, freed once.
        unsafe { ac.dealloc(p, layout()) };
        // No later deposit can mask an eviction at the headroom crossing.
        assert_eq!(ac.dbg_large_cache_used(), (i + 1) * 4 * MIB);
    }
}
#[test]
fn recycled_trim_resets_timer() {
    let mut ac = AllocCore::new().expect("core creation");
    ac.dbg_set_large_cache_budget(None);
    ac.dbg_set_decay_config(1000, 60_000, H);
    fill(&mut ac);
    assert_eq!(ac.dbg_large_cache_used(), 32 * MIB);
    ac.dbg_backdate_decay_timer(Duration::from_secs(1200));
    ac.evict_all();
    assert_eq!(ac.dbg_large_cache_used(), 0);
    // Refilling after trim must not inherit the backdated timer or stride.
    fill(&mut ac);
    assert_eq!(ac.dbg_large_cache_used(), 32 * MIB);
}
