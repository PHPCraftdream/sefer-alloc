// u31_growth_wall.rs — U31 wall-clock probe (measurement only). One growth16
// body per process on a fresh process heap: n blocks of 16 B @ align 8, all
// live, then all freed (mirrors prf01_support::R19Prf01::growth16). Prints
// the timed alloc loop, the free loop and their sum.
// Usage: u31_growth_wall <n>
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: u31_growth_wall <n>");
    let allocator = sefer_alloc::SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut addresses: Vec<usize> = Vec::with_capacity(n);
    let t0 = Instant::now();
    for _ in 0..n {
        // SAFETY: valid nonzero layout; freed exactly once below.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        addresses.push(block.expose_provenance());
    }
    let t1 = Instant::now();
    black_box(&addresses);
    for address in addresses {
        let block = core::ptr::with_exposed_provenance_mut(address);
        // SAFETY: uniquely owned live allocation, exact layout and allocator.
        unsafe { allocator.dealloc(block, layout) };
    }
    let t2 = Instant::now();
    let a = (t1 - t0).as_nanos();
    let f = (t2 - t1).as_nanos();
    println!("RESULT growth16 n={n} alloc_ns={a} free_ns={f} total_ns={}", a + f);
}
