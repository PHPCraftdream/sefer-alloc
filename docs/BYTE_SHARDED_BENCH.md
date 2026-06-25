# `ShardedByteArena` — benchmark notes & honest verdict (Phase 7d)

`ShardedByteArena` (feature `byte-sharded`) replicates the Phase-4 `ByteRegion`
across N shards, one `Mutex<ByteRegion>` each, with a thread-local router that
binds each thread to its own shard. The goal of Phase 7d was to descend the
single-writer sharding pattern to the **byte tier** and measure it honestly —
**not** to ship a production allocator.

## What was measured

`benches/byte_sharded.rs` (quick criterion profile: `sample_size(10)`, 150 ms
warm-up, 600 ms measurement). Each worker thread does `OPS_PER_THREAD = 4000`
alloc→dealloc cycles of a fixed 64-byte layout **on its own pointers** (no
cross-thread free — this targets the alloc/dealloc hot path, not the owner
scan). Swept over 1/2/4 threads. `ShardedByteArena` (one shard per thread) vs
the single-`Mutex` `ByteAllocator` baseline.

Host: Windows 10, the dev machine — `std::thread` workers, not pinned.

| threads | `ShardedByteArena` (median) | `ByteAllocator` 1×Mutex (median) |
| ------: | --------------------------: | -------------------------------: |
|       1 |                      ~560 µs |                          ~527 µs |
|       2 |                      ~2.46 ms |                         ~2.57 ms |
|       4 |                      ~7.91 ms |                         ~7.58 ms |

(Total work scales with thread count — N threads do N×4000 ops — so wall-clock
rising with threads is expected; what matters is the *relative* standing of the
two designs at each count.)

## Honest verdict

**This is not a demonstrated win, and it is not a `mimalloc` competitor.** Read
the numbers conservatively:

1. **The two designs are within noise of each other on this microbench.** At
   1 thread the single-`Mutex` `ByteAllocator` is actually slightly *faster* —
   the sharded arena pays a TLS lookup + (first-touch) shard claim + an extra
   indirection that the flat allocator does not. At 2/4 threads they trade
   places inside the measurement spread. The sharding advantage (writers in
   different shards never contend on a lock) is **real in principle** but is
   **not isolated by this benchmark**.

2. **The benchmark is dominated by per-iteration overhead, not allocation.**
   Each criterion iteration spawns `threads` fresh OS threads *and* constructs a
   fresh arena. On Windows, thread spawn is expensive enough to swamp the
   4000-op inner loop, so the measurement reflects spawn + construction cost as
   much as allocator throughput. A faithful contention benchmark would use a
   persistent thread pool hammering a shared arena for a fixed wall-clock window
   — that is **Phase 5 hardening territory**, deliberately out of scope for the
   short-scenario policy here.

3. **The arena never returns memory to the OS until it is dropped.** Like the
   Phase-4 byte research, `dealloc` only pushes blocks back onto a per-shard
   free list; chunks stay pinned for the arena's life. This is fine for a
   bounded, long-lived workload and wrong for a general-purpose allocator.

4. **Cross-thread `dealloc` pays an O(shards) owner scan.** A thread that frees
   a pointer it did not allocate walks the shards (locking each briefly) to find
   the owner. Cheap when shards are few; not free. (The plan's segment-aligned
   O(1) owner lookup was deliberately not taken — the scan keeps the unsafe
   surface minimal and the code obviously correct, the right trade for research.)

## A note on pre-warming (`prewarm`)

A natural question: *if the win is invisible, would pre-warming the shards help?*
`ShardedByteArena::prewarm()` exists and is worth calling — but it fixes a
**different** problem than the one that hides the win here:

- **What prewarm fixes:** cold-start latency. Without it, the *first* allocation
  in a shard pays for a 64 KiB chunk allocation from the OS plus first-touch page
  faults — a latency spike. `prewarm` carves a chunk per shard and touches the
  pages up front (callable from a background thread at startup, since the arena
  is `Send + Sync`). This matters for p99 tails in latency-sensitive services.
- **What prewarm does NOT fix:** the benchmark's blindness. The microbench is
  dominated by per-iteration thread spawn, not by cold chunks, so pre-warming
  does not move its numbers. Seeing the contention win still requires a
  persistent-pool benchmark (Phase 5).

So: pre-warm for latency, not to make this bench look better.

## What it *does* establish

- The sharding pattern composes onto the byte tier with **sound, miri-clean
  code** and **no new `unsafe`** beyond the already-confined `ByteRegion`
  aperture (`ShardedByteArena` is plain safe composition over
  `Mutex<ByteRegion>`; the only added `unsafe` in the tier is a one-line
  `unsafe impl Send for ByteRegion`, justified where it sits).
- Correctness holds under cross-thread free and concurrent per-shard churn
  (see `tests/byte_sharded.rs`): no corruption, no double-free, bounded chunk
  growth, owner-routed deallocation.

## Bottom line

`ShardedByteArena` is **honest research**: it shows the single-writer sharding
key transfers to raw-byte allocation safely, but it does **not** beat the system
allocator or `mimalloc`, and this microbench does not even isolate its intended
contention win. For a process-wide allocator, use `mimalloc`. Use this only as a
bounded, application-level, per-thread byte arena where you have measured that
it helps your specific workload.
