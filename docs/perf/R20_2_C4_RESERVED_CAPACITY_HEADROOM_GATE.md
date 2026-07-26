# R20-2 — C4: does reserved-capacity headroom reduce the medium→Large promotion memcpy?

**Task:** #347 (R20-2, P1). **MEASUREMENT ONLY, not a promotion decision.** This
document reports one new measurement — cell **C4** of the coordinated
Large-policy matrix that `docs/perf/R18_9_ADAPTIVE_LARGE_POLICY_DESIGN.md` §3
proposed and §9 named "the single highest-information missing measurement."
No `src/` file, no `Cargo.toml` feature list, and no existing script was
modified. `Cargo.toml`'s `production = [...]` list is untouched; this task
does **not** propose promoting anything into it.

**Date:** 2026-07-26. **Base revision:** `main` @ `6b5390d` (clean working
tree; R20-1, task #346, is the immediately preceding commit). **Platform
measured:** Windows 10 Pro x86-64, native — same single-host limitation as
every prior R13–R20 gate report (no Linux-native, macOS-native, or
multi-socket NUMA hardware was available to this session).

---

## 0. Headline summary

**VERDICT: NULL.** `large-reserved-capacity`'s geometric growth headroom
(enabled together with the `exact-span-large` it requires) does **not**
measurably reduce the medium→Large realloc-promotion cost that
`docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` (R18-2, task #331) found
and left RED. A **direct, load-matched, paired A/B/B/A comparison between C1
(`production,medium-classes`) and C4
(`production,medium-classes,exact-span-large,large-reserved-capacity`)** —
the decisive cell of this task — is **statistically indistinguishable from
noise**:

| Direct comparison | mean Δ (C1−C4) | SD | t | crit (p<0.05) | sign (C1/C4) | significant? |
|---|---:|---:|---:|---:|:---:|:---:|
| C1 vs C4, realloc phase, 20 pairs (80 launches) | +967 µs (C4 marginally faster) | 3.577 ms | 1.209 | 2.101 | 10/20 · 10/20 | **NO** |

C1's per-op realloc cost (this session, same host-load window): **49.6 µs**.
C4's: **48.6 µs**. A ~2% gap, with a sign test split dead-even (10/20) and
`t` far below `crit` — this is noise, not signal. **This confirms R18-2
§10.7's mechanism-level prediction**: the promotion `memcpy` (moving the
preserved prefix out of the medium segment into the fresh Large span) happens
*at promotion time*, before the fresh Large segment's `reserved_capacity`
headroom is established — so reserved capacity can only help a *subsequent*
grow past that point, never the copy that created the promoted block in the
first place.

**A genuine, orthogonal win was found and is worth recording separately**:
`exact-span-large`'s exact-rounding cuts resident commit roughly in half for
this workload (**50.5 MiB → 23.9 MiB**, same 172-segment/46% cache-hit-rate
signature in both arms) — a real memory-footprint improvement, but it is a
**memory** win, not a **realloc-speed** win, and does not move the R10-2 kill
gate at all.

**Methodological finding this task made along the way, stated up front
because it nearly produced a false "helps" conclusion:** a *naive*
cross-session comparison (this session's C4 number vs R18-2's previously
published C1 number, measured on a different day at a different host-load
level) looked like a ~24% realloc-time improvement — which would have read as
"headroom helps." Re-measuring C1 **fresh, in the same session, back-to-back
with C4** collapsed that apparent 24% gap to ~5% (53.9 µs vs 51.1 µs,
session-relative), and the **direct paired C1-vs-C4 test** (not each vs a
different-session baseline) showed even that residual gap is not
distinguishable from zero. The 24% figure was predominantly a host-load
artifact between sessions, not a feature effect — see §4 for the full trace.
This is exactly the class of confound `CLAUDE.md`'s environment-load
disclosure rule exists to catch, and is reported honestly here rather than
being allowed to stand as the naive number would have implied.

---

## 1. Why this measurement, and what it tests

`docs/perf/R18_9_ADAPTIVE_LARGE_POLICY_DESIGN.md` §3.1/§3.3/§9 named cell
**C4** — `production,medium-classes,exact-span-large,large-reserved-capacity`
measured against the R10-2 realloc-heavy harness (W1) — as the single
highest-information missing measurement in its proposed coordinated
Large-policy matrix, because:

- `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §2.1 argued that under
  plain `production`, `alloc_large` rounds every request up to a whole 4 MiB
  `SEGMENT`, so `large-reserved-capacity`'s extra geometric headroom is "moot"
  — the promoted block already gets a full 4 MiB span for free from rounding.
- `exact-span-large` removes that free headroom by shrinking the committed
  span to exactly the padded request. **Only** with `exact-span-large` ON does
  `large-reserved-capacity`'s headroom mechanism have anything to actually do
  for a promoted Large block.
- This exact combination — promotion (`medium-classes`) + shrunk span
  (`exact-span-large`) + geometric reserve (`large-reserved-capacity`) — had
  never been measured. The design doc framed the outcome as "binary-ish":
  either the realloc cost drops materially below C1's 67.6 µs/op baseline
  (headroom helps — worth a future coordinated-policy investment), or it does
  not (the residual memcpy is structural to the *first* promotion copy, and
  the only real lever is R10-2 §5's un-designed in-place-medium-grow
  mechanism).

This task executes exactly that single cell, on workload W1 only (the R10-2
realloc-heavy harness), per the task's explicit scope boundary — not the full
9-session §3 matrix.

---

## 2. Environment disclosure

- **Host / CPU:** 11th Gen Intel Core i7-11800H @ 2.30GHz, 8C/16T. Windows 10
  Pro x86-64, native. `rustc 1.97.0`, `cargo 1.97.0`.
- **Host CPU load, checked before each measurement window (shared dev-host —
  disclosed honestly, not hidden or downplayed, per this project's standing
  convention):**
  - Before the C4 gate run (§3): **52%** (a prior check at session start read
    69%, before any build started).
  - Before the fresh same-session C1 rebuild: **100%** momentarily (expected —
    the `cargo build` itself saturates CPU during compilation, not during
    measurement), settling to **65%** immediately before the measurement run.
  - Before the direct C1-vs-C4 comparison (§4, the decisive cell): **33%**
    (lightest-load window of the session).
  - Before the same-vs-same control (§5): **48%**.
  - **Range across all measurement windows this session: 33–69%** — lighter
    and less variable than R18-2's disclosed 66–94%, itself the reason a naive
    cross-session comparison against R18-2's published numbers is unsafe (§4).
- **Commit measured:** `main` @ `6b5390d` (clean working tree for all
  measurement runs; only later additions are this doc, its summary CSV, and
  the force-added raw logs).

---

## 3. C4 gate — production vs. C4, three phases (the primary session)

Built fresh this session:
- Arm A: `cargo build --release --example paired_ab_medium_off --features production`
- Arm B (C4): `cargo build --release --example paired_ab_medium_on --features production,medium-classes,exact-span-large,large-reserved-capacity`

Then `node scripts/r10_2_medium_gate.mjs --skip-build --pairs 20` (the exact
harness R10-2/R18-2 used, zero source/script changes — `--skip-build` reuses
the two just-built exes).

| phase | arm A (`production`) launch-mean | arm B (C4) launch-mean | paired Δ (A−B) | SD | t | crit | sign (A/B) | SD/Δ | resolvable? |
|---|---:|---:|---:|---:|---:|---:|:---:|---:|:---:|
| alloc | 3.790 ms | 0.146 ms | +3.644 ms | 308.5 µs | 52.821 | 2.101 | 0/20 · 20/20 | 8.5% | YES |
| free | 14.414 ms | 0.088 ms | +14.327 ms | 452.1 µs | 141.714 | 2.101 | 0/20 · 20/20 | 3.2% | YES |
| realloc | 0.042 ms | 49.045 ms | **−49.003 ms** | 3.094 ms | −70.831 | 2.101 | 20/0 (A-faster) | 6.3% | YES |

Every phase is a REAL, resolvable effect (SD is 3–9% of its own delta — the
R17-7 methodological check the project adopted after finding an
unresolvable effect in that round; here every effect is 11–31× its own SD,
comfortably resolvable).

**segments/commit/rss** (deterministic per arm, min==max across all 40
launches per arm per phase, confirming reservation-pattern stability):

| arm | segments | commit | rss |
|---|---:|---:|---:|
| A (`production`) | 329 | ≈34.5 MiB | ≈3.17 MiB |
| B (C4) | 172 | ≈23.9 MiB | ≈9.58 MiB |

**Cache-hit-rate proxy (per R14-4 §5 / R18-2 §10.6's method):** 172 fresh
segments out of the realloc phase's 320 `alloc_large` calls (16 objects × 20
rounds) ⇒ **~46% hit rate** — byte-for-byte identical to R18-2's C1
(`production,medium-classes`) proxy. This is the first confirmation that C4
does not change the allocation-pattern side of the mechanism at all: the same
fraction of promotions miss the 8-slot large cache, exactly as before.

**Realloc per-op** (960 realloc-grows per launch: 16 objects × 3 grow-steps ×
20 rounds): arm A ≈ 44 ns/op (near-zero in-place Large header update), arm B
(C4) ≈ **51.1 µs/op**.

**Full-round (alloc+free+realloc) vs. sub-window, per CLAUDE.md's dual-axis
rule:** full round A = 18.246 ms, full round B (C4) = 49.279 ms ⇒ **B ~2.70×
slower overall**. The realloc sub-window remains the number that decides the
kill-gate (still ~1,161× slower — see §3's realloc row: 49.045 ms / 0.042 ms).
As with every prior round in this series, the full-round net figure is
context, not the gate criterion itself (the gate is specifically on the
realloc sub-window).

Raw log (truncated per the R14-10 precedent — full stdout reproducible via
the command line above): `docs/perf/_raw_r20_2_c4_gate.log`.

---

## 4. Why a direct, load-matched C1-vs-C4 comparison was necessary

At this point the naive comparison against R18-2's published C1 number would
read: C4's realloc per-op here is 51.1 µs vs. R18-2's published C1 figure of
**67.6 µs/op** — a ~24% reduction, which on its face looks like "headroom
helps." **That comparison is unsafe and this section shows why, rather than
reporting the 24% figure as the verdict.**

R18-2 (`docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §10.1) explicitly
measured at 66–94% host CPU load (high, shared dev-host). This session's C4
run measured at 52–69% — a materially lighter and less variable load window.
Absolute wall-clock numbers on a shared dev host are sensitive to load, and a
same-workload comparison across two *different* sessions on two *different*
days cannot safely isolate "the feature changed the number" from "the host
was quieter this time."

**To control for this, C1 (`production,medium-classes`, no
`exact-span-large`/`large-reserved-capacity`) was rebuilt from scratch and
re-measured in THIS SAME session**, immediately before/after the C4 run
(same host-load era, ~52–69% throughout):

| phase | arm A launch-mean | arm B (C1, same-session) launch-mean | paired Δ (A−B) | SD | t | sign (A/B) | SD/Δ |
|---|---:|---:|---:|---:|---:|:---:|---:|
| alloc | 3.522 ms | 0.134 ms | +3.388 ms | 246.4 µs | 61.497 | 0/20 · 20/20 | 7.3% |
| free | 15.444 ms | 0.089 ms | +15.355 ms | 829.8 µs | 82.751 | 0/20 · 20/20 | 5.4% |
| realloc | 0.043 ms | 51.730 ms | −51.687 ms | 2.785 ms | −82.989 | 20/0 | 5.4% |

C1 same-session realloc per-op: 51,730,048 / 960 ≈ **53.9 µs/op**. This is
**already ~20% below R18-2's originally-published 67.6 µs/op** for the exact
same feature set (`production,medium-classes`) — confirming the gap is
predominantly a **session/host-load artifact**, not a change in the
mechanism. `exact-span-large`'s RSS effect is visible immediately in this
same-session C1 run too: commit here is **≈50.5 MiB** (matching R18-2's
~49.3 MiB closely, as expected — C1 has no `exact-span-large`), vs. C4's
~23.9 MiB in §3 — confirming the commit reduction really is attributable to
`exact-span-large`'s exact rounding, not a session artifact (this axis is
NOT load-sensitive the way wall-clock is).

Raw log: `docs/perf/_raw_r20_2_c1_samesession_control.log`.

### 4.1 The decisive test: C1 vs C4, directly paired (not each vs. a separate baseline)

The load-matched session-relative numbers (53.9 µs vs. 51.1 µs, a ~5% gap)
are still two independently-measured numbers, each against its own arm-A
baseline in its own A/B/B/A run — not a *direct* paired comparison of C1
against C4. To settle the question properly, the two treatment binaries were
paired against **each other** directly, using the existing
`scripts/paired-ab-runner.mjs` general-purpose config path (no script
modification — a JSON config naming two arbitrary commands as arms, exactly
the tool's documented `--config` usage):

```
node scripts/paired-ab-runner.mjs \
  --config docs/perf/paired_ab_runs/_r20_2_c1_vs_c4_realloc.json \
  --arms C1,C4 --pairs 20
```

(config: `{"metric":"realloc_ns","arms":{"C1":{...medium_on.exe built with
production,medium-classes...},"C4":{...medium_on_c4.exe preserved from §3's
build, production,medium-classes,exact-span-large,large-reserved-capacity...}}}`)

**Result — 20 pairs, 80 launches, realloc phase:**

| n | mean Δ (C1−C4) | SD | SE | t | df | crit (p<0.05) | significant? | sign (C1-faster / C4-faster) |
|---:|---:|---:|---:|---:|---:|---:|:---:|:---:|
| 20 | +967.1 µs | 3.577 ms | 799.8 µs | 1.209 | 19 | 2.101 | **NO** | 10/20 · 10/20 |

Per-op: C1 mean = 47,599,453 / 960 ≈ **49.58 µs/op**; C4 mean = 46,632,350 /
960 ≈ **48.58 µs/op**. `segments_reserved_total` = 172 in **both** arms across
all 80 launches (identical cache-hit-rate proxy — confirms, again, that C4
changes nothing about which promotions hit/miss the cache). Commit: C1 ≈
50.5 MiB, C4 ≈ 23.9 MiB (the `exact-span-large` RSS effect, reproduced a
third time, now inside the very comparison that shows no time effect).

**SD/Δ resolvability check, applied honestly to this cell too:** SD (3.577 ms)
is **370% of** the mean delta (967 µs) — this effect, whatever its true sign,
is far too small relative to this host's per-process jitter at this
measurement's timescale to resolve with n=20 pairs. This is reported as
exactly what it is: **not** "we proved the true effect is zero," but "this
host cannot distinguish a ~2% difference from noise at this sample size" —
paired with the sign test's dead-even 10/20 split, which is the signature of
a genuinely unresolvable-or-absent effect, not a real effect this host simply
can't quantify precisely (contrast with §3's realloc row, where every
resolvable real effect showed SD at 3–9% of its delta, not 370%).

**Why this doesn't change the verdict.** Even if the ~2% gap were a real,
resolvable effect (which it is not, per the above), it would still fall
enormously short of "material" by any reading of the design doc's own framing
(§9: "the realloc cost should drop materially below C1's 67.6 µs/op"). A 2%
change cannot plausibly be the thing that moves a ~1,180×/~380× regression.
Whether the true C1-vs-C4 gap is exactly 0% or some noise-floor-sized 1–2%,
the practical answer to "does reserved-capacity headroom reduce the
structural promotion memcpy?" is **no** either way.

Raw log (kept in full — short enough, and this is the decisive evidence, same
precedent as R18-2's control log): `docs/perf/_raw_r20_2_c1_vs_c4_direct.log`.

---

## 5. Same-vs-same control (harness honesty, per the established protocol)

```
node scripts/paired-ab-runner.mjs --config docs/perf/paired_ab_runs/_r10_2_realloc.json --arms A,A --pairs 20
```

| n | mean Δ | SD | t | crit | significant? | sign |
|---:|---:|---:|---:|---:|:---:|:---:|
| 20 | 880 ns | 11.200 µs | 0.351 | 2.101 | NO (expected) | 12/20 · 8/20 |

The harness shows no spurious self-difference on the realloc phase (the same
check R14-3/R17-7/R18-2 all required and passed). Load at time of this run:
48%.

Raw log (kept in full, matching the R18-2 precedent for the control run):
`docs/perf/_raw_r20_2_control_off_vs_off.log`.

---

## 6. Verdict and discussion

### 6.1 The verdict

**NULL.** `large-reserved-capacity`'s geometric growth headroom (with its
required `exact-span-large`) does **not** measurably reduce the
medium-to-Large realloc-promotion cost, on top of `medium-classes` alone. The
decisive, direct, load-matched C1-vs-C4 paired comparison (§4.1) shows no
statistically resolvable difference (t=1.209 ≪ crit 2.101, sign test dead
even 10/20), and even the small nominal gap observed (~2%, unresolved) is
far too small to plausibly account for closing R10-2's ~1,180×/~380×
kill-gate regression.

### 6.2 Why (mechanism confirmation)

This confirms, rather than merely repeats, R18-2 §10.7's prediction:
`large-reserved-capacity`'s `reserved_capacity` field is set on the **fresh**
Large segment *after* `alloc_large` returns it to the promotion call site
(`src/registry/heap_core_free.rs`'s `try_promote_to_large`). The promotion
`memcpy` — the 256 KiB `copy_nonoverlapping` moving the medium block's prefix
into that fresh span — happens **at that same moment**, before any
subsequent grow could ever consult the reserved headroom. Reserved capacity
can only pay off on a **later** in-place OPT-G grow past the block's
initially-committed span; it structurally cannot retroactively cheapen the
copy that created the promoted block. C4's measurement is the first direct
empirical confirmation of this — R18-2 inferred it from the mechanism; this
task measured it.

### 6.3 What C4 DID confirm as a genuine, separate finding

`exact-span-large`'s exact-rounding cuts steady-state commit for this
workload from ~50.5 MiB (C1) to ~23.9 MiB (C4) — roughly halved — while the
cache-hit-rate proxy (172/320, ~46%) is **identical** in both arms. This is a
real, reproducible memory-footprint benefit of `exact-span-large` (reproduced
three times across §3, §4, and §4.1's three independent runs), entirely
orthogonal to the realloc-time question this task's primary axis was built to
answer. It does not change the R10-2 kill-gate verdict — the gate is on
wall-clock, not RSS — but it is worth recording as a distinct, positive
signal for any future memory-footprint-focused evaluation of
`exact-span-large`.

### 6.4 What this means for R10-2's kill-gate and the R18-9 unified-policy question

- **R10-2's realloc kill-gate remains RED**, and this task adds C4 to the set
  of feature combinations that do not clear it (alongside R18-2's C1 and C3).
  The residual is confirmed, again, as **structural promotion-copy cost** —
  not cache-slot pressure (R14-4's original hypothesis, since revised), not
  the R17-4 leak (fixed, orthogonal), and now confirmed **not** reducible by
  giving the promoted segment growth headroom either.
- **The only remaining lever is R10-2 §5's un-designed
  in-place-medium-class-grow mechanism** (`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`
  §5 item 1) — carving the new slot within the same segment the medium block
  already lives in, avoiding the Large-segment round-trip and its copy
  entirely. This is `docs/perf/OPEN_ITEMS.md`'s Active item 1 (unchanged by
  this task — still NOT designed, NOT implemented).
- **For `R18_9_ADAPTIVE_LARGE_POLICY_DESIGN.md`'s broader question** ("could a
  unified `LargePolicy` coordinate the three opt-in Large features?"): this
  result is exactly the outcome that design doc's §8 risk-1 anticipated as
  the "predicted" branch — C4 is a null, so (per that document's own framing)
  "the coordinated matrix's most-likely-to-flip-the-gate cell is a null, and
  the honest conclusion is that the existing three features are already
  well-coordinated... and the ONLY remaining lever is R10-2 §5's not-yet-
  designed mechanism." This task's measurement makes that conclusion
  evidence-based rather than a prediction. A unified `LargePolicy` (§5 of the
  design doc) remains, per that document's own §5.3 realism verdict, a
  worthwhile **coordination** convenience (getting cache-budget × growth-
  factor trade-offs into one profile) — but it is not, and per this
  measurement cannot become, a fix for the R10-2 regression itself.

### 6.5 Non-goal statement (per this task's own scope)

This is a measurement, not a decision. It does **not** recommend promoting
`medium-classes`, `exact-span-large`, or `large-reserved-capacity` into
`production`; it does not modify `Cargo.toml`; and per the "null result is a
genuine, valuable finding" framing this task was given, no further action is
proposed here beyond updating `docs/perf/OPEN_ITEMS.md` (§7) and leaving the
in-place-medium-grow mechanism (item 1 there) as the one real remaining
lever.

---

## 7. `docs/perf/OPEN_ITEMS.md` update

Active item 2 ("R18-9 §9 — execute the §3 coordinated Large-policy matrix
(esp. cell C4)") is moved to "Recently resolved," citing this report and the
NULL verdict. See the index file's own diff for the exact wording; the
remaining Active/Deferred/Low-priority items are renumbered to stay
gap-free, with a cross-reference check for any place that names an item by
number.

---

## 8. Artifacts this task adds

- `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` — this document.
- `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE_summary.csv` —
  machine-readable companion (commit, features, per-phase means, paired
  Δ/SD/t/sign, SD/Δ ratio, resolvable flag, segments/commit/rss, cache-hit
  proxy, host load) covering all four measurement cells in this report (C4
  gate, C1 same-session control, the direct C1-vs-C4 comparison, and the
  same-vs-same harness-honesty control).
- `docs/perf/_raw_r20_2_c4_gate.log` — truncated per the R14-10 precedent
  (header + one sample RESULT block per phase + the three `=== A vs B ===`
  summary blocks; full stdout reproducible via
  `node scripts/r10_2_medium_gate.mjs --skip-build --pairs 20` after building
  arm A with `--features production` and arm B with
  `--features production,medium-classes,exact-span-large,large-reserved-capacity`).
- `docs/perf/_raw_r20_2_c1_samesession_control.log` — same truncation, for the
  fresh same-session C1 (`production,medium-classes`) rebuild used as the
  load-matched reference point (§4).
- `docs/perf/_raw_r20_2_c1_vs_c4_direct.log` — kept in full (657 lines,
  small); the decisive direct C1-vs-C4 paired comparison (§4.1).
- `docs/perf/_raw_r20_2_control_off_vs_off.log` — kept in full; the
  same-vs-same harness-honesty control (§5).
- `docs/perf/OPEN_ITEMS.md` — Active item 2 moved to "Recently resolved";
  remaining items renumbered (§7).
- No `Cargo.toml` edit. No `src/` file touched. No existing script modified —
  `scripts/r10_2_medium_gate.mjs` was invoked exactly as R18-2 invoked it
  (with arm B built with a different `--features` string, which the script
  already supports via `--skip-build` reusing pre-built exes); the one new
  config JSON (`docs/perf/paired_ab_runs/_r20_2_c1_vs_c4_realloc.json`, data
  only, gitignored like every other generated `paired_ab_runs/*.json`) uses
  `scripts/paired-ab-runner.mjs`'s existing, documented, general-purpose
  `--config`/`--arms` interface exactly as designed.
