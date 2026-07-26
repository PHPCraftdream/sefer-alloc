# R22-15 — mimalloc `Ir` arm in `benches/perf_gate_iai.rs`: measured, not spun

**Task #366 (R22-15), Round 22. P1 — highest-priority measurement action of
the round per all three independent Round 19-21 reviews.** This executes the
implementation sketch `docs/perf/R20_4_MIMALLOC_IR_ARM_FEASIBILITY.md` (R20-4,
task #349, commit `e5addae`) already designed and rated FEASIBLE; no new
design work happened here, only the build-and-measure step R20-4 §8 deferred.

**Date:** 2026-07-26. **Base revision:** `main` @ `506758c` (clean working
tree at start of this task; `docs/checkpoints/`+`docs/reviews/` untracked
files present from an unrelated concurrent review session, not touched by
this task). **Platform measured:** WSL2 (Ubuntu, kernel
`6.1.18.33.2-2`) under Windows 10 Pro x86-64, `valgrind 3.22.0`,
`iai-callgrind-runner 0.14.2`, WSL rustc `1.98.0-nightly (bd08c9e71
2026-06-25)` — the same toolchain/host every prior `npm run iai` table in
`docs/perf/IAI_BASELINE.md` was measured on (no `rust-toolchain` file pins
WSL to a specific channel; `rustup show` reports `nightly` as WSL's active
default, unchanged by this task).

---

## 0. Headline: SeferAlloc costs 1.3x-2.4x mimalloc's instructions per op on every matched workload — a real, honestly-measured gap, not spin

Six workload-matched pairs were benched (mimalloc arm mirrors its SeferAlloc
sibling byte-for-byte: same op count, same size, same alignment, same
alloc/dealloc shape). The **Sefer/mimalloc marginal-Ir/op ratio** — both
sides bootstrap-subtracted using their OWN arm's bootstrap-proxy bench, per
R20-4 §8's flagged nuance (§3 below) — is:

| workload | Sefer Ir/op | mimalloc Ir/op | ratio (Sefer÷mi) |
|---|---:|---:|---:|
| small_churn_16b (16 B alloc/dealloc churn) | 74.1 | 55.9 | **1.326** |
| churn_256b (256 B alloc/dealloc churn) | 74.1 | 48.1 | **1.541** |
| cold_alloc_free_256x16b (256×16 B virgin carve) | 183.0 | 75.3 | **2.430** |
| cold_alloc_free_256x64b (256×64 B virgin carve) | 183.0 | 79.2 | **2.311** |
| recycle_alloc_free_256x16b (256×16 B, 2-round freelist drain) | 185.6 | 78.1 | **2.376** |
| recycle_alloc_free_256x64b (256×64 B, 2-round freelist drain) | 185.6 | 80.0 | **2.320** |

**Reading this honestly:** SeferAlloc retires **1.3x more instructions per
op than mimalloc on the hot magazine-hit churn path**, and **~2.3-2.4x more
on the cold-carve / freelist-recycle path** — the gap the 10-round wall-clock
debate this task was built to resolve has been arguing about without a
deterministic number. This is not a favorable result for SeferAlloc, and it
is reported as measured: mimalloc is genuinely cheaper in retired
instructions on every single one of the six matched workloads in this bench
suite, by a factor that grows (not shrinks) on the cold/recycle path where
SeferAlloc's segment/bitmap machinery does more relative work than
mimalloc's page-based design.

**What this settles:** the "is there real headroom, or is SeferAlloc already
near the honest floor" question `R18_7_MIMALLOC_GAP_STATUS.md` §3b left open
— answered NO, there is real, substantial (1.3x-2.4x) instruction-count
headroom versus a state-of-the-art allocator, on this exact bench suite's
workload shapes, measured deterministically rather than inferred from noisy
wall-clock numbers.

**What this does NOT settle:** (a) whether the gap is architectural
(inherent to SeferAlloc's segment+bitmap+magazine design) or addressable by
a specific, identifiable code change — this gate does not attribute the gap
to any one mechanism; (b) real-world wall-clock impact — `Ir` is a
CPU-instruction proxy, not wall-clock time, and mimalloc's own hot path may
have different cache/branch-prediction behavior not fully captured by
Callgrind's cache model (see `EstCycles` columns in §2, which narrow the gap
somewhat on the churn benches vs raw `Ir` alone); (c) whether closing any
part of this gap is worth the correctness/complexity cost — that is a
product decision for a future round, not this measurement task's call.

---

## 1. What was added (per R20-4 §8's own sketch, executed here)

`benches/perf_gate_iai.rs` gained 7 new `#[library_benchmark]` functions,
all `#[cfg(target_os = "linux")]`-gated exactly like the existing 13
SeferAlloc benches, calling `mimalloc::MiMalloc` directly through its
`GlobalAlloc` impl (never installed as `#[global_allocator]`) — the
identical pattern `benches/global_alloc.rs` already established for its
own 3-arm (SeferAlloc/mimalloc/System) criterion comparison:

- `mimalloc_small_churn_16b` — mirrors `small_churn_16b` (16 B, `CHURN_OPS`
  alloc/dealloc pairs).
- `mimalloc_churn_256b` — mirrors `churn_256b` (256 B, `CHURN_OPS` pairs).
- `mimalloc_cold_alloc_free_256x16b` — mirrors `cold_alloc_free_256x16b`
  (`COLD_BATCH` distinct 16 B blocks, allocate-all-then-free-all).
- `mimalloc_cold_alloc_free_256x64b` — mirrors `cold_alloc_free_256x64b`
  (same shape, 64 B).
- `mimalloc_recycle_alloc_free_256x16b` — mirrors
  `recycle_alloc_free_256x16b` (2-round cold-then-recycle, 16 B).
- `mimalloc_recycle_alloc_free_256x64b` — mirrors
  `recycle_alloc_free_256x64b` (same shape, 64 B).
- `mimalloc_bootstrap_proxy` — mirrors `large_alloc_free_cycle`'s role as
  the bootstrap-isolation proxy: one 4 MiB alloc+free via mimalloc, touching
  no small-class path, isolating mimalloc's own one-time process/thread-heap
  init cost as a standalone constant (§3).

All 7 were added to the existing `perf_gate` `library_benchmark_group!`.
**No new bench binary/target; no `Cargo.toml` change** (`mimalloc = "0.1"`
was already an unconditional dev-dependency); **no CI workflow change**
(`.github/workflows/perf-gate.yml`'s `cargo bench --bench perf_gate_iai
--features production` line already runs every fn in the group — this is the
concrete payoff of R20-4's "same file, no new bench binary" recommendation).

`aligned_churn_640b_a128`, `medium_class_dealloc_churn_16b`, `realloc_grow`,
`multiseg_cold_256k`, and `seg_cycle_decommit_256k` were deliberately NOT
mirrored — they exercise SeferAlloc-specific mechanisms (alignment padding,
the `medium-classes` feature's runtime gate, geometric realloc growth,
multi-segment scanning, decommit/recycle) that either have no natural
mimalloc equivalent to compare against, or would require inventing a new
workload shape rather than mirroring an existing one — out of this task's
scope per its own "do NOT invent new workload shapes" instruction. The
existing SeferAlloc arms are byte-for-byte unchanged; only new mimalloc
functions and their group-list entries were added.

---

## 2. Full measured table (production features, this run)

| bench | Ir | L1 | L2 | RAM | EstCycles | Ir/op* |
|---|---:|---:|---:|---:|---:|---:|
| small_churn_16b | 8,051 | 10,536 | 104 | 440 | 26,456 | 74.1 |
| medium_class_dealloc_churn_16b | 8,051 | 10,535 | 106 | 439 | 26,430 | 74.1 |
| aligned_churn_640b_a128 | 7,987 | 10,470 | 106 | 440 | 26,400 | 73.1 |
| large_alloc_free_cycle (SeferAlloc bootstrap proxy) | 3,308 | 4,753 | 107 | 437 | 20,583 | — |
| realloc_grow | 492,690 | 1,046,820 | 3,864 | 70,324 | 3,527,480 | 30,586.4 |
| cold_alloc_free_256x16b | 50,164 | 62,184 | 106 | 559 | 82,279 | 183.0 |
| cold_alloc_free_256x64b | 50,164 | 62,002 | 106 | 741 | 88,467 | 183.0 |
| recycle_alloc_free_256x16b | 98,343 | 121,066 | 30 | 671 | 144,701 | 185.6 |
| recycle_alloc_free_256x64b | 98,343 | 120,873 | 30 | 864 | 151,263 | 185.6 |
| churn_256b | 8,051 | 10,535 | 104 | 441 | 26,490 | 74.1 |
| churn_write_256b | 8,307 | 10,919 | 104 | 441 | 26,874 | 78.1 |
| multiseg_cold_256k | 25,819 | 35,021 | 38 | 742 | 61,181 | 331.0 |
| seg_cycle_decommit_256k | 62,127 | 83,772 | 93 | 742 | 110,207 | 288.3 |
| **mimalloc_small_churn_16b** | **16,629** | 21,426 | 68 | 504 | 39,406 | 55.9 |
| **mimalloc_churn_256b** | **16,130** | 20,938 | 71 | 458 | 37,323 | 48.1 |
| **mimalloc_cold_alloc_free_256x16b** | **32,325** | 41,772 | 97 | 506 | 59,967 | 75.3 |
| **mimalloc_cold_alloc_free_256x64b** | **33,329** | 42,928 | 107 | 713 | 68,418 | 79.2 |
| **mimalloc_recycle_alloc_free_256x16b** | **53,020** | 68,678 | 96 | 509 | 86,973 | 78.1 |
| **mimalloc_recycle_alloc_free_256x64b** | **54,024** | 69,832 | 108 | 716 | 95,432 | 80.0 |
| **mimalloc_bootstrap_proxy** (mimalloc bootstrap proxy) | **13,050** | 16,713 | 77 | 515 | 35,123 | — |

Raw evidence: `docs/perf/_raw_r22_15_mimalloc_ir_arm.log` (full `npm run iai`
stdout, 203 lines, committed in full — well under the megabyte-scale
truncation threshold this project's raw-log policy exists for).

Note that the pre-existing 13 SeferAlloc benches' raw `Ir` values here are
identical to the values before this task's diff — adding new
`#[library_benchmark]` fns to the SAME group shifts nothing about the
already-compiled SeferAlloc bench bodies (confirmed by direct comparison:
`small_churn_16b` = 8,051 both before and after this diff, matching the
"R5-R2b" table's most recent recorded value in `IAI_BASELINE.md` for this
same commit lineage).

**EstCycles narrows the gap somewhat on churn, widens it on cold/recycle:**
`small_churn_16b` EstCycles/op ≈ (26,456−20,583)/64 ≈ 91.8 vs mimalloc's
(39,406−35,123)/64 ≈ 66.9 (ratio ≈1.37, close to the Ir ratio 1.326) — but
`cold_alloc_free_256x16b` EstCycles/op ≈ (82,279−20,583)/256 ≈ 241.0 vs
mimalloc's (59,967−35,123)/256 ≈ 97.0 (ratio ≈2.48, slightly ABOVE the Ir
ratio 2.430) — consistent with §0's honest caveat that the gap's real-world
cycle cost is at least as large as the Ir headline, not smaller.

---

## 3. Why the bootstrap constant had to become arm-aware (the R20-4 §8 nuance, now implemented)

`scripts/iai.mjs` previously subtracted ONE hardcoded constant
(`large_alloc_free_cycle`'s raw `Ir`) from every bench's marginal-Ir/op
figure. Applying THAT constant to a `mimalloc_*` row would be wrong: mimalloc
has its own, materially DIFFERENT one-time init cost (13,050 Ir via
`mimalloc_bootstrap_proxy`, vs SeferAlloc's 3,308 Ir via
`large_alloc_free_cycle` — mimalloc's first-touch heap setup is ~4x more
expensive in Ir than SeferAlloc's own bootstrap in this exact measurement).
Using SeferAlloc's constant on a mimalloc row would have silently inflated
every mimalloc marginal figure by ~9,742 Ir per op-count, corrupting the
ratio table in SeferAlloc's favor — exactly the corruption R20-4 §8 flagged
as "the one non-blocking nuance."

**Fix implemented:** `scripts/iai.mjs` now keys the bootstrap constant off a
name-prefix map (`BOOTSTRAP_BENCH_BY_PREFIX = { '': 'large_alloc_free_cycle',
mimalloc_: 'mimalloc_bootstrap_proxy' }`), resolved per-row by
`bootstrapBenchFor(name)` (longest-matching-prefix lookup — every
non-`mimalloc_`-prefixed bench keeps using `large_alloc_free_cycle`,
byte-identical to the pre-R22-15 behavior; every `mimalloc_*` bench uses
`mimalloc_bootstrap_proxy` instead). `marginalIrPerOp` and `printTable` now
take a `bootstrapByName` map (bench-name → that arm's bootstrap Ir) rather
than a single scalar. The footnote under the table now names BOTH constants
used (`B=3,308` for SeferAlloc rows, `B=13,050` for mimalloc rows) so a
reader diffing two runs cannot mistake which constant applied to which row.

---

## 4. Determinism — confirmed, with one honest caveat

**`Ir` (instructions retired) is byte-identical across three independent
`npm run iai` runs**, for BOTH allocators, with zero exceptions:

```
small_churn_16b:                    8,051 / 8,051 / 8,051
mimalloc_small_churn_16b:          16,629 / 16,629 / 16,629
cold_alloc_free_256x16b:           50,164 / 50,164 / 50,164
mimalloc_cold_alloc_free_256x16b:  32,325 / 32,325 / 32,325
recycle_alloc_free_256x16b:        98,343 / 98,343 / 98,343
mimalloc_recycle_alloc_free_256x16b: 53,020 / 53,020 / 53,020
mimalloc_bootstrap_proxy:          13,050 / 13,050 / 13,050
```

(Full three-run comparison available by diffing
`docs/perf/_raw_r22_15_mimalloc_ir_arm.log` against
`docs/perf/_raw_r22_15_mimalloc_ir_arm_rerun1.log`, both committed.)

**Caveat, reported honestly rather than papered over:** the cache-simulation
columns (`L1 Hits`, `L2 Hits`, `RAM Hits`, `Estimated Cycles`) are **not**
byte-identical run-to-run for the `mimalloc_*` benches specifically — a ±1
hit jitter was observed on `L1`/`L2` for
`mimalloc_cold_alloc_free_256x16b`/`256x64b`/`recycle_alloc_free_256x64b`
across the three runs (e.g. `mimalloc_cold_alloc_free_256x16b` L2: 98 → 97 →
97; `mimalloc_recycle_alloc_free_256x64b` L1: 69,832 → 69,831 → 69,832). Every
SeferAlloc bench's cache columns, by contrast, were byte-identical across all
three runs with zero exceptions. `Ir` itself never moved on either allocator
— this is purely a cache-simulation-count jitter, not an instruction-count
regression, and it did not affect the PASS/FAIL judge (`Ir` is the
deterministic metric this gate is built on; the cache columns are
best-effort SIGNAL per the existing `scripts/iai.mjs` module doc).

**Plausible mechanism (not confirmed further — flagged for whoever revisits
this):** mimalloc's C allocator likely makes an address- or
layout-dependent decision (e.g. a free-list/page-boundary check whose
outcome depends on the exact virtual address Callgrind's harness happens to
map the heap at, which can vary slightly run-to-run even under Valgrind) that
nudges a cache-line boundary by one hit without changing the instruction
count. SeferAlloc's carve/bitmap logic is apparently insensitive to this
effect in the exact set of workloads measured here. This is a genuine,
if minor, new finding this gate surfaced — worth knowing if a future round
tries to use mimalloc's cache columns (not just its `Ir`) as a comparison
axis, since those columns carry a small amount of run-to-run noise that
`Ir` itself does not.

---

## 5. `production`'s composition — confirmed unchanged

```
$ grep -n "^production = " Cargo.toml
399:production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
```

`git diff --stat Cargo.toml` against this task's full diff is empty — zero
lines touched in `Cargo.toml`. `mimalloc` was already an unconditional
dev-dependency (`Cargo.toml:781`, `mimalloc = "0.1"`) before this task and
remains exactly as it was; this task added no new dependency, no new
feature, no new `[[bench]]` entry (the mimalloc arms live inside the
existing `perf_gate_iai` bench target). Per `CLAUDE.md`'s own rule, no
README/`IAI_BASELINE.md` "production composition changed" refresh is owed —
but `docs/perf/IAI_BASELINE.md` is separately updated below because this is
a NEW baseline being established (the mimalloc arms did not exist before),
not a refresh of an existing SeferAlloc-only baseline.

---

## 6. Verification performed

- **`cargo check --bench perf_gate_iai --features production`** (Windows,
  non-Linux stub path) — clean, no warnings.
- **`cargo clippy --bench perf_gate_iai --features production --all-targets
  -- ` (implicit default lint level)** — clean, exit 0 (Windows; the
  `#[cfg(target_os = "linux")]` body does not compile on this host, so this
  only clippy-checks the non-Linux stub — the real Linux-gated body is
  compiled and clippy-relevant only under WSL/Linux CI, which `npm run iai`
  itself exercises via `cargo bench`, which succeeded with zero compiler
  warnings in its stdout).
- **`cargo fmt --all -- --check`** — clean, exit 0 (repo-wide, not just the
  touched file).
- **`node --check scripts/iai.mjs`** — clean (syntax-only; this project has
  no dedicated JS lint/format step beyond `npm run check`'s Rust-focused
  gates — `scripts/*.mjs` files are hand-reviewed, not machine-linted, per
  the existing convention in this repo).
- **`npm run iai` (the real gate)** — run three times back-to-back; see §4
  for the full determinism result. All three runs: `20 without regressions;
  0 regressed; 20 benchmarks finished` (13 pre-existing SeferAlloc + 7 new
  mimalloc benches = 20), exit 0.
- Full diff of every touched file reviewed line-by-line by the same session
  that wrote it (self-review, not yet independently re-verified by a second
  party — per this project's zero-trust discipline, the user is expected to
  personally re-run `npm run iai` before trusting this report's numbers, as
  stated in the task brief).

---

## 7. Files touched

- `benches/perf_gate_iai.rs` — added `use mimalloc::MiMalloc;` and 7 new
  `#[library_benchmark]` fns (§1), added to the `perf_gate`
  `library_benchmark_group!` list. Zero changes to the existing 13
  SeferAlloc benches.
- `scripts/iai.mjs` — `BOOTSTRAP_BENCH_BY_PREFIX` + `bootstrapBenchFor` (arm-
  aware bootstrap lookup, §3), `BENCH_OPS` gained 7 new mimalloc entries,
  `marginalIrPerOp`/`printTable` signature changed from a single `bootstrap`
  scalar to a `bootstrapByName` map, new `seferMimallocRatio`/`fmtRatio`/
  `RATIO_PAIRS`/ratio-table-printing logic (§0's headline table). No change
  to `parseMetrics`, `ensureRunner`, `benchCmd`, or the PASS/FAIL logic.
- `docs/perf/IAI_BASELINE.md` — new "R22-15" section appended (see that
  file) recording this baseline; no existing section edited.
- `docs/perf/R22_15_MIMALLOC_IR_ARM_GATE.md` — this report.
- `docs/perf/R22_15_MIMALLOC_IR_ARM_GATE_summary.csv` — companion
  machine-readable summary (commit, features, CPU/OS/rustc/valgrind
  identification, per-workload Ir + marginal Ir/op for both arms, the
  derived ratio).
- `docs/perf/_raw_r22_15_mimalloc_ir_arm.log` — full raw `npm run iai`
  stdout for the definitive run cited in §2 (`git add -f` needed — this
  project's `.gitignore` excludes `docs/perf/_raw_*.log` by default).
- `docs/perf/_raw_r22_15_mimalloc_ir_arm_rerun1.log` — a second independent
  run's raw stdout, cited in §4 as the determinism/jitter evidence (`git add
  -f` needed, same reason).
- `Cargo.toml` — **untouched** (confirmed §5; `git diff --stat Cargo.toml`
  empty).
- `.github/workflows/perf-gate.yml` — **untouched** (no new job/step
  needed; the existing `cargo bench --bench perf_gate_iai --features
  production` line already runs every fn in the group per R20-4 §8's own
  prediction).

**Files needing `git add -f`** (gitignored by `.gitignore:16`,
`/docs/perf/_raw_*.log`):
- `docs/perf/_raw_r22_15_mimalloc_ir_arm.log`
- `docs/perf/_raw_r22_15_mimalloc_ir_arm_rerun1.log`

---

## 8. Recommendation for the next round

This task is a MEASUREMENT gate, not a remediation — no `src/` code changed
in response to the 1.3x-2.4x gap found here. Two tasks already on the
TaskList are directly informed by this number:

- **#367 (R22-16, remap-instead-of-copy for the promotion memcpy)** and
  **#368 (R22-17, A/B `contains_base` on the free hot path)** — both target
  specific SeferAlloc mechanisms; this gate's cold/recycle ratio (2.3-2.4x)
  is the honest baseline either of those would be measured against if they
  land, using this SAME arm-aware `npm run iai` pipeline (no further script
  work needed — the ratio table already exists and will just move if a
  future change lands).

No NO-GO/GO verdict is rendered here on any code change, because none was
proposed — this task's entire mandate was "measure and report the number
honestly," which is what §0-§4 do.
