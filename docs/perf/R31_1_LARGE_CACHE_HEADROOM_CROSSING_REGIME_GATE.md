# R31-1 — large-cache `headroom_bytes` hit-rate at a burst size that GENUINELY exceeds 64 MiB

**Task #464 (R31-1), Round 31.** R30-6
(`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md`) found 64 MiB and 256 MiB
headroom tied at 100.0% hit rate, at a workload labelled "48 MiB/burst,
8×6 MiB objects." R31-12 (task #476, same round, `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md`'s
2026-07-30 addendum) independently confirmed that workload's ACTUAL rounded
working set is **64 MiB, not 48 MiB** (`AllocCore::alloc_large` rounds every
Large allocation's usable span UP to a whole number of 4 MiB `SEGMENT`s —
`src/alloc_core/alloc_core_large.rs:188-192` — so a 6 MiB object costs a
2-segment, 8 MiB span; 8 objects × 8 MiB = 64 MiB, confirmed directly by
R30-6's own committed CSV, whose `burst1_used_max_bytes` column reads
`67108864` = exactly 64 MiB in every one of its 36 rows). R30-6's 64-vs-256
MiB comparison therefore never left the 64 MiB boundary — this task supplies
the ONE missing measurement: hit rate at a burst size that genuinely EXCEEDS
64 MiB, per the task brief's own named range (128-272 MiB, R29-13's regime).

**Verdict: the tie breaks. Once the burst genuinely exceeds 64 MiB, 64 MiB
headroom costs the SAME real, reproducible 12.5-percentage-point hit-rate
loss R30-6 already measured for 16 MiB and 0 MiB headroom (87.5% vs 100.0%
for 256 MiB) — exact and identical at every thread count (1/8/32) and at
BOTH crossing-regime burst sizes tested (128 MiB and 288 MiB). The
AT_BOUNDARY control (R30-6's original 6 MiB/object, 64 MiB burst, run
in THIS SAME harness for a same-process comparison) reproduces R30-6's tie
exactly (100.0% both). MEASUREMENT ONLY — `DEFAULT_HEADROOM_BYTES` (256
MiB) is untouched; this report does not change any `src/` default.**

**Date:** 2026-07-30. **Base revision:** `main` @ `f93e66311ad3ea47aaa1a2fe2461caeb4c0968fe`
(clean at session start, confirmed via `git rev-parse HEAD` before any edit)
+ this task's working tree, landed together in the SAME commit this report
cites — per CLAUDE.md's R29-6 immutable-source-identity rule (form 1: the
actual landing commit SHA), filled in by this small follow-up commit after
the landing commit itself (chicken-and-egg: a commit cannot cite its own SHA
inside its own tree — matches the `1272a52`/`c7b3eda`/`d9d30cd`/`f93e663`
established pattern). **Landing commit:** `fc11cf3c03916079982bbc06bef8c2c80bf773ea`.
**Platform:** native Windows 10 Pro x86-64, 11th Gen Intel Core i7-11800H @
2.30GHz (8 cores / 16 logical), rustc 1.97.0 — the same host as R30-6.
**Feature set:** `production alloc-stats bench-internals`
(`examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`).

---

## 0. Headline numbers

### 0.1 Hit rate by burst size (subprocess-per-arm, registry-bypass, median of 3 reps)

| burst arm | per-object size | rounded span/object | burst total | headroom | threads | hits/possible | hit rate |
|---|---:|---:|---:|---:|---:|---:|---:|
| AT_BOUNDARY_6MiB (R30-6 control) | 6 MiB | 8 MiB | **64 MiB** | 64 MiB | 1/8/32 | 8/8, 64/64, 256/256 | **100.0%** |
| AT_BOUNDARY_6MiB (R30-6 control) | 6 MiB | 8 MiB | **64 MiB** | 256 MiB | 1/8/32 | 8/8, 64/64, 256/256 | **100.0%** |
| CROSSING_MODEST_12MiB | 12 MiB | 16 MiB | **128 MiB** | 64 MiB | 1/8/32 | 7/8, 56/64, 224/256 | **87.5%** |
| CROSSING_MODEST_12MiB | 12 MiB | 16 MiB | **128 MiB** | 256 MiB | 1/8/32 | 8/8, 64/64, 256/256 | **100.0%** |
| CROSSING_R29_13_34MiB | 34 MiB | 36 MiB | **288 MiB** | 64 MiB | 1/8/32 | 7/8, 56/64, 224/256 | **87.5%** |
| CROSSING_R29_13_34MiB | 34 MiB | 36 MiB | **288 MiB** | 256 MiB | 1/8/32 | 8/8, 64/64, 256/256 | **100.0%** |

Raw log: `docs/perf/_raw_r31_1_large_cache_headroom_crossing_regime_gate.log`
(54 child runs: 3 burst arms × 2 headroom arms × 3 thread counts × 3 reps).
Summary CSV: `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE_summary.csv`
(derived by `scripts/r31_1_derive_report_data.mjs`, which also
hard-asserts the headline arithmetic above — see §1.4).

**All 54 arms passed the path-activation oracle** (`admissions_ok` AND
`hits_ok`, both hard-asserted before the arm's numbers were trusted — same
two-piece oracle as R30-6, §1.2 below). **Every arm additionally passed a
NEW sanity assertion** (rejecting a physically-impossible RSS collapse
across the pure-idle window before it could enter any table — see §1.5;
this is the R31-12/task #476 repair, applied to this harness from the
start rather than retrofitted).

### 0.2 The tie breaks exactly where the arithmetic predicts

- **At the boundary (64 MiB burst, exactly AT the 64 MiB headroom target):**
  64 MiB and 256 MiB headroom are indistinguishable — both 100.0%. This
  reproduces R30-6's own finding byte-for-byte, now confirmed inside the
  SAME harness run (not merely re-cited from a different report), because
  `maybe_decay_large_cache`'s fast-path early-return
  (`large_cache_used_bytes <= headroom_bytes`,
  `src/alloc_core/alloc_core_large_cache.rs:320-330`) fires unconditionally
  when occupancy is AT OR BELOW the headroom target — decay never even
  attempts eviction.
- **Past the boundary (128 MiB or 288 MiB burst, genuinely EXCEEDING the 64
  MiB headroom target):** the SAME fast-path early-return does NOT fire for
  the 64 MiB arm (occupancy 128/288 MiB > 64 MiB target), so once the idle
  interval crosses the 1000 ms decay interval, a real decay tick evicts
  whole large-cache slots down toward the 64 MiB target — costing exactly
  1 of 8 slots (12.5 percentage points), identical in magnitude and exact
  reproducibility to R30-6's own 16 MiB/0 MiB findings at the (at-the-time)
  only-available 64 MiB burst size. The 256 MiB arm's occupancy (128/288
  MiB) never exceeds ITS 256 MiB target at either crossing-regime size, so
  the fast-path still fires for it and it stays at 100.0%.

**This is the "real cost appears" answer the task brief asked for**: 64 MiB
headroom is NOT a free lunch relative to 256 MiB in general — it only tied
256 MiB in R30-6 because that gate's workload happened to sit exactly at the
64 MiB boundary by an unintended rounding accident (12 objects at 6 MiB
"should" have been 48 MiB by the report's own prose, but rounds to 64 MiB in
practice). Once the burst genuinely exceeds 64 MiB — as R29-13's own 34
MiB/object workload does, and as a realistic 128 MiB-class burst does — 64
MiB headroom pays the same 12.5-percentage-point hit-rate cost 16 MiB and 0
MiB already paid in R30-6.

---

## 1. Methodology

### 1.1 Direct sibling of R30-6, same subprocess-per-arm shape

`examples/r31_1_large_cache_headroom_crossing_regime_gate.rs` is a
line-for-line-derived sibling of `examples/r30_6_large_cache_headroom_ab_gate.rs`
— same registry-bypass `HeapRegistry::claim_with_config` subprocess-per-arm
isolation, same mixed small+large workload shape (small churn + BURST1 →
IDLE(1200ms) → BURST2), same path-activation oracle, same R26-4
config-identity pieces. The ONE structural change: the object size and
headroom-arm grid are parameterized (`BURST_ARMS`, `HEADROOM_ARMS`) so a
single binary sweeps THREE burst sizes instead of R30-6's one, restricted to
the two headroom values (64/256 MiB) whose relationship is this task's whole
question (0/16 MiB already showed a real cost in R30-6 at the boundary size
and are not re-swept here — see the harness's own module doc).

### 1.2 Path-activation oracle + config identity (unchanged from R30-6)

Every child hard-asserts, before its `RESULT` lines print:

1. **`admissions_ok`** — `burst1_used_max > 0` (at least one large span was
   genuinely cached after BURST1's teardown).
2. **`hits_ok`** — `burst2_hits_sum > 0` (BURST2 genuinely reused a size
   class BURST1 populated).
3. **Resolved headroom** — read back via `HeapCore::dbg_decay_config()`,
   hard-`assert_eq!`'d against the requested value.
4. **`config_conflicts_delta == 0`** — read via `config_conflicts_total()`
   before/after.
5. **Process identity** — subprocess-per-arm (a fresh OS process per
   `(burst_label, headroom_bytes, thread_count, repetition)` cell, launched
   via `std::process::Command::new(current_exe)`), recorded in the CSV's
   `process_identity` column.

**All 54 child runs passed every self-check.**

### 1.3 Burst-size arms and their exact rounded working sets

| label | object size | header-rounded span | objects | total burst |
|---|---:|---:|---:|---:|
| `AT_BOUNDARY_6MiB` | 6 MiB | 8 MiB (2 segments) | 8 | 64 MiB |
| `CROSSING_MODEST_12MiB` | 12 MiB | 16 MiB (4 segments) | 8 | 128 MiB |
| `CROSSING_R29_13_34MiB` | 34 MiB | 36 MiB (9 segments) | 8 | 288 MiB |

Rounding arithmetic (verified against `alloc_core_large.rs:127-194`,
`SEGMENT = 4 MiB` = `1 << 22`, `src/alloc_core/os.rs:65`): `needed =
align_up(size_of::<SegmentHeader>(), PAGE) + align_up(size, align)`, then
(without the opt-in `exact-span-large` feature, which is NOT part of
`production` and is NOT enabled by this gate's feature set)
`usable = needed.div_ceil(SEGMENT) * SEGMENT`. For a 6 MiB object this is
`ceil((4096 + 6291456) / 4194304) = ceil(1.501...) = 2` segments = 8 MiB —
confirmed by every row of this gate's own `burst1_used_max_bytes` column
(`67108864` = 64 MiB for the AT_BOUNDARY arm, `8 × 8 MiB`).
`CROSSING_MODEST_12MiB` and `CROSSING_R29_13_34MiB` were chosen (12 MiB and
34 MiB respectively) so their rounded per-object spans (16 MiB, 36 MiB) push
the 8-object burst total to 128 MiB and 288 MiB — both genuinely past the 64
MiB headroom target, the latter landing inside R29-13's own regime
(34 MiB/object, the task brief's named range).

`LARGE_CACHE_SLOTS = 8` is held constant across every arm (`LARGE_OBJ_COUNT
= 8` in all three burst arms) — only the byte-size axis moves, so the
slot-count ceiling (the mechanism R30-6 §2 identified as why 64/256 MiB tied
in ITS workload) cannot itself explain any difference found here; only the
byte-size axis can.

### 1.4 Between-arm mechanism delta (CLAUDE.md's R30-8 rule) — checked, not hand-transcribed

`scripts/r31_1_derive_report_data.mjs` derives §0.1's table directly from
the raw per-child CSV block and hard-asserts, in-script, before printing
anything:

- `AT_BOUNDARY_6MiB`'s 64 MiB and 256 MiB hit rates are IDENTICAL at every
  thread count (the tie).
- `CROSSING_MODEST_12MiB`'s and `CROSSING_R29_13_34MiB`'s 64 MiB and 256 MiB
  hit rates DIFFER (the crossing-regime finding is the entire point of this
  gate — a script that silently tolerated a tie here would defeat the
  measurement).
- The gap at both crossing-regime sizes is exactly 12.5 percentage points
  (matching R30-6's own 16/0 MiB finding's magnitude).

A failing assertion aborts the script before any CSV or table is written —
see the script's own `HEADLINE ASSERTION FAILED` error paths. Run output
confirming all three assertions passed on this measurement is reproduced
verbatim at the top of `docs/perf/_raw_r31_1_large_cache_headroom_crossing_regime_gate.log`'s
companion script invocation (re-run via `node scripts/r31_1_derive_report_data.mjs`
against the committed raw log to reproduce).

The orchestrator itself additionally prints a raw `burst1_used_max` mechanism
table per (burst, threads) pair at 64 vs 256 MiB headroom (see the raw log's
own `=== between-arm mechanism delta ===` section) — `burst1_used_max` is
identical between the two headroom arms at every burst size (expected: it is
sampled right after BURST1's fill, before the idle-triggered decay tick has
had a chance to run — R29-13 §3's first-call-timer-priming rule means BURST1
itself never decays regardless of headroom). The actual mechanism
divergence is visible in BURST2's hit rate, which the §0.1 table reports.

### 1.5 New sanity assertion (R31-12/task #476 repair, applied here from the start)

R31-12 (the sibling task landing in the same round) found R30-6's raw log
carried one physically-impossible RSS sample (a 32-thread process's RSS
dropping from ~1.58 GiB to 424 KiB across a 1.2s pure-idle window with zero
deallocation activity) that was neither excluded nor flagged. This gate's
harness adds the fix directly (not merely documents it): every child
hard-asserts `rss_burst1_kib - rss_idle_kib <= rss_burst1_kib / 10 + 4096`
before its `RESULT` lines print — the assertion text explains why (no
deallocation activity occurs between these two samples, so a large drop
indicates a broken `proc_probe` sample, not real allocator behavior). All 54
runs in this gate passed the assertion with no violations.

---

## 2. What this gate does NOT claim

- **Not a re-sweep of 0/16 MiB headroom at the crossing-regime sizes.**
  R30-6 already established 0/16 MiB cost the SAME 12.5-percentage-point
  hit-rate loss relative to 256 MiB at the (unintentionally) at-boundary 64
  MiB burst size; this gate's whole point was resolving 64-vs-256 MiB, so
  0/16 MiB were not re-included in this grid (they would be expected, not
  informative, to show an equal-or-larger cost at a bigger burst — a
  candidate follow-up if that specific number is ever decision-relevant, not
  measured here).
- **Not an exhaustive burst-size characterization.** Two crossing-regime
  points (128 MiB, 288 MiB) were measured, both showing the identical 12.5pp
  gap — consistent with, but not proof of, the gap holding at every burst
  size above 64 MiB. A different burst size closer to the 64 MiB boundary
  (e.g. 68-96 MiB) could in principle show a smaller gap (fewer whole slots
  needing eviction) — this gate establishes that the tie breaks and by how
  much at two representative points, not the full curve, matching R30-6
  §5's own "not an exhaustive characterization" caveat.
- **No new latency-axis measurement.** This task's brief was hit-rate at
  >64 MiB burst occupancy; the real-`#[global_allocator]` latency A/B (R30-6
  §0.2) is not re-run here. R30-6's own latency null (§0.2) already used a
  100%-hit-rate-for-all-arms workload (its own §5 disclosure), so it cannot
  speak to the latency cost of the hit-rate loss this gate measures either —
  see R30-6's 2026-07-30 addendum (task #476) item 5 for the explicit
  statement of that gap.
- **This report does not change any `src/` default.**
  `DEFAULT_HEADROOM_BYTES` (256 MiB, `src/alloc_core/large_cache_config.rs`)
  is untouched. This is measurement only, feeding a FUTURE default-change
  decision that requires separate explicit user sign-off, per this task's
  own instruction.

---

## 3. Files changed

| file | change |
|---|---|
| `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs` | new — the crossing-regime hit-rate gate (subprocess-per-arm, registry-bypass, 3 burst sizes × 2 headroom arms × 3 thread counts × 3 reps) |
| `scripts/r31_1_derive_report_data.mjs` | new — derives this report's summary CSV + hard-asserts the headline arithmetic from the raw per-child CSV block (CLAUDE.md checked-script rule) |
| `Cargo.toml` | one new `[[example]]` entry (`required-features = ["alloc-global", "alloc-decommit", "bench-internals"]`, matching the r30_6 sibling) |
| `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` | this report (new) |
| `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE_summary.csv` | machine-readable summary, derived by the script above (new) |
| `docs/perf/_raw_r31_1_large_cache_headroom_crossing_regime_gate.log` | raw probe stdout, the canonical run cited above (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/OPEN_ITEMS.md` | item 27's "Current state" extended with this task's crossing-regime result (append-only) |
| `CHANGELOG.md` | Round 31 section extended with this task's entry (append-only) |

**No production source default changed.** `DEFAULT_HEADROOM_BYTES` (256
MiB, `src/alloc_core/large_cache_config.rs`) is untouched.

---

## 4. Reproduce

```text
cargo run --release --example r31_1_large_cache_headroom_crossing_regime_gate --features "production alloc-stats bench-internals"
node scripts/r31_1_derive_report_data.mjs
```

The orchestrator prints each child's `RESULT key=value` lines + `OK ...`
self-check/oracle summary, an aggregated (median) table, a between-arm
mechanism-delta table, and a CSV block (54 rows). The derivation script
re-parses that CSV block from the committed raw log, hard-asserts the
headline arithmetic, and writes the summary CSV. Measured full-matrix
wall-clock on this host: well under the default 2-minute Bash tool timeout.
