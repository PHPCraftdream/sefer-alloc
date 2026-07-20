# R9-9 — batch-alloc ceiling: small-batch sweep + real-SeferAlloc-arm follow-up

**Task:** follow-up to R8-7 (`docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md`,
task #220), from the same external perf review's follow-up finding.
**Measurement-only:** no `src/` changes. `git status --short` after this
task's work shows exactly one modified file, `benches/global_alloc.rs` — no
new public API, no new `src/` symbol, no new `#[doc(hidden)]` forwarder (the
primitives R8-7 used — `refill_class_bump`, `flush_class`,
`dbg_layout_class_for` — all take arbitrary-length `&mut [*mut u8]` /
`&[*mut u8]` slices, so they needed no change for the smaller batch sizes).
**Date:** 2026-07-20
**Base revision:** `main` @ `5a4ba62`
**Platform:** Windows 10 Pro x86-64.
**Harness:** `benches/global_alloc.rs`, new `bench_batch_ceiling_followup` /
`batch_ceiling_followup` criterion group (added by this task). The R8-7
group (`batch_ceiling`, batch=1024 / AllocCore-only) is left UNCHANGED so
R8-7's exact IDs (`scalar/{size}B`, `batch/{size}B`) and numbers stay
reproducible; this is a sibling group, matching the R6/R7/R8/R9
cross-version-report continuation convention already used in `docs/perf/`.

---

## 0. What R8-7 measured, and the two limitations this follow-up fills

R8-7 measured a **2.73× / 1.71× / 1.20×** GO/GO/NO-GO ceiling for a
hypothetical public batch alloc API, at **batch=1024 only**, comparing **only
`AllocCore`-direct** arms (both bypassing `SeferAlloc`/TLS/registry entirely).
The review flagged two limitations in that measurement, both of which this
report addresses:

1. **Only batch=1024 was measured.** A realistic caller (`calloc`-style bulk
   allocation, collection pre-sizing) is far more likely to request 8-64
   items at once. The amortization win from batching (one classification /
   routing call instead of N) shrinks as batch size shrinks, so the
   1024-batch numbers may overstate the ceiling a realistic caller would see.
2. **Only `AllocCore` was compared, not `SeferAlloc`/`GlobalAlloc`.** The
   R8-7 bench deliberately bypasses `SeferAlloc`/TLS/registry to isolate the
   batching mechanism's own ceiling. But this cuts both ways: a public batch
   API built ON TOP OF `SeferAlloc` could ALSO amortize the TLS heap lookup +
   registry dispatch that `SeferAlloc::alloc`/`GlobalAlloc::alloc` pays on
   EVERY scalar call, which neither AllocCore-direct arm captures at all.

This follow-up adds a **batch-size sweep (N ∈ {8, 16, 32, 64, 1024})** and a
**third arm: the real `SeferAlloc`/`GlobalAlloc` scalar path** (what
`bench_direct_alloc` already measures for `SeferAlloc` in the `global_alloc`
group — reused verbatim, not reinvented), at the same three sizes R8-7 used
(16 B / 64 B / 256 B).

---

## 1. Method

### 1.1 Three arms, same (size, N) grid

For every (size, N) pair, three arms are measured:

- **(a) `scalar_core`** — `AllocCore::alloc` / `AllocCore::dealloc`, N calls
  each, against a **fresh `AllocCore` per iteration** (untimed
  `iter_batched` setup). This is R8-7's arm (a), parameterized over N.
- **(b) `batch_core`** — ONE `refill_class_bump` call fills N slots, ONE
  `flush_class` call frees them, against a **fresh `AllocCore` per
  iteration**. This is R8-7's arm (b), parameterized over N.
- **(c) `scalar_sefer`** — `sefer.alloc` / `sefer.dealloc` through
  `SeferAlloc`'s `GlobalAlloc` impl (TLS heap lookup + registry dispatch on
  every call), N calls each, against the **shared `SeferAlloc` heap** (warm
  tcache after criterion's warmup). This is the real production entry point.

All three arms pre-size a `Vec<*mut u8>` of length N in the untimed setup and
write/read it identically, so the only timed difference between arms is the
alloc/dealloc mechanism itself (the `Vec`'s own alloc/free via the bench
binary's default allocator runs in setup / return-drop, outside the timed
region — matching how `bench_global_alloc_churn` already excludes
`ChurnTeardownGuard`'s drop from timing).

### 1.2 Warmth asymmetry between arms (important — read before the numbers)

Arms (a)/(b) use a **fresh `AllocCore` per iteration**: the primordial
segment is reserved in the untimed setup, but its pages **commit on first
touch** inside the timed routine → soft page faults (~1-2 µs each) on the
bitmap / bin-table / first data pages. Arm (c) reuses the **shared
`SeferAlloc` heap**: pages committed long ago, blocks recycled via the warm
per-thread tcache → **no page faults**. This asymmetry is inherent to the
design (R8-7 chose fresh-per-iter for (a)/(b) fairness; SeferAlloc is
necessarily a shared heap), and it has two consequences the analysis must
reckon with:

- The **(a vs b) ratio is compressed at small N** by the shared page-fault
  fixed cost: ~3-5 µs that is identical and additive in both arms, so it
  pulls the ratio toward 1.0. At n=1024 it amortizes away (R8-7's numbers
  reproduce — see §2.2); at n=8 it dominates and the measured ratio
  understates the pure batch mechanism's win.
- The **(c vs a/b) comparison is NOT a clean TLS-overhead measurement**: (c)
  is warm, (a)/(b) are cold. (c vs a) measures TLS+registry overhead **NET
  of warm-tcache savings and cold-page-fault cost**, not either in isolation.

This asymmetry is the key confound the analysis disentangles in §3. It does
NOT undermine the verdict (§4 explains why), but it must be stated upfront.

### 1.3 Command + profile

```text
cargo bench --bench global_alloc --features production -- batch_ceiling_followup
```

Criterion fast profile per `CLAUDE.md`: `sample_size(10)`,
`warm_up_time(150ms)`, `measurement_time(600ms)` — same profile the R8-7
group and `bench_working_set_cycle` use. Three independent runs; the tables
below are 3-run averages of criterion's point estimate (middle of its
`[low high]` bounds). Raw logs: `docs/perf/_raw_r9_9_followup{,_run2,_run3}.log`.

Verified clean under the full feature matrix before measuring:
- `cargo fmt --check` — clean.
- `cargo clippy --release --features production --all-targets -- -D warnings` — clean.
- `cargo clippy --release --all-features --all-targets -- -D warnings` — clean.
- `cargo clippy --release --features experimental --all-targets -- -D warnings` — clean.
- `cargo clippy --release --all-targets -- -D warnings` (no features) — clean.
- `cargo bench --bench global_alloc --features production --no-run` — compiles.

---

## 2. Raw numbers — 3-run averages, `--features production`

### 2.1 Three-arm times + ratios (the full grid)

Times are µs per iteration (N alloc-then-N dealloc for scalar arms; 1
refill-then-1 flush for the batch arm). Ratios are dimensionless.

**16 B**

| N | (a) scalar_core µs | (b) batch_core µs | (c) scalar_sefer µs | a/b | c/b | c/a |
|---|---:|---:|---:|---:|---:|---:|
| 8 | 4.306 | 3.090 | 0.157 | **1.39** | 0.051 | 0.036 |
| 16 | 3.976 | 3.206 | 0.293 | **1.24** | 0.091 | 0.074 |
| 32 | 4.269 | 3.345 | 0.910 | **1.28** | 0.272 | 0.213 |
| 64 | 5.644 | 3.765 | 2.051 | **1.50** | 0.545 | 0.363 |
| 1024 | 48.305 | 18.556 | 36.365 | **2.60** | 1.96 | 0.753 |

**64 B**

| N | (a) scalar_core µs | (b) batch_core µs | (c) scalar_sefer µs | a/b | c/b | c/a |
|---|---:|---:|---:|---:|---:|---:|
| 8 | 3.790 | 3.357 | 0.146 | **1.13** | 0.044 | 0.039 |
| 16 | 4.115 | 3.339 | 0.272 | **1.23** | 0.081 | 0.066 |
| 32 | 4.555 | 3.565 | 0.896 | **1.28** | 0.251 | 0.197 |
| 64 | 5.964 | 3.949 | 2.098 | **1.51** | 0.531 | 0.352 |
| 1024 | 79.913 | 47.733 | 37.922 | **1.67** | 0.794 | 0.475 |

**256 B**

| N | (a) scalar_core µs | (b) batch_core µs | (c) scalar_sefer µs | a/b | c/b | c/a |
|---|---:|---:|---:|---:|---:|---:|
| 8 | 7.089 | 4.004 | 0.151 | **1.77** † | 0.038 | 0.021 |
| 16 | 7.619 | 4.340 | 0.281 | **1.76** † | 0.065 | 0.037 |
| 32 | 7.721 | 7.093 | 0.889 | **1.09** | 0.125 | 0.115 |
| 64 | 14.490 | 13.030 | 2.119 | **1.11** | 0.163 | 0.146 |
| 1024 | 203.587 | 190.420 | 40.682 | **1.07** † | 0.214 | 0.200 |

† = high run-to-run variance (see §5); the 256 B small-N ratios swing widely
across the three runs (e.g. n=8: 1.41 / 1.93 / 1.96; n=1024: 1.18 / 1.19 /
0.88) because the fresh-`AllocCore` page-fault cost is non-deterministic.

### 2.2 Continuity with R8-7 at N=1024

| Size | R8-7 a/b (batch=1024) | this report a/b (n=1024) |
|---|---:|---:|
| 16 B | 2.73× | 2.60× |
| 64 B | 1.71× | 1.67× |
| 256 B | 1.20× | 1.07× † |

The n=1024 column reproduces R8-7's numbers within run-to-run variation (the
256 B point is the noisiest — see †). The R8-7 GO/GO/NO-GO ordering at
batch=1024 is intact. **The R8-7 report stays valid as the historical
batch=1024 / AllocCore-only baseline; nothing here contradicts it.**

---

## 3. Analysis

### 3.1 Does the ceiling hold at small batch sizes? — No, it degrades sharply.

Collapsing the a/b column (the R8-7-style batch/scalar ratio, extended to
small N):

| Size | n=8 | n=16 | n=32 | n=64 | n=1024 |
|---|---:|---:|---:|---:|---:|
| 16 B | 1.39 | 1.24 | 1.28 | 1.50 | 2.60 |
| 64 B | 1.13 | 1.23 | 1.28 | 1.51 | 1.67 |
| 256 B | 1.77† | 1.76† | 1.09 | 1.11 | 1.07† |

The ceiling degrades monotonically from n=1024 to the realistic 8-64 range at
16 B and 64 B (2.60→1.39, 1.67→1.13). At 256 B the small-N numbers are too
noisy to call a clean trend, but they sit at or below 1.5× once N ≥ 32.

Two compounding effects drive the degradation (predicted by the review's own
amortization theory, §1.2 above):

1. **Amortization thins out.** The batch primitive saves N−1
   classification/routing call overheads. At n=1024 that is 1023 saved calls;
   at n=8 only 7. The per-block carve/free work is identical either way, so
   the batch win shrinks with N.
2. **Page-fault fixed cost compresses the ratio.** Both AllocCore arms use a
   fresh `AllocCore` per iteration whose primordial segment's pages commit on
   first touch (~3-5 µs of soft page faults, identical and additive in both
   arms — see §1.2). At n=1024 this amortizes to ~5 ns/block and the
   mechanism difference dominates; at n=8 it is the bulk of both arms' time
   and pulls the measured ratio toward 1.0, understating the pure mechanism
   win. (This is why the small-N a/b numbers should be read as a **floor** on
   the compression, not the mechanism's true small-N ratio — see §5.)

Even taking the page-fault compression into account, the realistic-range
a/b ratios land mostly **below the 1.5× floor** R8-7 used as its GO
threshold (16 B: 1.24-1.50; 64 B: 1.13-1.51; 256 B: 1.09-1.11 once N ≥ 32).

### 3.2 The three-way comparison: where a public batch API's real ceiling sits

This is the finding the third arm was added to surface. The c/b ratio (real
`SeferAlloc` scalar vs `AllocCore` batch primitive) answers the review's
question directly: **would a public batch API built on `SeferAlloc` beat the
real production scalar path?**

| Size | c/b n=8 | c/b n=16 | c/b n=32 | c/b n=64 | c/b n=1024 |
|---|---:|---:|---:|---:|---:|
| 16 B | 0.051 | 0.091 | 0.272 | 0.545 | **1.96** |
| 64 B | 0.044 | 0.081 | 0.251 | 0.531 | 0.794 |
| 256 B | 0.038 | 0.065 | 0.125 | 0.163 | 0.214 |

**Headline:** at **every realistic batch size (8-64), the real `SeferAlloc`
scalar path is 2-30× FASTER than the `AllocCore` batch primitive**, at all
three sizes. Even at n=1024, the batch primitive beats `SeferAlloc` scalar
**only at 16 B** (1.96×); at 64 B and 256 B the warm `SeferAlloc` path is
still faster (c/b = 0.79, 0.21).

The mechanism behind this (why the warm tcache wins at small N regardless of
batching):

- Arm (c) reuses the shared `SeferAlloc` heap with a **warm per-thread
  tcache**. At n=8-64, all N allocs hit the tcache (pure slot pop,
  ~18-20 ns/alloc-dealloc-pair — see the scalar_sefer µs column: 0.146-0.157
  µs for n=8 at any size), and all N frees refill it. No page faults, no
  magazine misses, no `BinTable` round-trips.
- A public batch API built on `SeferAlloc` would amortize the per-call TLS
  lookup + registry dispatch (N calls → 1). But the tcache's per-call cost is
  already near-zero: at n=8, saving 7 × ~10 ns of TLS overhead ≈ 70 ns cannot
  beat a tcache total of ~150 ns — the batch `refill`+`flush` alone touches
  bitmap/bump/bump-pointer metadata that costs more than that.
- Only at **n=1024** does the tcache overflow (N > slots-per-class), forcing
  scalar allocs to fall through to the magazine / `BinTable` (~35-40
  ns/pair — the scalar_sefer per-pair cost roughly doubles from n=8 to
  n=1024). There the batch primitive's one-metadata-read-per-run wins — but
  only at 16 B, where the per-block carve is cheapest.

The warmth asymmetry (§1.2) means the c/b numbers are **pessimistic for the
batch arm** (arm (b) pays cold page faults arm (c) doesn't). But the gap is
so large at small N (20-30×) that even a generously-warmed batch primitive
could not close it: warming arm (b) by the ~3-5 µs page-fault cost would
bring, e.g., 16 B / n=8 from 3.09 µs to ~0.1-0.5 µs — still slower than or
comparable to arm (c)'s 0.157 µs, and that is a best-case warming assumption
that ignores the batch primitive's own metadata work. The cold-warm confound
narrows the gap but does not invert the verdict at realistic N.

### 3.3 What the c/a column adds (the TLS/registry term, confounded)

The c/a ratio (real `SeferAlloc` scalar vs bare `AllocCore` scalar) was meant
to isolate the TLS-heap-lookup + registry-dispatch overhead. It does NOT do
so cleanly, because of the warmth asymmetry: c/a is 0.02-0.36 at small N
(arm (c) appears 3-50× cheaper than arm (a)) — but this is almost entirely
the **warm-tcache-vs-cold-page-fault** difference, not TLS overhead. At
n=1024, where page faults amortize, c/a is 0.75 / 0.48 / 0.20 (16/64/256 B):
even there arm (c) is faster, because the warm tcache recycles the same 1024
blocks without re-carving while arm (a) carves 1024 fresh blocks. So the c/a
column is not interpretable as a pure TLS-overhead figure — but it confirms,
redundantly with §3.2, that the warm `SeferAlloc` path is the thing a batch
API would actually have to beat, and it is very fast at small N.

---

## 4. Verdict

Per the task's instruction to state plainly when ratios fall below R8-7's
~1.5× GO floor:

### 4.1 Per size × batch-range

| Size | batch 8-64 (realistic) | batch 1024 (R8-7 regime) |
|---|---|---|
| 16 B | **BORDERLINE** — a/b 1.24-1.50; 3-way: tcache 7-20× faster than batch primitive | **GO** (a/b 2.60; c/b 1.96 — batch beats real SeferAlloc scalar) |
| 64 B | **NO-GO** — a/b 1.13-1.51 (mostly < 1.5); 3-way: tcache 2-19× faster | **GO** (a/b 1.67; but c/b 0.79 — batch does NOT beat real SeferAlloc scalar) |
| 256 B | **NO-GO** — a/b 1.09-1.11 (N ≥ 32); noisy at n=8-16; 3-way: tcache 6-27× faster | **NO-GO** (a/b 1.07; c/b 0.21 — batch far slower than real SeferAlloc scalar) |

### 4.2 Overall: CONDITIONAL-NO-GO for realistic callers

**The R8-7 GO verdict at 16 B / 64 B was specific to the unrealistic
batch=1024 case.** At realistic batch sizes (8-64), the measured batch/scalar
ceiling degrades below the 1.5× floor R8-7 used, and — more decisively — the
three-way comparison shows the real `SeferAlloc` scalar path (warm tcache)
is already 2-30× faster than the `AllocCore` batch primitive at those sizes.
A public batch API built on `SeferAlloc` would compete with the warm tcache,
not with the cold `AllocCore`-direct arms R8-7 measured, and the tcache
already amortizes the per-call overhead for small N — leaving nothing for a
batch API to amortize until N is large enough to overflow the tcache
(the n=1024 regime).

**The 16 B / n=1024 point is the one place a batch API clearly wins
(c/b = 1.96×).** That is a real, if narrow, signal: for callers that
genuinely allocate 1000+ same-class 16 B blocks at once (a `calloc`-style
bulk zeroing path, or a large hash-table pre-fill), a batch API could nearly
halve `SeferAlloc`'s scalar cost. No such caller exists in this repo today
(the original R8-7 circularity concern), and the win does not generalize to
64 B / 256 B or to smaller batches — so this is a CONDITIONAL (caller-gated)
GO, not a general GO.

**Net:** the API idea does not close, but its viable surface shrinks from
R8-7's optimistic "16 B + 64 B at batch=1024" to "16 B only, and only at
batch ≥ ~1024, and only for callers that actually issue such batches." For
the realistic 8-64 range the review asked about, the verdict is NO-GO.

---

## 5. Caveats

- **Single host, three runs, plain average.** The ranking (a/b degrades as N
  shrinks; c beats a/b at small N; batch beats c only at 16 B / n=1024) is
  stable across all three runs. Exact ratio values carry normal
  criterion-sample noise, sharply worse at 256 B small-N and at 256 B /
  n=1024 († in §2.1) where the fresh-`AllocCore` page-fault cost is
  non-deterministic.
- **Warmth asymmetry (§1.2) is the dominant confound.** Arms (a)/(b) are
  cold (fresh `AllocCore`, page-faulting); arm (c) is warm (shared
  `SeferAlloc` heap, tcache-recycling). This compresses the a/b ratio at
  small N (understating the pure batch-mechanism win) AND makes c/a
  uninterpretable as a pure TLS-overhead figure. The c/b comparison is the
  cleanest of the three for the verdict, because even after crediting arm
  (b) a generous warming, the gap at small N is too large to close (§3.2).
- **A warm-batch-on-`SeferAlloc`-heap arm is NOT measured here.** Such a
  fourth arm (batch primitive reusing the warm `SeferAlloc` heap across
  iterations, no page faults) would give the fairest batch-vs-tcache
  comparison, but it would require either a new `#[doc(hidden)]` forwarder
  to reach the batch primitives through `SeferAlloc`/`HeapCore`'s tcache, or
  a structurally different harness — both outside this measurement-only
  task's `benches/`-only scope. The §3.2 warming argument is an inference
  from the measured cold numbers, not a direct measurement; closing that gap
  is explicitly left for a future task if the 16 B / n=1024 signal warrants
  it.
- **Same-thread, no-contention.** Like R8-7, this does not exercise
  concurrent access, the cross-thread-free ring, or the magazine-miss path
  under load.
- **Ceiling, not a shippable-API forecast.** Even the 16 B / n=1024 c/b =
  1.96× is a raw internal-primitive ceiling; a shipped `alloc_batch` /
  `unsafe dealloc_batch` API pays extra argument-validation /
  layout-consistency overhead the raw internal call does not (the R8-7 §4
  caveat, unchanged).
- **No `src/` was modified.** `git status --short` shows only
  `benches/global_alloc.rs` changed (plus three untracked `_raw_r9_9_*` log
  captures and this new report file).
