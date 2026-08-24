//! `bench-scale-tool` fixed-iteration benches for `numa-shim` (task #756).
//! This crate previously had zero benches of its own.
//!
//! Run:
//! ```text
//! RUSTFLAGS="--cfg numa_shim_mock" cargo bench -p numa-shim --bench numa_bench -- --calibrate 1
//! RUSTFLAGS="--cfg numa_shim_mock" cargo bench -p numa-shim --bench numa_bench
//! ```
//! (the `reserve_preferred_on_node` bench additionally needs
//! `--features vmem-integration`.)
//!
//! All workloads use the `mock` backend for deterministic runs on any host
//! (including systems without real NUMA hardware). The backend is enabled by
//! the build-time cfg `numa_shim_mock` (task #1288), no longer a Cargo feature.
//! The mock backend REPLACES the real platform backend, so the benched public
//! functions never touch real OS topology.
//!
//! Bench-name honesty (task #1333, eighteenth review F10): the
//! `current_node/first_call` and `current_node/warm_call` benches measure
//! MOCK dispatch + call-recording overhead ONLY. Under `numa_shim_mock`
//! there is no sysfs topology scan (no real cold path exists to measure)
//! and no real cache (both benches read the same scripted thread-local
//! slot), so NEITHER number is evidence about real-backend cold sysfs cost
//! or production warm-lookup cost. Real-backend cold/warm calibration is
//! measure-first work tracked in `docs/perf/OPEN_ITEMS.md` item 59; the
//! per-bench inline comments below restate this where it matters most.
//! Bench NAMES are kept unchanged for historical bench-id continuity.

fn main() {
    #[cfg(not(numa_shim_mock))]
    {
        // Fail-loud guard (task #1288): this bench measures the `numa_shim_mock`
        // recording backend, which is no longer a Cargo feature — it is compiled
        // in only by `RUSTFLAGS="--cfg numa_shim_mock"`. A runtime panic (rather
        // than `compile_error!`) so that every REAL-backend `cargo test` /
        // `cargo clippy --all-targets` row — which BUILD this bench target but
        // never RUN it — stays green; the only invocation that reaches this
        // panic is a `cargo bench` run without the cfg, which must fail loudly
        // rather than report a vacuous pass (the task #1070 "Breakage B" class).
        panic!(
            "numa_bench measures the numa-shim mock backend, but this binary was \
             built WITHOUT `--cfg numa_shim_mock`. Rerun with:\n  \
             RUSTFLAGS=\"--cfg numa_shim_mock\" cargo bench -p numa-shim --bench numa_bench"
        );
    }
    #[cfg(numa_shim_mock)]
    run();
}

#[cfg(numa_shim_mock)]
fn run() {
    use std::hint::black_box;

    use bench_scale_tool::Harness;
    use numa_shim::{current_node, mock};

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

    // ── reserve_preferred_on_node() ────────────────────────────────────────

    // task #1306: replaces the old `bind_range` benches (that API is gone).
    // Measures the mock record + validation overhead on the cheapest real
    // path: node 64 is out of the single-`u64` nodemask range, so every call
    // records once and returns `Err(InvalidNode)` without touching the OS or
    // allocating — the same record-overhead measurement intent the old
    // bind_range benches had. Requires `vmem-integration` (the function is
    // feature-gated), hence the cfg.
    #[cfg(feature = "vmem-integration")]
    h.bench("reserve_preferred_on_node/invalid_node_error", || {
        use numa_shim::{reserve_preferred_on_node, NodeId};
        let r = black_box(reserve_preferred_on_node(
            black_box(4096),
            black_box(4096),
            black_box(NodeId::new(64).expect("literal 64 is not the NO_NODE sentinel")),
        ));
        let _ = black_box(r);
    });

    h.run();
}
