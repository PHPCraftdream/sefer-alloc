# R24-11 — `bench_global_alloc_churn_with_teardown`@1024B residual root-cause: **pool-cap-exceeded (i)**

**Task #389 (R24-11), Round 24.** Follow-up to R24-10 (task #388), which
established the *mechanism* behind the 1024B teardown residual (the segment
decommit/release/re-reserve lifecycle that Mechanism-2's pool was built to
absorb) but did **not** determine which of three candidate explanations
dominates the *current* (post-Mechanism-2) residual number. R24-11 measures
process-wide counter deltas across the run to decide between:

- **(i)** the 4-segment / 16 MiB pool cap is exceeded by this bench's
  working-set/OPS/sample-size shape at 1024B (pool thrashes: fills, evicts,
  re-reserves anyway);
- **(ii)** the pool's decay tick (`maybe_decay_small_pool`,
  `src/alloc_core/alloc_core_small_pool.rs:516`) evicts entries *between*
  criterion iterations (the pool does not survive across the timed samples);
- **(iii)** residual per-free magazine-overflow batch-flush cost, independent
  of the pool (the "240 of 256 frees bypass the magazine" path itself, not the
  segment-lifecycle path).

**Verdict: (i) — the 4-segment pool cap is exceeded by this bench's 1024B
access pattern.** (ii) is ruled out by event count vs the decay interval; (iii)
is ruled out by a cross-size argument (batch-flush is present at every size,
yet Sefer is at parity everywhere it does *not* trip the pool). Measured
evidence and reasoning in §2–§4. This is a **config-tuning / stress-shape**
finding, not a code defect — no production logic is changed (scope was
root-cause + report only).

**Date:** 2026-07-28. **Base revision measured:** `main` @ `ce17311`.
**Platform:** native Windows x86-64 (shared host — wall-clock is inherently
noisy; the **counter deltas are the reliable signal**, the µs figures are
directional point estimates). **Feature set:** `production` (= `alloc-global`,
`alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory`,
`primordial-lazy-commit`, `class-aware-dirty`).

**Units correction (OBSERVED).** The R24-10 task brief carried the 1024B
figure as "106.1 **ns**/op". The criterion output is in **microseconds**: the
point estimate is **123.94 µs/op** (Sefer) vs **46.09 µs/op** (mimalloc) =
**2.69×**. The *ratio* in the brief (≈2.64×) was correct; only the absolute
*units* slipped (ns → µs). All numbers below are cited from the committed raw
logs in µs.

---

## 0. Headline

| size | Sefer w/ teardown (µs/op) | mimalloc w/ teardown (µs/op) | Sefer / mimalloc | `decommit_calls` Δ | `segments_released_total` Δ |
|---|---:|---:|---:|---:|---:|
| 16B   | 24.42  | 21.26 | 1.15× | **0**   | **0**   |
| 64B   | 23.35  | 22.20 | 1.05× | **0**   | **0**   |
| 256B  | 28.25  | 28.31 | 1.00× | **0**   | **0**   |
| 1024B | 123.94 | 46.09 | **2.69×** | **248** | **248** |

The pool/decommit path fires **only at 1024B** (248 events), exactly the one
size where Sefer falls behind mimalloc. At 16/64/256B the deltas are **0** and
Sefer is at parity. (`bench_global_alloc_churn_with_teardown`'s own per-size
`eprintln!` counter deltas, added R24-11, are the citation; raw log
`_raw_r24_11_churn_with_teardown.log`.)

---

## 1. Method

`bench_global_alloc_churn_with_teardown` (`benches/global_alloc.rs`) did not
previously emit counter deltas. R24-11 mirrors `bench_working_set_cycle`'s
established diagnostic exactly: for each size, snapshot `sefer.stats()` before
and after the whole rotated 3-arm block and `eprintln!` the
`decommit_calls` (= `AllocCore::dbg_decommit_count`) and
`segments_released_total` (= `AllocCore::dbg_segments_released_total`) deltas.
Both are **process-wide monotonic** atomics; mimalloc/System never touch
Sefer's statics, so the delta around the rotated block is Sefer-only. The
snapshots wrap the *outside* of `bench_three_arms_rotated` (like
`bench_working_set_cycle` wraps `group.bench_function`), so the timed routine
is untouched — the reported ns/op is unaffected (only diagnostic `eprintln!`
was added).

> **Counter reachability note (why `dbg_pooled_count` was not measured).** The
> task brief listed `AllocCore::dbg_pooled_count()` as a third counter to read.
> It is a **per-instance** method, and `SeferAlloc::current_heap()` (the only
> path from the bench's `SeferAlloc` to the per-thread `HeapCore`) is
> **private** (`fn current_heap`, `src/global/sefer_alloc.rs:282`). It is
> therefore **not reachable from the bench without a production-source change**
> (explicitly out of scope for this task). The two *process-wide* counters
> exposed via `sefer.stats()` are sufficient and decisive — they cleanly
> separate "pool engaged" (nonzero delta ⇒ i/ii) from "pool not engaged" (zero
> delta ⇒ iii), and the decay-interval argument (§4) then separates i from ii.
> `dbg_pooled_count` would only have *confirmed* the pooled occupancy; it is
> not needed to reach the verdict.

Three runs, all `cargo bench --features production --bench global_alloc -- <filter>`:

1. `-- "global_alloc_churn_with_teardown"` → `_raw_r24_11_churn_with_teardown.log`
2. `-- "working_set_cycle"` → `_raw_r24_11_working_set_cycle.log`
3. `-- "global_alloc_churn/SeferAlloc"` (the no-teardown sibling, Sefer arm
   only) → `_raw_r24_11_churn_no_teardown_sefer.log` (enables the
   teardown-attributable cost decomposition in §3).

---

## 2. Measured counter deltas (decisive evidence)

### 2.1 `bench_global_alloc_churn_with_teardown` (Sefer deltas)

| size | `decommit_calls` Δ | `segments_released_total` Δ |
|---|---:|---:|
| 16B   | 0   | 0   |
| 64B   | 0   | 0   |
| 256B  | 0   | 0   |
| 1024B | **248** | **248** |

### 2.2 `bench_working_set_cycle` (Sefer deltas — the canonical Mechanism-2 judge)

| size | ns/op (point) | `decommit_calls` Δ | `segments_released_total` Δ |
|---|---:|---:|---:|
| 16B   | 217 µs | 0 | 0 |
| 64B   | 226 µs | 0 | 0 |
| 256B  | 256 µs | **173** | **173** |
| 1024B | 271 µs | **373** | **373** |

`working_set_cycle` corroborates the same mechanism at a *different* size
threshold: its 64-concurrent-working-set shape trips the 4-cap starting at
**256B** (173) and worse at 1024B (373); this bench's 256-block-single-segment
shape trips it *only* at 1024B (248). The `working_set_cycle` doc
(`benches/global_alloc.rs` ~line 994) already recorded the historical
"173/367 … because demand exceeds the hard-capped 4-segment pool" figure
(PASS-3); today's **173/373** reproduces it (the 256B count is byte-identical,
1024B is within noise of the old 367). The pool-exceeding-at-larger-sizes
behavior is thus a **known, stable** property of the default 4-cap against
working-set-cycling shapes — R24-11's contribution is confirming
`churn_with_teardown`@1024B is the same class, and that (iii) is *not* the
residual.

---

## 3. Teardown-attributable cost decomposition (rules out iii)

Subtracting the no-teardown sibling (`bench_global_alloc_churn`, Sefer arm)
from the with-teardown bench isolates the pure teardown cost per iteration
(both time `churn_step(OPS=1024)`; only the with-teardown variant also runs
`churn_teardown(256)` inline):

| size | churn no-teardown (µs) | with-teardown (µs) | **teardown cost (µs)** | decommits/run |
|---|---:|---:|---:|---:|
| 16B   | 18.92 | 24.42  | **5.50**  | 0 |
| 64B   | 18.31 | 23.35  | **5.04**  | 0 |
| 256B  | 19.09 | 28.25  | **9.16**  | 0 |
| 1024B | 22.09 | 123.94 | **101.85** | **248** |

Teardown cost is **flat at ~5–9 µs** across 16/64/256B (the magazine-overflow
batch-flush of 256 frees — ~30 `flush_class` events at ~571 Ir each per
R24-2, ≈ 5–6 µs, size-independent), then **jumps to ~102 µs at 1024B**, where
*and only where* the 248 decommits fire. The ~93 µs excess (102 − 9) is
entirely correlated with the pool/segment-lifecycle path engaging. **Because
batch-flush (iii) is present and approximately constant at every size yet
Sefer is at parity wherever the pool is not engaged, (iii) cannot explain the
1024B-only slowdown.** This is the cleanest single argument against (iii).

---

## 4. Why (i), not (ii) or (iii)

- **Against (iii) — batch-flush.** §3: batch-flush is size-flat (~5–9 µs) and
  present at the parity sizes (16/64/256B, 0 decommits); the explosion is
  1024B-exclusive and coincides with the pool engaging. Correlation, not
  batch-flush.
- **Against (ii) — decay tick.** `maybe_decay_small_pool` evicts **at most one
  pooled segment per `decay_interval`** (default **1000 ms**,
  `src/alloc_core/large_cache_config.rs:21`) and only on the
  `reserve_small_segment` cold path. The 1024B Sefer arm's warmup+measurement
  window is ~0.75 s (150 ms warmup + ~600 ms measurement), so decay could fire
  **≤ 1** eviction in the entire run. The measured **248** is ~3 orders of
  magnitude beyond that ⇒ the segments are not being lost to the idle-decay
  timer; they are being decommitted because the cap is genuinely exceeded.
- **For (i) — cap exceeded.** By elimination and direct correlation: the 4-cap
  is exceeded by this bench's full-teardown-every-iteration shape at 1024B.
  Why 1024B and not smaller sizes: a 4 MiB small segment holds ~4096 blocks at
  1024B but ~262144 at 16B. The 256-block working set plus magazine-retained
  blocks plus the per-iteration carve-frontier advance exhaust a 1024B segment
  every handful of iterations, churning segments through the 4-cap faster than
  it absorbs them; at 16/64/256B the working set sits in a single segment that
  never exhausts, so the cap is never tripped (0 decommits).

### 4.1 Per-iteration cost shape (inferred, not directly per-event-measured)

The 248 decommits are an **event marker**, spread across the whole run
(criterion estimated ~4675 routine invocations for the 1024B Sefer arm ⇒
≈ 1 decommit per ~19 teardowns, ~5%). A naive "248 discrete decommits /
4675 iterations" amortization cannot by itself account for the full ~93 µs/iter
teardown excess — the `decommit_calls` counter records *that a decommit
happened*, not its per-event cost, and it does **not** capture the accompanying
segment **reserve / re-commit / header-setup** costs of the cap-exceeded
regime (`segments_reserved_total` was not snapshotted in this measurement).
The numbers are nevertheless *consistent* with the dominant cost being
concentrated in the ~5% of teardowns that trigger a full 4 MiB segment
lifecycle (reserve + commit + populate + later decommit + release + re-reserve
≈ low-ms each on Windows), which averages up to the observed ~102 µs/iter;
the cheap ~95% of teardowns look just like the 16B case (~5.5 µs). **Honest
caveat:** this per-event decomposition is an inference from the run-average +
event count, not a direct per-event timing; a follow-up could pin it down by
also snapshotting `segments_reserved_total` and timing isolated
reserve/decommit cycles. It does not affect the i-vs-ii-vs-iii verdict, which
rests on the (unit-independent, exact) counter deltas and the §3 cross-size
argument.

---

## 5. Implications of (i) — config-tuning, not a code defect

Per the task brief's "if (i) dominates" branch, this is reported, **not**
acted on (no default changed, no production file touched):

- **The cap exists to bound retained RSS** (`small_segment_pool_config.rs`:
  default `pool_segments = 4`, `pool_byte_cap = 16 MiB` — "retain at most 4
  empty small segments … and at most 16 MiB of committed pool RSS", per-thread
  via `HeapRegistry::claim`). Raising it would let more emptied segments be
  retained → fewer decommits → faster for *this* bench, at the cost of higher
  per-thread committed RSS (each retained segment is 4 MiB, mostly empty at
  1024B — 256/4096 ≈ 6% utilized when pooled, so RSS-inefficient).
- **This bench is a stress shape, not a representative workload.** A full
  256-block teardown *every iteration* (free the entire working set each cycle)
  is deliberately harsh; real workloads rarely free their whole working set on
  every cycle. The bench's value is as a **regression canary** for
  Mechanism-2 (§7), not as a typical-load proxy.
- **Flagged next step (NOT attempted here):** if closing the 1024B residual is
  desired, the warranted investigation is an **RSS gate** — sweep
  `pool_segments` (e.g. 4/8/16/32) with a generous `pool_byte_cap`, measuring
  both the `decommit_calls` delta (should fall) **and** peak RSS (will rise) at
  1024B, mirroring the established `bench_pool_cap_sweep` /
  `pool_cap_sweep_spread_and_drain` harness pattern already in this bench file
  (`POOL_CAP_SWEEP_VALUES`, gated `alloc-decommit + alloc-xthread`). The
  decision is a deliberate RSS-vs-throughput trade, not an obvious win.
- **Do not change the default without that RSS gate** (task constraint). Either
  outcome is defensible: a cap raise that passes an RSS gate, or documenting
  this bench as an intentionally pool-exceeding stress canary (consistent with
  its existing "deliberate diagnostic" framing — §7).

---

## 6. What this rules in/out for the sibling OPEN_ITEMS item 1

The task's "if (iii) dominates" branch (add a wall-clock-victim-workload data
point to OPEN_ITEMS item 1, the `contains_base`/free-path item that tracks
R24-3's `flush_magazine_class` NO-GO) **does not apply** — (iii) was refuted.
The magazine-overflow batch-flush path is *not* this bench's 1024B residual;
item 1's R24-3 NO-GO therefore has no new wall-clock victim here. (R24-3's
`flush_class` lever remains the item-1 "larger untried lever" on its own
merits; this bench is not evidence for or against it.)

---

## 7. Doc-comment fix applied (`benches/global_alloc.rs`)

`bench_global_alloc_churn_with_teardown`'s doc comment said it was kept "until
task #51 lands Mechanism-2" and framed the teardown gap as the segment
lifecycle cost — both **stale** (Mechanism-2 has landed and is in
`production`). Reworded to its current role: a **regression canary** for
Mechanism-2. Verified sibling claim before asserting it: **both** sibling
churn benches (`bench_global_alloc_churn` `:542` and
`bench_global_alloc_churn_write` `:715`) move teardown outside the timed region
via `ChurnTeardownGuard` (its `drop` runs `churn_teardown` untimed); this is
the **sole** bench in the file that times teardown inline — so it is the only
bench that would surface a pool/cap/decay/`alloc-decommit`-drop regression.
The comment now also records the R24-11 characterization (0 decommits at
16/64/256B = pool holds = parity; 248 at 1024B = cap-exceeded stress case =
the ~2.7× residual) and points the per-size counter deltas (added R24-11) at
their canary purpose: the 16/64/256B deltas should read 0 — a nonzero value
there means the pool regressed at a size that should fit in one segment. The
bench itself was **not** deleted or "fixed" (its own comment forbids that, and
the canary role makes it more valuable now, not less).

---

## 8. `docs/perf/OPEN_ITEMS.md` entry added

A new entry recording this re-measurement + verdict, in the R24-9 "Current
state" card format (Status / Current number-or-verdict / Next trigger /
Evidence) that every item now leads with. Records that **neither** perf index
had previously tracked "was Mechanism-2's effectiveness against this bench's
1024B number ever re-measured after it landed" — exactly the silently-dropped
follow-up class the CLAUDE.md "Phased delivery" convention (R18-8 / R22-3
lessons) exists to prevent. See the entry in `OPEN_ITEMS.md` (new item under
the [A] tier).

---

## 9. Files changed

| file | change |
|---|---|
| `benches/global_alloc.rs` | (1) doc comment of `bench_global_alloc_churn_with_teardown` rewritten to canary role + R24-11 characterization (§7); (2) per-size `sefer.stats()` before/after snapshots + `eprintln!` of `decommit_calls`/`segments_released_total` deltas added around the rotated block, mirroring `bench_working_set_cycle` (§1). No timed-routine change. |
| `docs/perf/R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE.md` | this report (new) |
| `docs/perf/R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r24_11_churn_with_teardown.log` | raw criterion + counter-delta output, run 1 (`.gitignore`d — `git add -f` at commit time) |
| `docs/perf/_raw_r24_11_working_set_cycle.log` | raw criterion + counter-delta output, run 2 (`git add -f` at commit time) |
| `docs/perf/_raw_r24_11_churn_no_teardown_sefer.log` | raw criterion output, run 3 (no-teardown sibling, Sefer arm — for the §3 decomposition; `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | new entry (§8) |

**No production source file touched** (`src/` unchanged). **No commit made**
— tree left unstaged for personal zero-trust review.

---

## 10. Reproduce

```
cargo bench --features production --bench global_alloc -- "global_alloc_churn_with_teardown"
cargo bench --features production --bench global_alloc -- "working_set_cycle"
cargo bench --features production --bench global_alloc -- "global_alloc_churn/SeferAlloc"
```

The `decommit_calls` / `segments_released_total` deltas appear on stderr
(`eprintln!`) interleaved with the criterion tables. Counter deltas are exact
(relaxed-atomic loads); µs/op are noisy point estimates on this shared host.
