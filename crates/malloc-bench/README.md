# malloc-bench-rs

Portable, generic-over-`GlobalAlloc` multi-threaded allocator benchmark
harness for Rust. The Rust answer to
[mimalloc-bench](https://github.com/daanx/mimalloc-bench) (which is C-only).

## Features

- Two realistic workloads: **larson** (server churn) and **mstress**
  (batch alloc+free).
- **Cross-thread free** — a fraction of blocks are handed off to another
  thread and freed there, exercising the allocator's remote-free path.
- **Generic over `GlobalAlloc`** — benchmark any allocator without setting
  `#[global_allocator]`. Compare multiple allocators in one binary.
- **Zero non-std dependencies** — PRNG is an internal xorshift64.
- **Deterministic** — fixed per-thread seeds; results are reproducible.
- Correctly measures **steady-state throughput**: threads are pre-spawned and
  synchronized with a barrier before the clock starts.

## Quick start

```toml
# Cargo.toml (benchmark binary or example)
[dev-dependencies]
malloc-bench-rs = "0.1"
```

```rust
use malloc_bench_rs::{run, Config, Workload};
use std::alloc::System;

fn main() {
    let cfg = Config {
        threads: 4,
        steps_per_thread: 200_000,
        working_set: 512,
        mstress_blocks: 256,
    };

    // Benchmark the system allocator with the larson workload.
    let ops = run(Workload::Larson, &cfg, || System);
    println!("System larson T=4: {:.2} M ops/sec", ops / 1e6);
}
```

## Thread-count sweep

```rust
use malloc_bench_rs::{sweep, Config, Workload};
use std::alloc::System;

let cfg = Config::default();
let results = sweep(Workload::Larson, &cfg, &[1, 2, 4, 8], || System);
for (t, ops) in results {
    println!("T={t}: {:.2} M ops/sec", ops / 1e6);
}
```

## Workloads

### larson

Server-churn pattern: each thread keeps a live working set of `working_set`
blocks. Each step frees a random slot and allocates a fresh random-sized block.
Every 16th step hands a block cross-thread. Models long-running server heaps.

### mstress

Batch-stress pattern: rounds of "fill `mstress_blocks` blocks → free half in
random order → refill → free all". A fraction (~12.5%) freed cross-thread.
Models request-scoped allocators and scripting-engine GC cycles.

## Size distribution

~97% of allocations are 16–512 bytes; ~3% are 512 bytes – 8 KiB. Aligned to
8 bytes.

## Safety

The harness calls `GlobalAlloc::alloc`/`dealloc` directly and upholds the
**free-exactly-once** invariant for every block: slot `Option` discipline
tracks local ownership; `mpsc` channel semantics transfer ownership
cross-thread. The safety argument is documented per call-site.

## Comparison with `examples/compare.rs`

```
cargo run --release -p malloc-bench-rs --example compare
```

Prints a T=1/2/4 sweep table comparing `System` vs `mimalloc` on both
workloads.

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)

at your option.
