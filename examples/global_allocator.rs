//! Minimal example: install `SeferAlloc` as the process's `#[global_allocator]`
//! and run a small workload through it.
//!
//! Run with:  `cargo run --example global_allocator --features alloc-global`
//!
//! Every allocation below (the `Vec`, the `String`s, the `HashMap`) is served by
//! `sefer-alloc`'s segment-backed, per-thread-heap allocator — a drop-in
//! replacement for the system allocator.

use std::collections::HashMap;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn main() {
    // A growing Vec (alloc + realloc churn).
    let mut v: Vec<u64> = Vec::new();
    for i in 0..100_000u64 {
        v.push(i);
    }
    let sum: u64 = v.iter().copied().sum();

    // A HashMap of owned strings (varied small allocations).
    let mut m: HashMap<u64, String> = HashMap::new();
    for i in 0..10_000u64 {
        m.insert(i, format!("value-{i}"));
    }

    assert_eq!(sum, (0..100_000u64).sum());
    assert_eq!(m.len(), 10_000);
    assert_eq!(m.get(&123).map(String::as_str), Some("value-123"));

    println!(
        "sefer-alloc global allocator OK — summed {} ints (={sum}) and stored {} map entries, \
         all through SeferAlloc.",
        v.len(),
        m.len(),
    );
}
