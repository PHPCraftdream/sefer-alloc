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
//!
//! Recording-state normalization (task #1354, twenty-fifth review P3-1):
//! every workload below records each call into the mock's single shared
//! capped thread-local log (`mock::CALLS_CAP` entries), so the cost a bench
//! reported used to depend on the log fill level the PREVIOUS workload (or
//! a positional filter, or a `--scale` factor) happened to leave behind —
//! order-, filter-, and scale-dependent results. Every workload now uses
//! `Harness::bench_batched` with a `mock::drain()` as the UNTIMED
//! per-iteration setup, so each timed iteration runs against the one KNOWN
//! state — an empty log — regardless of what ran before it. The harness's
//! documented tradeoff for this shape applies and is accepted deliberately:
//! one `Instant::now()` pair per iteration lands in the reported ns/op and,
//! for a routine this small, dominates it — read these numbers as a stable
//! RELATIVE mock-dispatch signal, not as an absolute ns cost of the public
//! functions.

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

    // Mock read + record against a KNOWN log state (empty). The comment here
    // used to claim this measures "the same cost the real backend pays after
    // its cache is populated" — wrong (twenty-fifth review P3-1, task #1354):
    // the real backend's WARM path still samples the CPU through a platform
    // call on EVERY invocation and probes the process-lifetime topology
    // cache (`sched_getcpu()` + reverse-index lookup on Linux,
    // `GetCurrentProcessorNumberEx` on Windows), while the mock path is a
    // thread-local slot read, a sentinel remap, and one `MockCall` push into
    // the recording log — a different cost shape entirely, not a proxy for
    // it. What this bench measures after the task #1354 normalization is
    // exactly that mock dispatch + record: the per-iteration `mock::drain()`
    // that empties the shared log runs as UNTIMED setup (`bench_batched`),
    // so the number no longer depends on workload order, filters, or scale —
    // at the cost of one `Instant::now()` pair per iteration being included
    // in ns/op (see the module doc's normalization paragraph).
    h.bench_batched("current_node/first_call", normalize_recording_log, |_| {
        let n = black_box(current_node());
        // Prevent the call from being optimized away.
        black_box(n);
    });

    // Warm-call: under mock this is the SAME cost shape as first_call — the
    // mock has no real cache, and both benches read the same scripted
    // thread-local slot (this one just scripts 7 instead of the default 0).
    // The bench id is kept purely for historical continuity (task #1333); a
    // first/warm split would only become meaningful under a real backend,
    // where the first call pays the sysfs topology scan (tracked in
    // `docs/perf/OPEN_ITEMS.md` item 59). Same drain-normalized
    // `bench_batched` shape and timer-overhead caveat as first_call above.
    mock::set_current_node(7);
    h.bench_batched("current_node/warm_call", normalize_recording_log, |_| {
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
    // feature-gated), hence the cfg. task #1354: same `bench_batched`
    // drain-normalization as the current_node benches above — this workload
    // records into the same shared capped log, so its numbers were equally
    // order/filter/scale-dependent before.
    #[cfg(feature = "vmem-integration")]
    h.bench_batched(
        "reserve_preferred_on_node/invalid_node_error",
        normalize_recording_log,
        |_| {
            use numa_shim::{reserve_preferred_on_node, NodeId};
            let r = black_box(reserve_preferred_on_node(
                black_box(4096),
                black_box(4096),
                black_box(NodeId::new(64).expect("literal 64 is not the NO_NODE sentinel")),
            ));
            let _ = black_box(r);
        },
    );

    h.run();
}

/// Untimed per-iteration setup (task #1354, twenty-fifth review P3-1):
/// normalize the mock's shared recording log to the one KNOWN state — empty
/// — so no bench's numbers depend on the log fill level a prior workload, a
/// positional filter, or a `--scale` run left behind. Draining (via the
/// public `mock::drain()`, the same normalization the test suite's
/// `fresh_drain()` helpers perform) also drops the drained records HERE, in
/// untimed setup, so no `Vec` allocation or free lands inside any timed
/// window — which is why this is a `bench_batched` setup closure rather
/// than a `mock::drain()` call inside the timed routine itself.
#[cfg(numa_shim_mock)]
fn normalize_recording_log() {
    drop(numa_shim::mock::drain());
}
