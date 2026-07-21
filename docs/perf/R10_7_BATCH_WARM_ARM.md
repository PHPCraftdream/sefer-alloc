# R10-7 — the warm-batch arm + tcache-aware batch (refuting R9-9's no-daylight inference)

**Task:** follow-up to R9-9 (`docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md`), which
concluded **CONDITIONAL-NO-GO** for a public batch alloc API on the strength of
an *inference* (§3.2): "even warmed, batch would still be slower than or
comparable to" the real `SeferAlloc` scalar path. R9-9 never measured a warm
batch arm (it admitted this in §5) — its §3.2 sign was *extrapolated* from the
cold `AllocCore` numbers, and the external review flagged the extrapolation as
unsupported for small N. R10-7 measures it instead of guessing, then — because
the measurement found daylight — implements and measures the tcache-aware batch
a real public API would actually ship.

**Date:** 2026-07-21
**Base revision:** `main` @ `cab6573`
**Platform:** Windows 10 Pro x86-64 (same host as R9-9 / R8-7).
**Verdict (TL;DR):** **GO** — for the *mechanism* and for the experimental
`#[doc(hidden)]` primitive. Warm-batch beats the warm `SeferAlloc` scalar path
at **every** (size, N); the implemented tcache-aware batch (`batch_tcache`)
beats it by **1.1–1.6×**; the AllocCore-direct ceiling (`batch_core_warm`) beats
it by **1.3–3.3×**. R9-9's "no daylight even warmed" premise is **empirically
refuted**. The project's "no committed public surface" stance is unchanged — the
batch ships as `#[doc(hidden)]` test/bench-only surface, exactly like R8-7's
`refill_class_bump`/`flush_class`, NOT as a stable public API.

---

## 0. The structural fact R9-9 §5 was uncertain about

R9-9 §5's caveat said a warm-batch arm "would require either a new
`#[doc(hidden)]` forwarder to reach the batch primitives through
`SeferAlloc`/`HeapCore`'s tcache, or a structurally different harness." The
implicit worry: maybe the batch primitive *fundamentally cannot* be warmed the
way the scalar path is (e.g. it always constructs cold local state).

**It can.** Verified directly in source:

- `AllocCore::refill_class_bump_impl` (`src/alloc_core/alloc_core_small_magazine.rs`)
  opens with a **freelist drain** (`drain_freelist_batch` on `small_cur`, then
  `find_segment_with_free`) and bump-carves **only the remainder** once the
  freelist is exhausted. So a `refill_class_bump` call against an `AllocCore`
  whose freelist was populated by a prior `flush_class` drains those warm blocks
  — no carve, no page fault.
- `AllocCore::alloc_small` (`src/alloc_core/alloc_core_small.rs:103`) — the
  scalar path — **also pops the freelist first** (`pop_free(small_cur)` →
  `find_segment_with_free` → carve).

So the batch primitive and the scalar primitive share the **same warmed
freelist substrate** as their first source. A persistent `AllocCore` reused
across iterations (its freelist populated by the prior iteration's `flush_class`)
is genuinely warm for **both** paths — no forwarder, no structural harness
change. This is the fact that makes a clean warm-batch-vs-warm-scalar comparison
possible benches-only (Part 1).

---

## 1. Part 1 — the warm-batch arm (d) + warm-scalar-core diagnostic (e)

### 1.1 Two new arms, added to `bench_batch_ceiling_followup`

Both reuse a **persistent warm `AllocCore`** (constructed once, pre-filled with
one `refill`+`flush` so its primordial pages are committed and its class
freelist holds N blocks before the first timed sample; criterion's own warmup
amplifies this to steady state):

- **(d) `batch_core_warm`** — ONE `refill_class_bump` + ONE `flush_class` per
  iteration on the persistent warm `AllocCore`. This is R9-9's cold arm (b) with
  the §1.2 cold-page-fault confound **removed**: same batch primitive, same
  substrate, but warm.
- **(e) `scalar_core_warm`** (diagnostic) — N `alloc` + N `dealloc` on the
  **same** persistent warm `AllocCore`. Because `alloc_small` pops the freelist
  first (same source `refill_class_bump` drains), (d) and (e) share ONE warm
  substrate; **(d)/(e) is the clean batch-vs-scalar mechanism ratio** R9-9's
  cold arm (a) could not isolate.

Reaching the batch primitive THROUGH the warm `HeapCore`/tcache (arm (c)'s path)
would need a `src/` forwarder (R9-9 §5) — forbidden for Part 1's benches-only
scope. So (d)/(e) measure the warm `AllocCore` **substrate** (the layer the
tcache sits on top of), not the tcache itself. Part 2 builds the tcache path.

### 1.2 Numbers — 3-run averages, `--features production` (ns/iter)

From `docs/perf/_raw_r10_7_warm_arm.log` (arms a/b/c/d/e, one process; (c)
re-measured in-process so the comparison is drift-controlled):

**16 B**

| N | (c) scalar_sefer | (d) batch_core_warm | (e) scalar_core_warm | d/c | d/e |
|---|---:|---:|---:|---:|---:|
| 8 | 180.2 ns | 120.7 ns | 185.0 ns | **0.67** | 0.65 |
| 16 | 323.1 ns | 212.1 ns | 370.9 ns | **0.66** | 0.57 |
| 32 | 944.2 ns | 417.1 ns | 767.1 ns | **0.44** | 0.54 |
| 64 | 2.226 µs | 704.9 ns | 1.201 µs | **0.32** | 0.59 |
| 1024 | 32.262 µs | 9.701 µs | 21.608 µs | **0.30** | 0.45 |

**64 B**

| N | (c) scalar_sefer | (d) batch_core_warm | (e) scalar_core_warm | d/c | d/e |
|---|---:|---:|---:|---:|---:|
| 8 | 163.3 ns | 124.5 ns | 196.4 ns | **0.76** | 0.63 |
| 16 | 321.2 ns | 187.3 ns | 394.6 ns | **0.58** | 0.47 |
| 32 | 958.3 ns | 413.2 ns | 797.8 ns | **0.43** | 0.52 |
| 64 | 2.451 µs | 781.3 ns | 1.620 µs | **0.32** | 0.48 |
| 1024 | 39.780 µs | 12.981 µs | 23.781 µs | **0.33** | 0.55 |

**256 B**

| N | (c) scalar_sefer | (d) batch_core_warm | (e) scalar_core_warm | d/c | d/e |
|---|---:|---:|---:|---:|---:|
| 8 | 166.0 ns | 108.9 ns | 185.7 ns | **0.66** | 0.59 |
| 16 | 281.4 ns | 178.8 ns | 347.6 ns | **0.64** | 0.51 |
| 32 | 901.7 ns | 402.8 ns | 706.3 ns | **0.45** | 0.57 |
| 64 | 2.105 µs | 699.4 ns | 1.330 µs | **0.33** | 0.53 |
| 1024 | 37.006 µs | 13.441 µs | 22.820 µs | **0.36** | 0.59 |

### 1.3 What this refutes

R9-9 §3.2's headline inference was: *"warming arm (b) by the ~3-5 µs page-fault
cost would bring, e.g., 16 B / n=8 from 3.09 µs to ~0.1-0.5 µs — still slower
than or comparable to arm (c)'s 0.157 µs."* The measured warm value at 16 B / n8
is **0.121 µs** (d) — at the *low* end of R9-9's guessed range, and **faster**
than arm (c)'s 0.180 µs. The sign is the **opposite** of R9-9's inference, at
**every** (size, N):

- **d/c = 0.30–0.76** — the warm batch primitive is **1.3×–3.3× faster** than the
  real warm `SeferAlloc` scalar path, across the entire grid (including the
  smallest batch, n=8). The win *grows* with N (more per-call TLS +
  classification + magazine-bit work amortised) but is already present at n=8.
- **d/e = 0.45–0.66** — on the *same* warm `AllocCore` substrate, batching alone
  (one `refill`+`flush` vs N `alloc`+`dealloc`) is **1.5×–2.2× faster**. This is
  the pure batch-mechanism win, uncontaminated by the tcache-vs-freelist
  substrate difference — the number R9-9 had no clean way to isolate.

The mechanism: the scalar path pays, per call, a TLS lookup + classification +
ownership routing + a `clear_magazine`/`mark_magazine` bitmap RMW
(`src/registry/heap_core_alloc.rs`'s magazine-hit arm, RAD-5). The batch
amortises all of that into one classification + one batched freelist
drain/flush. At n=8 that is already ~7 saved per-call overheads; at n=64 it is
~63. R9-9 assumed the magazine pop was "~18-20 ns, near-zero, nothing to
amortise" — the warm measurement shows the magazine pop is *not* free (the
bitmap RMW + base computation per block is the cost batching folds away).

---

## 2. Part 2 — the tcache-aware batch (the design a real API would ship)

Part 1 found daylight. Part 2 builds the design the task specifies — **"drain
what's already warm, batch-refill only the miss"** — and measures it. This is a
genuinely different design from R8-7/R9-9's measured arm (which called the
`AllocCore` batch primitive *directly*, bypassing the magazine): it goes
*through* the warm `HeapCore`/magazine a real `SeferAlloc` batch API must use.

### 2.1 Implementation (behind `#[doc(hidden)]`, NOT public API)

- **`HeapCore::alloc_batch(&mut self, layout, out: &mut [*mut u8]) -> usize`**
  (`src/registry/heap_core_alloc.rs`):
  1. classify **once**;
  2. **drain the per-class magazine** directly into `out` — the exact
     magazine-hit fast path, looped (pop + `clear_magazine` bit + hardened
     `bump_gen`);
  3. for the **remainder**, `AllocCore::refill_class_bump_checked` fills the rest
     **directly** into `out` (not via the magazine), with the same
     magazine-residency predicate + segment-owner stamping
     `refill_magazine_slow` uses. No block is parked in the magazine.
  - Non-`fastbin` fallback loops `AllocCore::alloc` (no magazine to drain).
- **`SeferAlloc::alloc_batch` / `dealloc_batch`** (`src/global/sefer_alloc.rs`):
  resolve the per-thread heap **once** (one TLS lookup for the whole batch),
  then delegate. `#[doc(hidden)]` — the established test/bench-only export
  pattern; **not** committed public API surface.
- `dealloc_batch` deliberately does **not** re-batch the magazine push/overflow
  (re-implementing that would duplicate the delicate M2 double-free oracles); it
  amortises only the TLS lookup + classification. The measured win is dominated
  by `alloc_batch`'s magazine-drain + batch-refill — see §2.3 for why this
  matters.

Correctness scope: each returned block undergoes the **same** state transition as
a single `alloc` (live + bitmap-allocated, owner-stamped, hardened-gen-bumped at
issue). The pre-existing cross-thread double-free residual (the "THIRD leg" at
`heap_core_free.rs`'s `dealloc_own_thread_with_base`) is **unchanged** — this
path reuses the exact refill + drain primitives, introducing **no new
invariant**.

### 2.2 Tests — `tests/batch_tcache.rs` (7 tests, all green)

Prove batch == N scalar calls: valid/aligned/distinct/writable blocks at every
(size, N) incl. N > `TCACHE_CAP` (the refill-remainder path); cross-compatibility
(batch blocks freed via scalar `dealloc` and vice-versa); no aliasing between a
live batch and a concurrent scalar alloc; warm steady-state over 50 cycles;
`dealloc_batch` null-skip; mixed size-classes back-to-back. Run clean under
`cargo test --features production` (full suite green, 0 regressions).

### 2.3 Numbers — 3-run averages, `--features production`

**Important methodological note (read before the numbers).** A single grouped
run of all six arms (a–f) produces **garbage** for arm (f): the
`AllocCore`-constructing arms (d)/(e) leave process/VM state that makes the
*next* `SeferAlloc`-heap arm pathologically slow and noisy (f/c swings 14–66×,
run-to-run SD 69–92%). This is a **harness in-group-ordering confound**, not a
property of arm (f) — arm (c), which runs *before* (d)/(e), is unaffected. The
clean comparison isolates `scalar_sefer` + `batch_tcache` together (same heap,
no `AllocCore`-direct interference). The numbers below are from that isolated run
(`docs/perf/_raw_r10_7_tcache_isolated.log`, run-to-run SD 1–7%):

| Size | N | (c) scalar_sefer | (f) batch_tcache | f/c |
|---|---|---:|---:|---:|
| 16 B | 8 | 142.9 ns | 129.9 ns | **0.91** |
| 16 B | 64 | 1.835 µs | 1.279 µs | **0.70** |
| 16 B | 1024 | 32.622 µs | 21.287 µs | **0.65** |
| 64 B | 8 | 137.0 ns | 112.5 ns | **0.82** |
| 64 B | 64 | 1.931 µs | 1.204 µs | **0.62** |
| 64 B | 1024 | 33.445 µs | 23.488 µs | **0.70** |
| 256 B | 8 | 132.3 ns | 120.7 ns | **0.91** |
| 256 B | 64 | 1.839 µs | 1.229 µs | **0.67** |
| 256 B | 1024 | 35.696 µs | 23.876 µs | **0.67** |

(Full grid in the raw log.) **`batch_tcache` (f) beats the real `SeferAlloc`
scalar path at every (size, N): f/c = 0.62–0.91 (1.1×–1.6× faster).** The win is
modest at n=8 (the magazine rarely overflows, so the scalar path is already
near its floor) and grows to ~1.5× at n=64/n=1024 (where batching folds away
many magazine-miss/overflow cycles).

### 2.4 The honest nuance — (f) is slower than (d)

The tcache-aware design (f) is the *realistic* one (it goes through the warm
magazine a `SeferAlloc` API must use), but it is **not** the fastest batch. A
clean head-to-head (d)+(f) run (`docs/perf/_raw_r10_7_d_vs_f.log`) shows:

| | f/d range |
|---|---|
| all (size, N) | **1.09–2.19** |

i.e. **(f) is 1.1×–2.2× slower than the AllocCore-direct batch (d).** Two reasons:

1. **Magazine per-block bookkeeping.** (f) drains the magazine with a
   `clear_magazine` bitmap RMW per block and `dealloc_batch` pushes back with a
   `mark_magazine` RMW per block; (d) bypasses the magazine entirely (one
   `drain_freelist_batch` + one `flush_run`). The RAD-5 magazine-residency
   bitmap that *strengthens* the scalar double-free guard is exactly the
   per-block cost batching into the magazine re-pays.
2. **`dealloc_batch` is not batch-optimised.** It loops the per-block `dealloc`
   (full routing + magazine push/overflow each); (d) does ONE `flush_class`. A
   batch-optimised dealloc (accumulate, one `flush_class`) would close some of
   this gap — deliberately deferred (it would re-implement the M2 double-free
   oracles; the risk is not worth it for this experimental surface).

So the ranking is **(d) AllocCore-direct > (f) tcache-aware > (c) scalar**. (d)
is a *ceiling* (it reaches past the magazine via a path a clean `SeferAlloc` API
would not take); (f) is the *realistic* design and still wins. A future
batch-optimised dealloc could move (f) toward (d).

---

## 3. Verdict — GO (refutes R9-9's no-daylight premise)

- **The mechanism has daylight.** R9-9 concluded CONDITIONAL-NO-GO on the
  inference that "even warmed, batch would still be slower." Measured: warm-batch
  beats warm-scalar by **1.3×–3.3×** (d/c) at every (size, N), and the pure
  batch-mechanism win on one warm substrate is **1.5×–2.2×** (d/e). The
  inference is empirically wrong; the sign was unknown and is now resolved.
- **The realistic design wins.** The implemented tcache-aware batch
  (`batch_tcache`, draining the magazine + batch-refilling the remainder) beats
  the production scalar path by **1.1×–1.6×** (f/c), with 7 correctness tests
  green and no new invariant.
- **Scope is honest.** The batch ships as `#[doc(hidden)]` test/bench-only
  surface (`SeferAlloc::alloc_batch`/`dealloc_batch`), mirroring how R8-7's
  `refill_class_bump`/`flush_class` are already exposed — **not** a committed
  public API. The project's repeated CONDITIONAL-NO-GO was about *committing
  public surface before a real consumer exists* (the R8-7 circularity concern);
  that gate is unchanged. What changed is the empirical premise underneath it:
  there *is* daylight, so the gate is now "waiting for a consumer" rather than
  "no win to capture."

**Net GO**, scoped to the experimental primitive. Promoting it to stable public
API still requires (a) a real in-tree consumer and (b) a batch-optimised dealloc
(to close the (f)→(d) gap) — both explicitly left for a consumer-driven task.

---

## 4. Caveats

- **Single host, three runs per comparison, plain average.** Each comparison
  (d/c, f/c, f/d) is measured in its OWN isolated 3-run process so the within-run
  baseline is drift-controlled; the (c) baseline differs slightly across the
  three runs (host drift) — each table uses its own run's (c), so the *ratios*
  are clean even though the absolute (c) values are not cross-comparable. The
  ranking (d > f > c, all beating c) is stable across all runs.
- **In-group ordering confound (§2.3).** Running all six arms in one process
  makes arm (f) pathologically slow (the `AllocCore`-constructing arms (d)/(e)
  poison the subsequent `SeferAlloc`-heap measurement). The confound-1
  `dbg_trim_current_thread` reset runs once per *group*, not per arm, so it does
  not protect arm (f) from arm (e)'s residue. Worked around here by isolating
  the (c)+(f) and (d)+(f) runs; a permanent fix would reset between arms or move
  (f) ahead of the `AllocCore`-direct arms. This is a **harness** issue, not a
  property of the batch primitive (the isolated numbers prove it).
- **`dealloc_batch` is not batch-optimised** (loops per-block `dealloc`); it
  amortises only TLS + classification. The alloc side carries the win.
- **Ceiling, not a shippable-API forecast.** Even (f)'s 1.1–1.6× is a raw
  internal-primitive ceiling; a shipped `alloc_batch`/`dealloc_batch` API pays
  extra argument-validation / layout-consistency overhead the raw internal call
  does not (the R8-7 §4 caveat, unchanged).
- **Same-thread, no-contention.** Does not exercise concurrent access, the
  cross-thread-free ring, or the magazine-miss path under load.

---

## 5. What changed in the tree

- `benches/global_alloc.rs` — three new arms in `bench_batch_ceiling_followup`:
  `(d) batch_core_warm`, `(e) scalar_core_warm` (Part 1), `(f) batch_tcache`
  (Part 2). No existing arm/ID touched.
- `src/registry/heap_core_alloc.rs` — `HeapCore::alloc_batch` (fastbin
  tcache-aware + non-fastbin fallback) + `alloc_batch_large` helper.
- `src/global/sefer_alloc.rs` — `SeferAlloc::alloc_batch` / `dealloc_batch`
  (`#[doc(hidden)]`).
- `tests/batch_tcache.rs` — 7 correctness tests.
- `README.md` — tier-2 unsafe inventory updated (heap_core_alloc.rs 2→4 sites:
  the two hardened `bump_gen` call-sites in `alloc_batch`); total 33→35.
- `docs/ARCHITECTURE.md` — `tests/*.rs` count 179→180.
- Raw logs: `docs/perf/_raw_r10_7_warm_arm.log`, `_raw_r10_7_tcache_isolated.log`,
  `_raw_r10_7_d_vs_f.log`, `_raw_r10_7_tcache_arm.log` (the last is the
  *confounded* grouped run, kept as evidence of the §2.3 caveat).

No `src/` public API surface added (the new methods are `#[doc(hidden)]`).
`git status --short`: modified `benches/global_alloc.rs`,
`src/registry/heap_core_alloc.rs`, `src/global/sefer_alloc.rs`, `README.md`,
`docs/ARCHITECTURE.md`; new `tests/batch_tcache.rs`, this report, and the raw
logs. **No commit made** — left for orchestrator review.
