# Byte tier benchmark — honest verdict (Phase 4)

The `byte` tier (`ByteRegion` + `ByteAllocator`) is **research, not a production
allocator**. Phase 4 exists to descend the design to raw bytes and document
where the safe membrane must open — not to compete with `mimalloc`. This file
records what the benchmark measured and an honest reading of it.

## What was measured

`benches/byte_alloc.rs` (criterion, quick profile) runs a **single-threaded,
batched alloc-then-dealloc loop** for one size class at a time, comparing
`ByteRegion` against `std::alloc::System`. Reproduce with
`cargo bench --features byte --bench byte_alloc`.

Median per batch (lower is better); variance was high (small-sample quick
profile), so read these as **order-of-magnitude**, not precise:

| Size | `ByteRegion` | `System` |
| --- | --- | --- |
| 8 B    | ~22 µs | ~94 µs |
| 64 B   | ~53 µs | ~83 µs |
| 256 B  | ~32 µs | ~88 µs |
| 1024 B | ~63 µs | ~88 µs |

## Verdict — read this carefully, it is NOT "we beat the system allocator"

In this micro-benchmark `ByteRegion` is faster. **This is not a fair or
meaningful win**, and it would be dishonest to present it as one:

- **It does not return memory to the OS.** `ByteRegion::dealloc` pushes the
  block onto a free list; `System::dealloc` actually returns it. A tight
  alloc/dealloc-the-same-class loop is the single workload where "never give
  memory back" wins — by *not doing the allocator's real job*. For varied or
  growing workloads the same property is a **memory leak in disguise**
  (unbounded chunk growth).
- **It is single-threaded here.** `ByteAllocator` serialises every call through
  one `Mutex`. Under real multi-threaded load that lock is a hard contention
  point; the system allocator (and `mimalloc`) use per-thread caches and would
  pull far ahead.
- **No fragmentation, no large-size story.** The bench uses one class at a time.
  Mixed sizes, alignment churn, and large allocations (which fall back to the
  system allocator anyway) are not exercised.
- **`mimalloc` was not measured.** The honest comparison target for a real
  allocator is `mimalloc`, and this tier makes no attempt to match it.

**Conclusion:** the number says "fast in a narrow reuse loop"; the engineering
says "this is a learning artifact, not a general-purpose allocator." This is the
acceptable, documented outcome the plan flagged from the start.

**resocks5's global allocator stays `mimalloc`** regardless of these numbers —
the byte tier is the design's *tzimtzum*, built to learn and to honour the
descent, not to ship as a process-wide allocator.

## What IS solid here

The tier is **miri-clean** (all `unsafe` validated, no UB), the placement logic
is pure safe integer arithmetic (the Cartographer), and the single `*mut u8`
aperture is confined and documented. The value of Phase 4 is that correctness
proof and the honest map of where `unsafe` must live — not the throughput.
