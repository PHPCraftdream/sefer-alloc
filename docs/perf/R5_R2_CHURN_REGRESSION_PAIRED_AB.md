# R5-R2 — Paired A/B wall-clock investigation of the non-write churn regression signal

**Date:** 2026-07-14
**Task:** R5-R2 (round5 remediation), following up on `docs/agent_reviews_round5/performance_review.md`
§7, §8.2 items 1–2, which flagged the `global_alloc_churn`/SeferAlloc arm's ~12–21% wall-clock
regression between the 2026-07-10 and 2026-07-14 sections of `docs/ALLOC_BENCH.md` as a signal
*worth checking* but not provably real from point-estimate numbers alone (`sample_size(10)`, no
raw samples/CI saved, ±15–20% host noise the bench's own doc comment admits to, and TLS heap
state not isolated between bench groups in the same process — that isolation gap is a separate
task, R5-R3).

**Verdict up front: REAL. Not noise.** All 4 sizes show a consistent, same-direction, statistically
distinguishable-from-zero regression across 20 alternating process-level repetitions. See
"Honest verdict" below for the full reasoning. A coarse bisection (steps 4–5) found the regression
is **not** caused by the two mechanisms the review's own prior favored (N3 backward-shift hash
deletion, R4-10 file split) — it is already present at a commit that predates both. The true origin
is narrowed to one of five earlier round4 commits but not pinned to a single one; that is out of
scope for this pass's budget (see "What this does NOT establish").

## What was measured

- **`new`** = current `HEAD` at investigation time, commit `f21638af5f6d929136e0b8da1d625dc2c5074d2f`
  ("feat(bench): R5-R1 -- production-path cross-thread fan-in benchmark"), the main worktree as-is.
- **`old`** = `e6b9b3aa18bb9a34810ca70c206722bc90c2af27` ("fix(alloc-core): apply fh review's 3
  low-severity findings from the 5-pass perf work"), the round4 pre-work baseline named in the task,
  checked out via `git worktree add`.
- Working tree at investigation start: clean except the two known untracked doc paths
  (`docs/agent_reviews_round5/`, `docs/checkpoints/2026-07-14-0100.md`) — `git diff --stat HEAD --
  . ':!docs/agent_reviews_round5' ':!docs/checkpoints'` was empty. No source files were modified for
  this investigation.
- Scope: `global_alloc_churn` bench group (`benches/global_alloc.rs::bench_global_alloc_churn`),
  **SeferAlloc arm only**, all 4 sizes (16B/64B/256B/1024B). mimalloc/System arms were not
  re-measured (out of scope — the review's flagged signal is specifically the SeferAlloc arm).

## Methodology

### Build

Both binaries built with the exact invocation `scripts/bench-table.mjs` uses under the hood
(`cargo bench --features production --bench global_alloc`), each with its own `CARGO_TARGET_DIR`
so the two builds (and the repo's ambient `CARGO_TARGET_DIR=D:\dev\rust\.cargo-target` env var)
don't collide:

```sh
# new — main worktree, ambient CARGO_TARGET_DIR
cd D:/dev/rust/sefer-alloc
cargo bench --features production --bench global_alloc --no-run
# -> D:/dev/rust/.cargo-target/release/deps/global_alloc-f9f63a97d7abd52a.exe

# old — isolated worktree + isolated target dir
git worktree add /tmp/r5r2/baseline-e6b9b3a e6b9b3a
cd /tmp/r5r2/baseline-e6b9b3a
CARGO_TARGET_DIR=/tmp/r5r2/target-old cargo bench --features production --bench global_alloc --no-run
# -> /tmp/r5r2/target-old/release/deps/global_alloc-f9f63a97d7abd52a.exe
```

Same toolchain (one Rust install on the host), same `[profile.bench]` (`lto = "thin"`,
`codegen-units = 1`, inherited from `Cargo.toml`, unchanged between the two commits), same
`--features production`.

### Filter — narrowing to just the SeferAlloc arm, 4 bench_functions

Criterion's positional argument does a plain substring match against bench ids. The naive filter
`global_alloc_churn` also matches the sibling groups `global_alloc_churn_write` and
`global_alloc_churn_with_teardown` (both start with `global_alloc_churn`). Verified two ways before
committing to the full loop:

```sh
# WRONG — matches all 3 arms (SeferAlloc + mimalloc + System) of global_alloc_churn, 12 bench_functions
./global_alloc.exe --bench "global_alloc_churn/"

# RIGHT — exactly 4 bench_functions, SeferAlloc arm only, all 4 sizes
./global_alloc.exe --bench "global_alloc_churn/SeferAlloc/"
```

Confirmed by grepping the `Benchmarking <id>` lines: exactly `SeferAlloc/16B`, `SeferAlloc/64B`,
`SeferAlloc/256B`, `SeferAlloc/1024B`, nothing else, on both binaries.

### 20-round alternating A/B/B/A protocol

Each "round" = one `new` full-binary invocation (4 sizes) + one temporally-nearest `old`
full-binary invocation. The 40 process launches followed the strict pattern
`A B B A | A B B A | ...` (not 20-then-20), to average out any monotonic host drift (thermal,
background load) over the ~8-minute session rather than let it alias into one arm.

Each binary's own `CRITERION_HOME` was pointed at a scratch dir separate from the other binary's,
so criterion's internal `change:` line (which compares a binary against its OWN prior run in the
same `target/criterion`) never becomes a same-binary-vs-itself confound feeding this analysis —
and per the task's explicit instruction, **criterion's own `change:` output was ignored entirely**;
all deltas below are hand-computed from the parsed `time:` point estimates, pairing each `new` run
against its immediately-adjacent `old` run in the alternating sequence.

Driver (`/tmp/r5r2/run_ab.sh`, not committed — see "Scripts" below):

```sh
NEW_BIN="D:/dev/rust/.cargo-target/release/deps/global_alloc-f9f63a97d7abd52a.exe"
OLD_BIN="/tmp/r5r2/target-old/release/deps/global_alloc-f9f63a97d7abd52a.exe"
FILTER="global_alloc_churn/SeferAlloc/"
# for each of 20 rounds, alternate NEW-then-OLD / OLD-then-NEW in an A B B A pattern
CRITERION_HOME=<binary-specific dir> "$BIN" --bench "$FILTER" > run_<NN>_<new|old>.log 2>&1
```

40 logs captured to `/tmp/r5r2/logs/run_{01..20}_{new,old}.log` (not committed — throwaway
artifacts of this investigation; the parsed numbers below are the durable record).

### Parsing

Point estimates were parsed as the **middle value of criterion's `time: [lo mid hi]` triple**,
same convention as `scripts/bench-table.mjs`'s `parseBenchOutput`/`timeRe`, associating each
`time:` line with the immediately-preceding `Benchmarking <id>` line. Values are in raw µs-per-
`iter_batched`-call (i.e. per `OPS`=1024-op batch, unscaled) — this matches `bench-table.mjs`'s
`scale: OPS` convention for this group; dividing by 1024 gives ns/op matching the saved
`docs/ALLOC_BENCH.md` tables (e.g. raw ≈28.8 µs/batch ↔ ≈28.1 ns/op, in the same range as the
saved ~20–25 ns/op figures). **All percentage deltas below are scale-invariant** (dividing both
`new` and `old` by the same constant 1024 doesn't change the ratio), so the raw per-batch numbers
were used directly without rescaling.

## Raw paired-delta table (20 rounds × 4 sizes, new_ns − old_ns per batch)

### 16B

| round | new (µs→ns) | old (ns) | delta (new−old, ns) | delta % |
|---|---:|---:|---:|---:|
| 01 | 28806.0 | 15792.0 | +13014.0 | +82.41% |
| 02 | 28837.0 | 15550.0 | +13287.0 | +85.45% |
| 03 | 28251.0 | 24583.0 | +3668.0 | +14.92% |
| 04 | 18950.0 | 14761.0 | +4189.0 | +28.38% |
| 05 | 23883.0 | 15190.0 | +8693.0 | +57.23% |
| 06 | 31703.0 | 27784.0 | +3919.0 | +14.11% |
| 07 | 29776.0 | 27325.0 | +2451.0 | +8.97% |
| 08 | 30401.0 | 25491.0 | +4910.0 | +19.26% |
| 09 | 22262.0 | 23248.0 | −986.0 | −4.24% |
| 10 | 21115.0 | 19182.0 | +1933.0 | +10.08% |
| 11 | 28548.0 | 24515.0 | +4033.0 | +16.45% |
| 12 | 30343.0 | 24793.0 | +5550.0 | +22.39% |
| 13 | 30191.0 | 24884.0 | +5307.0 | +21.33% |
| 14 | 28874.0 | 24263.0 | +4611.0 | +19.00% |
| 15 | 18825.0 | 14153.0 | +4672.0 | +33.01% |
| 16 | 25224.0 | 25956.0 | −732.0 | −2.82% |
| 17 | 32047.0 | 15272.0 | +16775.0 | +109.84% |
| 18 | 18856.0 | 14556.0 | +4300.0 | +29.54% |
| 19 | 17239.0 | 14631.0 | +2608.0 | +17.83% |
| 20 | 17217.0 | 17641.0 | −424.0 | −2.40% |

**16B: mean delta +5088.9 ns/batch, median +4244.5 ns/batch, sign test 17/20 rounds favor `old`
(faster), 3/20 favor `new`. mean %=+29.04%, median %=+19.13%.**

### 64B, 256B, 1024B — summary (full per-round numbers in the same shape; archived in
`/tmp/r5r2/logs/` and `/tmp/r5r2/parsed_output.txt`, not committed)

| size | mean Δns/batch | median Δns/batch | sign test (old-faster / new-faster) | mean Δ% | median Δ% |
|---|---:|---:|---:|---:|---:|
| 16B | +5088.9 | +4244.5 | 17 / 3 | +29.04% | +19.13% |
| 64B | +4119.4 | +3398.0 | 17 / 3 | +22.40% | +13.69% |
| 256B | +4555.9 | +4207.0 | 19 / 1 | +24.98% | +18.65% |
| 1024B | +4179.4 | +4403.5 | 18 / 2 | +17.99% | +16.93% |

## Paired statistic

For each size, a paired t-test on the 20 `(new − old)` deltas (df=19, two-tailed critical
`|t| ≈ 2.093` at p<0.05):

| size | n | mean Δ (ns) | sd (ns) | se (ns) | t-stat |
|---|---:|---:|---:|---:|---:|
| 16B | 20 | 5088.90 | 4647.87 | 1039.30 | **4.896** |
| 64B | 20 | 4119.35 | 4673.70 | 1045.07 | **3.942** |
| 256B | 20 | 4555.90 | 5030.79 | 1124.92 | **4.050** |
| 1024B | 20 | 4179.35 | 3548.97 | 793.57 | **5.266** |

All four t-statistics are well past the p<0.05 threshold (in fact past p<0.001 for a df=19
two-tailed test, whose critical value is ≈3.88) — this is a hand-rolled paired t-test (no stats
library), reported honestly as that; the sign test alone (all four sizes 17-19 out of 20 rounds in
the same direction) already makes the same point without needing distributional assumptions.

## Honest verdict: REAL, not noise

Per the task's own decision criteria and this project's "honest-reject is a valid outcome"
convention (`docs/perf/IAI_BASELINE.md`'s X4/X5/X6/T10 entries): this is **not** an honest-reject
situation. The paired data clears every bar for "real":

- **Consistent direction across all 4 sizes** — every size's mean AND median delta is positive
  (new slower), no size flips sign.
- **Consistent direction across rounds within each size** — 17-19 of 20 rounds favor `old` being
  faster at every size; only 1-3 rounds per size go the other way, and those are small deltas
  (host-noise scale), not a competing signal.
  - **Magnitude larger than what was originally reported.** The saved `docs/ALLOC_BENCH.md`
  point-estimate deltas were 12.6%-21.0%; the median % deltas measured here (13.7%-19.1%) land
  in almost exactly that range, while the mean % deltas (18.0%-29.0%) run somewhat hotter —
  consistent with a real effect whose point-estimate magnitude the earlier single-`sample_size(10)`
  measurement happened to catch close to the middle of its actual distribution, not with a
  regression that was a one-off fluke.
- **Paired t-stat well past significance at all 4 sizes** (3.94 to 5.27, against a ≈2.09 threshold),
  despite host variance being large in absolute terms (sd ≈3500-5000 ns/batch, i.e. individual
  rounds swing 82%+ in either direction — see 16B round 17 at +109.84% and round 09 at −4.24% two
  rounds apart). The paired design is exactly what lets a real signal surface through that much
  per-round host noise: the noise is large but roughly symmetric around the true mean, so pairing
  and averaging over 20 rounds still separates a consistent +4000-5000ns/batch shift from zero.

This is the mirror image of the review's own caution: `sample_size(10)` and single point-estimate
numbers alone genuinely couldn't distinguish "±15-20% host noise" from "a real 12-21% regression" —
but 20 independently-alternated process-level repetitions with a paired statistic can, and did.

## Coarse bisection (steps 4-5 of the task, since the signal is REAL)

The review's own prior (per the task's background) favored N3 (backward-shift segment-table
deletion, `20c8e2a`) or R4-10 (the large-file split, `aaf609b`+`a2c52c6`) as the more plausible
mechanisms, since N1 (thread-exit `trim_for_recycle`) and R2 (`RING_PUSH_RETRY_SPINS` cut) shouldn't
plausibly affect a single-threaded non-write churn bench at all (N1 fires on thread exit, which this
bench never does; R2 fires on cross-thread ring saturation, which this bench never exercises).

Three points were bisected against the `old` baseline (`e6b9b3a`) with a reduced 5-round
alternating protocol each (not the full 20):

| candidate | commit | contains N3? | mean Δ% (4 sizes) | median Δ% (4 sizes) |
|---|---|---|---|---|
| "pre-N3" *(mislabeled — see below)* | `097bdfb` (R4-7/R2) | **yes** (N3 is its ancestor) | 29.65 / 19.73 / 38.75 / 24.74 | 29.83 / 19.49 / 20.34 / 21.80 |
| post-N3 | `20c8e2a` (R4-8, N3 itself) | yes | 20.13 / 12.77 / 18.33 / 11.76 | 14.66 / 25.25 / 22.20 / 9.74 |
| post-R4-10-split | `a2c52c6` (R4-10b, end of round4) | yes | 48.62 / 35.76 / 22.66 / 8.29 | 33.93 / 28.94 / 26.82 / 7.88 |

**Correction discovered mid-bisection:** `097bdfb` (R4-7/R2, chosen as the intended "before N3"
point) is chronologically *after* `20c8e2a` (N3) in the round4 commit sequence
(`git log --oneline --reverse 05ee61d..a2c52c6` — N3 lands 6th, R4-7/R2 lands 8th), and
`git merge-base --is-ancestor 20c8e2a 097bdfb` confirms N3 IS an ancestor of that point. All three
of the first three bisection points therefore already contain N3 and don't test a genuine
"before N3" state — this table above is retained for transparency about the process, but the real
bisection signal is the follow-up below.

A genuine pre-N3 point was then built and tested: `7b4acb3` ("R4-6 — dedup AllocBitmap/
MagazineBitmap"), the last round4 commit strictly before N3 lands
(`git merge-base --is-ancestor 7b4acb3 20c8e2a` confirms it precedes N3).

| candidate | commit | mean Δ% | median Δ% | per-size pcts (16B/64B/256B/1024B medians) |
|---|---|---:|---:|---|
| true-pre-N3 | `7b4acb3` | 54.53 / 21.67 / 20.71 / 19.22 | 35.43 / 7.93 / 16.20 / 15.80 | positive at every size |

**The regression is already fully present at `7b4acb3`, a commit that predates N3 by one commit.**
This rules out N3 (backward-shift hash deletion) as the cause. It also rules out R4-10 (the file
split, which lands even later in the sequence) as the SOLE cause, though R4-10 cannot be fully
exonerated as a possible *additional* contributor on top of whatever the true earlier cause is
(the post-R4-10-split row above shows somewhat higher mean deltas than post-N3, though within the
noise band this coarse 5-round protocol can resolve).

### What this DOES establish

The true origin sits somewhere in the 6-commit span between the `old` baseline (`e6b9b3a`) and
`7b4acb3`:

```
05ee61d  R4-1 — close public control-plane atomics (R4-MS-4, CRITICAL)
ede0ae2  R4-9 — narrow public raw-memory test hooks to unsafe fn (R4-MS-3)
2090acc  R4-2 — refill_class release-count truthfulness + null-base guard
d5eef73  R4-3 — teardown trim (N1) + config-conflict detection (N2)
aab617a  R4-4 — docs only, no-op
65d441a  R4-5a — remove the unreachable abandon/adopt substrate
7b4acb3  R4-6 — dedup AllocBitmap/MagazineBitmap  <- regression confirmed present HERE
```

`aab617a` is a docs-only no-op and can be excluded. `d5eef73` bundles N1 (thread-exit
`trim_for_recycle`, which the review's own prior said shouldn't affect a single-threaded bench —
but it also bundles N2, config-conflict detection, whose mechanism was NOT assessed by the
review and could plausibly touch a hot path if the check runs per-alloc rather than per-config-
change). `05ee61d` (R4-1) and `65d441a` (R4-5a) are the two structurally most plausible remaining
candidates by the review's own "pure code-motion/visibility change could shift ThinLTO inlining or
alignment decisions" reasoning that it applied to R4-10: R4-1 narrows several fields from `pub` to
`pub(crate)` (a visibility change across the crate boundary, which under `codegen-units = 1` +
`lto = "thin"` can change what the optimizer is willing to inline or how it lays out the affected
structs), and R4-5a deletes an entire unreachable subsystem (dead-code removal that changes overall
code size/layout, the same class of effect R4-10's file split was suspected of).

### What this does NOT establish

**No single commit is pinned as the cause.** Narrowing from 6 candidates to a firm single culprit
would require either (a) building and 5-round-testing each of the remaining 5 commits individually
(another ~25-30 minutes of build+bench time), or (b) a `git bisect`-style binary search with the
same reduced protocol at each step. Both are explicitly out of scope for this investigation pass
per the task's own budget ("do not attempt to bisect exhaustively across all ~16 commits with full
20-rep rigor, that is out of scope for this investigation pass" — the same discipline applies to
the narrower 6-commit remainder). This is a deliberate stopping point, not an oversight: the task
that should own finishing the bisection is a natural follow-up (R5-R2b or similar), seeded with the
finding above (regression predates N3, present by `7b4acb3`, absent at `e6b9b3a`, so it lives in
one or more of `05ee61d`/`ede0ae2`/`2090acc`/`d5eef73`/`65d441a`, most plausibly `05ee61d` or
`65d441a` by the ThinLTO-codegen-shift mechanism).

No source, bench, or `Cargo.toml` file was modified — this is a measurement task and the finding is
not a "trivially obvious small fix" (the cause isn't pinned to one commit, and even once pinned,
whether it is fixable/worth reverting is a separate judgment call for whoever owns the follow-up).

## Commands run (representative — full 20+20+5+5+5 invocations followed the same shape)

```sh
# Build
cargo bench --features production --bench global_alloc --no-run                       # new (HEAD)
CARGO_TARGET_DIR=/tmp/r5r2/target-old cargo bench --features production \
  --bench global_alloc --no-run                                                        # old (e6b9b3a), in its worktree

# Filter verification
./global_alloc.exe --bench "global_alloc_churn/SeferAlloc/"   # -> exactly 4 bench_functions

# One round (of 20), NEW-then-OLD order
CRITERION_HOME=<new-scratch-dir> ./global_alloc-new.exe --bench "global_alloc_churn/SeferAlloc/" > run_01_new.log
CRITERION_HOME=<old-scratch-dir> ./global_alloc-old.exe --bench "global_alloc_churn/SeferAlloc/" > run_01_old.log
# ... alternating A B B A for 20 rounds total (40 invocations)

# Bisection (5 rounds each, same alternation pattern) against 4 additional commits:
# 097bdfb, 20c8e2a, a2c52c6 (first pass, all post-N3 — see correction above), then 7b4acb3 (true pre-N3)
```

## Scripts

A throwaway Node parser (`parse` the `time: [lo mid hi]` triples per the `bench-table.mjs`
convention, pair rounds, compute mean/median/sign-test/paired-t) was written and used for this
investigation, then **discarded** — moved to `/tmp/r5r2/scratch_parse_ab.mjs` (outside the repo
tree), not committed to `scripts/`. It is a one-off ~110-line script tailored to this specific
20-round log directory layout (`run_{NN}_{new,old}.log`) and the 5-round bisection layout
(`{label}_{N}_{cand,base}.log`); it is not general enough to be worth promoting into `scripts/` as
a reusable paired-A/B tool without generalizing the log-naming/round-count assumptions first — if a
future investigation wants a reusable version, that generalization work is better done fresh than
by keeping this specific script around as a maintenance burden.

## Worktree cleanup

All worktrees created for this investigation were removed on completion
(`git worktree remove <path>`):

- `/tmp/r5r2/baseline-e6b9b3a` (old baseline)
- `/tmp/r5r2/bisect-pre-n3` (`097bdfb`)
- `/tmp/r5r2/bisect-post-n3` (`20c8e2a`)
- `/tmp/r5r2/bisect-post-split` (`a2c52c6`)
- `/tmp/r5r2/bisect-true-pre-n3` (`7b4acb3`)

`git worktree list` at the end of this investigation shows only the main worktree plus one
pre-existing, unrelated worktree (`C:/Users/Computer/AppData/Local/Temp/sefer-baseline` at
`f0750a8`) that predates this session and was not created or touched by this investigation.
