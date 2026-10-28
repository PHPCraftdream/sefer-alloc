//! U32 growth16 time probe (measurement only): one fresh process = one
//! sample. `SeferAlloc::new()` (the `GlobalAlloc` face a `#[global_allocator]`
//! uses) issues `n` 16 B blocks, all live before any free (every block of a
//! fresh heap comes from the bump carve), then frees them. Prints one JSON
//! line with the alloc, free and total wall times in ns.
//!
//! Usage: u32_growth_time_probe <n>

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: u32_growth_time_probe <n>");
    let allocator = sefer_alloc::SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut addresses: Vec<usize> = Vec::with_capacity(n);
    let t0 = Instant::now();
    for _ in 0..n {
        // SAFETY: valid nonzero layout; every block stays live until the free
        // loop below, freed exactly once by the owner.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        addresses.push(block.expose_provenance());
    }
    let t1 = Instant::now();
    black_box(&addresses);
    for &address in &addresses {
        let block = core::ptr::with_exposed_provenance_mut(address);
        // SAFETY: retained exposed provenance, uniquely owned live block,
        // exact original layout and allocator.
        unsafe { allocator.dealloc(block, layout) };
    }
    let t2 = Instant::now();
    println!(
        "{{\"n\":{n},\"alloc_ns\":{},\"free_ns\":{},\"total_ns\":{}}}",
        (t1 - t0).as_nanos(),
        (t2 - t1).as_nanos(),
        (t2 - t0).as_nanos()
    );
}
