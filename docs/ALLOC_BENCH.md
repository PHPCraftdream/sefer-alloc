# `SeferAlloc` — benchmark & honest verdict

> ## 0.3.x re-measurement (2026-07-03) — after the P0–P5 perf arc
> (256 B churn caveat below is superseded by P6 — see the "P5 → P6" section)
>
> Re-run on the post-P5 tree (perf campaign #144–#149, on top of the 0.3.0
> post-review hardening #129–#143), same host and quick criterion profile
> (`sample_size(10)`, ~1 s warm-up / ~1 s measurement), noisy Windows dev
> machine (±15–20 %). `SeferAlloc` is called directly through its `GlobalAlloc`
> impl — apples-to-apples with `mimalloc 0.1` and `System`. Medians (two runs
> where a range is shown); trust the relative shape and order of magnitude, not
> exact percentages. The rigorous deterministic gate is `perf_gate_iai`
> (instruction counts, Linux CI — #127/#128/#144). The detailed Phase-11/13
> commentary below is retained for context; where it disagrees on absolute
> numbers, THIS section is the current one.
>
> **Large alloc+free — the flagship large-cache (`alloc-decommit`):**
>
> | Size | SeferAlloc | mimalloc | System | vs mimalloc | vs System |
> |---|---|---|---|---|---|
> | 4 MiB  | **~58 ns** | ~779 ns  | ~18.0 µs | ~13× | ~309× |
> | 16 MiB | **~63 ns** | ~890 ns  | ~15.3 µs | ~14× | ~242× |
> | 64 MiB | **~62 ns** | ~2.14 µs | ~18.3 µs | ~34× | ~295× |
>
> **Small class — churn (reuse) vs cold direct (first touch), post-P5:**
>
> | Size | Churn: Sefer | mi | Cold: Sefer | mi |
> |---|---|---|---|---|
> | 16 B   | ~24 µs (**1.63× faster**) | ~39 µs  | ~16–20 µs (~1.5× slower) | ~12 µs    |
> | 64 B   | ~32 µs (**1.68× faster**) | ~53 µs  | ~21–25 µs (~1.3× slower) | ~17–19 µs |
> | 256 B  | ~32 µs (1.16× slower)     | ~28 µs  | ~24 µs (≈ parity)        | ~24 µs    |
> | 1024 B | ~33 µs (**~5.9× faster**) | ~196 µs | ~25–26 µs (**~1.9× faster**) | ~46–49 µs |
>
> (Small rows are per-iteration batches — identical batch for all three
> allocators, so ratios are the signal. vs `System`: 3–6× faster throughout.)
>
> **The P0 → P5 delta story (what moved and why):**
>
> | Front | P0 baseline gap | P5 gap | lever |
> |---|---|---|---|
> | cold 16 B  | 2.6× slower | ~1.5× slower | Э1 bump-direct carve (P3) |
> | cold 64 B  | 2.0× slower | ~1.3× slower | Э1 bump-direct carve (P3) |
> | cold 256 B | 1.5× slower | ≈ parity     | Э1 + exact-256 class |
> | cold 1024 B | 1.2× faster | ~1.9× faster | (cold path, bytes-bound) |
> | churn 16 B | 1.26× faster | **1.63× faster** | Э2 + Э4 + Э5 (P1) |
> | churn 64 B | 1.23× faster | **1.68× faster** | Э2 + Э4 + Э5 (P1) |
> | churn 256 B | 1.25× slower | 1.16× slower | exact-256 class (P1) — **not overtaken** |
> | churn 1024 B | 5.8× faster | ~5.9× faster | (retained) |
>
> **What each eureka removed (all tautologies, never a guard):**
>
> - **Э1 (P3) — bump-direct batched carve, front A's main lever.** A freshly
>   bump-carved block already satisfies the M2 bitmap invariant
>   (`bit 0 = allocated`); the old refill drove every virgin block through a
>   `carve → BinTable → pop` round-trip (~40 metadata-touch instructions) that
>   moved it to "free" and instantly back to "allocated" — a tautology. The new
>   `refill_class_bump` carves a batch straight from the bump cursor into the
>   magazine (~6–8 instr/block) **without touching the bitmap** (bit 0 is
>   already correct). Freelist / ring-drain are still tried BEFORE bump-carve,
>   so freed blocks never go stale (no RSS drift). M2 byte-identical; D1 exact.
>   This roughly **halved the cold tiny-block gap** and brought cold 256 B to
>   parity. What remains on the tiniest cold sizes is honest per-block work —
>   page-map writes and page faults on genuinely fresh pages — not ceremony.
> - **Э2 / Э4 / Э5 (P1) — churn hit-path.** One-branch teardown resolver
>   (collapsing the `TORN` + `null` compare), classify-once (thread the size
>   class `c` through instead of recomputing `class_for` 2–3× per op), and a
>   lock-free hit counter (`load;store` instead of `lock xadd`) together
>   **widened the tiny-block churn lead** (16 B 1.26× → 1.63×, 64 B 1.23× →
>   1.68×).
> - **Exact-256 B class (P1).** `SMALL_CLASS_COUNT` 48 → 49 narrowed 256 B churn
>   from 1.25× → 1.16× slower — but did **not** overtake (see the honest ceiling
>   note below).
>
> **The honest ceiling — 256 B churn stays ~16 % behind mimalloc, by design.**
> The residual is the M2 alloc-bitmap read-modify-write on the *real* free path
> — the price of the exact double-free / foreign-free guarantee mimalloc does
> not offer, paid in full and deliberately NOT removed. Fully catching
> mimalloc's free path while keeping M2 on every substrate free would require a
> feature-gated `fast`/`hardened` split — a separate product decision (0.4+),
> not this arc. See
> [`perf/PERF_PLAN_beat_mimalloc_small_medium.md`](perf/PERF_PLAN_beat_mimalloc_small_medium.md)
> §"Honest ceiling".
>
> **Deterministic proof.** These are noisy single-host wall-clock numbers.
> The per-op instruction-count deltas of Э1–Э5 are proven deterministically by
> the `perf_gate_iai` gate (Valgrind, Linux-only CI): the P0 benches
> (`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`, #144) were added for
> exactly this; their `Ir` baseline is captured on the first Linux perf-gate
> run.
>
> **realloc / Vec:**
>
> | Bench | SeferAlloc | mimalloc | System |
> |---|---|---|---|
> | `realloc_grow_geometric` (64 B→4 MiB) | ~323 µs (**1.1× faster than mi, 8.8× than System**) | ~360 µs | ~2.85 ms |
> | `realloc_in_place_unfavorable` | ~1.68 ms (1.1× slower than mi, 4.9× faster than System) | ~1.55 ms | ~8.15 ms |
> | `Vec_push` | ~547 ns (~par) | ~496 ns | ~535 ns |
>
> **Verdict (as of P5):** large-object work is a decisive win (the shamir-db
> strength); small-object *reuse* beats mimalloc except at 256 B; cold
> first-touch of tiny blocks (16–64 B) is the documented worst-case where
> mimalloc's cheaper first-touch path leads. `System` trails everywhere by
> 3–300×. **This 256 B churn caveat was overturned in P6 — see the next
> section.**

---

## P5 → P6 — the Э6 magazine-oracle rewrite (#150–#152): the 256 B churn loss is eliminated

Same host, same quick criterion profile (`sample_size(10)`, noisy Windows dev
machine, ±15–20 %). Ratios within a run are the signal — host noise hits Sefer
and mimalloc equally; absolute µs are rough. Ratio = Sefer/mimalloc; **< 1.0 =
Sefer faster**.

### Two churn patterns — non-writing vs writing

Churn is now measured two ways:

- **`global_alloc_churn` (non-writing)** — the original bench; blocks are
  allocated and freed but **never written**. This is the artificial pattern
  where the old stale-key slow path bit hardest (the per-heap key stamped into
  the freed block's body survived untouched across the free).
- **`global_alloc_churn_write` (writing, new in P6.0)** — each block is written
  after alloc. This is **the realistic pattern**: real code writes to the
  memory it allocates. The writing table is the headline.

**Non-writing churn** (`global_alloc_churn`), median µs:

| size | Sefer (Э6) | mimalloc | ratio | pre-Э6 was |
|---|---|---|---|---|
| 16 B   | ~27 | ~41  | ~1.5× faster | 1.26× faster (P0) / 1.63× (P5) |
| 64 B   | ~37 | ~56  | ~1.5× faster | 1.23× faster (P0) |
| 256 B  | ~27 | ~28  | **≈ parity (0.97×)** | **1.16–1.25× SLOWER** — loss GONE |
| 1024 B | ~34 | ~232 | ~6.8× faster | 5.8× faster |

**Writing churn** (`global_alloc_churn_write`), median µs — the realistic one:

| size | Sefer (Э6) | mimalloc | ratio |
|---|---|---|---|
| 16 B   | ~25 | ~40  | **1.6× faster** |
| 64 B   | ~36 | ~61  | **1.7× faster** |
| 256 B  | ~31 | ~35  | **1.13× faster** |
| 1024 B | ~33 | ~225 | **~6.8× faster** |

On the realistic writing pattern sefer-alloc now **leads mimalloc at every
size**; even the artificial non-writing 256 B reached parity (it was 1.16–1.25×
slower through P5).

**Cold direct** (`global_alloc`, alloc N then free N, no reuse) is **unchanged
by Э6** — Э6 targets the churn free path; cold is carve / page-fault-bound.
Noisy medians: 16 B ~17.7 / mi ~11.1 (~1.6× slower), 64 B ~24.7 / ~18.4
(~1.3× slower), 256 B ~24–33 / ~26 (≈ parity, noisy), 1024 B ~26 / ~48
(~1.8× faster). No claim of a cold improvement here.

### The Э6 anatomy — what actually cost us the 256 B churn (the honest eureka)

The P5 writeup above blamed the residual 256 B loss on "the M2 bitmap price".
**That framing was incomplete.** The real cost was a stale per-heap key
(`TCACHE_KEY`) stamped into the freed block's **body** (word1) and read back as
a magazine double-free fast-path filter. Two consequences:

1. **Block-body touch on every free.** Writing the key touched a cold / conflict
   cache line at the 256 B stride — the "256 B churn loss". On a non-writing
   bench the key survived across the free, so the fast-path filter matched and
   forced a **slow-path scan on every free**.
2. **Unsound under user writes.** A double-free after the user overwrote word1
   could slip past the key filter and double-issue.

**Э6 removes `TCACHE_KEY` entirely.** The two exact oracles — the in-magazine
array scan and the `BinTable` `is_free` bitmap line, **both hot metadata** — now
run unconditionally, and **the free path never touches the block body**. Net:

- **The 256 B churn loss is eliminated** (parity non-writing, 1.13× lead
  writing).
- **M2 was STRENGTHENED, not traded.** The pre-Э6 flushed-double-free-after-
  user-write hole is now CLOSED, because the oracle no longer depends on
  block-body contents. Counterfactual proof:
  `tests/regression_magazine_oracles.rs` test (c) is RED pre-Э6, GREEN on Э6.
- **Our free path is now cheaper than mimalloc's** on this pattern — mimalloc
  writes `next` into the block body on every free; we write nothing to it.

Every P0–P6 speedup removed a tautology; here the guard got *stronger*, not
weaker.

### Caveat (unchanged)

These are noisy single-host wall-clock numbers. The deterministic per-op proof
is the `perf_gate_iai` instruction-count gate on Linux CI; the new
`churn_write_256b` bench (#150) joins the P0 benches for the `Ir` baseline. The
one remaining place mimalloc leads is **cold tiny (16–64 B)** — honest per-block
page-fault work on the carve path, untouched by Э6.

---

## Historical detail (Phase 11 / 13.4a)

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

### Bulk-vs-churn: what the "churn" row above actually measures

The table above uses `bench_direct_alloc` (`alloc 1024 → free 1024`). Strictly
speaking that is a **bulk** pattern, not a steady-state churn. Real workloads
interleave allocs and frees over a bounded working set. The bulk pattern is
the documented **worst case** for the per-thread magazine (`fastbin`,
default-on in `production`): every free overflows the magazine, every alloc
empties it. The 16 B/64 B/256 B bulk regression (~1.8–2.9× slower than
`mimalloc`) is the magazine's design trade-off, kept as a regression guard
rather than a representative workload.

A true steady-state churn microbench (`bench_churn_alloc`, added in P0 of
the `fastbin` project — `benches/global_alloc.rs`, group `global_alloc_churn`)
maintains a working set of 256 live blocks and frees/replaces one at random
per iteration. On that workload `SeferMalloc` is competitive-to-strongly-ahead:

| size | SeferMalloc | mimalloc | vs mimalloc |
| ---- | ----------: | -------: | -----------: |
|   16 B |  ~21.8 µs |  ~36.9 µs | **1.7× faster** |
|   64 B |  ~22.3 µs |  ~37.2 µs | **1.7× faster** |
|  256 B |  ~21.9 µs |  ~22.1 µs | parity |
| 1024 B |  ~21.9 µs |   ~159 µs | **7.3× faster** |

See [`FASTBIN_DESIGN.md`](FASTBIN_DESIGN.md) for the design and the full
P0 → P6 measurement history (sweep, win/loss ledger, production
decision).

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

### Heap == core pinning (Phase 13.6) — honest verdict: no win on this host

A heap is bound to its thread via TLS (`current_for_alloc`), so pinning the
**worker thread** to a fixed core keeps that heap's segments warm in one core's
cache without the allocator having to change anything — the per-thread-heap
analogue of the Phase-7c sharded thread-per-core topology. The macro-bench grew
an opt-in **pinned mode** (feature `pinning`; reuses the Phase-7c
`core_affinity` organ through `PinnedRunner::pin_current_thread_to_core` /
`available_cores` — no new dependency, no `unsafe`). Run:

```
cargo run --release --example malloc_macro \
  --features "alloc-global alloc-xthread pinning"
```

It runs the **whole sweep twice in one process** (unpinned baseline, then
pinned) so the two modes see the same warm machine state. Without the `pinning`
feature the example is byte-for-byte the pre-13.6 single (unpinned) sweep.

Measured on this host (**Windows 10, 16 logical cores**, 1/2/4 workers,
representative of two consecutive runs; M ops/sec, higher better):

**larson — SeferMalloc**

| T | unpinned | pinned |
| -: | -------: | -----: |
| 1 |   ~19 M  | ~20 M  |
| 2 | **~24 M**| ~21 M  |
| 4 | **~42 M**| ~32 M  |

**mstress — SeferMalloc**

| T | unpinned | pinned |
| -: | -------: | -----: |
| 1 |   ~25 M  | ~23 M  |
| 2 | **~41 M**| ~27 M  |
| 4 | **~63 M**| ~48 M  |

**Pinning does NOT help here — it hurts, the more so as T grows** (the same
trend holds for mimalloc and System in the pinned tables). This is the expected
outcome on a **16-core box running only 1–4 workers**: the OS scheduler has
plenty of idle cores and places the few hot threads well on its own. Forcing
worker *i* onto a fixed core id removes that freedom — round-robin from core 0
on an SMT machine can co-schedule two workers on the two hyperthreads of one
physical core (sharing its L1/L2), and pinning also blocks the scheduler from
migrating a worker off a core that the OS or another process is using. The
"keep the heap cache-warm" benefit a pinned topology *can* give (many workers,
core-count-saturated, cache-hot per-thread working sets) is simply not the
regime a 16-core dev box with ≤4 workers is in.

**Decision (honest keep-or-document):** the pinned mode is **kept as an opt-in
tool**, not made the default — exactly because the measurement says it does not
help *on this machine*. It is the right primitive to reach for on a
**core-saturated** deployment (workers ≈ cores, thread-per-core executors like
glommio/monoio, NUMA boxes) where keeping a heap's segments resident on one
core's cache and avoiding cross-NUMA migration genuinely pays — but that regime
must be measured on the target host, not assumed. On this 16-core Windows dev
machine with few workers, leaving the OS scheduler in charge (the default,
unpinned path) is the faster choice. No numbers were tuned to flatter the
feature; the slowdown is reported as found.

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

Per `ALLOC_PLAN.md` §5 P11 / §8, production trust is earned only after the full
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
  `ALLOC_PLAN_PHASE12-13.md`).
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

---

## NUMA — opt-in locality steering (Phase B–E of #58)

### What was added

Feature flag `numa-aware = ["alloc-core"]`, **default OFF**.  Enabling it
steers new segment reservations to the NUMA node of the calling thread:

- **Linux**: `mbind(2)` with `MPOL_PREFERRED` — called after `mmap`, before
  first page access, so page-faults bring physical pages from the preferred
  node.
- **Windows**: `VirtualAllocExNuma` replaces `VirtualAlloc` — the preferred
  node is passed at reservation time (Windows has no `mbind` equivalent for
  already-reserved ranges).
- **macOS / miri**: no-op — Apple Silicon is UMA; there is no public NUMA
  syscall.  `current_node()` returns `NO_NODE` (`u32::MAX`), `bind_segment`
  is a no-op compile-time constant.

Without the flag the build is **byte-for-byte unchanged** — no new code
executes, no layout shifts (the `node_id: u32` field in `SegmentHeader` is
always compiled in to keep the struct layout stable across feature configs, but
is initialised to `NO_NODE` and never read without `numa-aware`).

### Integration points

| Location | Change |
|---|---|
| `src/alloc_core/numa.rs` | Confined-`unsafe` OS seam: `current_node()`, `bind_segment()`, `reserve_aligned_on_node()` |
| `src/alloc_core/segment_header.rs` | `node_id: u32` field (+4 bytes; still ≪ PAGE) |
| `src/alloc_core/alloc_core.rs` | `reserve_small_segment` stamps `node_id`; `find_segment_with_free` prefers same-node segments; `alloc_large` steers large segments |

### What is verified

| Test file | What it checks |
|---|---|
| `tests/numa_seam.rs` | `current_node()` returns `NO_NODE` or `< 64`; `bind_segment` with `NO_NODE` / zero len is a no-op; `reserve_aligned_on_node` returns a SEGMENT-aligned pointer |
| `tests/numa_segment_id.rs` | Small and large `AllocCore` allocations carry `node_id == current_node()` at allocation time |
| `tests/numa_alloc.rs` | Integration: same-thread segments share `node_id`; cross-thread free across potential node boundaries does not panic or corrupt; two-thread consistency of `stamped == observed` |

`tests/numa_alloc.rs` is **ENV-guarded** (`SEFER_NUMA_TEST=1`) — without the
variable every test body returns immediately (passes on CI single-NUMA
machines).  To execute the full test:

```sh
SEFER_NUMA_TEST=1 \
  cargo test \
    --features "alloc-core alloc-global alloc-xthread alloc-decommit numa-aware" \
    --test numa_alloc
```

Run inside a QEMU VM with `-numa node,...` or on a kernel booted with
`numa=fake=N` to exercise a real multi-node topology.

### What is NOT verified — honest N/A

**Latency-reduction numbers are not in this table.** The benefit of local-node
page allocation is a *memory-access latency* reduction — measurable only when
the workload's data fits in a NUMA node's local DRAM and accesses to remote
DRAM are the bottleneck.  That regime requires **real multi-socket hardware**:

- AWS `c5n.metal` / `i3.metal` (Xeon, 2 physical sockets)
- AWS `r6g.metal` (Graviton 2, multiple NUMA domains)
- Dual-socket development box

QEMU `-numa` / `numa=fake` verify correctness of the `mbind` / stamping path
but cannot reproduce the physical latency asymmetry (all "nodes" share the same
physical socket and DRAM controllers).  Until measurements on real hardware are
available the NUMA latency column is **N/A**.

### Synergy with the `pinning` feature

`numa-aware` alone is **best-effort**: if the OS migrates a thread to a
different NUMA node after its initial segment was reserved, subsequent accesses
to that segment will be cross-node.  The allocator uses strategy (a) — ignore
migration, steer only NEW reservations — as the MVP policy.

Combining `numa-aware + pinning` makes the locality **deterministic**: the
`pinning` feature (via `core_affinity`) pins each worker thread to a specific
core for the duration of its run.  A pinned thread on core *k* always resides
on the same NUMA node, so every new segment it reserves is local.

```sh
cargo run --release --example malloc_macro \
  --features "alloc-global alloc-xthread pinning numa-aware"
```

Without `pinning`, `numa-aware` helps under workloads with low thread
migration (e.g. long-lived worker threads, DBMS executors) but is not a
guarantee.  With `pinning`, locality is guaranteed for the lifetime of the
pinned run.

---

## Large-cache (OPT-E) — `alloc-decommit` required

### What was added

Feature-gated on `alloc-decommit`, `AllocCore` holds a small fixed-size
free-cache for large segments (`LARGE_CACHE_SLOTS = 2`). When a large
allocation is freed, instead of releasing the OS reservation immediately
the segment is deposited into the cache (reservation stays live, pages
stay committed — no decommit on deposit, so no recommit is needed on hit).
The next `alloc_large` of a compatible size
(`needed <= cached_size <= needed * 2`) hits the cache, skipping the OS
mmap/VirtualAlloc entirely.

**Admission policy (per-shard byte budget, #90–#95):** the original
per-span cap `MAX_CACHED_LARGE_BYTES = 64 MiB` was removed in #90 — it
prevented caching large spans on machines that have the headroom. The
cache is now byte-budget'd per shard (default unbounded — clients
override with `SEFER_LARGE_CACHE_BUDGET=…` supporting `K`/`M`/`G`
suffixes). Lazy 10 %/sec exponential decay back to `live + headroom`
(headroom default 256 MiB, env-overridable) keeps the cache bounded
without a background thread. FIFO eviction on budget overflow.

The OS reservation is released either on the next `Drop` of `AllocCore`
(if the cached segment is never reused) or when the decay/budget logic
evicts the slot.

### Numbers — `benches/large_realloc.rs`, `large_alloc_free` group

Run: `cargo bench --bench large_realloc --features "alloc-global alloc-decommit" -- large_alloc_free`

Host: Windows 10, dev machine. Numbers are medians from criterion `sample_size(10)`.

**Before OPT-E** (`--features alloc-global`, no cache):

| size  | SeferMalloc | mimalloc  | System    |
| ----- | ----------: | --------: | --------: |
| 4 MiB |   ~237 µs   |  ~753 ns  |  ~18.7 µs |
| 16 MiB|   ~657 µs   |  ~851 ns  |  ~17.5 µs |
| 64 MiB|  ~1.97 ms   |  ~2.0 µs  |  ~18.3 µs |

**After OPT-E + byte-budget admission (current state, post-#90/#94/#95):**
`--features "alloc-global alloc-decommit"`:

| size  | SeferMalloc (cache hit) | mimalloc | System   | vs mimalloc | speedup vs before |
| ----- | -----------------------: | -------: | -------: | ----------: | ----------------: |
| 4 MiB |                **~46 ns** | ~743 ns  | ~17.5 µs |  **~16× faster** |        **~5,200×** |
| 16 MiB|                **~46 ns** | ~861 ns  | ~14.6 µs |  **~19× faster** |       **~14,300×** |
| 64 MiB|                **~63 ns** | ~2.43 µs | ~16.9 µs |  **~39× faster** |       **~31,300×** |

At all three sizes the cache eliminates the OS round-trip entirely. The
cached path is: scan the `LARGE_CACHE_SLOTS` (8) cache slots (bounded, O(1)), call `table.register` (O(1) for the
recycled NULL slot), write a 96-byte `SegmentHeader` struct, return a
pointer. No syscall, no page-table work.

**64 MiB is now cached** (it was not under the original per-span cap;
#90 removed the cap). The per-shard byte budget admits any single span
as long as the budget allows it; clients who want a hard cap can set
`SEFER_LARGE_CACHE_BUDGET`. See [`ALLOC_PLAN_PHASE12-13.md`] /
checkpoint notes for the redesign rationale.

### Why pages are kept committed (no decommit on deposit)

An earlier version decommitted the payload pages on cache deposit
(`VirtualFree(MEM_DECOMMIT)`) and recommitted on cache hit
(`VirtualAlloc(MEM_COMMIT)`). On Windows, committing 8 MiB of pages costs
~50 µs regardless of the warm/cold state — essentially the same as a full
mmap round-trip. Removing the decommit/recommit pair dropped the 4 MiB hit
from ~50 µs to ~45 ns: a 1,100× additional improvement.

Trade-off: cached segments hold their pages committed between uses, increasing
RSS by `usable_size` per cached slot (max 2 × 64 MiB = 128 MiB with current
constants). For workloads that alloc/free large blocks infrequently, the
`alloc-decommit` feature without OPT-E (or a future time-based eviction) is
preferable. OPT-E is optimal for workloads with repeated large-allocation churn
at the same size class.

### Known limitation — `MPOL_PREFERRED` not `MPOL_BIND`

The current Linux implementation uses `MPOL_PREFERRED` (mode 1): the kernel
*prefers* to allocate physical pages from the requested node but falls back to
any available node under memory pressure.  This avoids OOM-abort on saturated
NUMA nodes at the cost of occasional cross-node pages.

If strict binding (`MPOL_BIND`, mode 2 — pages must come from the requested
node or OOM) is needed, switching is a one-line change in `numa.rs` and is a
planned follow-up, not part of this phase.

### RSS impact

NUMA steering does not increase or decrease the number of segments reserved —
it only influences *which physical pages* back those segments.  The RSS profile
is unchanged.  The `alloc-decommit` feature (Phase 35) remains the correct
lever for RSS reduction.
