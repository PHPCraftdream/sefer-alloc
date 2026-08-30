//! `bench-scale-tool` fixed-iteration benches for `TaggedIndexStack` (task #762).
//! This crate previously had zero benches of its own — it is a lock-free
//! Treiber stack whose entire value is concurrent throughput, yet no benchmark
//! coverage existed.
//!
//! Run:
//! ```text
//! cargo bench -p tagged-index-stack --bench tagged_index_stack_bench -- --calibrate 1
//! cargo bench -p tagged-index-stack --bench tagged_index_stack_bench
//! ```

use std::hint::black_box;
use std::time::Instant;

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

    // push_pop/single_thread: push then pop on the same index.
    // Measures the CAS cost without real contention.
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();
        let index = 1u32;

        h.bench("push_pop/single_thread", move || {
            stack.push(&links, black_box(index));
            black_box(stack.pop(&links));
        });
    }

    // push/empty_stack: push onto an empty stack.
    // Measures the full push path including the empty→non-empty transition.
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();

        h.bench("push/empty_stack", move || {
            stack.push(&links, black_box(1u32));
        });
    }

    // pop/nonempty: pop from a nonempty stack.
    // Measures the pop path with guaranteed success (no None case).
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();
        stack.push(&links, 1u32);

        h.bench("pop/nonempty", move || {
            black_box(stack.pop(&links));
        });
    }

    // churn: steady-state push/pop churn on the same index.
    // Pop (always succeeds), then push it back.
    // Measures the combined push+pop cost in a steady state.
    {
        let links = ArrayLinks::<LINKS_SIZE>::new();
        let stack = Stack::new();
        stack.push(&links, 1u32);

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

    // Shared stack and links.
    // Stack's head is an AtomicU64, and ArrayLinks stores AtomicU32s.
    // Both are Sync, so we can share them by & reference directly.
    // We create them in the outer scope and pass &'static references to threads.
    let shared_links: &'static ArrayLinks<LINKS_SIZE> = Box::leak(Box::new(ArrayLinks::new()));
    let shared_stack: &'static Stack = Box::leak(Box::new(Stack::new()));

    // contention/push_pop: each thread seeds its own index into the shared
    // stack, then repeatedly pops WHATEVER is currently on top (which may be
    // its own index or one "stolen" from another thread racing on the same
    // shared head) and immediately pushes that exact value back.
    //
    // Correctness note: an earlier draft of this workload had each thread
    // cycle through a fixed local counter of indices regardless of what
    // pop() actually returned. That is unsound under real contention: since
    // pop() can return ANY thread's live index (not necessarily the one this
    // thread just pushed), a thread could cycle back to and re-push an index
    // that is STILL live somewhere else in the shared stack -- a double-push
    // of a not-yet-retrieved index, which silently corrupts the free-list's
    // link structure (documented as an explicit caller contract in push()'s
    // "# Caller contract" section in the crate docs, which also notes the
    // only bound push() itself checks is INDEX_MASK -- not "is this index
    // already live", since a liveness check would cost an O(n) chain walk
    // per push). Always re-pushing
    // exactly the value pop() returned (the same pattern contention/churn
    // below already uses) sidesteps this entirely: every value that is ever
    // live in the stack was placed there by exactly one push, and every
    // subsequent operation on it is pop-then-immediate-repush of that same
    // value, so no index is ever pushed while still reachable elsewhere.
    let start = Instant::now();
    let mut ops_per_thread = Vec::with_capacity(num_threads);

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for thread_id in 0..num_threads {
            let handle = s.spawn(move || {
                let mut ops = 0u64;
                // Each thread seeds one distinct index (never reused by any
                // other thread), so the initial push is race-free by
                // construction.
                let seed_idx = (thread_id * LINKS_SIZE / num_threads) as u32;
                shared_stack.push(shared_links, seed_idx);
                ops += 1;

                while start.elapsed().as_secs() < DURATION_SECS {
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
                }
                ops
            });
            handles.push(handle);
        }
        // Join all threads AFTER they've all started.
        for handle in handles {
            ops_per_thread.push(handle.join().unwrap());
        }
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops / DURATION_SECS;
    println!(
        "contention/push_pop: {} ops/sec total ({} threads, {} sec)",
        total_ops_per_sec, num_threads, DURATION_SECS
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);

    // contention/churn: all threads do steady-state churn (pop then re-push).
    // This measures throughput under contention with a always-nonempty stack.
    // Pre-fill the stack with some indices before spawning threads.
    let prefill_count = 64u32;
    for i in 0..prefill_count {
        shared_stack.push(shared_links, i);
    }

    let start = Instant::now();
    let mut ops_per_thread = Vec::with_capacity(num_threads);

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(num_threads);
        for thread_id in 0..num_threads {
            let handle = s.spawn(move || {
                let mut ops = 0u64;
                // This thread's dedicated fallback index (distinct per
                // thread_id, never reused by any other thread) for the rare
                // "stack drained" branch below. `fresh_idx_outstanding`
                // tracks whether THIS thread's fallback push is still live
                // in the stack -- pushing it a second time while the first
                // push hasn't been retrieved yet would double-push a still-
                // live index (the same free-list-corrupting hazard fixed in
                // contention/push_pop above), so the fallback only fires
                // once per outstanding push, not unconditionally every time
                // pop() happens to return None.
                let fresh_idx = (thread_id as u32 * 1000) % (LINKS_SIZE as u32);
                let mut fresh_idx_outstanding = false;
                while start.elapsed().as_secs() < DURATION_SECS {
                    if let Some(idx) = shared_stack.pop(shared_links) {
                        if idx == fresh_idx {
                            fresh_idx_outstanding = false;
                        }
                        // Immediately re-push (steady-state churn).
                        shared_stack.push(shared_links, idx);
                        ops += 2;
                    } else if !fresh_idx_outstanding {
                        // Stack drained — push this thread's own fallback
                        // index to keep the benchmark alive. Rare under
                        // contention with prefill.
                        shared_stack.push(shared_links, fresh_idx);
                        fresh_idx_outstanding = true;
                        ops += 1;
                    }
                    // else: our fallback index is still outstanding
                    // somewhere in the stack -- just retry pop() next
                    // iteration rather than double-pushing it.
                }
                ops
            });
            handles.push(handle);
        }
        // Join all threads AFTER they've all started.
        for handle in handles {
            ops_per_thread.push(handle.join().unwrap());
        }
    });

    let total_ops: u64 = ops_per_thread.iter().sum();
    let total_ops_per_sec = total_ops / DURATION_SECS;
    println!(
        "contention/churn: {} ops/sec total ({} threads, {} sec, prefill={})",
        total_ops_per_sec, num_threads, DURATION_SECS, prefill_count
    );
    println!("  Per-thread breakdown: {:?}\n", ops_per_thread);

    println!("=== All contention benchmarks complete ===");
}
