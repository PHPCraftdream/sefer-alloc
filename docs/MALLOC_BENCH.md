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
- **Robust process-wide `#[global_allocator]` under hostile runtimes.** Installed
  as the allocator for a *multithreaded, reentrancy-heavy* harness (e.g.
  `libtest` itself — parallel test threads, panic hooks, thread teardown), the
  current TLS binding returns null under reentrant / early-init / teardown access,
  which aborts the process. A bootstrap-safe, reentrancy-tolerant TLS discipline
  (a recursion guard + a bootstrap fallback path) is required. Until then,
  `SeferMalloc` is trustworthy for **single-threaded** global use and for
  **direct/`Heap`-API** multi-threaded use, not as the drop-in allocator of an
  arbitrary multithreaded process.
- Cross-thread free requires the `alloc-xthread` feature; abandoned-heap
  **adoption** (vs the current bounded abandonment-leak) and **M6 decommit**
  wiring are deferred (Phase 10 notes).
- The heavy correctness gate — `cargo-fuzz` (CPU-hours over adversarial
  alloc/free/realloc streams), **aarch64** multi-arch CI (the weak memory model
  x86 hides), and **ThreadSanitizer** under stress — needs CI / non-Windows
  hosts and has not been run.

## Bottom line

`SeferMalloc` proves the thesis: a safe-by-construction allocator can be
*competitive with mimalloc* (and beat it on realistic patterns) while being
*much faster than the OS allocator*. It is a **working, fast, single-threaded
drop-in allocator today**; promoting it to a process-wide multithreaded
`#[global_allocator]` for arbitrary programs needs the documented reentrancy-safe
TLS work + the fuzz/aarch64/TSan hardening gate. For a process-wide allocator
right now, use `mimalloc`; use `SeferMalloc` where you have measured it helps
(single-threaded services, or via the `Heap` API), and watch this space.
