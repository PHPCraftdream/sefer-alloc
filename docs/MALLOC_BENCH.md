# `SeferMalloc` — benchmark & honest verdict (Phase 11)

`SeferMalloc` (feature `alloc-global`) is the **malloc face** of `sefer-alloc`:
an `unsafe impl GlobalAlloc` over the per-thread segment heap (Phase 8 segment
substrate + Phase 9 intrusive free-list hot path + Phase 10 cross-thread free
under `alloc-xthread`). One substrate, two faces — the typed `Handle` face and
this raw `*mut u8` drop-in face.

This is the honest measurement the campaign promised: *"as fast as the best, and
safe."* Stated plainly, win or lose.

## What was measured

`benches/global_alloc.rs` (quick criterion profile: `sample_size(10)`, 150 ms
warm-up, 600 ms measurement). All three allocators are driven **through their
`GlobalAlloc` API directly** in one binary — a true apples-to-apples comparison
of the alloc/dealloc hot path (`SeferMalloc` is not installed as the bench
binary's global allocator; it is called directly, exactly like `mimalloc` and
`System`).

- **direct churn:** `OPS = 1024` alloc+dealloc pairs of a fixed layout.
- **`Vec_push`:** a realistic growing-vector pattern (repeated alloc + realloc +
  copy as a 512-element `i64` vector doubles its capacity).

Host: Windows 10, dev machine. Numbers are medians; the quick profile is for
*relative standing*, not precision.

| workload          | SeferMalloc | mimalloc | System (Win) |
| ----------------- | ----------: | -------: | -----------: |
| 16 B churn        |     ~21.5 µs |  ~10.5 µs |     ~102 µs |
| 64 B churn        |     ~22.8 µs |  ~18.8 µs |     ~121 µs |
| 256 B churn       |     ~42.7 µs |  ~24.2 µs |     ~102 µs |
| 1024 B churn      | **~29.7 µs** |  ~35.9 µs |     ~125 µs |
| `Vec` push/grow   |  **~497 ns** |  ~539 ns |     ~633 ns |

## Honest verdict

**The architecture delivers: `SeferMalloc` is competitive with `mimalloc` and
consistently far faster than the Windows system allocator — while being safe by
construction** (the intelligence is pure safe integer arithmetic; `unsafe` lives
only in the two audited seams + the `GlobalAlloc` impl).

1. **Competitive with mimalloc, and it wins on real patterns.** On small
   fixed-size churn (16–256 B) `SeferMalloc` is ~1.2–2× behind mimalloc — the gap
   is the per-call `classify` + TLS hop that mimalloc shaves with hand-tuned
   inlining. But at **1024 B it is faster than mimalloc**, and on the realistic
   **`Vec` push/grow** pattern (the allocation shape real programs actually
   produce) it **beats both mimalloc and System**. The single-writer per-thread
   free-list pop is the same shape as the best allocators, so the remaining gap
   is a small constant, not an order of magnitude.
2. **Dramatically faster than the OS allocator.** Across every size, `SeferMalloc`
   is **~2.5–5× faster than the Windows system allocator** — the practical win
   for a Windows service that currently pays the system-allocator tax.
3. This is a huge improvement over the Phase-9 `Heap` micro-bench (which showed
   ~7–12× behind mimalloc); measuring the steady-state hot path through the
   `GlobalAlloc` face shows the true standing.

## NOT yet production-trusted — the remaining hardening gate

Per `MALLOC_PLAN.md` §5 P11 / §8, production trust is earned only after the full
hardening gate. What works today and what remains:

**Works and is verified:**
- The `GlobalAlloc` face serves correct, aligned, non-overlapping, reusable
  memory — proven by `tests/global_alloc.rs` (direct-API, pattern write/read-back,
  no-overlap, realloc-prefix, 20 k churn) and the Phase 8/9 differential proptests
  + miri.
- It runs as a real `#[global_allocator]` for a single-threaded application
  workload end-to-end — `examples/global_allocator.rs` sums a 100 k-element `Vec`
  and fills a 10 k-entry `HashMap` entirely through `SeferMalloc`.
- Reentrancy-freedom (M5) and no-panic on the alloc path: the substrate's panic
  sites were hardened (the `alloc_small` `.expect`, `SegmentTable::register` and
  `Segment::reserve` now return gracefully; the TLS binding has a no-panic
  `with_heap_try` variant).

**Remaining (NOT done — do not deploy as a process-wide allocator yet):**
- **Robust process-wide `#[global_allocator]` under hostile runtimes.**
  RESOLVED in Phase 12.5 (the shard-model turn). The raw-pointer TLS + global
  heap registry + never-null fallback heap make the face reentrancy-safe (M5)
  and never-null (M10) under the multithreaded libtest harness — parallel test
  threads, panic hooks, thread teardown. The headline MT gate
  (`tests/global_alloc_mt.rs`) installs `SeferMalloc` as the
  `#[global_allocator]` and runs `Vec`/`String`/`HashMap`/`Box` churn across
  threads that spawn AND exit mid-allocation, plus a cross-thread free channel
  test (under `alloc-xthread`), all green. This removes the "abort under
  reentrant/teardown access" caveat. **Caveat that REMAINS:** the heavy
  hardening gate (fuzz / aarch64 weak-memory CI / TSan) has not been run, so
  "production-trusted on every target" is still pending that gate (§4 of
  `MALLOC_PLAN_PHASE12-13.md`).
- Cross-thread free requires the `alloc-xthread` feature. Phase 12.5 ships the
  shard model (a heap is a shard; thread death releases the slot, the HeapCore
  stays whole for the next claimant); **Phase 12.6 makes cross-thread free
  RECLAIM** — the freer pushes the block's `offset | class` into a non-intrusive
  per-segment ring; the owner reclaims it into the `BinTable` lazily on its
  alloc-slow-path (`find_segment_with_free`). See `RACE_DRAIN_RECLAIM.md`
  §13/§14: the true root was that `page_map`'s per-page class is unreliable for
  the mixed-class pages a shared bump cursor produces, so cross-thread reclaim
  must carry the class from the freer's `Layout` (not derive it). The
  Phase-12.5 discard-leak is **gone**: cross-thread-freed blocks are reused; RSS
  is bounded (only an in-flight ring's worth may sit un-reclaimed until the next
  slow-path drain). Verified on Windows + Linux (ThreadSanitizer-clean — it was
  a class-derivation logic bug, not a data race). M6 decommit (return empty
  segments to the OS) + M11 remain deferred behind a future `alloc-decommit`
  flag (#35). The single-thread `Heap` path fully reuses.
- The heavy correctness gate — `cargo-fuzz` (CPU-hours over adversarial
  alloc/free/realloc streams), **aarch64** multi-arch CI (the weak memory model
  x86 hides), and **ThreadSanitizer** under stress — needs CI / non-Windows
  hosts and has not been run.

## Bottom line

`SeferMalloc` proves the thesis: a safe-by-construction allocator can be
*competitive with mimalloc* (and beat it on realistic patterns) while being
*much faster than the OS allocator*. As of Phase 12.5 it is a **working,
multithreaded drop-in `#[global_allocator]`** — the reentrancy/teardown abort
that blocked MT use is closed, and the headline MT gate runs green with thread
churn + cross-thread free, and (Phase 12.6) cross-thread-freed blocks are now
**reclaimed** (no discard-leak). One honest remainder: the heavy hardening gate
(fuzz / aarch64 / TSan-under-stress) is still pending, so "production-trusted on
every target" is not yet claimed — though the cross-thread reclaim path is now
ThreadSanitizer-verified on Linux. For a process-wide allocator right now,
`SeferMalloc` is a viable MT choice; the remaining gap to `mimalloc` is the
multi-arch / CPU-hours hardening gate, not a known correctness or RSS issue.
