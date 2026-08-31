//! `bench-scale-tool` fixed-iteration benches for `TaggedIndexStack`:
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
use tagged_index_stack::{ArrayLinks, TaggedIndexStack};

/// Use 16-bit indices (65535 usable indices, 0xFFFF reserved for empty).
/// This is the documented practical choice in the crate docs.
type Stack = TaggedIndexStack<16>;

/// Number of indices in the ArrayLinks backing store.
/// Must be > 0 and < 2^16 (the usable range at INDEX_BITS=16).
const LINKS_SIZE: usize = 256;

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
    // violation of push()'s documented caller contract ("index must NOT
    // already be reachable from the stack") that also writes a
    // self-referential link (link[1] = 1).
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();
        let index = 1u32;

        h.bench("push_pop/single_thread", move || {
            stack.push(&links, black_box(index));
            black_box(stack.pop(&links));
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
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();

        h.bench("pop/empty_fast_path", move || {
            black_box(stack.pop(&links));
        });
    }

    // churn: steady-state push/pop churn on the ORDINARY (non-empty) path.
    // Seeded with 8 indices before the timed closure -- one iteration pops
    // the top (leaving >= 7 elements, so pop always takes the `next != TAIL`
    // branch, never the drain-to-empty H-2 branch) and immediately pushes it
    // back onto a still-non-empty stack (so push always takes the
    // `cur_idx as u32` branch, never the empty-sentinel branch). This is
    // deliberately the complement of push_pop/single_thread above, which
    // measures exactly those two sentinel-transition branches -- churn never
    // touches the empty state at all, at any point during the loop.
    // Re-pushes exactly the value popped, so the stack's composition (which
    // 8 indices are present) never drifts across iterations.
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();
        for i in 0..8u32 {
            stack.push(&links, i);
        }

        h.bench("churn", move || {
            let idx = stack.pop(&links).unwrap();
            stack.push(&links, idx);
        });
    }

    h.run();

    // ── Multi-threaded contention workloads ────────────────────────────────────
    //
    // bench_scale_tool::Harness is single-threaded only, so we measure contention
    // throughput manually with real threads. This is the standard pattern for
    // benchmarking lock-free structures under contention.

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

    // Work duration: 1 second per benchmark.
    const DURATION_SECS: u64 = 1;

    // Deadline-check granularity for both contention loops below: checking
    // `Instant::now()` every single iteration would make the clock read
    // itself a significant fraction of what's being measured (two short
    // atomic pop/push ops); checking once per this many iterations instead
    // keeps the clock-read overhead negligible relative to the work being
    // timed.
    const DEADLINE_CHECK_INTERVAL: u32 = 256;

    // Shared stack and links.
    // Stack's head is an AtomicU64, and ArrayLinks stores AtomicU32s.
    // Both are Sync, and both contention phases run inside `std::thread::scope`,
    // whose spawned closures can borrow these locals by plain `&` reference for
    // the scope's duration -- no heap leak / 'static coercion needed.
    let shared_links = ArrayLinks::<LINKS_SIZE>::new();
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
    // link structure (documented as an explicit caller contract in push()'s
    // "# Caller contract" section in the crate docs, which also notes the
    // only bound push() itself checks is INDEX_MASK -- not "is this index
    // already live", since a liveness check would cost an O(n) chain walk
    // per push). Always re-pushing exactly the value pop() returned (the
    // same pattern contention/churn below already uses) sidesteps this
    // entirely: every value that is ever live in the stack was placed there
    // by exactly one push, and every subsequent operation on it is
    // pop-then-immediate-repush of that same value, so no index is ever
    // pushed while still reachable elsewhere.
    // The timed window starts at a shared barrier release -- all spawn and
    // setup cost excluded -- and each worker checks the clock only once per
    // DEADLINE_CHECK_INTERVAL iterations instead of every iteration
    // (mechanism documented on the const above).
    let barrier = std::sync::Barrier::new(num_threads + 1);
    let (elapsed, ops_per_thread) = std::thread::scope(|s| {
        let shared_links = &shared_links;
        let shared_stack = &shared_stack;
        let barrier = &barrier;
        let mut handles = Vec::with_capacity(num_threads);
        for thread_id in 0..num_threads {
            let handle = s.spawn(move || {
                // One-time seed push, BEFORE the barrier -- so it, and this
                // thread's own spawn latency, never land inside the
                // measured window.
                let seed_idx = (thread_id * LINKS_SIZE / num_threads) as u32;
                shared_stack.push(shared_links, seed_idx);

                // Every worker (and the coordinating main thread below, the
                // barrier's `num_threads + 1`-th participant) blocks here
                // until all have finished setup, then all are released at
                // approximately the same instant.
                barrier.wait();
                let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);

                let mut ops = 0u64;
                let mut since_check = 0u32;
                loop {
                    if let Some(idx) = shared_stack.pop(shared_links) {
                        // Re-push exactly what we popped -- never a value
                        // this thread invented independently of pop()'s
                        // result, so it can never collide with a value
                        // still live elsewhere in the stack.
                        shared_stack.push(shared_links, black_box(idx));
                        ops += 2;
                    }
                    // A momentary None (all live indices transiently held by
                    // other threads between their own pop/push pair) is not
                    // an error here -- just spin to the next iteration.
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

        // Main thread joins the same barrier: its own `Instant::now()`
        // right after `wait()` returns is a good proxy for the same instant
        // every worker's local deadline was computed from, so `elapsed`
        // below excludes all spawn and setup time.
        barrier.wait();
        let start = Instant::now();
        let ops_per_thread: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        (start.elapsed().as_secs_f64(), ops_per_thread)
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops as f64 / elapsed;
    println!(
        "contention/push_pop: {:.0} ops/sec total ({} threads, {} sec target, {:.3} sec measured)",
        total_ops_per_sec, num_threads, DURATION_SECS, elapsed
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);

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
    // from the stack -- a violation of push()'s documented caller contract,
    // which closes the free-list into a cycle; pop() may then never return
    // None again, and churn would measure a corrupted, cyclic structure
    // instead of LIFO throughput. The prefill must therefore start from a
    // known-empty stack.
    while shared_stack.pop(&shared_links).is_some() {}
    // Cheap sanity check that the drain really emptied the stack: a leftover
    // live index here would silently reintroduce the double-push bug above.
    assert!(shared_stack.pop(&shared_links).is_none());

    // Pre-fill the now provably empty stack with 0..prefill_count.
    for i in 0..prefill_count {
        shared_stack.push(&shared_links, i);
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

    let barrier = std::sync::Barrier::new(num_threads + 1);
    let (elapsed, ops_per_thread) = std::thread::scope(|s| {
        let shared_links = &shared_links;
        let shared_stack = &shared_stack;
        let barrier = &barrier;
        let mut handles = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            let handle = s.spawn(move || {
                barrier.wait();
                let deadline = Instant::now() + Duration::from_secs(DURATION_SECS);

                let mut ops = 0u64;
                let mut since_check = 0u32;
                loop {
                    let idx = shared_stack.pop(shared_links).expect(
                        "contention/churn: stack drained -- invariant violated \
                         (see prefill_count/num_threads assert above)",
                    );
                    // Immediately re-push (steady-state churn).
                    shared_stack.push(shared_links, idx);
                    ops += 2;

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

        barrier.wait();
        let start = Instant::now();
        let ops_per_thread: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        (start.elapsed().as_secs_f64(), ops_per_thread)
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops as f64 / elapsed;
    println!(
        "contention/churn: {:.0} ops/sec total ({} threads, {} sec target, {:.3} sec measured, prefill={})",
        total_ops_per_sec, num_threads, DURATION_SECS, elapsed, prefill_count
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);

    println!("=== All contention benchmarks complete ===");
}
