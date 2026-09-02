//! `bench-scale-tool` fixed-iteration benches for `ArrayIndexStack`:
//! single-threaded push/pop paths plus multi-threaded contention workloads —
//! this is a lock-free Treiber stack whose value is concurrent throughput, so
//! contention coverage matters alongside the single-threaded rows.
//!
//! Run:
//! ```text
//! cargo bench -p tagged-index-stack --bench tagged_index_stack_bench -- --calibrate 1
//! cargo bench -p tagged-index-stack --bench tagged_index_stack_bench
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use bench_scale_tool::Harness;
use tagged_index_stack::ArrayIndexStack;

/// Use 16-bit indices (65535 usable indices, 0xFFFF reserved for empty).
/// This is the documented practical choice in the crate docs.
type Stack = ArrayIndexStack<16, LINKS_SIZE>;

/// Number of indices in the fused stack's ArrayLinks links array.
/// Must be > 0 and < 2^16 (the usable range at INDEX_BITS=16).
const LINKS_SIZE: usize = 256;

/// Fairness signal printed after each contention benchmark: the
/// cap-sweep investigation (docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md) found
/// per-thread throughput skew is where the interesting signal hides, and
/// nothing printed it as a number before now. max/min is the spread across
/// all threads (a true ratio, hence the `x`); min/mean is the worst thread's
/// SHARE of an even split (1.0 = perfectly fair), printed as a percentage —
/// no `x` suffix, which would invite reading 0.38 as "0.38 times worse"
/// rather than "38% of a fair share".
fn print_fairness(ops_per_thread: &[u64]) {
    let max = *ops_per_thread.iter().max().unwrap() as f64;
    let min = *ops_per_thread.iter().min().unwrap() as f64;
    let mean = ops_per_thread.iter().sum::<u64>() as f64 / ops_per_thread.len() as f64;
    // A thread starved to zero ops (single-run outliers of exactly this
    // shape appear in the committed cap-sweep data at higher backoff caps)
    // makes max/min infinite -- print an explicit degenerate-cell marker
    // instead of computing the division.
    if min == 0.0 {
        println!(
            "  Fairness: DEGENERATE -- one thread completed zero ops, so max/min is undefined (not computed)\n"
        );
    } else {
        println!(
            "  Fairness: max/min = {:.2}x spread, min/mean = {:.1}% of the even split (100% = fair)\n",
            max / min,
            100.0 * min / mean
        );
    }
}

// Work duration: 1 second per benchmark.
const DURATION_SECS: u64 = 1;

// Deadline-check granularity for both contention loops below: checking
// `Instant::now()` every single iteration would make the clock read
// itself a significant fraction of what's being measured (two short
// atomic pop/push ops); checking once per this many iterations instead
// keeps the clock-read overhead negligible relative to the work being
// timed.
const DEADLINE_CHECK_INTERVAL: u32 = 256;

// Uncounted warm-up before the timed window opens: the lead from the
// coordinator's post-rendezvous clock read to the window opening -- lets
// caches, branch predictors and the contention steady-state settle so
// the first counted iterations are representative rather than
// cold-start-shaped.
const WARMUP: Duration = Duration::from_millis(200);

// Upper bound on how late a worker may enter the counted window after
// it opens. Under the published-window protocol below, the coordinator
// computes the window only AFTER every worker has reached the ready
// barrier, so a worker's normal path from barrier release to window
// entry is one warm-up clock-check granularity (microseconds). Entering
// more than MAX_WINDOW_ENTRY_LATENESS late means the thread was stalled
// somewhere on that path for a sizeable fraction of the 1-second
// window: its count would silently miss that fraction while the
// denominator still covers the full window -- exactly the failure mode
// this harness must never paper over -- so the sample aborts loudly
// instead of reporting a plausible-looking number.
const MAX_WINDOW_ENTRY_LATENESS: Duration = Duration::from_millis(100);

// Published-window protocol shared by both contention phases
// (contention/push_pop and contention/churn): workers announce readiness
// at `barrier_ready`, the coordinator then computes the window from its
// own clock and publishes it in a `OnceLock` cell, and `barrier_window`
// releases everyone into their warm-up against the now-known window.
// Because the window is computed only after full rendezvous, no fixed
// spawn+rendezvous budget has to be trusted. The old fixed BARRIER_LEAD
// lead time (window computed before spawning) silently trusted
// thread-spawn + rendezvous to finish within the lead; on a slow CI
// runner or VM it could not, and part of the window was lost with no
// signal. The window is now computed at/after
// full rendezvous, so there is no fixed spawn+rendezvous budget left to
// exceed, and the only residual stall path -- a worker descheduled
// between the rendezvous and its window entry -- is covered by the
// MAX_WINDOW_ENTRY_LATENESS guard the workers check before counting.
// Each worker checks the clock only once per DEADLINE_CHECK_INTERVAL
// iterations inside the timed loop (mechanism documented on the const
// above), and runs an uncounted warm-up until the shared window opens.
//
// `setup` runs per thread BEFORE the ready barrier (so its cost, and
// the thread's spawn latency, never land inside the measured window);
// `iteration` performs ONE iteration of the workload and returns how
// many ops it counted (0 or 2). The same `iteration` body is used for
// both the uncounted warm-up and the timed loop. `elapsed` is measured
// from the SHARED window anchor (`timed_start`), so it excludes all
// spawn and setup time by construction. Measuring elapsed from the
// shared anchor to the last join honestly includes any worker's
// overshoot past `deadline` (up to DEADLINE_CHECK_INTERVAL - 1
// unobserved iterations) instead of hiding it in the numerator.
fn run_contention_phase(
    name: &str,
    extra_note: &str,
    num_threads: usize,
    setup: impl Fn(usize) + Sync + Send,
    iteration: impl Fn() -> u64 + Sync + Send,
) -> (f64, Vec<u64>) {
    let timed_start_cell: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let barrier_ready = std::sync::Barrier::new(num_threads + 1);
    let barrier_window = std::sync::Barrier::new(num_threads + 1);
    let (elapsed, ops_per_thread) = std::thread::scope(|s| {
        let timed_start_cell = &timed_start_cell;
        let barrier_ready = &barrier_ready;
        let barrier_window = &barrier_window;
        let setup = &setup;
        let iteration = &iteration;
        let mut handles = Vec::with_capacity(num_threads);
        for thread_id in 0..num_threads {
            let handle = s.spawn(move || {
                // One-time setup, BEFORE the barrier -- so it, and this
                // thread's own spawn latency, never land inside the
                // measured window.
                setup(thread_id);

                // Every worker (and the coordinating main thread below, the
                // barriers' `num_threads + 1`-th participant) blocks here
                // until all have finished setup. The coordinator then
                // publishes the timed window and the second barrier
                // releases everyone into their warm-up against it.
                barrier_ready.wait();
                barrier_window.wait();
                let timed_start = *timed_start_cell
                    .get()
                    .expect("coordinator publishes the timed window before releasing barrier_window");
                let deadline = timed_start + Duration::from_secs(DURATION_SECS);
                // Warm-up: run the workload uncounted until the SHARED
                // window opens, so caches, branch predictors and the
                // contention steady-state settle before any op is counted
                // and every thread's counted window is the same one. The
                // clock check uses the SAME DEADLINE_CHECK_INTERVAL cadence
                // as the timed loop below: checking every iteration would
                // roughly halve the warm-up's op rate (the clock read is a
                // significant fraction of a two-atomic-op iteration) and
                // settle a different steady state than the one measured.
                // Up to DEADLINE_CHECK_INTERVAL - 1 warm-up iterations may
                // land inside the counted window past the check that opens
                // it -- uncounted, mirroring the timed loop's own deadline
                // overshoot.
                let mut since_check = 0u32;
                loop {
                    iteration();
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= timed_start {
                            break;
                        }
                    }
                }

                // Entry-lateness guard: under the published-window protocol
                // the only way to reach here late is being descheduled on
                // the path from barrier rendezvous to window entry, which
                // would silently shorten this thread's count while the
                // shared denominator still covers the full window.
                let entered = Instant::now();
                let entry_lateness = entered.duration_since(timed_start);
                assert!(
                    entry_lateness <= MAX_WINDOW_ENTRY_LATENESS,
                    "{name}: worker entered the counted window {entry_lateness:?} after it opened \
                     (allowed up to {MAX_WINDOW_ENTRY_LATENESS:?}) -- the thread was stalled on its way from the \
                     barrier rendezvous to the window opening, so part of the shared window would silently be \
                     missing from its count while the elapsed denominator still covers the full window; aborting \
                     loudly instead of reporting a plausible-looking number",
                );

                let mut ops = 0u64;
                let mut since_check = 0u32;
                loop {
                    ops += iteration();
                    since_check += 1;
                    if since_check >= DEADLINE_CHECK_INTERVAL {
                        since_check = 0;
                        if Instant::now() >= deadline {
                            break;
                        }
                    }
                }
                ops
            });
            handles.push(handle);
        }

        // Coordinator side: after every worker has announced readiness, the
        // rendezvous itself provides the happens-before edge -- the value
        // set here after `barrier_ready.wait()` is visible to every worker
        // after their `barrier_window.wait()` -- so the window can be
        // computed from a clock read at/after full rendezvous, with no
        // fixed lead to exceed.
        barrier_ready.wait();
        let timed_start = Instant::now() + WARMUP;
        timed_start_cell
            .set(timed_start)
            .expect("timed window must be published exactly once per phase");
        barrier_window.wait();
        let ops_per_thread: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let elapsed = Instant::now().duration_since(timed_start).as_secs_f64();
        (elapsed, ops_per_thread)
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops as f64 / elapsed;
    println!(
        "{name}: {:.0} ops/sec total ({} threads, {} sec target, {:.3} sec measured{extra_note})",
        total_ops_per_sec, num_threads, DURATION_SECS, elapsed
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);
    print_fairness(&ops_per_thread);
    (elapsed, ops_per_thread)
}

fn main() {
    let mut h = Harness::new("tagged_index_stack_bench", env!("CARGO_MANIFEST_DIR"));

    // ── Single-threaded workloads ──────────────────────────────────────────────

    // push_pop/single_thread: push onto an EMPTY stack, then pop back to
    // empty. Each iteration measures both tagged-CAS transitions: the
    // empty→non-empty push (the CAS installs the index over the empty
    // sentinel, taking push's `next_link = TAIL` branch) and the
    // drain-to-empty pop (taking pop's last-element `next == TAIL`
    // branch, which preserves the running ABA tag across the transition --
    // the H-2 path documented in the crate docs).
    //
    // The push-onto-empty shape is load-bearing: pushing without first
    // popping would re-push index 1 while it is still the live head -- a
    // violation of push_index's documented caller contract ("index must NOT
    // already be reachable from ANY stack that reads and writes the same
    // link cells") that also writes a self-referential link (link[1] = 1).
    {
        let stack = Stack::new();
        let index = 1u32;

        h.bench("push_pop/single_thread", move || {
            // SAFETY: index 1 is in-domain and was popped at the end of the previous iteration, so not live.
            unsafe { stack.push(black_box(index)) }.expect("bounded bench run never nears TAG_MAX");
            black_box(stack.pop());
        });
    }

    // pop/empty_fast_path: pop() on a never-populated stack -- the empty
    // early return: one Acquire load of the head word plus the is_empty
    // mask check; no link read, no CAS. The stack is constructed already
    // empty and stays empty for the entire benchmark (the closure only
    // pops, never pushes), so every timed call unconditionally takes the
    // empty fast path -- no dependency on any harness warm-up behavior.
    //
    // The row is deliberately left non-self-restoring -- a pop-then-repush
    // closure would be exactly the `churn` row below -- so the name
    // documents what is actually timed: the empty early return, not a
    // successful pop.
    {
        let stack = Stack::new();

        h.bench("pop/empty_fast_path", move || {
            black_box(stack.pop());
        });
    }

    // churn: steady-state push/pop churn on the ORDINARY (non-empty) path.
    // Seeded with 8 indices before the timed closure -- one iteration pops
    // the top (leaving >= 7 elements, so pop always takes the `next != TAIL`
    // branch, never the drain-to-empty H-2 branch) and immediately pushes it
    // back onto a still-non-empty stack (so push always takes
    // the head-index branch, never the empty-sentinel branch). This is
    // deliberately the complement of push_pop/single_thread above, which
    // measures exactly those two sentinel-transition branches -- churn never
    // touches the empty state at all, at any point during the loop.
    // Re-pushes exactly the value popped, so the stack's composition (which
    // 8 indices are present) never drifts across iterations.
    {
        let stack = Stack::new();
        for i in 0..8u32 {
            // SAFETY: fresh stack (domain 0..8); each index is in-domain and pushed exactly once.
            unsafe { stack.push(i) }.expect("fresh head has tag budget");
        }

        h.bench("churn", move || {
            let idx = stack.pop().unwrap();
            // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
            unsafe { stack.push(idx) }.expect("bounded bench run never nears TAG_MAX");
        });
    }

    h.run();

    // ── Multi-threaded contention workloads ────────────────────────────────────
    //
    // bench_scale_tool::Harness is single-threaded only, so we measure contention
    // throughput manually with real threads. This is the standard pattern for
    // benchmarking lock-free structures under contention.
    //
    // This section is hand-rolled and outside `Harness`, so it is NOT what
    // `--calibrate <secs>` calibrates -- `Harness::run()` above already
    // returned once it wrote the manifest, and `bench-scale-tool`'s own
    // `--calibrate` handling (`run_harness`, its `lib.rs`) has nothing to do
    // with a workload it doesn't manage. Skip this section under
    // `--calibrate` (mirroring bench-scale-tool's own
    // `args.iter().any(|a| a == "--calibrate")` check) so the documented
    // `-- --calibrate 1` invocation in this file's header doc comment
    // returns quickly instead of also burning the full ~2s of contention
    // workload below.
    if std::env::args().any(|a| a == "--calibrate") {
        println!(
            "(--calibrate passed: skipping multi-threaded contention section -- \
                   it is outside bench-scale-tool's Harness and has nothing to calibrate)"
        );
        return;
    }

    println!("\n=== Multi-threaded contention benchmarks ===\n");

    // Number of threads to use: CPU count (clamped to a reasonable max for testing).
    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8); // Cap at 8 for consistent benchmarking across machines

    println!(
        "Using {} threads (based on available_parallelism, capped at 8)",
        num_threads
    );

    // Shared stack -- the fused `ArrayIndexStack` owns both the head (an
    // AtomicU64) and the links (ArrayLinks stores AtomicU32s) internally.
    // Both are Sync, and both contention phases run inside
    // `std::thread::scope`, whose spawned closures can borrow this local by
    // plain `&` reference for the scope's duration -- no heap leak /
    // 'static coercion needed.
    let shared_stack = Stack::new();

    // contention/push_pop: each thread seeds its own index into the shared
    // stack, then repeatedly pops WHATEVER is currently on top (which may be
    // its own index or one "stolen" from another thread racing on the same
    // shared head) and immediately pushes that exact value back.
    //
    // Correctness note: each thread must re-push exactly the value pop()
    // returned -- never a value from a fixed local counter of indices,
    // regardless of what pop() actually returned. That shape is unsound
    // under real contention: pop() can return ANY thread's live index (not
    // necessarily the one this thread just pushed), so a thread cycling a
    // fixed local counter could re-push an index that is STILL live
    // somewhere else in the shared stack -- a double-push of a
    // not-yet-retrieved index, which silently corrupts the free-list's
    // link structure (documented as an explicit caller contract in push_index's
    // "# Safety" section in the crate docs, which also notes the
    // only bound push_index itself checks is INDEX_MASK -- not "is this index
    // already live", since a liveness check would cost an O(n) chain walk
    // per push). Always re-pushing exactly the value pop() returned (the
    // same pattern contention/churn below already uses) sidesteps this
    // entirely: every value that is ever live in the stack was placed there
    // by exactly one push, and every subsequent operation on it is
    // pop-then-immediate-repush of that same value, so no index is ever
    // pushed while still reachable elsewhere.
    // The timed window is ONE shared pair of instants (`timed_start` /
    // `deadline` below) published by the coordinator AFTER every worker has
    // reached the ready barrier, so every worker -- and the coordinator's
    // elapsed denominator -- times against the SAME window instead of each
    // worker's own post-barrier-resume clock (the old shape let scheduler
    // skew decorrelate the numerator's exposure window from the
    // denominator). The old fixed BARRIER_LEAD lead time (window computed
    // before spawning) silently trusted thread-spawn + rendezvous to finish
    // within the lead; on a slow CI runner or VM it could not, and part of
    // the window was lost with no signal. The window
    // is now computed at/after full rendezvous, so there is no fixed
    // spawn+rendezvous budget left to exceed, and the only residual stall
    // path -- a worker descheduled between the rendezvous and its window
    // entry -- is covered by the MAX_WINDOW_ENTRY_LATENESS guard the
    // workers check before counting. Each worker checks the clock only
    // once per DEADLINE_CHECK_INTERVAL iterations inside the timed loop
    // (mechanism documented on the const above), and runs an uncounted
    // warm-up until the shared window opens.
    // Seed indices are `thread_id * LINKS_SIZE / num_threads` -- distinct
    // for every thread only while `num_threads <= LINKS_SIZE`. True today
    // (num_threads capped at 8 above, LINKS_SIZE = 256), but nothing
    // asserted it. Mirrors contention/churn's analogous
    // `num_threads <= prefill_count` assert below.
    assert!(
        num_threads <= LINKS_SIZE,
        "contention/push_pop's seed formula (thread_id * LINKS_SIZE / num_threads) \
         requires num_threads <= LINKS_SIZE so every thread's seed index stays distinct"
    );

    // A momentary None (all live indices transiently held by
    // other threads between their own pop/push pair) is not an error
    // here -- just spin to the next iteration; the iteration closure
    // contributes 0 ops for it.
    let iteration = || {
        if let Some(idx) = shared_stack.pop() {
            // Re-push exactly what we popped -- never a value
            // this thread invented independently of pop()'s
            // result, so it can never collide with a value
            // still live elsewhere in the stack.
            // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
            unsafe { shared_stack.push(black_box(idx)) }
                .expect("bounded bench run never nears TAG_MAX");
            2
        } else {
            0
        }
    };
    run_contention_phase(
        "contention/push_pop",
        "",
        num_threads,
        |thread_id| {
            // SAFETY: fresh empty stack (domain 0..LINKS_SIZE); each thread's seed index is distinct and pushed once.
            unsafe { shared_stack.push((thread_id * LINKS_SIZE / num_threads) as u32) }
                .expect("fresh head has tag budget")
        },
        iteration,
    );

    // contention/churn: all threads do steady-state churn (pop then re-push).
    // This measures throughput under contention with a always-nonempty stack.
    // Confound: prefill below seeds indices 0..64 CONTIGUOUSLY (16 indices
    // per 64-byte ArrayLinks cache line, see ArrayLinks's own "Layout note"
    // doc), unlike contention/push_pop above (one seed per thread, spread by
    // LINKS_SIZE/num_threads) -- so this row's throughput also reflects
    // link-array false sharing on top of head-CAS contention, and undercounts
    // pure head-CAS throughput. Treat this row as a LOWER BOUND on head-CAS
    // throughput alone, not a clean isolation of it.
    let prefill_count = 64u32;

    // Drain the shared stack back to empty before prefilling. Phase 1
    // (contention/push_pop) leaves every seeded index still live on this
    // stack: each of its iterations is a balanced pop-then-repush of the same
    // value, so nothing there ever removes an index. Prefilling on top of
    // those leftovers would double-push at least index 0 (thread 0's seed is
    // always 0 and the prefill range starts at 0) while it is still reachable
    // from the stack -- a violation of push_index's documented caller contract,
    // which closes the free-list into a cycle; pop() may then never return
    // None again, and churn would measure a corrupted, cyclic structure
    // instead of LIFO throughput. The prefill must therefore start from a
    // known-empty stack.
    while shared_stack.pop().is_some() {}
    // Cheap sanity check that the drain really emptied the stack: a leftover
    // live index here would silently reintroduce the double-push bug above.
    assert!(shared_stack.pop().is_none());

    // Pre-fill the now provably empty stack with 0..prefill_count.
    for i in 0..prefill_count {
        // SAFETY: stack provably drained above; each index 0..prefill_count is in-domain and pushed exactly once.
        unsafe { shared_stack.push(i) }.expect("freshly-drained head has tag budget");
    }

    // With `prefill_count` unique indices prefilled and at most
    // `num_threads` threads each holding at most one popped-and-not-yet-
    // repushed index at a time, the stack can never observe fewer than
    // `prefill_count - num_threads` elements -- at least 56 with today's
    // constants. A `None` here is therefore not a legitimate steady-state
    // outcome to route around with a fallback workload (the old per-thread
    // `fresh_idx`/`fresh_idx_outstanding` machinery, now removed) -- it is
    // an invariant violation, so it now hard-panics via `.expect(...)`
    // instead.
    assert!(
        num_threads <= prefill_count as usize,
        "contention/churn's invariant (stack never empties) requires num_threads <= prefill_count"
    );

    run_contention_phase(
        "contention/churn",
        &format!(", prefill={prefill_count}"),
        num_threads,
        |_| {},
        || {
            let idx = shared_stack
                .pop()
                .expect("contention/churn: stack drained -- invariant violated (see prefill_count/num_threads assert above)");
            // Immediately re-push (steady-state churn).
            // SAFETY: idx was just returned by pop, so it is not live; in-domain by construction.
            unsafe { shared_stack.push(idx) }.expect("bounded bench run never nears TAG_MAX");
            2
        },
    );

    println!("=== All contention benchmarks complete ===");
}
