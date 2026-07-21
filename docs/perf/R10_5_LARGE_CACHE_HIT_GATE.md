# R10-5 — `medium-classes-wide` warm-Large-cache-hit gate: the fair recycle comparison

**Task:** #230 (R10-5) — the methodological fix to R9-4 §2.4's "consolation
prize" claim. R9-4 argued the 1.5 / 1.75 MiB wide classes (which carry NO
density win — R9-4 §2.3 confirmed REAL density = 1, same as the Large path)
still earn their place because a same-size recycle converts "the Large path's
~90 µs free (`VirtualFree` + re-`VirtualAlloc`) into a ~60 ns freelist
push/pop." An external review pointed out the methodology gap: under
`production` the Large-segment free-cache (`OPT-E`, `LARGE_CACHE_SLOTS = 8`,
gated on `alloc-decommit`) is ACTIVE, so a **warm** Large-cache HIT recycles a
recently-freed span via cheap in-process bookkeeping (header rewrite + table
re-register — pages stay committed, NO recommit, NO syscall; see
`src/alloc_core/alloc_core_large.rs` lines 158-166), NOT the full ~90 µs
`VirtualFree`+`VirtualAlloc` round-trip. So R9-4's ~90 µs number is the
Large-cache **MISS** cost, not the **typical WARM-cache recycle cost** most
steady-state programs actually see — R9-4 may have compared the small path
against the Large path's *worst* case, not its *typical* case.

This judge closes that gap: a direct production wall-clock comparison of
WARM-recycle via the small path's freelist against a **WARM Large-cache HIT**
(not a miss) for the SAME alloc/free/alloc steady-state pattern, at 1.5 MiB
and 1.75 MiB specifically (1.25 MiB is skipped — it has a genuine,
cache-comparison-agnostic 2× density win per R9-4 §2.3, so this question does
not apply to it).

**Measurement-only:** no `src/` changes, no `Cargo.toml` feature-bundle
change. The `production` feature list is byte-identical to `main`; `alloc-stats`
(NOT part of `production`) is passed only on these two probe binaries'
`--features` build lines so the per-hit `large_cache_hits` counter is live
(the cache-hit PROOF — without it, this report would repeat exactly the
methodology gap it exists to fix).
**Date:** 2026-07-21
**Base revision:** `main` @ `fed3d45` (dirty: the 5 new measurement files this
task adds — `examples/paired_ab_large_cache_{off,on}.rs`,
`examples/_shared/paired_ab_large_cache_workload.rs`,
`scripts/r10_5_large_cache_gate.mjs`, this doc — plus the `Cargo.toml`
`[[example]]` registrations).
**Platform:** Windows 10 Pro x86-64, native. 11th Gen Intel Core i7-11800H @
2.30 GHz, 8 cores / 16 logical. Power plan: Balanced. `rustc 1.97.0`.
**Harness:** `scripts/paired-ab-runner.mjs` (`--config` mode) driven by
`scripts/r10_5_large_cache_gate.mjs`, 20 A/B/B/A blocks (4 launches each) per
size, 2 sizes = 160 total fresh-process launches (plus the 4-launch cache-hit
proof gate). Two separately-named probe binaries
(`paired_ab_large_cache_off` / `paired_ab_large_cache_on`) built from one
shared `include!`d workload, differing ONLY in Cargo feature set.

---

## 1. Scope recap — what R9-4 did NOT measure (and why this judge exists)

R9-4 §2.4's consolation-prize claim rested on a single cited number (R8-9
§4.3's ~90 µs Large free) compared against the small path's ~60 ns freelist
op — a ~1,500× ratio. But R8-9 measured the Large path with a working set
that exceeded `LARGE_CACHE_SLOTS` (forcing cache MISSES, i.e. real OS
`VirtualFree`/`VirtualAlloc` churn). The claim therefore compares the small
path against the Large path's **cold/miss** cost, not its **warm/hit** cost.
This judge measures the comparison R9-4 needed but did not make:

| Report | What it measured | What it did NOT measure |
|---|---|---|
| R8-9 (#222) | `AllocCore`-direct Large-path alloc/free at medium sizes, working set > `LARGE_CACHE_SLOTS` (cache MISSES, ~90 µs free cited). | Did NOT isolate the warm-cache HIT cost; the working set guaranteed misses. |
| R9-4 (#226) | Density geometry of the 3 wide classes (deterministic); the ~90 µs → ~60 ns recycle claim was CITED from R8-9, not re-measured under warm-cache conditions. | Did NOT verify the recycle claim against a warm Large cache; assumed the Large path always pays the full OS round-trip. |
| **R10-5 (this)** | **Warm steady-state alloc+free recycle via the small path's freelist vs a WARM Large-cache HIT (working set < `LARGE_CACHE_SLOTS`, cache proven warm via `large_cache_hits` counter), via 160 independent process launches with paired t-test + sign test per size.** | — |

---

## 2. Methodology

### 2.1 The probe binaries

Two example binaries (`examples/paired_ab_large_cache_off.rs`,
`examples/paired_ab_large_cache_on.rs`) are byte-identical wrappers around one
shared `include!`d workload
(`examples/_shared/paired_ab_large_cache_workload.rs`). Each installs
`SeferAlloc` as the real `#[global_allocator]`, runs the warm-recycle
workload, and emits `RESULT` lines. The ONLY difference is the Cargo feature
set at build time:

```text
# Arm A (baseline): 1.5/1.75 MiB route Large, WARM cache steady state
cargo build --release --example paired_ab_large_cache_off \
  --features "production,alloc-stats"

# Arm B (treatment): 1.5/1.75 MiB route small-path freelist recycle
cargo build --release --example paired_ab_large_cache_on \
  --features "production,medium-classes-wide,alloc-stats"
```

Both arms are built with the full `production` feature set
(`alloc-global` + `alloc-xthread` + `alloc-decommit` + `fastbin` +
`alloc-segment-directory`), so the Large-segment free-cache is ACTIVE in the
baseline — this is a fair comparison of "baseline Large-path WITH its warm
8-slot cache" vs "small-path freelist", not "cache on vs cache off".

**Why `alloc-stats` is added to BOTH arms (and only to these probe
binaries).** The per-hit `large_cache_hits` counter is gated behind
`alloc-stats` (default OFF, NOT in `production`) — without it the counter
reads 0 even when hits occur, and the cache-hit PROOF (§2.4) would be
impossible. So both arms add `alloc-stats` on their `--features` build line.
The `production` feature list in `Cargo.toml` is **untouched** — this is
exactly the usage `alloc-stats` exists for (see `Cargo.toml` lines 231-245:
*"`alloc-stats` ... default OFF ... enable ... when you want the counters"*).
The per-hit increment is a single Relaxed load+store on the owning thread
(NOT a `lock xadd`; see `alloc_core_large.rs` lines 129-157) — negligible
against the µs-scale Large-path work, and asymmetric only in that the baseline
arm pays it (the treatment arm's small path does not hit the Large cache, so
it does not increment it). The timing impact is sub-ns per hit and cannot
plausibly flip a 2×+ signal.

### 2.2 The warm-recycle workload

The workload (argv[1] = allocation size in KiB, one size per process launch so
each size's cache state is independent — no cross-size contamination) runs a
tight alloc/free/alloc steady-state loop:

| Stage | What it does | Why |
|---|---|---|
| **Warm-up (untimed)** | `WARMUP_ROUNDS = 3` full alloc+free cycles at `WS_LEN = 6` objects. | Populates the Large cache (baseline) / the small-path freelist (treatment) so the timed region starts in genuine steady state. After warm-up the baseline's cache holds 6 recently-freed dedicated 4 MiB spans. |
| **Timed steady state** | ONE `Instant` pair around `ROUNDS = 3000` full alloc+free cycles. Each cycle allocates `WS_LEN = 6` objects at the selected size, writes the first 16 bytes (page touch + dead-code fence), then frees all 6. | Times the steady-state warm recycle. The single `Instant` pair (not per-round) keeps clock overhead at ~2 QPC reads total (~150 ns out of a multi-ms region — 0.00x%), so the measurement is dominated by the actual alloc/free work, not the clock. |

### 2.3 Working set design — why `WS_LEN = 6` (< `LARGE_CACHE_SLOTS = 8`)

This is the **key methodological difference from R10-2**. R10-2 deliberately
used a working set of 16 (2× the cache) to force the baseline into Large-cache
MISSES and measure "medium-classes vs the cold Large path" — the right design
for *that* question. THIS task needs the OPPOSITE: a working set that stays
WITHIN the cache capacity so the baseline's Large allocations consistently
HIT the warm cache. `WS_LEN = 6` is deliberately below `LARGE_CACHE_SLOTS = 8`
(a 2-slot safety margin, so no eviction pressure): after warm-up, every
baseline alloc pops a span from the warm cache (a HIT) and every free
re-deposits it (the cache stays at 6 spans for the entire timed region). The
treatment arm's `WS_LEN = 6` objects each take a dedicated segment anyway
(density = 1 for 1.5/1.75 MiB per R9-4 §2.3), but recycle via the small-path
freelist / empty-segment pool (no OS churn). `WS_LEN = 6` (not 4) was chosen
to give each timed `Instant` region enough work (~12 ops × 3000 rounds) to
span multi-milliseconds for good clock resolution, while staying safely under
the 8-slot cap.

### 2.4 The cache-hit PROOF (the methodological heart of this gate)

The whole point of this judge is to NOT repeat R9-4's gap (comparing against
the Large path's worst case). So the workload reads
`SeferAlloc::stats().large_cache_hits` and emits it as a RESULT line on EVERY
launch. Under the baseline arm this MUST read ~`WS_LEN × ROUNDS` (every
steady-state alloc hit the warm cache); under the treatment arm it MUST read
0 (the small path never touches the Large cache). The driver
(`scripts/r10_5_large_cache_gate.mjs`) runs a PROOF gate before any timing:
it launches each arm once per size and **asserts**
`baseline.large_cache_hits > 0 && treatment.large_cache_hits == 0`, aborting
the whole run if either fails — so no timing number in this report is trusted
without the warm-vs-warm proof. The driver re-runs the same proof at the top
of every full run (cheap — 4 launches).

### 2.5 The A/B/B/A protocol and paired statistics

`scripts/r10_5_large_cache_gate.mjs` writes two per-size `--config` JSONs
(`recycle_ns` metric, argv size 1536 / 1792) and invokes
`scripts/paired-ab-runner.mjs` twice — once per size. Each invocation runs 20
A/B/B/A blocks (the pattern `A B B A | A B B A | …` averaged across 20 blocks
= 80 process launches per size, 160 total), pairing each block's A-sample mean
against its temporally-adjacent B-sample mean, then computing:

- **Paired t-test** (mean of 20 `(A − B)` deltas, sample stddev, standard
  error, `t = mean/se`, two-tailed against the df=19 critical value 2.101 at
  p<0.05 — the EXACT methodology from R5_R2 / R10-2).
- **Sign test** (count of deltas favoring each side).

The A/B/B/A ordering (not A/B/A/B) averages out monotonic host drift across
each 4-launch block. Each size gets its OWN independent A/B/B/A session.

### 2.6 Exact reproduction commands

```text
# Option A: build both arms + cache-hit proof gate + run both sizes:
node scripts/r10_5_large_cache_gate.mjs --pairs 20

# Option B: build separately, then run with --skip-build:
cargo build --release --example paired_ab_large_cache_off  --features "production,alloc-stats"
cargo build --release --example paired_ab_large_cache_on  --features "production,medium-classes-wide,alloc-stats"
node scripts/r10_5_large_cache_gate.mjs --skip-build --pairs 20

# Cache-hit proof gate ONLY (no timing — the sanity check first):
node scripts/r10_5_large_cache_gate.mjs --verify-only

# Quick smoke (4 pairs per size):
node scripts/r10_5_large_cache_gate.mjs --quick
```

Raw captured provenance (one JSON per size, each with every raw per-process
sample — including `large_cache_hits` per launch — git commit, rustc version,
CPU info, power plan, and the Cargo feature note — not committed per repo
convention):
- `docs/perf/paired_ab_runs/2026-07-21T01-19-23-153Z.json` — 1.5 MiB (1536 KiB)
- `docs/perf/paired_ab_runs/2026-07-21T01-19-25-113Z.json` — 1.75 MiB (1792 KiB)

---

## 3. The cache-hit PROOF — this comparison is genuinely warm-vs-warm

Before any timing number, the proof gate (§2.4) establishes that the baseline
is hitting the warm cache and the treatment is on the small path. The proof
holds with zero variance across **all 40 baseline launches** (20 per size) and
all 40 treatment launches:

| Size | Arm A (off) `large_cache_hits` (all 20 launches) | Arm B (on) `large_cache_hits` (all 20 launches) | Verdict |
|---|---|---|---|
| 1.5 MiB (1536) | **18012** (min..max = 18012..18012) | **0** (min..max = 0..0) | PROVEN warm-vs-warm |
| 1.75 MiB (1792) | **18012** (min..max = 18012..18012) | **0** (min..max = 0..0) | PROVEN warm-vs-warm |

**Reading.** `18012` = `WS_LEN × (ROUNDS + WARMUP_ROUNDS − 1)` = `6 × (3000 + 2)`
exactly — i.e. every single steady-state alloc (3000 rounds × 6) plus the
warm-up rounds' recycles hit the warm cache, with not a single miss. This is
the direct, per-launch refutation of the concern that motivated this task: the
baseline is NOT paying the ~90 µs `VirtualFree`+`VirtualAlloc` round-trip R9-4
§2.4 cited; it is recycling via the warm Large cache's cheap in-process path.
The treatment's `0` confirms the small path is taken instead. The timing
numbers in §4 are therefore warm-vs-warm by construction, not by assumption.

**Secondary proof (no OS churn in either arm).** Both arms show
`segments_released_total = 0`, `decommit_calls = 0`, and
`segments_reserved_total` constant across all launches (A = 7, B = 6 — the
working set plus one std/Vec overhead segment, never growing with rounds). So
neither arm touches the OS in steady state; both recycle purely in-process.
The comparison is small-path-freelist-recycle vs Large-cache-recycle — exactly
the question this gate exists to answer.

---

## 4. Results — paired A/B/B/A warm-recycle wall-clock

Each cell below is the mean of 20 paired block-values (each block-value =
mean of the block's 2 same-arm launches). A = `production,alloc-stats`
(Large path, warm cache); B = `production,medium-classes-wide,alloc-stats`
(small path, freelist recycle). Δ = A − B (positive ⇒ A slower ⇒ B faster).
"Per object-recycle" divides the region's total by `WS_LEN × ROUNDS = 18000`
(one object-recycle = one alloc + one free).

### 4.1 1.5 MiB (1536 KiB) — small path is ~2.2× FASTER than warm Large-cache

| Metric | A (off, warm cache) | B (on, small path) | Δ (A−B) |
|---|---:|---:|---:|
| mean | 1.370 ms | 0.619 ms | +0.751 ms |
| min..max | 1.136..1.780 ms | 0.406..0.943 ms | — |
| **per object-recycle** (18000) | **76.1 ns** | **34.4 ns** | **+41.7 ns** |

| Statistic | Value |
|---|---|
| paired t | **14.676** (df=19, crit=2.101) → **REAL** |
| sign test | B-faster **20/20**, A-faster 0/20 |

### 4.2 1.75 MiB (1792 KiB) — small path is ~2.6× FASTER than warm Large-cache

| Metric | A (off, warm cache) | B (on, small path) | Δ (A−B) |
|---|---:|---:|---:|
| mean | 1.433 ms | 0.557 ms | +0.876 ms |
| min..max | 1.150..1.888 ms | 0.412..0.789 ms | — |
| **per object-recycle** (18000) | **79.6 ns** | **31.0 ns** | **+48.6 ns** |

| Statistic | Value |
|---|---|
| paired t | **17.279** (df=19, crit=2.101) → **REAL** |
| sign test | B-faster **20/20**, A-faster 0/20 |

### 4.3 Reading

Both sizes tell the same story with the same statistical strength
(t = 14.7–17.3, sign 20/20). The small-path freelist recycle
(~31–34 ns/object-recycle) is **meaningfully faster** than the WARM
Large-cache recycle (~76–80 ns/object-recycle) — a consistent **~2.2–2.6×
speedup, saving ~42–49 ns per object-recycle**. The signal is real and
unambiguous (paired t far past the p<0.05 critical value; sign test unanimous
in both sizes).

---

## 5. What this means for R9-4's "consolation prize" claim

### 5.1 The claim's DIRECTION is CONFIRMED; its MAGNITUDE was wrong by ~600×

R9-4 §2.4 framed the recycle win as "~90 µs → ~60 ns" — a ~1,500× speedup.
That number compared the small path against the Large path's **cache-MISS**
cost (a real `VirtualFree` + re-`VirtualAlloc` round-trip, as R8-9 measured
with a cache-exceeding working set). Against the **fair baseline** — the
Large path's warm-cache HIT, which is what most steady-state programs actually
see — the real comparison is:

```text
                            R9-4's framing        THIS gate (fair, warm-vs-warm)
Large-path steady recycle:  ~90,000,000 ns (miss) ~76-80 ns/object-recycle (warm HIT)
Small-path steady recycle:  ~60 ns (claimed)      ~31-34 ns/object-recycle (measured)
speedup ratio:              ~1,500×               ~2.2-2.6×
absolute saving:            ~90 µs/cycle          ~42-49 ns/object-recycle
```

The small path IS faster (direction confirmed, t = 14.7–17.3, sign 20/20), but
the magnitude is **~2.3×, not ~1,500×**. The warm Large cache is already in
the same nanosecond ballpark as the small freelist (~76 ns vs ~32 ns) — it is
NOT paying the ~90 µs R9-4's framing implied. The consolation prize is real
but modest.

### 5.2 Two corrections R9-4 §2.4 needs

1. **The comparison baseline.** "the Large path's ~90 µs free" is the
   cache-MISS cost (working set > `LARGE_CACHE_SLOTS`); the typical
   steady-state cost is the warm-cache HIT (~76–80 ns/object-recycle, this
   gate). The claim should cite the warm baseline, not the miss.
2. **The small-path number.** "~60 ns freelist push/pop" was cited from R8-9's
   six-class medium substrate; this gate measures ~31–34 ns/object-recycle for
   the 1.5/1.75 MiB wide classes specifically (faster than the cited 60 ns —
   the wide classes' single-block-per-segment geometry makes the freelist op
   cheaper than the denser medium classes' magazine path).

Both corrections STRENGTHEN the small path's absolute number but DRAMATICALLY
shrink the ratio (the Large warm hit is far cheaper than the miss R9-4
compared against).

---

## 6. Kill-gate / GO-NO-GO verdict

| # | Criterion (the task's decision rule) | Target / expectation | Measured | Verdict |
|---|---|---|---|---|
| K1 | Is the comparison genuinely warm-vs-warm (not accidentally warm-vs-cold)? | baseline `large_cache_hits` large, treatment 0 | baseline = **18012** on ALL 40 launches; treatment = **0** on ALL 40 launches (§3) | **PASS** (proof holds with zero variance) |
| K2 | Is the small-path recycle meaningfully faster than a WARM Large-cache hit? | paired t > crit, sign lopsided, non-trivial ratio | 1.5 MiB: t=14.676, sign 20/20, **2.21×**; 1.75 MiB: t=17.279, sign 20/20, **2.57×** (§4) | **PASS** (meaningfully faster) |
| K3 | Does the Large-cache HIT path come close to or beat the small path? | NO (would trigger narrowing) | Large warm recycle ~76–80 ns vs small ~31–34 ns — Large is ~2.3× SLOWER (§4) | **PASS** (Large is clearly slower, not comparable) |
| K4 | Sanity: do both arms genuinely install SeferAlloc and recycle in-process? | `segments_reserved_total > 0`, no OS churn | A=7, B=6 (both > 0); `segments_released_total=0`, `decommit_calls=0` in both (§3) | **PASS** |

### Verdict: **CONFIRMED — R9-4's "consolation prize" claim holds (direction correct), with a mandatory magnitude correction**

The small-path freelist recycle for the 1.5 / 1.75 MiB wide classes is
**meaningfully faster** than a WARM Large-cache HIT (the fair baseline, not
the cache miss R9-4 compared against): ~2.2–2.6× (~42–49 ns saved per
object-recycle), statistically unambiguous (paired t = 14.7–17.3, sign test
20/20 in both sizes), proven warm-vs-warm via the `large_cache_hits` counter
on every one of the 80 timed launches. Per the task's explicit decision rule,
this CONFIRMS R9-4's consolation-prize claim — it just needs its methodology
corrected to compare against the right baseline (warm HIT, not miss), which
shrinks the headline ratio from ~1,500× to ~2.3×.

### What this does NOT mean

- **It does NOT make 1.5/1.75 MiB a headline density win.** R9-4 §2.3's
  density finding stands: both classes carry REAL density = 1 (same as the
  Large path). Their sole justification remains the recycle speedup this gate
  confirms — now correctly sized at ~2.3×, not ~1,500×.
- **It does NOT override the 1.25 MiB class's primacy.** 1.25 MiB has BOTH the
  2× density win (R9-4 §2.3) AND this same recycle speedup — it is strictly
  better-justified than 1.5/1.75 MiB on every axis. 1.25 MiB remains the
  headline wide class.
- **The absolute saving is modest.** ~42–49 ns/object-recycle, with both paths
  sub-100 ns. A program doing heavy 1.5 MiB alloc/free churn benefits; a
  program that rarely churns in this range will not notice (the warm Large
  cache already serves it in ~76 ns).

---

## 7. Recommendation

1. **CONFIRM R9-4 §2.4's consolation-prize claim — with the §5.1 magnitude
   correction written into the report.** The small-path recycle for 1.5/1.75
   MiB is genuinely faster than a fair warm-Large-cache baseline (~2.3×, not
   ~1,500×). The claim's *direction* was right; its *baseline* (cache miss vs
   cache hit) and therefore its *ratio* were wrong. R9-4 §2.4 should be
   amended to cite this gate's warm-vs-warm numbers as the fair comparison,
   keeping the original miss-cost number only as the cold-path worst case.
2. **Keep 1.5/1.75 MiB in `medium-classes-wide` — do NOT narrow to 1.25 MiB
   on the recycle axis.** The task's narrowing branch ("recommend narrowing to
   JUST 1.25 MiB") was conditional on "the difference is small or the Large
   cache hit is comparable/faster" (§K3). Neither holds: the difference is
   ~2.3× (t = 14.7–17.3, sign 20/20) and the Large warm hit is clearly slower.
   The 1.5/1.75 MiB classes earn their place on the recycle axis — a real,
   ~42–49 ns/object-recycle win over the warm Large cache. (1.25 MiB remains
   the stronger class because it adds the independent 2× density win on top.)
3. **Do NOT promote `medium-classes-wide` into `production` on this evidence
   alone.** R10-2's NO-GO on the realloc axis (the 256 KiB–1 MiB range, ~2,111×
   realloc regression) applies to the wide classes too — the same move-leg
   mechanism fires for 1.5/1.75 MiB realloc-grow. Promotion remains a separate
   explicit decision that must weigh this gate's confirmed-but-modest recycle
   win against R10-2's realloc regression and the target workload's
   realloc/churn intensity in the 1–1.75 MiB range.
4. **The infrastructure is reusable.** `node scripts/r10_5_large_cache_gate.mjs`
   reproduces the full proof-gated 2-size A/B/B/A measurement. The cache-hit
   proof gate (`--verify-only`) is the sanity check to run first on any future
   re-measurement or after touching the Large-cache code — it catches a
   regression to warm-vs-cold before any timing number is trusted.

---

## 8. Caveats

- **`alloc-stats` adds a per-hit counter increment to the baseline arm only.**
  The increment is a single Relaxed load+store (no `lock xadd`) on the owning
  thread (`alloc_core_large.rs` lines 129-157) — sub-ns per hit, applied to
  the baseline's ~18,012 hits. Against the ~µs-scale Large-path work this is
  negligible, and it cannot plausibly manufacture a 2×+ signal. The treatment
  arm does not pay it (its small path never hits the Large cache). This is the
  honest cost of making the warm-vs-warm proof possible without modifying
  `src/` or `Cargo.toml`.
- **Per-sample timing spread is wide** (baseline 1.136–1.888 ms across the 20
  blocks — host noise). The PAIRED design controls for this: each block's
  A−B delta is the statistical unit, and the deltas are tight (se = 51 µs vs
  mean Δ = 751–876 µs → t = 14.7–17.3). The wide absolute spread is monotonic
  host drift that the A/B/B/A ordering averages out within each block.
- **Single host, Windows native.** All 160 launches ran on the same physical
  host (i7-11800H, Balanced power plan). The t-statistics and sign tests are
  far enough past the thresholds that cross-host variation would not flip the
  verdict.
- **Working set is below `LARGE_CACHE_SLOTS`.** `WS_LEN = 6` (< 8) guarantees
  warm hits (proven in §3) — the OPPOSITE of R10-2's cache-exceeding design.
  This is the correct design for THIS question (warm-vs-warm) but means these
  numbers are NOT comparable to R10-2's (which measured the cold/miss path).
  The two judges are complementary: R10-2 measures "medium-classes vs cold
  Large path"; this judge measures "medium-classes-wide small path vs warm
  Large-cache path".
- **No same-vs-same control run was included.** The runner's `--arms A,A`
  honesty check was not run for the config-mode arms (matching R10-2's caveat).
  The t-statistics (14.7–17.3) and sign tests (20/20) are well past the
  thresholds, and the cache-hit proof (§3) independently confirms the arms
  exercise genuinely different code paths — but a future replication could
  add `node scripts/paired-ab-runner.mjs --config docs/perf/paired_ab_runs/_r10_5_1536.json --arms A,A`.
- **Density = 1 for both classes stands (R9-4 §2.3).** This gate measures the
  recycle axis only; it does not revisit the density geometry. R10-4 (a
  design-only sibling task proposing 3/2/2 density) is NOT shipped; current
  density for 1.5/1.75 MiB is still 1, and this gate's verdict is unaffected
  by R10-4.
- **`src/` and `Cargo.toml` feature-bundle were NOT modified.** Confirmed: the
  only `Cargo.toml` change is the two `[[example]]` registrations for the new
  probe binaries; the `production` feature list is byte-identical to `main`.
  `alloc-stats` is passed only on the probe binaries' `--features` build line.
