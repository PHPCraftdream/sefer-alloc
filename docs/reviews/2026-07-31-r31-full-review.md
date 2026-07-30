# Round 31 — independent full review (2026-07-31)

**Reviewer:** independent read-only review agent (second set of eyes; not the
orchestrating session that implemented and signed off on Round 31).
**Scope:** `14a9ef3..HEAD` (`aaed7c4d9eafd9b2f60e0289d6dc1ba476a90d48`), 13
commits, 76 files, +24,471/−349 lines. Every commit message and every diff
hunk was read in full — no sampling.
**Method:** CLAUDE.md's own zero-trust discipline applied literally — diff read
line by line, tests checked for non-vacuity, headline numbers **recomputed
from the committed raw artifacts** (not from the reports' own summary tables),
build/test/lint commands actually executed, and one out-of-tree compile probe
built to verify a feature-gating claim.

---

## 0. Executive summary

| Tier | Count | Verdict |
|---|---:|---|
| **P0** (soundness / correctness / build break) | **0** | Explicitly: none found. |
| **P1** (real but non-critical: misleading claim, missing evidence, broken convention) | **3** | All three are *claim-accuracy / process* defects, not data defects. |
| **P2** (process / polish) | **12** | |

**Nothing shipped a wrong number.** I independently recomputed six headline
results (R31-0's activation percentages and all 24 `notouch`/`onebyte`/`full`
wall-clock deltas across both runs, R31-1's crossing-regime hit rates,
R31-2's full t/se/MDE/CI statistics and the 320-launch mechanism-delta claim,
R31-3's multi-heap linear-scaling and all seven A/B t-statistics, and R31-12's
SEGMENT-rounding arithmetic straight from `alloc_core_large.rs`) and **every
one reproduced exactly**. The round's arithmetic and data hygiene are the best
of any round I have evidence for in this repo.

The three P1s are all of one shape: **a report claims a methodological
property it does not actually have.** In two of the three cases the underlying
conclusion is nonetheless correct (I verified it independently by another
route); in the third the gap is that a recurring defect class was diagnosed
but never codified.

---

## 1. Build / test / lint health on `HEAD` — actually run

All commands executed against `aaed7c4` with a clean working tree
(`git status --porcelain` → `?? .claude/` only).

| Command | Result |
|---|---|
| `cargo check` | **PASS** (exit 0) |
| `cargo clippy --features production -- -D warnings` | **PASS** — `Finished dev profile`, zero diagnostics |
| `cargo clippy --all-features --all-targets -- -D warnings` | **PASS** — `Finished dev profile in 30.35s`, zero diagnostics |
| `cargo test --features "production bench-internals alloc-stats"` | **PASS** — 230 `test result: ok` lines; aggregate **455 passed, 0 failed, 3 ignored** |
| `cargo fmt --check` | **PASS** — no output, exit 0 |

This matches the per-task commit messages' own claims (230 test binaries, 0
failed) exactly.

---

## 2. Scope-creep / unauthorized-change audit — CLEAN

Verified directly, not taken on trust:

```
$ git show 14a9ef3:Cargo.toml | grep -n "^production = "
399:production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
$ grep -n "^production = " Cargo.toml
399:production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
```

- **`production` feature list is byte-identical**, same line number, before and
  after. R31-0's and R31-3's explicit "did NOT touch it" claims are **CONFIRMED**.
- **`Cargo.toml`'s entire diff is pure addition** — `git diff 14a9ef3..HEAD --
  Cargo.toml | grep '^-' | grep -v '^---' | wc -l` → **0**. Only new
  `[[example]]`/`[[bench]]` registrations.
- **No version bump** — `version = "0.3.0"` before and after.
- **`Cargo.lock` untouched** — `git diff --stat 14a9ef3..HEAD -- Cargo.lock`
  is empty. No dependency drift.
- Both GO-supporting reports (R31-0 §5, R31-3 §5) are framed as **proposals
  awaiting sign-off**, not enactments; `CHANGELOG.md`'s Round 31 header opens
  with "**Runtime improvements this round: 0.**" and its `#### Runtime
  improvements` section reads "_None this round._". Honest framing confirmed.

---

## 3. The highest-priority adversarial check: new safe `pub fn` taking a raw pointer + touching allocator metadata

**Result: none introduced. The retrofit is sound.** Detail:

### 3.1 `ReservedSmallSegment` — unforgeability holds across the crate boundary

`src/alloc_core/reserved_small_segment.rs` adds a private-field newtype whose
only constructor is `pub(super) fn new_from_reservation`. `alloc_core` is
`pub mod` from `src/lib.rs:302` and `reserved_small_segment` is `pub mod` at
`src/alloc_core/mod.rs:99`, so the *type* is externally nameable — but the
constructor is not, so **no external crate can mint a handle**. Verified by
whole-repo grep: exactly one call site,
`src/alloc_core/alloc_core_small_pool.rs:1095`, immediately after a real
`reserve_small_segment_impl()` success; and exactly one `into_base` call site,
`:1117`. `dbg_decomp_release` losing its `unsafe fn` marker is therefore
justified — the raw-pointer precondition genuinely moved into the type.

`ReservedSmallSegment` holds a `*mut u8`, so it is automatically `!Send`/`!Sync`;
`into_base` uses `core::mem::forget(self)` correctly (no `unsafe` needed, no
resource leaked — the release itself happens in the caller).

### 3.2 The orchestrator's own flagged `pub(super)` concern — **CONFIRMED, and it is documentation-only**

The doc comment on `new_from_reservation` says it is "callable only from
within `alloc_core_small_pool.rs`'s own module tree", and the R31-4 commit
message repeats the claim. **This is factually wrong.**
`src/alloc_core/mod.rs:99` declares `pub mod reserved_small_segment;` as a
*direct child of `alloc_core`*, so `pub(super)` resolves to
`pub(in crate::alloc_core)` — reachable from **every** sibling module under
`alloc_core` (`alloc_core_large.rs`, `alloc_core_small.rs`,
`alloc_core_small_magazine.rs`, …), not just `alloc_core_small_pool.rs`.
Rust has no sibling-only visibility, so the stated scoping is not even
expressible.

**It is not a live exploit** — the grep above shows zero other callers — and
the load-bearing property (external unforgeability) is unaffected. Filed as
**P2-4** below. I independently reached the same conclusion the orchestrating
session recorded in its checkpoint, by reading the `mod` declarations rather
than by trusting that note.

### 3.3 One coverage narrowing the retrofit introduced

Before R31-4, `dbg_decomp_reserve_and_keep` returned `Option<*mut u8>` — a
`dbg_*`-named safe `pub fn` returning a raw pointer, i.e. exactly the shape
`tests/dbg_hook_safety_tripwire.rs`'s scanner exists to see. After R31-4 the
raw-pointer return lives on `ReservedSmallSegment::base(&self) -> *mut u8`,
which the scanner **cannot see**: `scan_file` (`:814`) only matches
`pub fn dbg_` / `pub unsafe fn dbg_`. See **P2-12**.

---

## 4. Independently recomputed headline numbers

I recomputed more than the three requested. Every figure below was derived by
me from the committed raw artifacts, not read out of a report table.

### 4.1 R31-2 — "mechanism delta stays ZERO through cap 32" — **CONFIRMED EXACTLY**

Recomputed from the four `docs/perf/paired_ab_runs/2026-07-30T20-5*.json`
provenance files:

| comparison | my mean Δ (ns) | my sd | my se | my t | my MDE | my MDE % | CSV says |
|---|---:|---:|---:|---:|---:|---:|---|
| cap4_vs_cap8 | −1,542,507.50 | 22,032,070.72 | 4,926,520.78 | −0.3131 | 10,350,620.16 | 4.5271 | identical |
| cap4_vs_cap16 | +400,845.00 | 17,396,588.16 | 3,889,995.37 | +0.1030 | 8,172,880.27 | 4.4359 | identical |
| cap4_vs_cap32 | −3,916,725.00 | 17,627,033.88 | 3,941,524.60 | −0.9937 | 8,281,143.19 | 3.9441 | identical |
| control | +8,519,315.00 | 21,391,771.06 | 4,783,345.43 | +1.7810 | 10,049,808.74 | 5.1647 | identical |

Every cell in `R31_2_..._summary.csv` reproduces to the last digit, including
the CI bounds and the sign counts (11/9, 11/9, 10/10, 9/11).

The load-bearing mechanism claim, checked over **every** launch record:

```
total launches: 320   (4 comparisons × 80 = 20 pairs × A/B/B/A)
distinct decommit_calls_total values: [40]   undefined: 0
```

**Bit-identical `40` in all 320 process launches — confirmed.** The "320
process launches" figure is also exactly right.

### 4.2 R31-1 — crossing-regime hit-rate break — **CONFIRMED EXACTLY**

From `R31_1_..._summary.csv`'s own per-arm `burst2_hits_median`/`burst2_possible`:

| burst | headroom | 1 thread | 8 threads | 32 threads | hit rate |
|---|---:|---|---|---|---|
| AT_BOUNDARY_6MiB | 64 MiB | 8/8 | 64/64 | 256/256 | 100.0 % |
| AT_BOUNDARY_6MiB | 256 MiB | 8/8 | 64/64 | 256/256 | 100.0 % |
| CROSSING_MODEST_12MiB | 64 MiB | 7/8 | 56/64 | 224/256 | **87.5 %** |
| CROSSING_MODEST_12MiB | 256 MiB | 8/8 | 64/64 | 256/256 | 100.0 % |
| CROSSING_R29_13_34MiB | 64 MiB | 7/8 | 56/64 | 224/256 | **87.5 %** |
| CROSSING_R29_13_34MiB | 256 MiB | 8/8 | 64/64 | 256/256 | 100.0 % |

The 12.5-percentage-point break is exact at every thread count and both
crossing sizes; the in-harness control reproduces R30-6's tie. `oracle_pass_all_reps = 1`
in all 18 rows.

### 4.3 R31-12 — the SEGMENT-rounding arithmetic — **CONFIRMED FROM SOURCE**

Read `src/alloc_core/alloc_core_large.rs:142-192` directly rather than
trusting the claim:

```rust
let hdr_aligned = align_up(size_of::<SegmentHeader>(), align.max(PAGE));
let needed      = hdr_aligned.checked_add(align_up(size, align))?;
// #[cfg(not(feature = "exact-span-large"))]
let usable      = needed.div_ceil(SEGMENT) * SEGMENT;   // SEGMENT = 4 MiB
```

Applying it (align = 8 ⇒ `hdr_aligned` = one PAGE = 4 KiB), for 8 objects:

| object | `needed` | `div_ceil(·, 4 MiB)` | span | × 8 | matches |
|---|---|---:|---|---|---|
| 6 MiB | 6 MiB + 4 KiB | 2 | 8 MiB | **64 MiB = 67,108,864** | R30-6 CSV `burst1_used_max_bytes` and R31-1 `AT_BOUNDARY` ✓ |
| 12 MiB | 12 MiB + 4 KiB | 4 | 16 MiB | **128 MiB = 134,217,728** | R31-1 `CROSSING_MODEST` ✓ |
| 34 MiB | 34 MiB + 4 KiB | 9 | 36 MiB | **288 MiB = 301,989,888** | R31-1 `CROSSING_R29_13` ✓ |

Note the 12 MiB case is only correct **because of the header term** — 12 MiB
alone is already a whole multiple of `SEGMENT`. The measured value (16 MiB
span) confirms the header is genuinely in the arithmetic, which is a stronger
validation of R31-12's claim than the 6 MiB case alone provides. Cross-checked
against R30-6's committed CSV: `burst1_used_max_bytes` is `67108864` in every
section-1 row.

### 4.4 R31-0 — `notouch` wall-clock deltas — **CONFIRMED EXACTLY, from the raw logs**

Computed by me from `_raw_r31_0_off.log` / `_raw_r31_0_on.log` (run 1) and
`_raw_r31_0_off_run2.log` / `_raw_r31_0_on_run2.log` (run 2):

| size | OFF r1 | ON r1 | my Δ r1 | report | OFF r2 | ON r2 | my Δ r2 | report |
|---|---:|---:|---:|---|---:|---:|---:|---|
| 4k | 2,342.1 | 257.5 | **−89.00 %** | −89.0 % ✓ | 2,851.2 | 299.6 | −89.49 % | −89.5 % ✓ |
| 16k | 11,229.6 | 564.6 | **−94.97 %** | −95.0 % ✓ | 14,034.2 | 440.0 | −96.86 % | −96.9 % ✓ |
| 64k | 52,116.7 | 754.2 | **−98.55 %** | −98.6 % ✓ | 47,182.5 | 460.0 | −99.03 % | −99.0 % ✓ |
| 128k | 97,198.3 | 1,354.6 | **−98.61 %** | −98.6 % ✓ | 93,875.0 | 1,496.7 | −98.41 % | −98.4 % ✓ |

I also recomputed all eight `onebyte`/`full` cells in both runs (e.g. 4k/onebyte
r1 −7.55 % → r2 +15.66 %; 16k/onebyte r1 +10.14 % → r2 −40.30 %; 16k/full r1
+35.96 % → r2 −9.09 %) — every one matches §3.1's table, and the report's
sign-inconsistency characterisation is fair.

**Activation:** all 24 ON-binary cells report `min_act_pct = 100.00` with
`oracle = PASS`; all 4 retention probes PASS with the exact expected
`retained`/`mask` values (4k: 15 retained, mask 32767 = 2^15−1; 16k: 3
retained, mask 7). "4/4 retention PASS + 24/24 activation PASS at 100.00 %
minimum" — **confirmed**.

### 4.5 R31-3 — multi-heap RSS and the seven A/B statistics — **CONFIRMED EXACTLY**

Parsed every `OK …` line out of `_raw_r31_3_multi_heap_rss_{off,on}.log`:

| arm | threads | reps | `sum/max` | `used_post_teardown_max` | `config_conflicts_delta` | `extension_materialised_count` |
|---|---:|---:|---|---:|---:|---:|
| off | 1 / 8 / 32 | 3 each | 1.0000 / 8.0000 / 32.0000 | 452,984,832 | 0 | 0 |
| on | 1 / 8 / 32 | 3 each | 1.0000 / 8.0000 / 32.0000 | 260,046,848 | 0 | 1 / 8 / 32 |

Exact linear scaling confirmed. All seven paired-A/B t-statistics match the
raw runner output verbatim: turnover 127.776 (sign 0/20 vs 20/20), its control
0.739 (10/20 vs 10/20), narrow N=1 7.113 (1/20 vs 19/20), N=2 17.843, N=4
10.945, N=1 control −0.105, N=4 control 1.282.

---

## 5. Non-vacuity of new/changed tests

### 5.1 `tests/r31_4_reserved_small_segment_handle.rs` — honest, sound, but weaker than it needed to be

The file's own module doc states plainly that "a compile error is, by
definition, not something a `#[test]` function can exercise", records that
`trybuild` was checked for and is **not** a dev-dependency (I verified: zero
`trybuild` hits crate-wide), and chooses to write the positive path plus a
prose argument instead. **That is honest, not hand-waved** — it does not claim
runtime proof of a compile-time property, and it names the two options it
weighed. The two tests themselves are genuine regression coverage (they would
not have compiled at all before the retrofit, and
`repeated_reserve_release_cycles_stay_healthy` pre-fills the pool so every
iteration drives the real release branch).

**But the "two options weighed" analysis missed a third, free one** — see
**P2-5**: `assert!(core::mem::needs_drop::<ReservedSmallSegment>())` is a
one-line, dependency-free, genuinely non-vacuous runtime assertion of half the
compile-error argument (a `Drop` type can never be `Copy` — a hard rustc rule),
and it would fail the moment a future refactor removed the `Drop` impl and
added `Copy`.

### 5.2 `tests/dbg_hook_safety_tripwire.rs` — three new tests, all non-vacuous

- `has_bench_internals_cfg_rejects_cfg_attr_shape` asserts the *new* 6-byte
  `#[cfg(` matcher rejects `#[cfg_attr(…)]`, **and** asserts the fixture
  genuinely starts with the old 5-byte prefix but not the new 6-byte one —
  i.e. it proves the fixture actually distinguishes old from new. It would
  have failed under the pre-R31-4 scanner. **Non-vacuous.**
- `scan_file_treats_cfg_attr_bench_internals_hook_as_ungated` exercises the
  *real* `scan_file` classifier end-to-end on a synthetic hook and asserts
  `!hit.bench_internals_gated` — under the old scanner this would have been
  `true`. **Non-vacuous.**
- `no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape` is a forward-looking
  structural guard over `src/` + `crates/`. Vacuous today by design (there are
  no offenders), which the test's own doc says explicitly. Acceptable.

I reviewed the scanner change itself for regressions: the loop bound moved
`i + 4 < len` → `i + 5 < len` (correct for a 6-byte window), the
unterminated-attribute advance moved `i += 5` → `i += 6` (correct), and the
success path (`i = j + 1; break;`) is unchanged. No new false-negative.

The two allowlist removals (`dbg_decomp_release` ×2 from `UNSAFE_HOOKS`,
`heap_core_diag.rs::dbg_large_cache_hits` from `PURE_OBSERVERS`) are consistent
with the code changes, and the tripwire's own completeness assertions still
pass (7/7 in the isolated feature combination, and in the full run).

### 5.3 `tests/profile.rs` (rewritten, 7 tests) — non-vacuous on the axis-independence claim

Every test reads the **resolved** value back through `AllocCore::dbg_pool_cap()`
/ `dbg_decay_config()`, not the requested builder value. The two axis-isolation
tests directly encode the R31-9 fix:
`throughput_small_pool_alone_does_not_perturb_large_cache_axis` asserts
`headroom == 256 MiB` — which the *old* bundled `Profile::Throughput` would
have failed (it set 64 MiB). `all_axis_combinations_resolve_independently`
covers the full 2×3 cross product, and
`no_small_pool_policy_reintroduces_the_r27_1_noop_trap` guards the clamp trap
CLAUDE.md names. This is a materially stronger test file than the one it
replaced.

---

## 6. Convention compliance

### 6.1 Append-only correction convention — **HONORED**

`git diff 14a9ef3..HEAD -- <file> | grep '^-' | grep -v '^---' | wc -l`:

| file | added | **removed** |
|---|---:|---:|
| `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` | 49 | **0** |
| `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` | 161 | **0** |
| `docs/CORRECTNESS_OPEN_ITEMS.md` | 125 | **0** |
| `CHANGELOG.md` | 23 | **0** |
| `docs/perf/R30_6_..._summary.csv` | 1 | 1 (a `commit_sha=<placeholder>` header comment only; **all data rows byte-identical**) |

Both superseded gate reports gained a dated §8 that explicitly says "This
section is appended, not a rewrite — every number and claim above stays exactly
as originally published", and R30-3's §8 correctly preserves the still-valid
parts of its own finding (the Ir evidence, the bare-`AllocCore` refill
diagnosis) rather than repudiating the whole report. **Exemplary.**

`docs/perf/OPEN_ITEMS.md` has 71 removed lines — item 25's old "RESOLVED,
NO-GO" body was replaced. That is correct behaviour for a *living index* (which
CLAUDE.md explicitly treats as mutable: "move it to that index's 'Recently
resolved' trail"), and the replacement text preserves R30-3's history
accurately. `README.md`'s 39 removals are the unavoidable consequence of the
breaking `Profile` API change. No finding.

### 6.2 Raw logs + summary CSV + immutable source identity — present for all four gate reports

All four R31 gate reports ship `_raw_*.log` (or `git add -f`'d provenance
JSONs) **and** a `_summary.csv` **and** a provenance paragraph. Two CSV-quality
defects and one provenance-form issue are filed below (P2-1, P2-9, P2-10), but
no report is missing an artifact class.

### 6.3 Doc-drift counters — **INDEPENDENTLY VERIFIED CORRECT**

Ran CLAUDE.md's own self-verifying command:

```
tier-1 (module-level #![allow(unsafe_code)]): 20   (13 in src/, 7 in crates/)
tier-2 (item-level  #[allow(unsafe_code)]):   66   across 18 files
tests/*.rs:                                  228
```

README's updated "**20** tier-1 … **66** tier-2 across **18** files" and
`docs/ARCHITECTURE.md`'s 227 → 228 are both exactly right. The 68 → 66 delta
corresponds precisely to the two `dbg_decomp_release` sites the retrofit made
safe.

### 6.4 Commit-prefix taxonomy (R30-12) — followed

`bench(perf)` ×3 for measurement-only work, `docs(perf)`/`docs` ×6 for
doc-only follow-ups, `fix(alloc-core)` for R31-4, `refactor(profile)` for
R31-9. No bare `perf(...)` on measurement work. `refactor(profile)` is outside
R30-12's four-prefix list but serves the rule's purpose better than
`perf(opt-in)` would (no perf changed). No finding.

### 6.5 Round-32 deferral — recorded honestly

`docs/checkpoints/2026-07-31-0100.md` names all seven deferred tasks by
number (#468, #469, #470, #472, #474, #475, #477), states the reasoning
("recognized that the originally-planned 14-task queue reproduced the exact
'mostly meta-work, few product decisions' pattern the review criticized"), and
lists the two production-composition decisions as **open questions awaiting
sign-off**. It even self-reports the `pub(super)` doc overclaim I confirmed
independently in §3.2. Internally consistent with the commits. No finding.

### 6.6 R31-9's breaking-change justification — holds

`Cargo.toml` line 3 reads `version = "0.3.0"` both before and after; the
`Profile::{Rss,Balanced,Throughput}` enum is genuinely gone from `src/` with
**no `#[deprecated]` shim and no compatibility alias** (grep confirms zero
`src/` references and zero `deprecated` in `profile.rs`), consistent with the
"unreleased, free to break" reasoning stated. (I cannot verify crates.io
publish status offline; the version-number claim itself is confirmed.)

---

## 7. Findings

### P1 — real defects (3)

---

#### **P1-1 — `scripts/r31_0_summary.mjs` computes no deltas and asserts nothing, yet R31-0 claims its headline table is "derived by one checked script"**

**Where:** `scripts/r31_0_summary.mjs` (all 130 lines);
`docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` §3 and §3.1.

**What's wrong.** §3 states: *"Derived by `scripts/r31_0_summary.mjs` from the
primary run-1 raw logs (one checked script, not hand-transcribed, per
CLAUDE.md's derived-tables rule)."* §3.1 goes further: *"`Δ (run2)` is the SAME
cells recomputed from the independent repeat-run logs … **both by the same
checked script pass**."*

Reading the script in full:

- It **never reads the run-2 logs**. Its own header comment (lines 16-18) says
  so: *"the run-2 logs (`_raw_r31_0_*_run2.log`) … are NOT folded into the
  summary."* `readFileSync` is called on exactly two files, both run-1.
- It **computes no percentage of any kind.** `parseRows` splits lines on `,`;
  `fmtVirginRecycled`/`fmtRetention` re-emit `fields[n]` verbatim into a CSV
  row. There is no arithmetic anywhere in the file.
- It contains **zero assertions** — the only non-`console.log` output is one
  `console.error` progress line. Contrast: `r31_1_derive_report_data.mjs` has
  6 `throw`s, `r31_2_derive_report_data.mjs` asserts its MDE arithmetic
  (`:134`), and `r31_12_repair_r30_6_data.mjs` has 4 hard assertions.

So the entire Δ column of §3.1 — including all four bolded `notouch`
headlines that carry the round's P0 verdict — was **hand-computed**, which is
exactly the practice CLAUDE.md's R30-9 rule points 1, 2 and 6 exist to
forbid, and the report asserts the opposite.

**Why it matters.** This is the same defect class R30-9 was written for
(R29-3's landing commit `db35617` documents prose citing numbers that matched
neither saved raw log). Round 31's P0 headline currently has no mechanical
guard against a transcription error.

**Mitigating.** I recomputed all 24 cells across both runs (§4.4) and every one
is arithmetically correct. This is a claim-accuracy and process-hardening
defect, not a wrong number.

**Suggested fix.** Extend `r31_0_summary.mjs` to read both run pairs, emit the
Δ columns into the CSV, and `throw` if any of the four `notouch` percentages
deviates from the published value — then re-cite §3/§3.1 truthfully.

---

#### **P1-2 — R31-2 §4.3 point 2 states a RESOLVED-config read-back that does not exist; `config_conflicts_total()` is never read**

**Where:** `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md` §4.3 points 2-3;
`examples/_shared/r31_2_pool_cap_threshold_workload.rs:141-179`.

**What's wrong.** §4.3 point 2 reads: *"**RESOLVED** — read back at runtime and
emitted as `RESULT pool_segments_requested=`/`RESULT pool_byte_cap_requested=`
in every launch."*

The workload's `run_arm` signature is
`fn run_arm(arm_name, global, requested_pool_segments: u64, requested_pool_byte_cap: u64)`
and its body is `proc_probe::emit_u64("pool_segments_requested", requested_pool_segments)`
— it echoes the **compile-time constant it was passed**. Nothing is read back
from the allocator. I confirmed against the provenance JSONs: every launch
record's key set is
`block, slot, arm, elapsed_ns, threads, rounds_per_thread, pool_segments_requested,
pool_byte_cap_requested, segments_reserved_total, segments_released_total,
decommit_calls_total, large_cache_hits, rss_after_kib, commit_after_kib,
wall_clock_iso` — **no resolved value, no conflict counter.** §4.3 point 3
declares the conflict counter "does not apply".

`HeapCore::dbg_pool_cap()` — added by R26-2 (`src/registry/heap_core_diag.rs:294`)
*specifically* to close this gap after the R25-5 incident CLAUDE.md's R26-4
rule narrates — was available, free, and unused. **Both sibling harnesses in
the same round do it correctly**: `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`
hard-asserts the resolved headroom (`:300-306`) *and* asserts
`conflicts_delta == 0` (`:389`), and R31-3's multi-heap gate emits
`config_conflicts_delta=0` per child.

CLAUDE.md's R26-4 rule is stated absolutely: *"A config-sweep row missing any
of these is not usable as GO/NO-GO evidence."*

**Why it matters.** R31-2 is a config sweep whose whole verdict is "the
mechanism doesn't move at any cap" — the single failure mode that would
invalidate it is arms silently running under the wrong cap, which is precisely
the R25-5 bug. The report's structural argument ("a compile-time `static` has
no runtime resolution step") is also inaccurate: `AllocCore::new_with_config`
*does* resolve at runtime (`alloc_core.rs:961-963`).

**Mitigating — I checked the conclusion by another route and it holds.**
Reading `src/alloc_core/alloc_core.rs:949-965`: `core.pool_cap =
resolved_pool_segments().min(resolved_pool_byte_cap() / SEGMENT)`, with the old
`POOL_MAX_SLOTS` clamp explicitly removed by R26-2 ("a caller who asks for
`.pool_segments(64)` genuinely GETS a cap of 64"). For the four arms'
constants — (4, 16 MiB), (8, 32 MiB), (16, 64 MiB), (32, 128 MiB) with SEGMENT
= 4 MiB — the `min` is the identity in every case: 4, 8, 16, 32. Subprocess
isolation additionally makes cross-arm leakage structurally impossible. **The
verdict is sound; the evidence as reported is not what the report says it is.**

**Suggested fix.** Either add a `dbg_pool_cap()` read-back + a
`config_conflicts_total()` delta emit and re-run (cheap — the harness already
exists), or rewrite §4.3 points 2-3 to state honestly that resolution was
established by source reading and structural argument, not read back at
runtime.

---

#### **P1-3 — the round's own P0 defect class ("the judge measured the wrong allocator layer") was never codified anywhere**

**Where:** `CLAUDE.md` (unchanged: `git diff 14a9ef3..HEAD -- CLAUDE.md` → **0
lines**); `docs/perf/OPEN_ITEMS.md`; `docs/CORRECTNESS_OPEN_ITEMS.md`.

**What's wrong.** Round 31's centerpiece finding is that R30-3 built a
methodologically compliant judge — it *had* a path-activation oracle, it
*honestly reported* ~3 % activation, it *correctly diagnosed* the refill
dilution — and still shipped a wrong verdict, because it measured
`AllocCore::alloc_zeroed` (the magazine-bypass substrate) instead of
`HeapCore::alloc_zeroed` (the chain `SeferAlloc`'s `#[global_allocator]`
actually uses). The oracle's signal was a property of the *substrate*, and was
read as a property of the *feature*.

This is the third instance of one meta-pattern:
- R25-5 measured the wrong **config** → CLAUDE.md gained the R26-4 rule.
- R29-16 measured the wrong **code path** → CLAUDE.md gained the R30-8 rule.
- R30-3 measured the wrong **layer** → **nothing was written down.**

Grepping the whole repo for a layer rule: `grep -in "allocator layer\|layer
under test\|which layer" CLAUDE.md` returns **nothing**; `grep -rn "allocator
layer" docs/perf/OPEN_ITEMS.md docs/CORRECTNESS_OPEN_ITEMS.md` returns exactly
one hit — line 207, inside item 25's *narrative* of what happened, not a
tracked follow-up with an owner. The one deferred rule-writing task, #472
(R31-8), is scoped to "cost and benefit must be measured in the same workload
regime" — a different rule.

**Why it matters.** CLAUDE.md's own "Round start" bullet exists because "the
in-session TaskList does not survive a session boundary … these indexes do."
A fresh session in Round 32 inherits **no** memory that this defect class
exists. R30-8's own precedent is that catching a class once and not codifying
it is how it recurs — that rule's text says as much.

**Suggested fix.** Add a short CLAUDE.md rule (sibling to R26-4 and R30-8):
*a gate report must name the exact entry point under test — `AllocCore::…` vs
`HeapCore::…` vs a real `#[global_allocator]` — and state why that layer is
the one the decision applies to; a judge measuring below the layer a feature
actually ships at is not usable as promotion evidence.* At minimum, file it as
an owned item in `docs/perf/OPEN_ITEMS.md` so Round 32 inherits it. R31-0's own
report already contains a ready-made narrative for the rule's citation block.

---

### P2 — process / polish (12)

**P2-1 — R31-0's summary CSV is structurally ragged.**
`awk -F',' '{c[NF]++}'` → 49 rows with 24 fields, **4 rows with 16 fields**,
under a single 24-column header, interleaved mid-file with no section marker.
The four `retention` rows are emitted by `fmtRetention` against a
`RETENTION_HEADER` constant that is *defined in the script but never written to
the file*. Any standard CSV reader mis-keys them (`expected_hits` = `true`,
`mean_zp` = `PASS`, `min_zp` = the landing SHA). This defeats the stated
purpose of the summary-CSV rule ("grep/diff-able across rounds without
re-parsing prose"). *Fix:* emit two files, or write the second header line, or
pad to the wide schema. (R31-1 and R31-2's CSVs are uniform; R31-3's is a
correctly-quoted 7-column long format and is fine.)

**P2-2 — R31-0's CSV publishes a knowingly-vacuous statistic without marking
it.** All 24 OFF-binary rows carry `mean_act_pct`/`min_act_pct` of `100.00`
(virgin) or `0.00` (recycled), derived from `SMALL_ZERO_PASS_CALLS`, which §2.2
of the report itself proves is *never incremented on the OFF binary* (the
counter lives entirely inside the `#[cfg(feature = "virgin-zero-skip")]`
branch). Only the `oracle=NA` column signals this. A script reading the CSV
sees "100 % activation" for a binary where the metric is meaningless. *Fix:*
emit `NA` in those two columns on the OFF arm too.

**P2-3 — R31-0 §3.3 cites specific numbers from an uncommitted third run.**
*"A third, uncommitted ad-hoc re-run … (not saved as a cited raw log) … landed
in the same −91 %/−97 %/−99 %/−99 % range."* This is precisely the R29-3
pattern R30-9 point 2 was written against (prose quoting a run with no
artifact). It is explicitly labelled "corroborating, not part of the cited
evidence set", which is honest and materially better than R29-3 — but four
specific figures from an unreproducible run are still in the report. *Fix:*
either commit the log or drop the figures and keep the qualitative statement.

**P2-4 — `ReservedSmallSegment`'s `pub(super)` scoping claim is wrong in three
places.** See §3.2. `src/alloc_core/reserved_small_segment.rs:23-27` and
`:80-85` ("callable only from within `alloc_core_small_pool.rs`'s own module
tree"), `:108-112` ("not exposed outside this module tree"), and the R31-4
commit message all overstate. Actual scope is `pub(in crate::alloc_core)` —
every sibling module. **Verified not exploitable** (single caller). *Fix
(doc-only):* "reachable from anywhere inside `alloc_core`; in practice called
from exactly one site (`alloc_core_small_pool.rs:1095`). Rust has no
sibling-module-only visibility, so this is the tightest expressible bound."

**P2-5 — the double-release counterfactual has a cheap runtime half the file's
own analysis missed.** `tests/r31_4_reserved_small_segment_handle.rs` weighs
exactly two options (trybuild vs. prose). A third exists at zero cost:
`assert!(core::mem::needs_drop::<ReservedSmallSegment>())` — `needs_drop` is
`const`-evaluable and callable at runtime, and a type with a `Drop` impl can
never be `Copy` (hard rustc rule). Combined with the by-value signature (which
the file already exercises), that is the complete compile-error argument, and
unlike the prose it *would fail* if a refactor dropped `Drop` and added `Copy`.

**P2-6 — `ReservedSmallSegment` is not `#[must_use]`.** `let _ =
ac.dbg_decomp_reserve_and_keep();` compiles with no warning and then fires the
`Drop` `debug_assert!(false, "…reservation leaked…")` at *runtime* in debug
builds. `#[must_use]` on the struct turns the most likely misuse into a
compile-time warning. One attribute.

**P2-7 — R31-1 misattributes "36 rows" to R30-6's CSV.** Report line 13:
*"confirmed directly by R30-6's own committed CSV, whose `burst1_used_max_bytes`
column reads `67108864` … in every one of its **36 rows**."* R30-6's committed
CSV has **12** section-1 data rows (each a median of 3 reps). The 36 rows live
in the raw log — which `scripts/r31_12_repair_r30_6_data.mjs:56` correctly
asserts (`expected 36 rows in R30-6 raw log`). The claim is true of the raw
log, wrong about the artifact named. *Fix:* cite the raw log, or say "12 CSV
rows / 36 underlying arms".

**P2-8 — unit error inside R31-3's summary CSV.** Row
`multi_heap_rss,rss_post_kib_per_heap,off,threads=8,410112,kib,"3280892/8 = ~410
MiB/heap (400.5 rounded)"` — 410,112 KiB is **400.5 MiB**, not ~410 MiB; the
note contradicts itself in one string (KiB read as MiB). Its two siblings
(threads=1 "~403 MiB", threads=32 "~400 MiB") are correct. Exactly the
data-hygiene class R31-12 spent this round repairing in R30-6.

**P2-9 — immutable source identity is produced *after* measurement in all four
gate reports.** Each cites its own **landing commit** SHA. All four provenance
JSONs record `git_dirty: true` against the pre-task base
(`d9d30cd`/`f93e663`/`14a9ef3`). CLAUDE.md's R30-9 point 7 requires the identity
to be *"produced BEFORE measurement, not assembled after the fact"* and captured
*"from something that exists AT measurement time"* — a landing commit made
after the fact assumes, without proving, that the measured working tree equals
the committed tree (it demonstrably does not: the reports themselves were
written after measurement and are in that commit). This is a round-wide,
inherited pattern and strictly stronger than the R27-3/R27-4 baseline the rule
was written against. *Fix:* one `git write-tree` (or `git diff | sha256sum`)
immediately before each measurement run, cited alongside the landing SHA.

**P2-10 — intra-round doc drift: R31-2's own new comments reference an API
R31-9 removed later in the same round.** `Cargo.toml:1792` (added by R31-2)
says *"(8,32MiB) — `Profile::Throughput`'s current value"*, and
`docs/perf/OPEN_ITEMS.md:123` (also added by R31-2) says *"document the 8/32 MiB
recipe as `Profile::Throughput`"*. `Profile::Throughput` no longer exists at
`HEAD`. `Cargo.toml:1774` carries the same stale reference from R30-7.
Cosmetic, but it is a fresh stale-reference introduced *within* the round.

**P2-11 — `AllocCore::dbg_large_cache_hits` remains a safe `pub fn` in a plain
`production` build.** Verified by out-of-tree compile probe (see §8): it
compiles against `features = ["production"]` alone. R31-4 tightened only the
`HeapCore` delegation. It is allowlisted in the tripwire's `PURE_OBSERVERS`
(`tests/dbg_hook_safety_tripwire.rs:213`) and is a zero-argument `&self` counter
read with no pointer and no mutation, so it is a *sanctioned* exception — but
CLAUDE.md's benchmark-hook rule 2 ("no production caller ⇒ MUST default to
`bench-internals`") applies to it by the identical reasoning R31-4 used against
its own sibling, and the commit does not say why the pair was split. Worth one
sentence of justification, or a matching tightening.

**P2-12 — the retrofit narrowed tripwire coverage of the hook it hardened.**
`scan_file` (`tests/dbg_hook_safety_tripwire.rs:814`) matches only
`pub fn dbg_` / `pub unsafe fn dbg_`. The raw-pointer return that used to live
on `dbg_decomp_reserve_and_keep` (and was therefore scanned) now lives on
`ReservedSmallSegment::base(&self) -> *mut u8`, which is invisible to the
scanner. Harmless today (`bench-internals`-gated; returns a pointer the caller
already legitimately holds). *Fix:* rename to `dbg_base()`, or widen the
scanner to also enumerate `#[doc(hidden)] pub fn` returning `*mut`/`*const` on
measurement-only types.

**(Also noted, no finding filed):** `examples/_shared/r31_2_pool_cap_threshold_workload.rs:34`
claims the 4-size mix is "well under the 16 KiB `SMALL_MAX` boundary".
`SMALL_MAX` is `SIZE_CLASS_TABLE[TABLE_LEN-1]` ≈ **253 KiB**
(`src/alloc_core/size_classes.rs:22,169`), not 16 KiB. Inherited verbatim from
`examples/_shared/r30_7_server_shaped_workload.rs:36` by the deliberate
byte-identical-copy design, so it propagated rather than originated here — but
it is now wrong in two files. R31-3's missing N=2 same-vs-same control **is**
disclosed explicitly in the report ("Same-vs-same controls (harness sanity,
N=1 and N=4)"), so no finding.

---

## 8. Verification commands and evidence trail

| Claim checked | How | Result |
|---|---|---|
| `HeapCore::dbg_large_cache_hits` absent from plain `production` | out-of-tree crate in `/tmp`, path dep, `features = ["production"]`, took a fn pointer to the method | **`error[E0599]: no associated function … dbg_large_cache_hits`** — R31-4's P2-2 fix **CONFIRMED** |
| …and present with the gate satisfied (counterfactual) | same probe, `features = ["production","bench-internals","alloc-stats"]` | **compiles** — the probe is non-vacuous |
| `AllocCore::dbg_large_cache_hits` still in `production` | same probe, `features = ["production"]` | **compiles** → P2-11 |
| `production` list unchanged | `git show 14a9ef3:Cargo.toml` vs `HEAD` | byte-identical |
| Cargo.toml is additive only | `git diff … \| grep '^-' \| grep -v '^---' \| wc -l` | `0` |
| 320 launches, `decommit_calls_total` ≡ 40 | Node walk of all 4 provenance JSONs' `raw_process_launches` | `total 320`, `distinct [40]` |
| R31-2 statistics | Node recompute of mean/sd/se/t/MDE/CI from `deltas_a_minus_b_ns` | all 4 rows exact |
| R31-3 multi-heap scaling | Node parse of every `OK …` line in both raw logs | `sum/max` = 1/8/32 exactly; `cfg_conflicts_delta` 0 |
| R31-0 wall-clock deltas | manual recompute from all four raw logs | all 24 cells × 2 runs match |
| SEGMENT rounding | read `alloc_core_large.rs:142-192`, applied to 6/12/34 MiB | 64/128/288 MiB — matches both CSVs |
| unsafe seam counts | `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` | 20 tier-1 / 66 tier-2 / 18 files — README correct |
| test-file count | `ls tests/*.rs \| wc -l` | 228 — ARCHITECTURE.md correct |

The temporary probe crate was created under `/tmp/r31probe` and deleted; **no
repository file was modified by this review** other than the creation of this
report.

---

## 9. Overall verdict

**Round 31 is safe to consider done as currently committed.** Nothing in it is
a soundness bug, a correctness regression, or a build break: `cargo check`,
`cargo clippy --features production -- -D warnings`, `cargo clippy
--all-features --all-targets -- -D warnings`, `cargo fmt --check` and `cargo
test --features "production bench-internals alloc-stats"` (455 passed / 0
failed / 3 ignored across 230 binaries) are all green on `aaed7c4`;
`Cargo.toml`'s `production` line and the crate version are byte-identical to
`14a9ef3`; and every one of the six headline results I recomputed from the
committed raw artifacts — R31-0's 24 activation cells and all 48 wall-clock
deltas, R31-1's 18 crossing-regime hit rates, R31-2's full statistics plus the
bit-identical `decommit_calls_total = 40` across all 320 process launches,
R31-3's multi-heap linear scaling and all seven t-statistics, and R31-12's
SEGMENT-rounding arithmetic read straight out of `alloc_core_large.rs` —
reproduced exactly. The `ReservedSmallSegment` retrofit is a genuine structural
improvement that introduces no new instance of the raw-pointer hook class it
was built to close, and the append-only correction discipline was honored
without a single removed line in either superseded gate report. The three P1s
are all "a report claims a methodological property it does not have" rather
than "a report published a wrong number", and in the two that affect a verdict
(P1-1, P1-2) I verified the underlying conclusion independently by a second
route and it holds. **The one thing I would not defer past Round 32 is P1-3**:
the round's defining insight — that a judge can satisfy every existing rule,
including R30-8's own path-activation oracle, and still measure the wrong
allocator layer — exists today only as narrative prose inside a gate report and
one `OPEN_ITEMS.md` item body. `CLAUDE.md` was not touched this round at all,
and neither open-items index carries an owned follow-up for it, so a fresh
session in Round 32 inherits no memory that this defect class exists. That is
the exact failure mode R26-4 and R30-8 were each written to stop, one level up.
P1-1 and P1-2 are cheap to close in the same pass (one script change, one
harness field or one honest paragraph), and the twelve P2s are polish that can
ride along with them.
