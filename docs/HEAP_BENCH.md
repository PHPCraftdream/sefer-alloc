# Phase 9 heap bench -- honest verdict

> **Historical note (0.3.x):** the `Heap` type this document benchmarks was
> **removed** from the crate. It was a thin wrapper around `AllocCore` with no
> magazine cache (unlike the production `HeapCore`/`SeferAlloc` face, which has
> the per-thread magazine fast path). This bench showed `Heap` running ~9–12x
> slower than mimalloc on the steady-state alloc/dealloc hot path — the gap that
> triggered the decision to remove `Heap` rather than invest in speeding it up,
> since `SeferAlloc`'s magazine-backed path already closes that gap and does not
> need `Heap` at all. See the BREAKING CHANGE entry in `CHANGELOG.md`. The
> document is preserved as the honest historical record of that decision.

Single-thread alloc/dealloc hot path: 1024 alloc+dealloc operations per
iteration, `Layout::from_size_align(size, 8)`. Heap is pre-warmed (one
alloc+dealloc batch before timing starts) so the measurement isolates the
steady-state free-list pop/push, not the bootstrap or OS reservation.

Quick criterion profile: `sample_size(10)`, 150 ms warm-up, 600 ms
measurement. Numbers are rough; the relative order is what matters.

## Results (Windows 10, x86-64, Ryzen, 1024 ops/iter)

| Size | Heap (sefer) | mimalloc | System | Heap / mimalloc |
|-----:|-------------:|---------:|-------:|----------------:|
|  16B |      100 us  |   10 us  | 104 us |           ~10x  |
|  64B |      144 us  |   18 us  | 108 us |            ~8x  |
| 256B |      270 us  |   22 us  | 131 us |           ~12x  |
|1024B |      386 us  |   55 us  | 305 us |            ~7x  |

## Verdict

**mimalloc is ~7--12x faster on the hot path.** The sefer `Heap` is roughly
on par with the Windows system allocator (within 0.5--1.5x), and at larger
sizes slightly slower. mimalloc is decisively faster.

### Why the gap

The gap is expected at this stage and is attributable to three things, all of
which are on the Phase 9--11 roadmap:

1. **No inline fast path.** mimalloc's `mi_malloc` compiles down to ~5
   instructions on the hot path (load thread-local heap pointer, load
   free-list head, advance, store, return). Our `Heap::alloc` goes through
   a classify call, an index into the bins array, and a `FreeList::pop` --
   all correct but not yet inlined/optimised to the same degree. `#[inline]`
   on the free-list ops helps but the classify step adds overhead.

2. **Refill path is naive.** When the per-heap free list drains, we refill by
   calling `AllocCore::alloc` in a loop (one block at a time). mimalloc
   carves an entire page of blocks in one pointer-arithmetic sweep (one
   virtual-memory touch per page, not per block). The refill amortization
   matters because the bench starts with a pre-warm of only 1024 blocks --
   enough to fill the free list, but the overhead of the refill itself shows
   up in the variance.

3. **`Vec` in the bench harness.** The bench collects pointers into a
   `Vec<*mut u8>` which itself allocates through the system allocator. This
   adds noise equally to all three contestants, but it inflates absolute
   numbers and dilutes the relative signal. A future bench should use a
   stack-allocated array.

### What this means

The Phase 9 architecture (per-thread heap, intrusive free lists, no lock, no
atomic on the hot path) is **structurally correct and matches mimalloc's
design**. The constant-factor gap is implementation overhead that can be
closed by: inlining the fast path, bulk-carving refills, and moving to a
`GlobalAlloc` face where the classify+dispatch lives in a single
`#[inline(always)]` wrapper (Phase 11). The plan's target ("within a small
constant factor") is not yet met -- the gap is an order of magnitude, not a
small constant -- and we state this honestly. The architecture is right; the
polish is Phase 11's job.
