//! `SeferAlloc::dbg_current_pool_cap`: the resolved small-segment pool cap of
//! the calling thread's heap, read back through a real `#[global_allocator]`
//! built with `(pool_segments = 8, pool_byte_cap = 32 MiB)` — the R26-4
//! config evidence the pool sweeps need (a request is not the cap that ran).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{LargeCacheConfig, SeferAlloc, SmallSegmentPoolConfig};

const CONFIG: LargeCacheConfig = LargeCacheConfig::new().pool(
    SmallSegmentPoolConfig::new()
        .pool_segments(8)
        .pool_byte_cap(32 * 1024 * 1024),
);

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);

#[test]
fn resolved_pool_cap_is_read_back_from_the_bound_heap() {
    let layout = Layout::from_size_align(64, 8).unwrap();
    // SAFETY: valid nonzero Layout; the block is freed below with the same Layout.
    let p = unsafe { GLOBAL.alloc(layout) };
    assert!(!p.is_null());
    assert_eq!(
        GLOBAL.dbg_current_pool_cap(),
        8,
        "min(pool_segments = 8, pool_byte_cap / 4 MiB = 8)"
    );
    // SAFETY: the live block allocated above, exact original Layout, freed once.
    unsafe { GLOBAL.dealloc(p, layout) };
}
