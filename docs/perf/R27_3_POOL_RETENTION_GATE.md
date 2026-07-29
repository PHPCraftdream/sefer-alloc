# R27-3 — the pool RETENTION gate (victim-activation-proven cap-4-vs-cap-8 retention cost)

**Task #421 (R27-3), Round 27.** The load-bearing measurement tasks
R27-4/#422 (paired default-change decision), R27-5/#423 (adaptive/process-wide
pool-budget design re-evaluation), and the conditional R27-11/#429 all cite
this report's data. It implements
`docs/reviews/2026-07-28-r26-readonly-review.md`'s "Required retention gate"
verbatim: subprocess-per-arm isolation + R26-1's config self-verification, but
at the **pressure-producing batch 120**, recording peak-live AND post-teardown
AND post-idle RSS/commit, final/max `dbg_pooled_count`, and
decommit/release/reserve counters — with the **cap-4 arm PROVEN to saturate**
(`decommit_delta > 0`) and the **cap-8 arm PROVEN to retain >4 segments**
(`pooled_hw_max > 4`).

**Verdict: cap 8 DOES retain more than cap 4 — REAL and PROVEN, not
RSS-neutral.** Under the pressure-producing batch-120 workload, cap 8 retains a
higher pool high-water than cap 4 (6 vs 4 pooled segments per heap, proven via
`dbg_pooled_count`), which translates to a **post-teardown RSS delta of ~+8 MiB
per materialised heap (~2 segments)** — scaling linearly with thread count (1T:
+8,096 KiB; 8T: +65,560 KiB; 32T: +261,424 KiB; per-heap 8,096 / 8,195 / 8,170).
This **refutes R26-1's "RSS-neutral" framing for the retention axis** — that
framing was valid only as a statement about *peak-live-set* RSS under R26-1's
lower-pressure batch-50 shape, which (as R26-1 §9 already conceded) never
proved victim activation. The retention cost is **not reclaimable by idle**
(the small-pool decay is event-driven, no background thread — pooled_count and
RSS are flat across a 2 s idle window, empirically confirmed) but **IS
reclaimable by explicit drain** (`dbg_drain_small_pool` releases the pooled
segments; RSS drops by ~the pooled count × 4 MiB).

**This task does not change any `src/` default** — measurement only.
`DEFAULT_POOL_SEGMENTS` / `DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`. No
`src/` file was touched at all (the needed read-back accessor
`HeapCore::dbg_pool_cap` already exists from R26-1; `pool_byte_cap` is not
stored in `AllocCore`, so the resolved byte cap is established by
`dbg_pool_cap() == requested_pool_segments`, which proves the byte cap did not
bind).

**Date:** 2026-07-29. **Base revision measured:** `main` @ `aff0aeb` + this
task's uncommitted working tree. **Platform:** native Windows 10 Pro x86-64, 16
logical cores (shared host — RSS/commit deltas are noisy point estimates; the
cap/conflicts/pooled/decommit self-checks are exact). **Feature set:**
`production` + `alloc-stats` (matching R25-5/R26-1/R26-3's build).

---

## 0. Headline numbers — post-teardown retention (median of 3 reps)

All numbers are the **median of 3 repetitions** per `(pool_segments,
thread_count)` cell, with `(min..max)` range in the raw log
(`docs/perf/_raw_r27_3_pool_retention_gate.log`). Every cell was self-verified
before its number was trusted (see §1.2–§1.3).

### Post-teardown RSS — `rss_post_kib` (KiB, median)

| `pool_segments` (byte_cap) | 1 thread | 8 threads | 32 threads | per-heap (8T/32T) |
|---:|---:|---:|---:|---:|
| **4 (16 MiB) — baseline** | **26,652** (26,532..26,680) | **190,296** (190,268..190,828) | **745,936** (745,360..745,992) | 23,787 / 23,311 |
| **8 (32 MiB) — candidate** | **34,748** (34,664..34,748) | **255,856** (255,772..256,340) | **1,007,360** (1,007,008..1,008,080) | 31,982 / 31,480 |
| **cap8 − cap4 Δ** | **+8,096** | **+65,560** | **+261,424** | **+8,195 / +8,170** |

The cap8−cap4 post-teardown retention delta is **~+8 MiB per materialised heap
(~2 segments)** and is **consistent across thread counts** (8,096 / 8,195 /
8,170 KiB/heap) — i.e. it scales **linearly** with thread count, exactly as
R26-1 found for peak-live-set RSS. At 32 threads the aggregate retention cost
is **~255 MiB** (261,424 KiB).

### Pooled-segment retention — proven via `dbg_pooled_count`

| `pool_segments` | pooled high-water (per heap) | pooled final (per heap) | pooled final SUM (all heaps) 1T/8T/32T |
|---:|---:|---:|---:|
| **4** | **4** (reached cap) | **4** | 4 / 32 / 128 |
| **8** | **6** (retained >4) | **5** | 5 / 40 / 160 |

cap 8's pool high-water (**6**) exceeds cap 4's (**4**) by +2 segments; the
post-teardown point sample (pooled final) is 5 vs 4 (+1/heap). The RSS delta
(~+2 segments/heap) is larger than the pooled-final delta (+1/heap) because cap
8 also retains additional **committed-but-not-pooled** segments it never
decommits (`decommit_delta = 0` for cap 8, vs 274–1,446 for cap 4), which cap
4's cap-driven churn releases. Both axes (pooled count + RSS) independently
confirm cap 8 retains more.

### Decommit / saturation (proven)

| `pool_segments` | decommit_delta (median) 1T/8T/32T | verdict |
|---:|---:|---|
| **4** | 274 / 1,226 / 1,446 | **saturated** (decommit_delta > 0 — pool overflowed, extras decommitted) |
| **8** | 0 / 0 / 0 | **no saturation** (cap 8 absorbs the ~6-segment demand) |

This is the **counterfactual** R26-1's batch-50 RSS axis lacked: cap 4's pool
provably overflows the 4-segment cap (decommits), while cap 8 provably does not.
An RSS equality between two arms that never exercised their capacity difference
would be vacuous; here both arms are proven to have exercised it.

---

## 1. Methodology

### 1.1 Subprocess-per-arm isolation (kept from R26-1)

Every `(pool_segments, pool_byte_cap, thread_count, repetition)` tuple runs in
its OWN freshly-spawned OS process (re-exec'ing the same binary via
`std::env::current_exe()` + `std::process::Command` with env vars encoding the
arm). A fresh process has a fresh, empty `HeapRegistry`, so the registry-slot
first-claim-wins reuse bug that invalidated R25-5's RSS axis is eliminated by
construction. Each worker claims its heap via
`HeapRegistry::claim_with_config` (not `SeferAlloc`, sidestepping its private
TLS), following `r13_9_class_aware_dirty_sidecar_rss.rs`'s claim/recycle
precedent. 18 child processes total (2 configs × 3 thread-counts × 3 reps).

### 1.2 Self-verification — the two hard asserts (kept from R26-1)

Each child hard-`panic!`s (not soft-logs) on either failure BEFORE its number is
trusted:

1. **Resolved cap equals requested** — every claimed heap's
   `HeapCore::dbg_pool_cap()` (the resolved `min(pool_segments,
   pool_byte_cap/SEGMENT)`) must equal the requested `pool_segments`. For both
   configs this held (`verified_cap == 4` / `== 8`), which ALSO proves the byte
   cap did not bind (if it had, `dbg_pool_cap()` would be strictly less than
   `pool_segments`). `pool_byte_cap` is not stored in `AllocCore` (only the
   resolved `pool_cap` is), so this equality is the honest read-back proof of
   the resolved byte-cap identity — no new `src/` accessor was needed.
2. **`config_conflicts_total()` delta == 0** — fresh process ⇒ first claim is
   unconditionally the arm's config ⇒ no conflict possible.

**All 18 child runs passed both self-checks** (every CSV row shows
`verified_cap == pool_segments`, `config_conflicts_delta == 0`).

### 1.3 Victim activation — the NEW hard asserts (the core of this gate)

Per the R26 review's "Required retention gate": an RSS number is only
trustworthy if the capacity difference was actually exercised. Each child
additionally hard-`panic!`s:

- **cap-4 arm:** `decommit_delta > 0` — the ~6-segment demand must overflow a
  4-segment pool and force decommits. (Measured: 274 / 1,226 / 1,446 at 1/8/32T.
  `decommit_delta` is a single process-wide relaxed-atomic read — cheapest
  possible, zero hot-path instrumentation — and a nonzero delta is the DIRECT
  proof the pool was full.)
- **cap-8 arm:** `pooled_hw_max > 4` — cap 8 must retain BEYOND cap-4's
  4-segment bound. (Measured: `pooled_hw_max = 6`.) AND `decommit_delta == 0`
  — cap 8 must absorb the demand without saturating. (Measured: 0.)

**All arms passed.** Without these asserts, an "RSS equality" would be vacuous
(this is exactly the gap R26-1 §9 flagged).

### 1.4 Workload fidelity — the pressure-producing batch 120

`SIZE=1024`, `CHURN_WORKING_SET=256`, `OPS=1024`, **`PRESSURE_BATCH_SIZE=120`**
(the batch R25-5/R26-3 established reliably exceeds 4 segments and settles
around 6 — NOT R26-1's batch 50, which fits inside the 4-segment/16 MiB
retention region and never proved victim activation). The churn primitives
(`hc_churn_prefill`/`hc_churn_step`/`hc_churn_teardown`/`hc_run_batch`) are
byte-for-byte copies of `benches/global_alloc.rs`'s via R25-5/R26-1, including
the **batched-setup shape** (`collect batch_size prefills into a Vec UP FRONT,
THEN churn+teardown each`) that R25-5's module doc proved is REQUIRED to
reproduce the segment-pressure signal (a naive sequential loop never trips the
cap). Pressure window: 1.5 s; pooled_count sampled after each batch (cheap
field read).

### 1.5 Retention-over-time sampling (the NEW axis)

Per arm, after the pressure window stops (working set freed, pooled segments
retained), recorded: peak-live RSS/commit (during pressure); RSS/commit
immediately post-teardown; RSS/commit after 100 ms / 1 s / 2 s idle; final AND
high-water `dbg_pooled_count` per heap; `pooled_predrain` (re-sample right
before drain — empirical proof of idle constancy); decommit/release/reserve
counters (`AllocCore::dbg_decommit_count()` / `dbg_segments_released_total()` /
`dbg_segments_reserved_total()`); then an explicit
`HeapCore::dbg_drain_small_pool()` + post-drain RSS/commit. Workers keep their
heaps alive through the idle + drain window so pooled segments stay committed
and observable in process RSS.

### 1.6 Decay mechanism — read from source (not guessed)

The small-segment pool **shares the large-cache decay interval**
(`maybe_decay_small_pool`, `src/alloc_core/alloc_core_small_pool.rs`, reuses
`self.decay_config.decay_interval`; default **1000 ms** from
`large_cache_config.rs::DEFAULT_DECAY_INTERVAL_MS`). It is **event-driven**:
it fires inline on the `reserve_small_segment` cold path during allocation
pressure, evicting one FIFO-oldest pooled segment per tick — **no background
thread**. Pure idle (no allocations) therefore does NOT decay the pool. The
2 s idle window here (> the 1 s interval) empirically confirms this.

---

## 2. The retention cost is REAL — and how it decomposes

The cap8−cap4 post-teardown RSS delta is **~+8 MiB/heap (~2 segments)**, stable
across thread counts. It decomposes into two reclaimability tiers:

| tier | per-heap | what it is | reclaimable by |
|---|---:|---|---|
| pooled segment | ~+4 MiB (1 pooled seg: 5 vs 4) | empty small segment held in the hysteresis pool | `dbg_drain_small_pool` (released; RSS drops) |
| committed (non-pooled) | ~+4 MiB (residual after drain) | a segment cap 8 never decommits that cap 4's churn releases | only thread-exit / recycle (not small-pool drain) |
| **total post-teardown** | **~+8 MiB** | | |

Evidence for the decomposition: draining releases exactly the pooled segments
(cap4: 26,652→10,272 = −16,380 ≈ 4 seg; cap8: 34,748→14,284 = −20,464 ≈ 5 seg),
and the **post-drain residual** Δ (cap8−cap4) drops to ~+4 MiB/heap (1T: +4,012;
8T: +4,104; 32T: +4,078) — i.e. the pooled tier is reclaimed, the
committed-non-pooled tier (~+1 segment) persists.

**Why this is larger than R26-3's +4,100 KiB side-channel observation.** R26-3's
`rss_after_kib` (cap8=34,576 vs cap4=30,476, Δ=+4,100) was a SIDE-EFFECT of its
latency A/B judge, not a controlled retention measurement, and its cap4 figure
(30,476) is higher than this gate's (26,652) because R26-3's 9-batch-together
workload left cap4 with more committed pages. The cap8 figures match closely
(R26-3: 34,576; this gate: 34,748). This gate's controlled,
victim-activation-proven measurement is the authoritative one: **the retention
cost is ~+8 MiB/heap, not ~+4 MiB and not zero.** R26-3's +4,100 KiB was a real
but workload-dependent lower-bound; this gate shows the full cost under the
canonical batch-120 pressure shape.

---

## 3. Decay: no background reclamation during idle (proven empirically)

| metric | post-teardown | +100 ms | +1 s (=decay interval) | +2 s | pre-drain |
|---|---:|---:|---:|---:|---:|
| cap4 1T RSS (KiB) | 26,652 | 26,656 | 26,656 | 26,656 | — |
| cap8 1T RSS (KiB) | 34,748 | 34,752 | 34,752 | 34,752 | — |
| cap4 pooled (per heap) | 4 | 4 | 4 | 4 | 4 |
| cap8 pooled (per heap) | 5 | 5 | 5 | 5 | 5 |

**RSS and pooled_count are flat across the entire 2 s idle window** (within 4 KiB
for RSS; `pooled_predrain == pooled_final` for pooled_count — every arm, every
rep). This empirically confirms the source-level finding: the small-pool decay
is event-driven (fires only on `reserve_small_segment`), so **idle does not
reclaim retained segments.** The configured decay interval (1 s) governs only
the in-line eviction cadence DURING continued allocation pressure, not idle
reclamation. The retention persists until either (a) the heap does more work
that triggers `reserve_small_segment` decay ticks, or (b) an explicit drain /
thread-exit / recycle.

**Reclaimability is demonstrated, not assumed:** the explicit `dbg_drain_small_pool`
after the idle window releases the pooled segments and RSS drops by ~the pooled
count × 4 MiB (e.g. cap8 1T: 34,748 → 14,284 = −20 MiB ≈ 5 segments). So the
retention is bounded and drainable, not a permanent pin — consistent with the
`regression_c3_unbounded_recycle` guarantee — but it does NOT self-decay on idle.

---

## 4. Implications for the pending decisions (R27-4/#422, R27-5/#423, R27-11/#429)

- **R27-4/#422 (paired default-change `(4,16MiB)→(8,32MiB)`):** the trade is now
  quantified. cap 8 eliminates the decommit churn (latency axis, R25-5/R26-3:
  20→0 decommits/run, ~16% faster) at a **post-teardown retention cost of ~+8
  MiB/heap** (~+4 MiB of which is pooled/drainable, ~+4 MiB committed-non-pooled),
  scaling **linearly** to ~+255 MiB at 32 concurrent heaps. This is a genuine,
  now-quantified RSS-vs-throughput trade — NOT the cost-free "RSS-neutral" change
  R26-1's lower-pressure measurement implied. The decision (promote or not) is
  R27-4's; this report supplies the missing retention number.
- **R27-5/#423 (adaptive/process-wide pool budget):** R26-9 closed this design
  on the (now-refuted) premise that there was "no cap-specific RSS cost." This
  gate PROVES there IS a cap-specific, per-heap, linearly-scaling retention cost
  (~+8 MiB/heap) that an adaptive/process-wide budget could bound while letting
  individual hot heaps exceed 4 segments. R26-9's closure condition ("ONLY if
  R26-1's corrected RSS gate exposes a real cap-8 memory penalty") is now MET.
  The idle-no-decay finding (§3) is directly relevant: an adaptive design that
  wants to reclaim idle retention cannot rely on the existing decay interval —
  it would need an active drain/scavenge, since pure idle does not reclaim.
- **R27-11/#429 (conditional):** gated on this report; the data above is what it
  consumes.

---

## 5. What this gate does NOT claim

- **No latency re-measurement** — R25-5/R26-3's latency/decommit axis (cap4→8:
  20→0 decommits, ~16% faster, self-verified through the real
  `#[global_allocator]`) is unaffected by the R25-5 RSS bug and stands. This gate
  measures ONLY the retention axis (and confirms the decommit counter as a
  victim-activation byproduct).
- **Windows-native only** — same shared-host RSS-noise caveat every prior gate
  carries; the pooled/decommit/conflict self-checks are exact, and the
  per-heap retention (~8 MiB) is consistent across 1/8/32 threads (the linear
  scaling is the strong signal, not any single absolute RSS figure).
- **The committed-non-pooled residual (~+4 MiB/heap post-drain)** is
  RSS-measured but not fully reconciled to a single tracked counter (it reflects
  segments cap 8 never decommits vs cap 4's churn-released ones); it is real and
  consistent across reps but its exact segment-table accounting is not pinned
  down here. The pooled tier (~+4 MiB) IS pinned to a tracked counter
  (`dbg_pooled_count`).
- **The ~6-segment peak demand** is this workload's (batch-120, 1024B,
  256-working-set); workloads with different pressure shapes will retain
  different absolute counts, but the cap8 > cap4 RELATIVE retention and the
  linear per-heap scaling are the structural findings.

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r27_3_pool_retention_gate.rs` | new — the subprocess-per-arm retention probe (orchestrator re-execs once per arm; child claims heaps via `HeapRegistry::claim_with_config`, self-verifies cap+conflicts, hard-asserts victim activation, samples peak/post/idle/drain RSS + pooled_count + decommit/release/reserve counters). Measurement-only, same category as `r25_5`/`r26_1`/`r26_3`. |
| `Cargo.toml` | added `[[example]]` entry for `r27_3_pool_retention_gate` with `required-features = ["alloc-global", "alloc-xthread", "alloc-decommit"]` (matching the r25_5/r26_1 siblings — prevents the E0601 build failure a missing entry causes under plain `--features production`). |
| `docs/perf/R27_3_POOL_RETENTION_GATE.md` | this report (new) |
| `docs/perf/R27_3_POOL_RETENTION_GATE_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r27_3_pool_retention_gate.log` | raw probe stdout, the canonical run cited throughout (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | item 13's "Current state" bullets updated + new dated paragraph appended (append-only) |

**No production source file changed.** No `src/` file touched at all
(`DEFAULT_POOL_SEGMENTS` / `DEFAULT_POOL_BYTE_CAP` remain `4` / `16 MiB`).

---

## 7. Reproduce

```text
cargo run --release --example r27_3_pool_retention_gate --features "production alloc-stats bench-internals"
```

> **Update (R27-7/task #425, 2026-07-29):** the original reproduce command used
> `--features "production alloc-stats"`. `HeapCore::dbg_pool_cap` (the
> self-verification accessor this probe depends on) was re-gated to additionally
> require `bench-internals` (no production caller → R25-10 sub-rule 2), so
> `bench-internals` must now be appended to the `--features` list. The probe's
> measured numbers and verdict are unchanged.

The orchestrator prints each child's `RESULT key=value` lines + `OK ...`
self-check/victim-activation summary, then the aggregated (median, min..max)
table, the cap8−cap4 retention-delta summary, and a CSV block (one row per
child). 18 child processes, ~1.5 s pressure + ~2 s idle + drain each ≈ **~66 s
total wall-clock** on this 16-core host (measured). Each child independently
hard-asserts `verified_cap == pool_segments`, `config_conflicts_delta == 0`,
the cap-4 saturation (`decommit_delta > 0`) / cap-8 retention
(`pooled_hw_max > 4`) precondition, and `pooled_predrain == pooled_final` (idle
constancy) — any failure `panic!`s loudly in that child's stderr and fails the
orchestrator.
