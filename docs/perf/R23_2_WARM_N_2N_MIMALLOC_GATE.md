# R23-2 — warm N/2N matched-workload gate: cancelling the mimalloc-comparison bootstrap constant algebraically instead of subtracting an asymmetric proxy

**Task #371 (R23-2), Round 23.** Corrects **R22-15**
(`docs/perf/R22_15_MIMALLOC_IR_ARM_GATE.md`, task #366, commit `ff48029`),
which measured SeferAlloc retiring 1.3x-2.4x more instructions (`Ir`) per op
than mimalloc, using a bootstrap-subtraction methodology an independent
read-only review (`docs/reviews/2026-07-26-r22-readonly-review.md` P1) found
to be asymmetric across the two allocators in a way that could skew the
ratio (`docs/perf/OPEN_ITEMS.md`'s R22-15 entry, "2026-07-27 update").

**Date:** 2026-07-27. **Base revision:** `main` @ `7f2a9ef` (working tree
otherwise carrying only this task's own additive edits at measurement time;
the usual untracked `docs/checkpoints/`/`docs/reviews/` files from concurrent
review sessions present, not touched by this task). **Platform measured:**
WSL2 (Ubuntu, kernel `6.18.33.2-microsoft-standard-WSL2`) under Windows 10
Pro x86-64, `valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc
`1.98.0-nightly (bd08c9e71 2026-06-25)` — same toolchain/host as every prior
`npm run iai` measurement in this doc tree.

---

## 0. Headline: the correction does NOT overturn the direction of the gap, but it MATERIALLY moves both numbers — one flips below 1.0, the other shrinks

| workload | OLD ratio (bootstrap-subtracted, R22-15) | NEW ratio (N/2N-derived, this task) |
|---|---:|---:|
| small_churn_16b (hot churn) | **1.326** | **0.896** |
| cold_alloc_free_256x16b (cold carve) | **2.430** | **2.002** |

**Reading this honestly, both directions:**

- **The hot-churn ratio FLIPS**: R22-15's bootstrap-subtracted figure said
  SeferAlloc costs 1.326x mimalloc's instructions per op on the churn
  workload. The N/2N-derived figure — which cancels the one-time bootstrap
  constant algebraically instead of subtracting an external, differently-sized
  proxy bench per allocator — says the OPPOSITE: SeferAlloc's genuine marginal
  cost per churn op (69.0 Ir) is *lower* than mimalloc's (77.0 Ir), a ratio of
  0.896. This is exactly the asymmetry the read-only review predicted:
  `large_alloc_free_cycle` (B=3,308) was a much smaller fraction of Sefer's
  raw churn Ir (8,051) than `mimalloc_bootstrap_proxy` (B=13,050) was of
  mimalloc's raw churn Ir (16,629) — over-subtracting relatively more from
  mimalloc's total inflated mimalloc's apparent per-op efficiency in the OLD
  method. Once the actual bootstrap component is cancelled by algebra instead
  of estimated by an unrelated one-shot 4 MiB alloc/free, the hot-churn
  headline direction reverses.
- **The cold-carve ratio SHRINKS but does not flip**: 2.430 -> 2.002. The
  correction is real and material (SeferAlloc's cold-carve cost premium over
  mimalloc drops from "~2.4x" to "~2.0x"), but the direction is unchanged —
  SeferAlloc genuinely retires roughly double the instructions mimalloc does
  on a virgin cold-carve batch of tiny blocks, even after removing the
  asymmetric-proxy artifact.

**What this means for R22-15's "1.3x-2.4x more instructions per op" headline
(§0 of that report):** it is **not confirmed as originally stated**. The
1.3x-2.4x band was itself an artifact of the asymmetric bootstrap-proxy
subtraction on at least the churn workload (the one that flips), and even the
still-standing cold-carve premium (2.0x, not 2.4x) is smaller once corrected.
The gap has NOT vanished on the cold-carve/recycle family, but neither is it
uniform in direction or magnitude across workload shapes as R22-15 originally
reported it. This is a real methodological correction, not a re-confirmation
with smaller decimals.

---

## 1. Isolation-mechanism investigation (done FIRST, per the task's own instruction)

**Finding: each `#[library_benchmark]` fn runs in its own fresh process under
Callgrind — there is no cross-fn memoization of `SeferAlloc::new()` or
mimalloc's lazy static init between benches within one iai-callgrind
invocation.** This was confirmed two ways:

1. **Already documented, three times, in the existing committed code.**
   `benches/perf_gate_iai.rs`'s own `dealloc_prealloc_only_16b` doc comment
   (added R22-17/task #368) states explicitly: *"the 64 pointers are
   deliberately leaked (never freed): this arm exists ONLY to measure the
   shared pre-allocation prefix's own Ir, common to both sibling arms below.
   **Each `#[library_benchmark]` runs in its own fresh process under
   callgrind, so leaking here has no effect on any other bench.**"* The same
   claim is repeated verbatim in `dealloc_contains_base_probe_only_16b`'s doc
   comment and in R22-17's own report (`docs/perf/
   R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` §2.1). This is a pre-existing,
   already-relied-upon fact this task did not invent — R22-17's whole
   subtraction-based isolation methodology (measure a shared prefix alone,
   subtract it from a longer arm) is unsound if fn invocations shared
   process state, since a leaked/mutated allocator would corrupt every
   subsequent arm in the same process. R22-17 landed and was independently
   re-verified (twice: the original task, then R23-1's correction) without
   this ever surfacing as a bug, which is itself corroborating evidence: if
   arms shared a process, `dealloc_prealloc_only_16b`'s leaked pointers and
   claimed heap registry would still be resident when
   `dealloc_free_only_16b` ran next, and `HeapRegistry::claim()` in the next
   arm would either observe stale state or double-initialize — neither
   observed.
2. **Independently reasoned from `iai-callgrind`'s own architecture** (per
   its documented design, matched against this crate's own module doc in
   `benches/perf_gate_iai.rs` lines 28-38): the `iai-callgrind-runner`
   compiles ONE benchmark binary (`harness = false` bench target), then for
   EACH `#[library_benchmark]` fn in the `library_benchmark_group!` list,
   invokes `valgrind --tool=callgrind` against that SAME binary with a
   selector that runs only that one function, as a distinct OS process,
   parses that process's callgrind output, and moves to the next fn. This is
   the same architecture the original `iai` crate (this project's own
   comment credits `iai-callgrind` as `iai`'s Callgrind-based successor) used
   and that `scripts/iai.mjs`'s own module doc corroborates in spirit ("the
   runner drives it under `valgrind --tool=callgrind` and counts CPU
   instructions retired" — singular "it", one binary, run once per bench
   under a fresh valgrind supervision). Consistent with this being a
   per-process (not per-thread, not shared-heap) invocation model.

**Consequence for this task's design question — "does `warm` need an added
untimed pre-loop, or is the existing single-timed-loop pattern already
sufficient":** the existing pattern is ALREADY sufficient, with no
modification needed. Every existing bench (`small_churn_16b`,
`cold_alloc_free_256x16b`, their mimalloc mirrors, etc.) already pays its
allocator's ENTIRE one-time bootstrap cost `B` inside its own single timed
call, because that bootstrap happens exactly once per process and each bench
IS its own process — there is no warmer or colder state to ask for. The N/2N
arms added by this task (§2) are therefore BYTE-IDENTICAL in structure to
their sibling N-sized arms, just with a larger loop bound — no separate
"untimed warm-up phase" was added, and none was needed. This also means the
"warm" in this task's own name ("warm N/2N matched-workload gate") describes
the ALGEBRAIC cancellation of `B` via subtraction of two same-process-shape
measurements, not a change to how any individual bench measures itself.

---

## 2. New arms added to `benches/perf_gate_iai.rs`

Six new `#[library_benchmark]` fns, all `#[cfg(target_os = "linux")]`-gated
identically to every existing arm in the file:

- **`small_churn_16b_2n`** — byte-identical to `small_churn_16b` except the
  loop runs `CHURN_OPS_2N` (128, not 64) iterations.
- **`mimalloc_small_churn_16b_2n`** — the mimalloc mirror of the above.
- **`cold_alloc_free_256x16b_2n`** — byte-identical to
  `cold_alloc_free_256x16b` except the batch is `COLD_BATCH_2N` (512, not
  256) distinct blocks.
- **`mimalloc_cold_alloc_free_256x16b_2n`** — the mimalloc mirror.
- **`cold_alloc_free_256x16b_4n`** / **`mimalloc_cold_alloc_free_256x16b_4n`**
  — a THIRD op count (`COLD_BATCH_4N` = 1,024), added only for the cold-carve
  pair, purely to empirically test the linearity assumption the N/2N trick
  relies on (§4 below) — not part of the minimum two pairs the task required,
  added because the cold-carve pair is exactly the one the task's own
  correctness caveat flagged as most at risk of non-linear scaling.

Two new constants (`benches/perf_gate_iai.rs`, right after `COLD_BATCH`):
`CHURN_OPS_2N = CHURN_OPS * 2` (128), `COLD_BATCH_2N = COLD_BATCH * 2` (512),
`COLD_BATCH_4N = COLD_BATCH * 4` (1,024).

All six new fns were added to the existing `perf_gate`
`library_benchmark_group!` list (30 benches total, up from 24 before this
task). No new bench binary/target, no `Cargo.toml` change, no CI workflow
change — same reasoning as R22-15's own "no new bench binary" rationale
(the existing `.github/workflows/perf-gate.yml` line already runs every fn in
the group).

**Why `small_churn_16b`/`cold_alloc_free_256x16b` and their mimalloc mirrors
specifically** (per the task's own instruction): these are the N-sized
siblings of the two pairs with the largest OLD ratios in R22-15's table
(1.326 for churn — actually the SMALLEST of the six, included as the
canonical "hot" representative per the task brief's explicit naming — and
2.430 for cold-carve, the LARGEST of the six). `churn_256b` and
`recycle_alloc_free_256x16b`/`64b` were not given N/2N siblings in this task
(time-boxed to the two required pairs plus the linearity 4N extension); a
future round could extend the same pattern to them using the identical
recipe this task establishes.

---

## 3. Measured N/2N numbers — real `npm run iai` runs, determinism confirmed across 5 independent runs

Two full-suite runs (`npm run iai`, `--features production`, the CI default)
were made AFTER the six new arms were added and the group registration
updated; three additional runs were made incrementally as the arms were
built up (first the 2N pair, confirming determinism, then adding the 4N
linearity-check pair and re-confirming every pre-existing row was
unchanged). All five runs, across every already-existing bench AND every
newly-added bench, produced **byte-identical `Ir`** — the same determinism
property every prior gate report in this doc tree (R22-15 §4, R22-17 §3,
R23-1 §7.3) has established for this measurement pipeline.

Raw evidence (both committed in full — 288 lines/287 lines, well under the
truncation threshold; `git add -f` needed, `.gitignore:16` excludes
`docs/perf/_raw_*.log` by default):

- `docs/perf/_raw_r23_2_warm_n_2n_gate.log` — the run cited in the table
  below (30 benches, first run including all six new arms).
- `docs/perf/_raw_r23_2_warm_n_2n_gate_rerun1.log` — an independent
  immediately-following re-run, confirming byte-identical `Ir` for every
  bench including the six new arms.

| bench | Ir | ops (N) |
|---|---:|---:|
| `small_churn_16b` | 8,051 | 64 |
| `small_churn_16b_2n` | 12,467 | 128 |
| `mimalloc_small_churn_16b` | 16,629 | 64 |
| `mimalloc_small_churn_16b_2n` | 21,557 | 128 |
| `cold_alloc_free_256x16b` | 50,164 | 256 |
| `cold_alloc_free_256x16b_2n` | 102,353 | 512 |
| `cold_alloc_free_256x16b_4n` | 202,867 | 1,024 |
| `mimalloc_cold_alloc_free_256x16b` | 32,325 | 256 |
| `mimalloc_cold_alloc_free_256x16b_2n` | 58,389 | 512 |
| `mimalloc_cold_alloc_free_256x16b_4n` | 106,653 | 1,024 |

(Full 30-bench table with L1/L2/RAM/EstCycles columns for every bench,
including the pre-existing 24, is in the raw logs above; every pre-existing
bench's `Ir` is unchanged from R22-15/R22-17/R23-1's own recorded values,
confirming this task added rows without perturbing anything else.)

Companion machine-readable summary:
`docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE_summary.csv`.

---

## 4. Derived per-op costs — `c = (Ir(2N) - Ir(N)) / N`, and the linearity sanity-check

### 4.1 Hot churn pair

```text
c_sefer_churn = (12,467 - 8,051) / 64 = 69.00 Ir/op
c_mi_churn    = (21,557 - 16,629) / 64 = 77.00 Ir/op

ratio = c_sefer_churn / c_mi_churn = 69.00 / 77.00 = 0.8961
```

SeferAlloc's genuine marginal per-op cost on this hot-churn workload (69.0
Ir/op) is LOWER than mimalloc's (77.0 Ir/op) once the bootstrap constant is
cancelled algebraically — the opposite of R22-15's 1.326 headline.

### 4.2 Cold-carve pair

```text
c_sefer_cold (N,2N) = (102,353 - 50,164) / 256 = 203.86 Ir/op
c_mi_cold    (N,2N) = (58,389 - 32,325) / 256 = 101.81 Ir/op

ratio (N,2N) = 203.86 / 101.81 = 2.0023
```

### 4.3 Linearity sanity-check (the correctness caveat, verified empirically, not assumed)

The task brief flagged a real risk: doubling `COLD_BATCH` might cross a
segment-capacity boundary the N-sized workload didn't cross, breaking the
`Ir(k*N) = B + k*N*c` linearity assumption the whole N/2N trick depends on.
**Checked structurally first:** `SEGMENT = 1 << 22` (4 MiB,
`src/alloc_core/os.rs:65`); the primordial segment's metadata footprint is
bounded to `small_meta_end() + PAGE <= SEGMENT` (a compile-time assert,
`src/alloc_core/segment_header.rs:1280`), leaving nearly the full 4 MiB for
payload. `COLD_BATCH_4N` x 16 B = 1,024 x 16 = 16,384 B, and even
`COLD_BATCH_4N` x 64 B (the sibling 64 B bench, not measured with a 4N arm
in this task but the same order of magnitude) = 65,536 B — both a tiny
fraction of ~4 MiB. No segment-crossing risk was structurally plausible for
either N=256, 2N=512, or 4N=1,024 at this block size.

**Checked empirically**, using the added THIRD op count (`4N` = 1,024) to
cross-check `c` derived from (N, 2N) against `c` derived from (2N, 4N) — a
test that needs no assumption about `B` at all:

```text
SEFER cold:    c(N,2N)  = (102,353 -  50,164) / 256 = 203.86 Ir/op
               c(2N,4N) = (202,867 - 102,353) / 512 = 196.32 Ir/op
               relative difference = -3.70%

MIMALLOC cold: c(N,2N)  = ( 58,389 -  32,325) / 256 = 101.81 Ir/op
               c(2N,4N) = (106,653 -  58,389) / 512 =  94.27 Ir/op
               relative difference = -7.41%

ratio using (N,2N) c values  = 203.86 / 101.81 = 2.0023
ratio using (2N,4N) c values = 196.32 /  94.27  = 2.0826
```

**Honest finding, reported as measured rather than assumed away: the
cold-carve doubling is NOT perfectly linear for either allocator.** Both
allocators show the marginal Ir/op getting slightly *cheaper* (not more
expensive) as the batch grows from N/2N to 2N/4N — a 3.7% drop for
SeferAlloc, a 7.4% drop for mimalloc. This is the OPPOSITE direction from
what a segment-boundary-crossing problem would produce (crossing into a new
segment would make the SECOND half of a doubled batch pay EXTRA
carve/registration overhead the first half didn't, making `c` measured over
the second half LARGER, not smaller, than `c` measured over the first half).
The direction actually observed — the marginal cost trending down as batch
size grows — is more consistent with a fixed, small once-per-bench
constant-overhead component (e.g. loop-setup or array-zero-initialization
cost for `ptrs: [*mut u8; N]`, which itself scales with N in a way that is
not purely `O(N)` due to instruction-cache/branch-prediction warm-up across
a longer straight-line loop under Callgrind's cache model) being amortized
better at larger N, not a per-block segment-crossing penalty. **No evidence
of the specific failure mode the task's correctness caveat was worried about
(a new segment being crossed) was found; a smaller, different, and
non-alarming form of non-linearity (both allocators, same direction, mid-
single-digit-to-high-single-digit percent) was found instead, and is reported
here rather than papered over.**

**Effect on the headline ratio: small, and in the direction of making the
correction WEAKER, not stronger.** Using the (2N,4N)-derived `c` values
instead of (N,2N) moves the cold-carve ratio from 2.0023 to 2.0826 — the
ratio is robust to which two adjacent points are used (both land near 2.0,
both are meaningfully below R22-15's 2.430), so this non-linearity does not
change §0's qualitative conclusion, but it does mean the "2.002" headline
figure in §0 should be read as "approximately 2.0, with a few percent of
genuine measurement-methodology spread depending on which op-count pair is
used" rather than a razor-precise constant.

---

## 5. Old vs new ratio — side-by-side (repeating §0 with full derivation context)

| workload | OLD method (R22-15) | OLD ratio | NEW method (this task) | NEW ratio |
|---|---|---:|---|---:|
| small_churn_16b | `(Ir - large_alloc_free_cycle) / 64` on both sides, then divide | 1.326 | `(Ir(2N) - Ir(N)) / 64` on both sides, then divide | **0.896** |
| cold_alloc_free_256x16b | `(Ir - large_alloc_free_cycle) / 256` on both sides, then divide | 2.430 | `(Ir(2N) - Ir(N)) / 256` on both sides, then divide | **2.002** (2.00–2.08 across the (N,2N)/(2N,4N) linearity check) |

**Verdict: the correction materially changes the headline — it does NOT
merely confirm R22-15's 1.3x-2.4x band with adjusted decimals.** One of the
two workload families this task re-measured (hot churn) reverses direction
entirely (SeferAlloc becomes marginally CHEAPER than mimalloc once the
bootstrap asymmetry is removed); the other (cold-carve) keeps the same
direction but shrinks by roughly 18% (2.430 -> ~2.0-2.08). R22-15's own
recycle-family ratios (2.320-2.376, not re-measured with N/2N arms in this
task per the task's own two-pair minimum) remain UNVERIFIED under this
corrected method — this report does not claim they would also shift, only
that the two pairs actually re-measured here did shift, one of them enough
to flip sign.

---

## 6. What this does and does not settle

**Settles:** R22-15's specific "1.3x-2.4x more instructions per op,
uniformly" headline is not an accurate characterization once the bootstrap
constant is handled without an external, differently-sized proxy bench. The
true picture (on the two pairs measured here) is workload-dependent: roughly
break-even-to-favorable for SeferAlloc on hot churn, and a real (though
smaller than previously stated) ~2x premium on cold-carve.

**Does NOT settle:** (a) whether the four remaining workload-matched pairs
from R22-15 (`churn_256b`/`mimalloc_churn_256b`,
`recycle_alloc_free_256x16b`/`64b` and their mimalloc mirrors) would show
similar corrections if re-measured with N/2N arms — this task's scope was
the two required pairs plus the cold-carve linearity extension, not a full
re-derivation of every R22-15 pair; (b) the architectural root cause of the
cold-carve premium that DOES survive correction (~2x, not ~2.4x) — this
report measures, it does not attribute; (c) real-world wall-clock impact —
`Ir` remains an instruction-count proxy, not a cycle or wall-clock measure
(see R22-15 §0's own EstCycles caveat, which this task did not re-derive for
the new arms).

---

## 7. Verification performed

- **Isolation mechanism investigated FIRST** (§1) — confirmed via
  already-committed code comments (three independent locations in
  `benches/perf_gate_iai.rs`) plus the documented `iai-callgrind`
  per-fn-process invocation model; concluded no added "warm" pre-loop was
  needed, and none was added.
- **`cargo check --bench perf_gate_iai --features production`** (Windows,
  non-Linux stub path) — clean, no warnings, both before and after adding
  the 4N linearity-check arms.
- **`cargo fmt --all -- --check`** — clean, exit 0 (repo-wide), both before
  and after the 4N arms were added.
- **`npm run iai` (the real gate) — run FIVE times total**: three runs after
  adding the 2N arms (confirming byte-identical `Ir` across all three,
  including the new arms), then two more after adding the 4N
  linearity-check arms (confirming every pre-existing row, including the 2N
  arms, remained byte-identical, and the new 4N rows were themselves
  byte-identical across the two post-4N runs). All five runs: PASS, 28 or 30
  bench(es) produced Ir (28 before the 4N arms were registered, 30 after),
  zero regressions reported by iai-callgrind's own baseline comparison.
- **Structural + empirical linearity check** (§4.3) — confirmed no
  segment-crossing risk structurally (4 MiB `SEGMENT`, negligible payload
  footprint at all three op counts), then empirically cross-checked `c`
  derived from two different adjacent point-pairs (N,2N vs 2N,4N) — found a
  genuine small (3.7%-7.4%) non-linearity in BOTH allocators, same direction,
  reported honestly (§4.3) rather than assumed away; confirmed it does not
  reverse or substantially change the corrected ratio's qualitative
  conclusion.
- **`production`'s feature composition confirmed unchanged**:
  `grep -n "^production = " Cargo.toml` still returns `["alloc-global",
  "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory",
  "primordial-lazy-commit", "class-aware-dirty"]`, byte-identical to
  pre-task. `git status --short` confirms `Cargo.toml` is not in this task's
  diff.
- No production code (`src/`) touched — only `benches/perf_gate_iai.rs`
  gained six new bench functions plus three new op-count constants, and this
  report/CSV/raw-log/`OPEN_ITEMS.md`/`R22_15_...md` docs were added/edited.
- Full diff of the touched bench file reviewed line-by-line by the same
  session that wrote it (self-review; per this project's zero-trust
  discipline the user is expected to personally re-run `npm run iai` before
  trusting this report's numbers, as with every prior gate report in this
  tree).

---

## 8. Files touched

- `benches/perf_gate_iai.rs` — added `CHURN_OPS_2N`/`COLD_BATCH_2N`/
  `COLD_BATCH_4N` constants and six new `#[library_benchmark]` fns
  (`small_churn_16b_2n`, `mimalloc_small_churn_16b_2n`,
  `cold_alloc_free_256x16b_2n`, `mimalloc_cold_alloc_free_256x16b_2n`,
  `cold_alloc_free_256x16b_4n`, `mimalloc_cold_alloc_free_256x16b_4n`),
  registered in the existing `perf_gate` `library_benchmark_group!` list.
  Zero changes to any pre-existing bench fn's body.
- `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md` — this report.
- `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE_summary.csv` — companion
  machine-readable summary (commit, features, CPU/OS/rustc/valgrind
  identification, per-workload Ir at N/2N/4N, derived `c`, old vs new
  ratio).
- `docs/perf/_raw_r23_2_warm_n_2n_gate.log` — full raw `npm run iai` stdout
  for the definitive run cited in §3 (30 benches, includes all six new
  arms). `git add -f` needed (`.gitignore:16` excludes `docs/perf/_raw_*.log`
  by default).
- `docs/perf/_raw_r23_2_warm_n_2n_gate_rerun1.log` — a second independent
  run's raw stdout, cited in §3 as the determinism evidence. `git add -f`
  needed, same reason.
- `docs/perf/R22_15_MIMALLOC_IR_ARM_GATE.md` — correction section appended
  (§9, see that file), original content preserved verbatim.
- `docs/perf/OPEN_ITEMS.md` — R22-15's entry gets a "DONE" follow-up note
  citing this task's corrected numbers.
- `Cargo.toml` — **untouched** (confirmed §7; `git status --short` shows no
  `Cargo.toml` entry).
- `.github/workflows/perf-gate.yml` — **untouched** (no new job/step needed;
  the existing `cargo bench --bench perf_gate_iai --features production`
  line already runs every fn in the group, same reasoning as R22-15).

**Files needing `git add -f`** (gitignored by `.gitignore:16`,
`/docs/perf/_raw_*.log`):

- `docs/perf/_raw_r23_2_warm_n_2n_gate.log`
- `docs/perf/_raw_r23_2_warm_n_2n_gate_rerun1.log`

---

## 9. Recommendation for the next round

This task is a MEASUREMENT/methodology-correction gate, not a remediation —
no `src/` code changed in response to either the OLD or NEW ratio. Two
follow-ups worth a future round's attention, neither implemented here:

- **Extend the N/2N pattern to the four remaining R22-15 pairs**
  (`churn_256b`, `recycle_alloc_free_256x16b`/`64b`) to get the fully
  corrected picture across all six original workload shapes, not just the
  two this task's minimum scope covered.
- **The hot-churn reversal (0.896, SeferAlloc cheaper than mimalloc) is a
  genuinely new, favorable finding** that R22-15's methodology masked —
  worth flagging in any future product-facing perf narrative, though this
  report makes no recommendation beyond stating the corrected number, per
  this project's "measure and report honestly, remediation is a separate
  task" convention.
