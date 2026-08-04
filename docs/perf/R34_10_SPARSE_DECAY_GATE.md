# R34-10 (task #529) — sparse-decay accumulation gate: the stride-throttle retention bound does NOT hold over consecutive sparse intervals

Date: 2026-08-04.

source_identity (captured BEFORE measurement, per CLAUDE.md R29-6):
`git write-tree` tree SHA **`bb67abc538d5570e45fba42d8613470838934a2f`**
(stages `examples/r34_10_sparse_decay_gate.rs`, `Cargo.toml`, `scripts/r34_10_sparse_decay_summary.mjs`
over base `0e29fc2`; reconstruct via `git read-tree bb67abc538d5570e45fba42d8613470838934a2f`).
Supplementary binary hash (option 4): SHA256
`f02b93fb6fa9fd1fa9906a45fc9db45ecc1a7743062741acb0f4151643fc08ce`.

## 0. What this is

R32-8 (task #499, `docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`)
shipped `DECAY_CLOCK_CHECK_STRIDE = 64`
(`src/alloc_core/alloc_core_large_cache.rs:30`): once
`maybe_decay_large_cache` is past its headroom fast-exit, it only actually
reads the clock every 64th call, trading decay-tick promptness for fewer
`Instant::now()` reads. Its §4 measured the BENEFIT (~61% fewer ns/call above
headroom, confirmed) and its §9 (R33-6, task #511) measured the COST of ONE
missed interval (exactly one 36 MiB segment of retention for n_ops ≤ 8).
R33-6's §9.3 then asserted: **"the cost is bounded by one segment per missed
decay interval … the throttle delays the tick, it does not skip it entirely
across multiple intervals."**

That assertion was never tested over many CONSECUTIVE sparse intervals. The
decay mechanism is EVENT-DRIVEN (a tick can only fire on a large alloc/free),
and `run_decay_step` fires at most ONE step per clock read with NO catch-up
loop (`src/alloc_core/alloc_core_large_cache.rs:482-486`). A workload doing one
large alloc/free per second over many seconds can keep the throttled arm from
reading the clock for ~32 consecutive intervals (64 ops / 2 ops-per-cycle),
while the unthrottled arm fires a tick every interval. The gap between the two
arms' retained cache can then accumulate to several segments — not one.

**This gate measures that gap directly, as a per-interval time series, instead
of asserting it. Verdict: the bound does NOT hold.** The primary alloc+free
profile at 1 event/interval accumulates a peak retention gap of **4 segments
(16 MiB = 4 × the one-segment bound)**, persisting for 27 consecutive intervals
(intervals 3–29), because the throttled arm does not catch up after a clock read.
Direction for the fix is R34-11 (task #530, adaptive), NOT increasing the fixed
stride — this task only measures and documents the defect.

## 1. Methodology

Subprocess-per-(profile, events, arm) isolation (fresh OS process ⇒ fresh
registry ⇒ no cross-arm op-counter or `FORCE_DECAY_CLOCK_READ` leakage),
matching R33-6/R32-8. `AllocCore::dbg_set_force_decay_clock_read` is the
old-shape/new-shape switch:

- **`unthrottled`** (`forced = true`): bypasses the stride throttle, reading
  the clock on EVERY call past headroom — the stride=1 baseline. (It also
  bypasses the headroom fast-exit, but that is a pure optimization: when
  `used ≤ headroom`, `run_decay_step` computes `excess = 0` and releases
  nothing, so the decay outcome is identical to the fast-exit path — only the
  unnecessary clock read differs, which is exactly what makes this arm the
  "always check the clock" baseline the task asks for.)
- **`throttled`** (`forced = false`): the real shipped stride-64 path.

Both arms use the IDENTICAL headroom (16 MiB, `LargeCachePolicy::LowHeadroom`)
and the IDENTICAL workload, so the comparison is clean: only the stride differs.

**Matrix:** events-per-interval ∈ {1, 2, 4, 8} × 40 consecutive intervals × 3
profiles × 2 arms = 24 subprocess children. 1 rep each — the mechanism is
deterministic (R33-6 already showed byte-identical results across 3 reps; the
op-counting stride is exact, not noisy).

### 1.1 Why decay_interval = 100 ms, not the 1000 ms shipped default

The stride throttle is OP-COUNT-based (`DECAY_CLOCK_CHECK_STRIDE = 64` ops),
completely independent of the wall-clock `decay_interval` value: at 1
event/interval the throttled arm hits the 64-op stride boundary after the same
number of intervals whether each interval is 100 ms or 1000 ms. The interval
length changes only the wall-clock COST per missed interval (the "seconds late"
axis in §4), not the op-counting mechanism or the segment-accumulation bound
this gate tests. A 100 ms interval keeps the full 40-interval matrix runnable
in ~2.5 minutes instead of ~25 minutes. The "seconds late" numbers are
reported at 100 ms and then scaled to the 1000 ms shipped default in §4 so the
real-world cost is visible.

### 1.2 Config-resolution evidence (R26-4 rule)

Every child self-verifies, via `heap.dbg_decay_config()` (the diagnostic
surface, not assumed): `verified_headroom == 16777216` (16 MiB) AND
`verified_interval_ms == 100` AND `config_conflicts_delta == 0` (fresh process
⇒ first claim is unconditionally the arm's config). **All 24/24 children
passed** — the derive script (`scripts/r34_10_sparse_decay_summary.mjs`)
asserts each invariant and would THROW on any violation.

### 1.3 Path-activation oracle (R30-8 rule)

Three evidence pieces per child, all asserted by the derive script:

1. **Headroom crossed:** `used_baseline > headroom_bytes` (the workload
   genuinely entered the above-headroom regime the stride applies to). All
   24/24: `headroom_crossed == 1`. For allocfree/allocate, `used_baseline` =
   33 554 432 (32 MiB = 8 × 4 MiB); for deallocate, 20 971 520 (20 MiB = 5 × 4
   MiB, the partial pre-fill — see §3.1).
2. **Unthrottled arm read the clock:** `guard_passed_delta ≥ 1` at end (the
   baseline arm actually exercised the clock-read path). All 24/24.
3. **Stride mechanism differs across arms:** the throttled arm's
   `guard_passed_delta` is materially below the unthrottled arm's in every
   cell — see Table 2.

## 2. Results — alloc+free (PRIMARY, sustained)

Each event = one alloc+free cycle (2 `maybe_decay_large_cache` calls). The
cache pre-fills to 8 slots (32 MiB), well above the 16 MiB headroom; decay is
the only drain. This is the cleanest signal and the one the headline verdict
rests on.

### Table 1 — peak retention gap (throttled − unthrottled `used_post`), alloc+free

(produced by `scripts/r34_10_sparse_decay_summary.mjs` from
`docs/perf/_raw_r34_10_sparse_decay_gate.log`; NOT hand-transcribed)

| events/interval | peak gap (MiB) | peak gap (segments) | peak @ interval | throttled final (MiB) | unthrottled final (MiB) | throttled clock-reads | unthrottled clock-reads |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 16.00 | 4.00 | 3 | 28.00 | 16.00 | 1 | 80 |
| 2 | 16.00 | 4.00 | 3 | 24.00 | 16.00 | 2 | 160 |
| 4 | 16.00 | 4.00 | 3 | 16.00 | 16.00 | 4 | 320 |
| 8 | 12.00 | 3.00 | 2 | 16.00 | 16.00 | 4 | 640 |

Each "segment" = one 4 MiB `SEGMENT` (a 2 MiB `OBJ_BYTES` request rounds up to
one 4 MiB cached span). "peak gap (segments)" = `peak_gap_bytes / 4194304`,
computed and asserted by the derive script.

**At 1 event/interval the peak gap is 4 segments (16 MiB) = 4 × one segment.**
The throttled arm retains the full 32 MiB cache while the unthrottled arm drains
to the 16 MiB headroom (4 segments released over intervals 0–3). The gap opens
at interval 0 (1 segment) and grows to 4 segments by interval 3, then persists
unchanged through interval 29 — 27 consecutive intervals at the peak. At
interval 30 the throttled arm finally reads the clock (its first read, at
~op 64 = ~interval 30–32 depending on the pre-fill's residual op-count), fires
one tick, drops to 28 MiB. The gap then falls to 3 segments (12 MiB) and
persists to the end of the run (interval 39) — the throttled arm does NOT catch
up, because it will not read the clock again until ~op 128.

### Table 2 — alloc+free events=1/interval: full 40-interval time series

(the per-interval trace the verdict rests on; produced by the derive script)

| interval | throttled used (MiB) | unthrottled used (MiB) | gap (MiB) | gap (segments) | throttled released Δ | unthrottled released Δ |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 32.00 | 28.00 | 4.00 | 1.00 | 0 | 1 |
| 1 | 32.00 | 24.00 | 8.00 | 2.00 | 0 | 2 |
| 2 | 32.00 | 20.00 | 12.00 | 3.00 | 0 | 3 |
| 3 | 32.00 | 16.00 | **16.00** | **4.00** | 0 | 4 |
| 4–29 | 32.00 | 16.00 | **16.00** | **4.00** | 0 | 4 |
| 30–39 | 28.00 | 16.00 | 12.00 | 3.00 | 1 | 4 |

(Intervals 4–29 collapsed — byte-identical 32.00 / 16.00 / 16.00 / 4.00 every
row; the full 40-row table is in the derive script's stdout and the raw log.)
The throttled arm releases **1 segment over the entire 40-interval run**
(`throttled released Δ = 1`); the unthrottled arm releases **4**
(`unthrottled released Δ = 4`). The throttled arm's single release happens at
interval 30 (its first and only clock read); it does not read the clock again
before the run ends.

## 3. Results — dealloc-only and alloc-only (secondary)

### 3.1 dealloc-only (transient growth-phase signal)

Pre-fills 5 of 8 slots (20 MiB > 16 MiB headroom, 3 free slots), then frees a
pre-allocated pool at `events`/interval. The gap manifests only during the
cache-filling phase (the 3 free slots accept deposits without eviction); once
the cache saturates (8 slots), every deposit evicts the FIFO-oldest —
deposit-eviction becomes indistinguishable from decay-eviction and both arms
converge.

| events/interval | peak gap (segments) | peak @ interval | finding |
|---:|---:|---:|---|
| 1 | 2.00 | 2 | growth-phase gap (3 free slots fill over 3 intervals), then converges to full cache |
| 2 | 0.00 | — | cache saturates in interval 0 (2 deposits ≥ 3 free? see below) — no measurable gap |
| 4 | 0.00 | — | cache saturates immediately |
| 8 | 0.00 | — | cache saturates immediately |

At events ≥ 2, the cache saturates within the first interval (the unthrottled
arm's decay tick opens one slot per interval, but ≥ 2 deposits refill it and
overflow into eviction immediately), so the gap is structurally zero — a NULL
result that confirms the stride's retention cost ONLY manifests when there is a
gap between a decay eviction and the refill that would mask it (i.e. the
alloc+free and alloc-only profiles, not a constantly-refilled full cache).

### 3.2 alloc-only (finite drain-phase signal)

Pre-fills all 8 slots (32 MiB), then allocs `events`/interval (held, draining
the cache). The gap manifests only during the drain to headroom (4 segments
above headroom); once the cache hits headroom, decay stops and both arms
converge to an empty cache.

| events/interval | peak gap (segments) | peak @ interval | finding |
|---:|---:|---:|---|
| 1 | 2.00 | 1 | drain-phase gap (unthrottled decays while draining → drains faster), then both empty |
| 2 | 2.00 | 1 | same, faster drain |
| 4 | 1.00 | 0 | brief |
| 8 | 0.00 | — | drains instantly, no decay window |

These two profiles are reported honestly as secondary: dealloc-only's signal is
transient (filling-phase only) and alloc-only's is finite (drain-bounded). The
sustained, clean signal is alloc+free (§2).

## 4. The "ops late" vs "seconds late" axes

The review explicitly demanded these be reported separately. **"ops late"** =
the throttled arm's clock-read deficit (how many `maybe_decay_large_cache`
calls past headroom let the clock go unread) — the op-counting axis the stride
mechanism lives on, independent of the interval length. **"seconds late"** = the
wall-clock span from measurement start to the throttled arm's FIRST decay tick
(read from the time series: the interval at which `throttled released Δ` first
goes nonzero, × the per-interval wait) — the real-world cost axis.

### Table 3 — ops-late vs seconds-to-first-decay, alloc+free (PRIMARY)

| events/interval | throttled clock-reads | unthrottled clock-reads | ops late (deficit) | throttled first-decay @ interval | seconds to first decay @ 100 ms gate | seconds to first decay @ 1000 ms shipped default |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 / 80 | 80 / 80 | 79 | 30 | 4.5 s (30 × 150 ms) | **~33 s** (30 × 1100 ms) |
| 2 | 2 / 160 | 160 / 160 | 158 | 15 | 2.3 s (15 × 150 ms) | **~17 s** (15 × 1100 ms) |
| 4 | 4 / 320 | 320 / 320 | 316 | 7 | 1.1 s (7 × 150 ms) | **~8 s** (7 × 1100 ms) |
| 8 | 4 / 640 | 640 / 640 | 636 | 3 | 0.5 s (3 × 150 ms) | **~3 s** (3 × 1100 ms) |

(The "X / Y" fractions name numerator and denominator inline per CLAUDE.md:
throttled clock-reads = actual reads out of total past-headroom calls. The
"1000 ms shipped default" column scales the first-decay interval by a 1000 ms
interval + 100 ms margin instead of the gate's 100 ms + 150 ms; the
first-decay INTERVAL is op-count-based and thus interval-independent, only the
wall-clock it represents scales.)

At the shipped 1000 ms default interval, the throttled arm at 1 event/interval
goes **~33 seconds** before firing its first decay tick — not "until the next
interval" as the optimistic phrasing in R33-6 §9.3 suggested. The unthrottled
arm fires on its very first call of interval 0.

## 5. Verdict — the bound does NOT hold

R33-6 §9.3's assertion — "the cost is bounded by one segment per missed decay
interval … the throttle delays the tick, it does not skip it entirely across
multiple intervals" — is **refuted by measurement**. Over 40 consecutive sparse
intervals at 1 alloc+free event/interval:

- **Peak retention gap: 4 segments (16 MiB), 4 × the one-segment bound.**
  (numerator: 16 777 216 bytes retained by throttled over unthrottled;
  denominator: 4 194 304 bytes per segment = 4.00 segments.)
- The gap **persists at ≥ 3 segments for 38 of 40 intervals** (intervals 2–39):
  38/40 = 95.0% of the run. (numerator: 38 intervals; denominator: 40 total.)
- The throttled arm **does not catch up** after its single clock read at
  interval 30: it releases 1 segment (32→28 MiB) and then holds at 28 MiB for
  the remaining 9 intervals, still 3 segments (12 MiB) above the unthrottled
  arm's 16 MiB headroom floor.
- The throttled arm released **1 segment over the entire run vs the
  unthrottled arm's 4** (1/4 = 25% of the unthrottled release; numerator 1,
  denominator 4).

The root cause is structural: `run_decay_step`
(`src/alloc_core/alloc_core_large_cache.rs:497-511`) runs exactly ONE eviction
step per clock read — there is no loop to catch up on the multiple intervals
that elapsed since the last tick. So a throttled arm that skips N clock reads
fires ~1 tick where the unthrottled arm fired ~N, and the retention gap grows by
~1 segment per skipped tick until the cache hits headroom (the only floor).

## 6. Why increasing the fixed stride is NOT the fix

The bound fails because the stride is FIXED at 64 ops regardless of event rate.
At high event rates (alloc+free events=8: 16 ops/interval), the throttled arm
reads the clock every 4 intervals and catches up (peak gap 3 segments, final
gap 0). At low event rates (events=1: 2 ops/interval), it reads every 32
intervals and falls permanently behind. A larger fixed stride would make the
low-rate case WORSE; a smaller one would erase the latency benefit R32-8
measured. The direction that addresses the actual mechanism — "read the clock
more often when events are sparse, less often when they are dense" — is an
ADAPTIVE stride, which is R34-11 (task #530). This task deliberately does NOT
build or measure an adaptive arm (R34-11 is blocked by this task's verdict).

## 7. Immutable source identity + reproducibility

- **Source identity (R29-6):** `git write-tree` tree SHA
  `bb67abc538d5570e45fba42d8613470838934a2f`, captured BEFORE measurement by
  staging the three code files (`examples/r34_10_sparse_decay_gate.rs`,
  `Cargo.toml`, `scripts/r34_10_sparse_decay_summary.mjs`) and writing the
  index. Reconstruct: `git read-tree bb67abc538d5570e45fba42d8613470838934a2f`.
- **Supplementary binary hash:** SHA256
  `f02b93fb6fa9fd1fa9906a45fc9db45ecc1a7743062741acb0f4151643fc08ce`
  (`target`/`.cargo-target`/`release/examples/r34_10_sparse_decay_gate.exe`).
- **Raw per-sample data:** `docs/perf/_raw_r34_10_sparse_decay_gate.log`
  (committed with `git add -f` per the raw-log policy) — 960 per-interval
  `RESULT ts=1` time-series rows + 24 `config` rows + 24 `oracle` rows, one
  per child.
- **Summary CSV:** `docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv` — 12 rows
  (3 profiles × 4 events arms), produced by the derive script.
- **Reproduce:**
  `cargo run --release --example r34_10_sparse_decay_gate --features "production alloc-stats bench-internals internals"`
  then
  `node scripts/r34_10_sparse_decay_summary.mjs bb67abc538d5570e45fba42d8613470838934a2f`.
- **Environment:** Intel_Core_i7-11800H_2.30GHz, Windows_10_Pro_10.0.19045,
  feature set `production alloc-stats bench-internals internals`.

## 8. Verification

- `cargo clippy --example r34_10_sparse_decay_gate --features "production
  alloc-stats bench-internals internals" -- -D warnings` — **clean**.
- `cargo fmt --check` — **clean**.
- Derive script `scripts/r34_10_sparse_decay_summary.mjs` — run against the
  committed raw log; produced the summary CSV + all tables in this report; all
  24 config-row invariants and 24 oracle-row invariants passed (the script
  THROWS on any violation).

## 9. Files changed

- `examples/r34_10_sparse_decay_gate.rs` (new) — the gate (orchestrator +
  child, 3 profiles, per-interval time-series emission).
- `Cargo.toml` — registers the example with `required-features`.
- `scripts/r34_10_sparse_decay_summary.mjs` (new) — the checked derive script
  (raw log → summary CSV + report tables; asserts all invariants).
- `docs/perf/_raw_r34_10_sparse_decay_gate.log` (new, `git add -f`) — cited raw
  per-sample evidence.
- `docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv` (new) — machine-readable
  companion.
- `docs/perf/R34_10_SPARSE_DECAY_GATE.md` (new) — this report.

## 10. Open-items index update

This finding is filed in `docs/perf/OPEN_ITEMS.md` (item: R34-10 found the
stride-throttle retention bound does not hold over consecutive sparse intervals;
fix direction = R34-11 adaptive stride) per the "Round start: check BOTH
open-items indexes" convention, so a fresh round inherits it.
