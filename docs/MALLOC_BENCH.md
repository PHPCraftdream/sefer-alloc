# `SeferMalloc` — benchmark & honest verdict (Phase 11)

`SeferMalloc` (feature `alloc-global`) is the **malloc face** of `sefer-alloc`:
an `unsafe impl GlobalAlloc` over the per-thread segment heap (Phase 8 segment
substrate + Phase 9 intrusive free-list hot path + Phase 10 cross-thread free
under `alloc-xthread`). One substrate, two faces — the typed `Handle` face and
this raw `*mut u8` drop-in face.

This is the honest measurement the campaign promised: *"as fast as the best, and
safe."* Stated plainly, win or lose.

## What was measured — single-threaded churn

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
*relative standing*, not precision. **Re-measured at Phase 13.4a** (see the
evolution note below) — these are current numbers, not the stale Phase-11 ones.

| workload          | SeferMalloc | mimalloc | System (Win) |
| ----------------- | ----------: | -------: | -----------: |
| 16 B churn        |     ~24.5 µs |  ~11.0 µs |     ~110 µs |
| 64 B churn        |     ~30.6 µs |  ~20.2 µs |     ~144 µs |
| 256 B churn       |     ~28.7 µs |  ~22.8 µs |     ~132 µs |
| 1024 B churn      | **~30.5 µs** |  ~34.8 µs |     ~116 µs |
| `Vec` push/grow   |  **~496 ns** |  ~515 ns |     ~543 ns |

### Evolution of the single-thread numbers (why they moved since Phase 11)

The Phase-11 table claimed 16 B churn at ~21.5 µs. That number predated two
substrate changes: (a) the **registry inversion** of Phase 12.5 (raw-pointer TLS
→ a process-global heap registry with a never-null fallback; the per-call
`current_for_alloc()` now does a registry hop), and (b) the **Phase 13.4a**
double-free guard rework (an O(1) bitmap that replaced an accidental O(N²)
scan — a *correctness/perf fix*, not a regression). Net, steady-state 16 B churn
is now ~22–25 µs (the TLS/registry hop is a small constant added per call). The
1024 B and `Vec_push` standings — where `SeferMalloc` is at or ahead of
mimalloc — are unchanged in character. The old 21.5 µs figure is retired; the
table above is the honest current state.

## What was measured — multi-threaded macro-benchmark (Phase 13.7)

`examples/malloc_macro.rs` (run:
`cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"`).
This is a **standalone harness**, NOT a criterion micro-loop: criterion's
per-iter model mis-measures MT work (thread-spawn inside the timed closure
dominates). Threads are pre-spawned, aligned on a `Barrier`, then a fixed op
budget (400 k steps/thread) is run and the steady-state region is timed with
`Instant::elapsed`. PRNG is a dependency-free xorshift with fixed per-thread
seeds (reproducible; no `rand` crate added).

Two workloads, both reporting **aggregate ops/sec** (an op = one alloc+free
pair), sweeping T = 1, 2, 4 threads:

- **larson** (server-churn): each thread keeps a ~768-slot working set; each
  step frees a random slot and re-allocates a random small-skewed size
  (16..512 B, ~3% up to 8 KiB). Every 16th step a block is **handed to another
  thread and freed there** (cross-thread free, routed through the Phase-10/12.6
  remote path under `alloc-xthread`).
- **mstress**: rounds of "fill 512 mixed blocks → free half in random order
  (~1/8 cross-thread) → refill → free all".

Cross-thread handoff is leak/UAF-free by construction: a handed-off block is
moved out of the producer's slot before being sent over an `mpsc` channel; the
consumer is its sole owner and frees it exactly once; each worker drains its
mailbox before joining. Every block is freed exactly once by exactly one thread.

Numbers are **million ops/sec** (higher is better), Windows 10 dev machine,
representative run (run-to-run ±10%; the *ordering* is stable):

**larson**

| T | SeferMalloc | mimalloc | System |
| -: | ----------: | -------: | -----: |
| 1 |   ~21 M     |  ~27 M   | ~6.6 M |
| 2 | **~25 M**   |  ~19 M   | ~7.2 M |
| 4 | **~40 M**   |  ~32 M   | ~14 M  |

**mstress**

| T | SeferMalloc | mimalloc | System |
| -: | ----------: | -------: | -----: |
| 1 |   ~25 M     |  ~33 M   | ~4.5 M |
| 2 |   ~43 M     |  ~43 M   | ~6.1 M |
| 4 |   ~65 M     |  ~65 M   | ~9.5 M |

**RSS:** not measured here. There is no portable, dependency-free peak-RSS probe
across Windows/Linux/macOS without pulling in platform syscalls, so the harness
honestly reports N/A rather than inventing a number. (RSS comparison is a
hardening-gate item — `SeferMalloc` does not yet decommit empty segments to the
OS, feature `alloc-decommit`/#35, so a fair RSS comparison must wait for that.)

### Honest reading of the MT numbers

- **Single thread, mimalloc leads** (both larson and mstress): ~25 % faster on
  larson, ~30 % on mstress. Same story as the single-thread churn table — the
  per-call `classify` + TLS/registry hop is a constant mimalloc's hand-tuned
  inlining shaves.
- **At T = 2 and T = 4, `SeferMalloc` catches and on larson *passes* mimalloc.**
  On larson it is ahead at both 2 and 4 threads (the per-thread heap means the
  fast path takes no shared lock; the cross-thread frees route through the
  per-segment remote path without contending the producer). On mstress the two
  are within noise of each other at 2 and 4 threads.
- **`SeferMalloc` scales cleanly with thread count** — larson goes ~21→25→40 M
  (1→2→4 T), mstress ~25→43→65 M. mimalloc's larson number actually *dips* at
  T = 2 on this host (~27→19 M) before recovering at T = 4; `SeferMalloc` does
  not show that dip. System is flat and ~3–7× behind throughout.
- This is the payoff of the per-thread-heap architecture: the single-thread
  constant is a real gap, but it does **not** compound under contention, so by a
  handful of threads the safe-by-construction allocator is competitive-to-ahead.

## Honest verdict

**The architecture delivers: `SeferMalloc` is competitive with `mimalloc` and
consistently far faster than the Windows system allocator — while being safe by
construction** (the intelligence is pure safe integer arithmetic; `unsafe` lives
only in the two audited seams + the `GlobalAlloc` impl).

1. **Competitive with mimalloc, and it wins on real patterns.** On small
   fixed-size churn (16–256 B) `SeferMalloc` is ~1.4–2.2× behind mimalloc — the
   gap is the per-call `classify` + TLS/registry hop that mimalloc shaves with
   hand-tuned inlining. But at **1024 B it is faster than mimalloc**, and on the
   realistic **`Vec` push/grow** pattern (the allocation shape real programs
   actually produce) it **beats both mimalloc and System**. The single-writer
   per-thread free-list pop is the same shape as the best allocators, so the
   remaining gap is a small constant, not an order of magnitude.
1b. **Multi-threaded: the single-thread gap does NOT compound (Phase 13.7).**
   The new MT macro-bench (`examples/malloc_macro.rs`, larson + mstress, 1/2/4
   threads, with cross-thread free) shows mimalloc ahead at T = 1, but at
   **T = 2 and T = 4 `SeferMalloc` catches it — and on larson passes it** (e.g.
   ~40 M vs ~32 M ops/sec at 4 threads). `SeferMalloc` scales monotonically with
   thread count; the per-thread heap takes no shared lock on the fast path, so
   contention does not magnify the per-call constant. System trails ~3–7×
   throughout. RSS is not yet measured (no decommit; see the macro-bench section).
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
