//! `bench-scale-tool` fixed-iteration benches for `numa-shim` (task #756).
//! This crate previously had zero benches of its own.
//!
//! Run:
//! ```text
//! cargo bench -p numa-shim --bench numa_bench --features mock -- --calibrate 1
//! cargo bench -p numa-shim --bench numa_bench --features mock
//! ```
//!
//! All workloads use the `mock` backend for deterministic runs on any host
//! (including systems without real NUMA hardware). The `mock` feature REPLACES
//! the real platform backend, so `bind_range` is safe to call with dummy
//! pointers (the mock never dereferences them).

#![cfg(feature = "mock")]

use std::hint::black_box;

use bench_scale_tool::Harness;
use numa_shim::{bind_range, current_node, mock, NO_NODE};

/// Page-sized buffer for bind_range workloads.
///
/// The mock backend never touches this memory, but we pass a valid allocation
/// to keep the test honest — the real backend would require it.
const PAGE: usize = 4096;

fn main() {
    let mut h = Harness::new("numa_bench", env!("CARGO_MANIFEST_DIR"));

    // ── current_node() ─────────────────────────────────────────────────────

    // First-call cache miss cost: reads from the scripted mock slot every call.
    // Under the real backend, the first call would hit sysfs/syscall cold path;
    // under mock, this measures the mock-read overhead (the same cost the real
    // backend pays after its cache is populated).
    h.bench("current_node/first_call", || {
        let n = black_box(current_node());
        // Prevent the call from being optimized away.
        black_box(n);
    });

    // Warm-call cache hit cost: same as first-call under mock (mock has no
    // real cache), but this separates the semantics for documentation clarity.
    // A future real-backend calibration would show a visible speedup here.
    mock::set_current_node(7);
    h.bench("current_node/warm_call", || {
        let n = black_box(current_node());
        black_box(n);
    });

    // ── bind_range() ───────────────────────────────────────────────────────

    // bind_range on a valid page-sized allocation to a real node (7).
    // Under mock, this records the call without touching memory; the cost is
    // the mock-record overhead. Under the real backend, this would issue
    // `mbind(2)` (Linux) or be a no-op (Windows/macOS).
    let mut buf: Vec<u8> = vec![0u8; PAGE];
    let base = buf.as_mut_ptr();
    let len = buf.len();
    mock::set_current_node(7);
    h.bench("bind_range/page_to_node_7", move || {
        // SAFETY: `buf` is a live heap allocation owned by this scope.
        // Under mock, the function never dereferences `base`.
        unsafe {
            bind_range(black_box(base), black_box(len), black_box(7));
        }
    });

    // bind_range with NO_NODE sentinel — a short-circuit no-op.
    // This must return immediately without any backend work.
    h.bench("bind_range/no_node_noop", move || {
        // SAFETY: NO_NODE causes an early return before any OS call.
        unsafe {
            bind_range(black_box(base), black_box(len), black_box(NO_NODE));
        }
    });

    // bind_range with len == 0 — another short-circuit no-op.
    h.bench("bind_range/zero_len_noop", move || {
        // SAFETY: len == 0 causes an early return before any OS call.
        unsafe {
            bind_range(black_box(base), black_box(0), black_box(7));
        }
    });

    h.run();
}
