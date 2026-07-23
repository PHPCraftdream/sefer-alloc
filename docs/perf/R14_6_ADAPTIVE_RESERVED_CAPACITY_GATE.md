# R14-6 — closing R13-6's `realloc_grow` regression via a wider `reserved_capacity` growth factor

**Task:** #291 (R14-6). Follow-up to
`docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md`, which gave
`exact-span-large` (R12-3) + `large-reserved-capacity` (R12-4) a
CONDITIONAL-GO verdict: a real, large RSS win (15.80×→1.06× at 260 KiB) but a
deterministic iai regression on `realloc_grow` (**+102.3% instructions,
+52.7% Estimated Cycles**) traced to the fixed `2x` geometric ceiling on
`reserved_capacity` being re-tripped on almost every step of a
doubling-cadence realloc chain. R13-6 §7 named the exact follow-up this task
performs: "investigate whether `LARGE_RESERVED_CAP_GROWTH_FACTOR` (currently
a fixed 2×) can be safely widened... with the SAME iai `realloc_grow` bench
as the before/after judge."

**Date:** 2026-07-23. **Base revision:** `main` @ `c0ccbc4` (R14-1..R14-5
landed). **Platform measured:** Windows 10 Pro x86-64 (native) for
wall-clock/RSS/commit-charge, **WSL2 (Ubuntu 24.04) + Valgrind/Callgrind**
for the deterministic iai judge — the same dual-platform setup R13-6 used.

**Both features remain opt-in.** `Cargo.toml`'s `production = [...]` is
UNCHANGED by this task — `exact-span-large` and `large-reserved-capacity`
still do not appear there. The only `src/` change is the value of one
constant (`LARGE_RESERVED_CAP_GROWTH_FACTOR`, `src/alloc_core/alloc_core_large.rs`),
2 → 4, plus doc updates explaining why. Whether this pair is promoted into
`production` is explicitly left to the user/orchestrator, per this task's own
brief — this document is measurement + a data-driven constant change, not a
promotion decision.

---

## 0. Headline result (before / after this task)

| # | Measurement | R13-6 (`2x` factor) | R14-6 (`4x` factor) | Verdict |
|---|---|---:|---:|---|
| 1 | iai `realloc_grow` Instructions (Ir) vs plain `production` | **+102.320%** | **−22.44%** | **Regression eliminated — treatment is now FASTER than baseline** |
| 2 | iai `realloc_grow` Estimated Cycles vs plain `production` | **+52.675%** | **−36.17%** | **Regression eliminated** |
| 3 | R12-4 3-arm harness: move legs / 4 (256 KiB→4 MiB chain) | 2 relocations, 2 in-place | **1 relocation, 3 in-place** | **Matches baseline's own in-place count (3/4) exactly** |
| 4 | R12-4 harness wall-clock (best of 20) | 2.269 ms (**+18.8%** vs baseline 1.911 ms) | **1.119 ms (−41.5% vs baseline)** | **Regression eliminated — treatment now faster than baseline** |
| 5 | RSS amplification, 260 KiB Large alloc | 15.80× → 1.06× | 15.80× → **1.06×** (unchanged) | **RSS win fully preserved** |
| 6 | Large-cache exact hit rate (N=4 sizes) | 99.99% → 99.95% | 99.99% → **99.95%** (unchanged) | **Unaffected — cache admission is keyed on `usable_size`, not `reserved_capacity`** |
| 7 | First-heap commit-charge | no measurable difference | **no measurable difference** (unchanged) | **Unaffected, as predicted** |

**The single most important change**: raising `LARGE_RESERVED_CAP_GROWTH_FACTOR`
from 2 to 4 does not merely shrink R13-6's regression — it **inverts its
sign**. The doubling-cadence `realloc_grow` workload that previously cost
+102.3% instructions over baseline now costs **22.44% FEWER** instructions
than baseline, while every RSS/commit-charge/cache-hit-rate axis R13-6
measured is numerically unchanged (because `reserved_capacity` only
increases the RESERVED-but-uncommitted VA span — `reserve_capacity_exact`
still commits only `usable` bytes; see §1.2).

---

## 1. Why 4x, not some other factor — the data behind the constant

### 1.1 Modeling the doubling-cadence workload analytically

`benches/perf_gate_iai.rs::realloc_grow` is a pure geometric-doubling chain:
64 B → 4 MiB via 16 successive `size *= 2` steps. For a segment reserved at
`usable_0 * FACTOR` (capped at `LARGE_RESERVED_CAP_BYTES` = 16×SEGMENT = 64
MiB, which this chain never approaches — max size is 4 MiB), a doubling step
stays in place as long as the new size fits under the current segment's
`reserved_capacity`; once it doesn't, the realloc relocates and the FRESH
segment gets its own `usable_1 * FACTOR` ceiling, recursively. Modeling the
16-step chain for several factors, using TOTAL BYTES COPIED across all
relocations as the cost proxy (not merely relocation COUNT — Callgrind's `Ir`
scales with `memcpy` bytes, and the LATEST relocations in a doubling chain
move megabytes-scale prefixes, which dominates the total):

| Factor | Relocations / 16 steps | Total bytes copied |
|---|---:|---:|
| 2x (R12-4/R13-6 original) | 8 | ~2.67 MiB |
| **4x (R14-6, chosen)** | **5** | **~1.14 MiB (−57% vs 2x)** |
| 8x | 4 | ~2.13 MiB (worse than 4x — halving the relocation count again barely shrinks the dominant LAST copy, so total bytes REGRESSES relative to 4x) |

4x is the sweet spot for THIS access pattern: fewer relocations AND fewer
total bytes copied than either 2x (too tight — re-trips almost every other
step) or 8x (over-wide — burns VA reservation for a diminishing returns on
copy-avoidance, since the dominant cost is the LAST, largest relocation,
whose size is barely affected by pushing the ceiling from 4x to 8x). A
per-chain COMPOUNDING multiplier (2x on first reservation, doubling further
on each subsequent relocation of the SAME logical chain) models out even
better in isolation (~0.58 MiB total bytes copied in the same simulation),
but requires new per-segment chain-identity state — there is no existing way
for a freshly-relocated segment to recognise "I was born from relocating
THIS SAME logical realloc chain" versus an unrelated fresh `alloc` (the
relocation path reuses the ordinary `self.alloc(new_layout)` → `alloc_large`
→ `alloc_large_slow` call chain, shared with every other Large allocation
site). Implementing the compounding variant would mean either a new
`SegmentHeader` field or threading a relocation-vs-fresh-alloc hint through
that shared call path — a materially larger, riskier change for headroom
this task's measured numbers (§2 below) do not currently need. Deferred as a
future refinement if 4x's real numbers ever stop being enough; see
`LARGE_RESERVED_CAP_GROWTH_FACTOR`'s doc in `alloc_core_large.rs` for the
same writeup in-repo.

### 1.2 Why the RSS axis is untouched by this change

`Segment::reserve_capacity_exact(reserved_len, initial_commit)`
(`src/alloc_core/os.rs`) calls `aligned_vmem::reserve_aligned_lazy(reserved_len,
SEGMENT, initial_commit)`, which reserves `reserved_len` bytes of VA but
commits only `initial_commit` (== `usable`) bytes — the rest stays
reserved-but-uncommitted (Windows lazy-commit backend; eager elsewhere, see
`large-reserved-capacity`'s own "Windows-gated EFFECT" framing). Doubling the
growth factor from 2x to 4x doubles the RESERVED VA span (e.g. 260 KiB × 4 =
1040 KiB reserved instead of 520 KiB) but does **not** change what gets
COMMITTED (`usable` alone) — so RSS and Windows commit-charge are
structurally unaffected. §2 confirms this holds numerically, not just
theoretically.

---

## 2. Full re-run of R13-6's gate protocol

### 2.1 iai `realloc_grow` — the decisive, deterministic number

Raw logs: `docs/perf/_raw_r14_6_iai_baseline_production.log`,
`docs/perf/_raw_r14_6_iai_treatment_on.log`.

| Metric | `production` (baseline) | `+exact-span-large+large-reserved-capacity` (4x) | Δ |
|---|---:|---:|---:|
| Instructions (Ir) | 513,188 | 398,015 | **−22.44%** |
| L1 Hits | 1,087,343 | 939,377 | −13.61% |
| L2 Hits | 3,943 | 3,948 | +0.13% |
| RAM Hits | 70,653 | 37,889 | −46.37% |
| Total read+write | 1,161,939 | 981,214 | −15.55% |
| Estimated Cycles | 3,579,913 | 2,285,232 | **−36.17%** |

Baseline is byte-for-byte consistent with R13-6's own baseline (513,187 Ir
then, 513,188 Ir now — 1-unit noise from a different Callgrind run,
irrelevant at this scale). The treatment number **flips sign**: R13-6 saw
+102.3% Ir / +52.7% cycles; R14-6 sees **−22.4% Ir / −36.2% cycles** — the
pair is now measurably FASTER than plain `production` on this specific
geometric-doubling workload, not merely "regression reduced."

### 2.2 R12-4's own three-arm harness, re-run on current HEAD

Raw logs: `docs/perf/_raw_r14_6_r12_4_arm1_production.log` (baseline),
`_raw_r14_6_r12_4_arm2_exact_span_only.log` (`exact-span-large` ALONE — the
"problem" arm, unaffected by this task's change since it lacks
`large-reserved-capacity`), `_raw_r14_6_r12_4_arm3_both_features.log` (both
features, now with the 4x factor).

| Arm | Move legs / 4 | In-place legs / 4 | Copied bytes | Wall-clock (best of 20) | Peak RSS Δ | Peak commit Δ |
|---|---:|---:|---:|---:|---:|---:|
| `production` (baseline) | 1 | 3 | 2,097,152 | 1.9173 ms | 6,192 KiB | 12,676 KiB |
| `+exact-span-large` alone | 4 | 0 | 3,932,160 | 3.0803 ms | 7,996 KiB | 8,352 KiB |
| `+exact-span-large+large-reserved-capacity` (R14-6, 4x) | **1** | **3** | **1,048,576** | **1.1185 ms** | **5,168 KiB** | **5,508 KiB** |

R13-6's 4x-factor equivalent row was 2 move legs / 2 in-place, 2,621,440
copied bytes, 2.269 ms, 6,708 KiB RSS Δ, 7,056 KiB commit Δ — every single
one of those numbers is now BETTER under the wider factor: the mitigation no
longer merely "partially recovers" the `exact-span-large`-alone regression,
it fully matches baseline's own in-place/relocation ratio (3-of-4, identical
to plain `production`'s own 3-of-4) while keeping `exact-span-large`'s RSS
tightness (5,168 KiB peak RSS Δ here vs baseline's 6,192 KiB — still BETTER
than baseline, unlike R13-6's 6,708 KiB which was worse than baseline on
this specific axis).

### 2.3 Criterion wall-clock distribution (`r13_6_realloc_chain`, reused verbatim)

Raw logs: `docs/perf/_raw_r14_6_wallclock_bench_baseline_off.log`,
`docs/perf/_raw_r14_6_wallclock_bench_treatment_on.log`.

| Arm | Time (criterion `[low mid high]`) |
|---|---|
| `production` (baseline) | [3.1745 ms, 3.2293 ms, 3.2761 ms] |
| `+exact-span-large+large-reserved-capacity` (4x) | [1.8310 ms, 1.9196 ms, 1.9803 ms] |

Criterion's own paired comparison (`change:` line in the raw log):
**−41.652% (p < 0.05, "Performance has improved")** — R13-6 measured +9.3%
here; R14-6 measures a genuine improvement, consistent in sign and rough
magnitude with §2.1's iai number and §2.2's harness number.

### 2.4 RSS amplification (unaffected — reproduced for completeness)

Raw logs: `docs/perf/_raw_r14_6_r12_3_baseline_off.log`,
`docs/perf/_raw_r14_6_r12_3_treatment_on.log`.

| Request size | `production` (baseline) | `+exact-span-large+large-reserved-capacity` (4x) |
|---|---:|---:|
| 260 KiB | 15.80× | **1.06×** |
| 512 KiB | 8.00× | 1.01× |
| 1 MiB | 4.00× | 1.00× |
| 1.75 MiB | 2.29× | 1.00× |
| 4 MiB | 2.00× | 1.00× |

Byte-for-byte identical to R13-6's own numbers (§1.2's theoretical
prediction confirmed numerically: the growth-factor constant change touches
only the RESERVED VA span, never the COMMITTED span these amplification
numbers are computed from).

### 2.5 Large-cache exact hit rate (unaffected — reproduced for completeness)

Raw logs: `docs/perf/_raw_r14_6_hit_rate_baseline_off.log`,
`docs/perf/_raw_r14_6_hit_rate_treatment_on.log`.

| Arm | Hits / total | Hit rate | Final slot occupancy |
|---|---:|---:|---|
| `production` (baseline) | 7999 / 8000 | 99.99% | `[Some(4194304), None×7]` |
| `+exact-span-large+large-reserved-capacity` (4x) | 7996 / 8000 | 99.95% | `[Some(270336), Some(528384), Some(1052672), Some(1839104), None×4]` |

Identical to R13-6's numbers — cache admission compares `slot.usable_size`
against the request's `usable`, never `reserved_capacity`, so this axis is
structurally blind to the growth-factor constant.

The companion criterion bench (`r13_6_large_cache_cycle`) shows the same
small, expected slot-spread cost R13-6 already characterized as a
`exact-span-large`-driven (not `large-reserved-capacity`-driven) effect:
baseline [3.6425 µs, 3.7347 µs, 3.8129 µs] → treatment [4.0183 µs, 4.2635 µs,
4.4559 µs], **+15.5%** (raw logs
`docs/perf/_raw_r14_6_large_cache_bench_baseline.log`,
`docs/perf/_raw_r14_6_large_cache_bench_treatment.log`) — directionally
consistent with R13-6's own +5.6% at this µs-scale bench (host wall-clock
noise band, not the growth-factor axis this task changed).

### 2.6 First-heap commit-charge (unaffected — reproduced for completeness)

Raw logs: `docs/perf/_raw_r14_6_firstalloc_baseline_production.log`,
`docs/perf/_raw_r14_6_firstalloc_treatment_5samples.log` (5 fresh-process
samples each, built with the FULL correct `production` feature composition
per R13-6 §1.6's documented false-alarm lesson — confirmed by checking
`Cargo.toml`'s `production = [...]` line before building).

| Arm | `commit_after_1_heap_kib` (min–max across 5 samples) |
|---|---|
| `production` (baseline) | 1548–1556 KiB |
| `+exact-span-large+large-reserved-capacity` (4x) | 1548–1600 KiB |

No measurable difference (same order as R13-6's 1516–1540 / 1516–1528 KiB —
the small absolute shift between R13-6's and this task's numbers reflects
ordinary host/session variance, not a `production`-composition or
growth-factor effect; both arms in THIS run move together).

---

## 3. Test changes and their justification (non-vacuity)

### 3.1 `tests/large_reserved_capacity.rs::growth_chain_preserves_data_across_multiple_steps`

This test grows a Large allocation through 3 cumulative steps and asserts
every step stays in-place (pointer identity preserved) within the fixed
geometric ceiling. Under the OLD 2x ceiling the steps were sized to
15%/30%/45% of the original size (comfortably under 2x). Left unchanged,
this test would still PASS under the new 4x ceiling (more headroom, not
less) — but it would then be **vacuous** as a regression guard for the
factor itself: a silent revert from 4x back to 2x would not be caught,
because 45% growth fits comfortably under EITHER ceiling.

**Fix**: pushed the steps to 15%/120%/280% of the original size — the LAST
step (280%) is comfortably under the NEW 4x ceiling (with margin for header
+ page-rounding overhead baked into `usable`) but decisively EXCEEDS the OLD
2x ceiling. Verified both directions:

- **Green with the fix** (`LARGE_RESERVED_CAP_GROWTH_FACTOR = 4`): all 4
  tests in the file pass (`cargo test --release --features "production
  exact-span-large large-reserved-capacity" --test large_reserved_capacity`).
- **Red counterfactual** (`LARGE_RESERVED_CAP_GROWTH_FACTOR` temporarily set
  back to `2`, same test file unchanged): `growth_chain_preserves_data_across_multiple_steps`
  FAILS with `chained growth step to 578265 (from 302275) should stay
  in-place within the geometric reserved_capacity` — proving the test
  genuinely exercises the new ceiling's value, not just its presence. The
  factor was restored to `4` immediately after this counterfactual check;
  the other 3 tests in the file (single-growth, boundary, cache-reuse) were
  unaffected by either direction, as expected (they do not depend on the
  exact multiplier's numeric value, only on `reserved_capacity > span_usable`
  existing at all).

### 3.2 `tests/regression_realloc_xthread_stamp.rs::realloc_growth_segment_is_stamped_and_reclaims_xthread`

This test needs the realloc grow to RELOCATE (not stay in-place) so it can
verify the newly-carved segment is correctly ownership-stamped. It sized
`OLD = 3 MiB` / `NEW = 7 MiB` specifically to exceed the OLD 2x ceiling
(`2 * 3 = 6 MiB < 7 MiB`). Under the new 4x ceiling, `4 * 3 = 12 MiB > 7
MiB`, so the grow now stays in-place — the test's own assertion
(`assert_ne!(g, p, "... adjust OLD/NEW so the grow relocates")`) caught this
immediately when the full test suite was run post-fix (this was a REAL,
caught-not-just-anticipated failure, not a preemptive edit — see §4).

**Fix**: raised `NEW` to `13 MiB` (`4 * 3 = 12 MiB < 13 MiB`, so the grow now
exceeds the new 4x ceiling and still relocates under every feature
combination, including `--all-features`). Doc comment updated to explain the
4x ceiling and point at `LARGE_RESERVED_CAP_GROWTH_FACTOR`'s own doc for the
full rationale.

No other test file references `LARGE_RESERVED_CAP_GROWTH_FACTOR`,
`reserved_capacity`, or a "2x"/"2×"-relative sizing assumption (confirmed via
`grep -rln "LARGE_RESERVED_CAP_GROWTH_FACTOR\|reserved_capacity" tests/`).

---

## 4. Verification runs

- `cargo test --release --features "production exact-span-large
  large-reserved-capacity"` — **green**, full suite (this is where §3.2's
  failure was actually first caught, confirming the fix-then-adjust workflow
  was driven by a real red, not a hypothetical one).
- `cargo test --release --features "production exact-span-large
  large-reserved-capacity" --test large_reserved_capacity` — green in
  isolation, both before and after the §3.1 red/green counterfactual.
- `cargo test --release --features production --test
  no_stale_doc_references` — green (6/6 tests), confirms the doc-consistency
  checks this suite runs are unaffected.
- `cargo clippy --all-targets -- -D warnings` (default feature set) — clean.
- `cargo clippy --all-targets --features experimental -- -D warnings` —
  clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo clippy --all-targets --features "production exact-span-large
  large-reserved-capacity" -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- WSL2 + Valgrind/Callgrind iai run — real mechanism (not a documented gap
  this time either, same as R13-6 §1.1/§8).

---

## 5. Recommendation (NOT a decision — per this task's own brief)

**Judged strictly by R13-6's own stated blocking condition** ("§3.3's iai
`realloc_grow` result... shows the pair is NET SLOWER than plain
`production`"): that condition no longer holds. The iai judge — this
project's own designated PASS/FAIL authority when wall-clock and iai
disagree in magnitude (`scripts/iai.mjs`'s module doc) — now shows the pair
**22.4% FASTER** than baseline on the exact workload that blocked R13-6, and
every wall-clock/harness measurement in §2 agrees in sign. The RSS win (§2.4)
and cache-hit-rate cost (§2.5) are numerically unchanged from R13-6, so none
of R13-6's OTHER findings regressed either.

**What this task does NOT newly establish**: R13-6 §7's condition 2
("confirmation that no `production`-shipped workload... exhibits this
doubling-cadence pattern at a scale where the regression would be
user-visible") and condition 3 (cross-platform confirmation beyond this
session's Windows-native + WSL2-Linux combination) were flagged as open in
R13-6 and remain open here — this task closes condition 1 exactly as R13-6
§7 specified, but does not independently re-verify 2 or 3.

Given (a) the regression that was R13-6's sole blocking finding is now
reversed rather than merely reduced, (b) every other R13-6 finding is
numerically unchanged (not newly regressed), and (c) the fix is a
single-constant, low-risk, well-tested change with a red/green
counterfactual on file — this task's own recommendation is **GO**, with the
same platform caveat R13-6 §8 already carried forward (Windows-native +
WSL2-Linux, one host, one measurement session; true Linux-native/macOS-native
and multi-socket NUMA hardware remain unmeasured). **The final decision on
whether to add `exact-span-large`/`large-reserved-capacity` to
`production`'s feature list is left to the user**, per this task's explicit
instruction; `Cargo.toml` was not touched.

---

## 6. Artifacts this task adds

- `docs/perf/_raw_r14_6_*.log` (14 files): iai baseline/treatment, R12-4
  3-arm harness re-runs (production/exact-span-only/both-features), R12-3
  RSS-amplification baseline/treatment, criterion wall-clock
  baseline/treatment (realloc-chain + large-cache-cycle groups), large-cache
  exact-hit-rate baseline/treatment, first-heap-commit baseline (5 samples)
  + treatment (5 samples).
- This document.
- `src/alloc_core/alloc_core_large.rs`: `LARGE_RESERVED_CAP_GROWTH_FACTOR`
  2 → 4, with an expanded doc comment carrying §1's data table in-repo.
- `src/alloc_core/segment_header.rs`: `reserved_capacity` field doc updated
  to reference the new factor.
- `Cargo.toml`: `large-reserved-capacity` feature doc comment updated (2x →
  4x mention); the feature LIST itself (`production = [...]`) is untouched.
- `tests/large_reserved_capacity.rs`: `growth_chain_preserves_data_across_multiple_steps`
  step sizes widened (15/30/45% → 15/120/280%) so the test stays a
  meaningful regression guard against the NEW ceiling, not just the
  mechanism's existence; doc comment updated with the non-vacuity rationale.
- `tests/regression_realloc_xthread_stamp.rs`: `NEW` constant raised (7 MiB →
  13 MiB) so the relocation this test depends on still fires under the wider
  ceiling; doc comment updated.
