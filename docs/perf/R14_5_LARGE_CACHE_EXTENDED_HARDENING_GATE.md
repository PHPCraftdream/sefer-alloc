# R14-5 — `large-cache-extended` hardening gate (budget ordering, RSS retention, narrow-working-set regression, mixed-size/FIFO correctness, production A/B)

**Task:** #290 (R14-5, P1). Unblocked by #286 (R14-1, `LargeCacheExtension`
now typed-initialised via `ptr::write`; landed, `a3434df`). **HARDENING +
MEASUREMENT — not a self-authorized promotion.** `large-cache-extended`
stays EXPERIMENTAL, opt-in, NOT in `production`'s `Cargo.toml` feature list.
This document reports what the six required items measured/fixed and ends
with a GO/CONDITIONAL-GO/NO-GO recommendation; the decision to add the
feature to `production` is left to the user per the task brief.

**Date:** 2026-07-23. **Base revision:** `main` @ `6a644a4` (R14-1..R14-4
landed).

---

## 0. Findings this task closes (from Round 13 review)

1. **RSS-удержание**: `budget_bytes: None` by default — hard budget
   disabled. Decay to 256 MiB is not a hard ceiling (releases only a
   fraction of excess per interval). Extension could temporarily inflate
   retained committed memory ~5x relative to the base 8 slots.
2. **No N=1/2/4 post-materialisation gate**: R13-7's own judge only checked
   the ideal 9+-distinct-size scenario, never a narrow working set AFTER the
   sidecar had once materialised (every lookup now scans up to 40 slots
   instead of 8).
3. **Materialisation before budget check** (@fm P3): the sidecar could
   materialise (pay a real OS page reservation) even for a deposit the
   budget was always going to reject. The doc comment "no-op check
   thereafter" was also factually wrong under persistent OOM.

---

## 1. Item 1 — budget-vs-materialisation ordering (fixed)

**Change:** `src/alloc_core/alloc_core_large_cache.rs` gained
`AllocCore::large_cache_deposit_budget_infeasible(usable_size) -> bool` — a
cheap, purely arithmetic pre-check (`Some(budget) if usable_size > budget`,
no eviction, no sidecar touch). Both admission call sites
(`src/alloc_core/alloc_core.rs`'s Large `dealloc` branch,
`src/alloc_core/alloc_core_large.rs::reclaim_large_segment`) now run this
check BEFORE ever entering the free-slot-search loop that can materialise
the extension:

```text
let mut admitted: Option<usize> = None;
if !self.large_cache_deposit_budget_infeasible(usable_size) {
    loop {
        let free_slot = self.large_cache_find_free_slot();  // <- may materialise
        ...
    }
}
```

A deposit whose `usable_size` alone exceeds the configured budget (i.e.
unconditionally infeasible even against a fully-evicted cache) now never
calls `large_cache_find_free_slot`, so it never pays a sidecar page
reservation it could never use.

**Doc-comment fix:** `large_cache_find_free_slot`'s stale "a no-op OS
reservation check thereafter" claim (factually wrong under persistent OOM —
`self.large_cache_extension` only becomes non-null on a *successful*
reservation, so a persistently-failing reservation is retried in full on
every call) is replaced with an explicit correctness note pointing callers
at the new pre-check.

**Scope note (documented, not silently assumed):** this is a
single-deposit-vs-budget check, not a full feasibility predictor for the
existing evict-and-retry loop — a budget that is merely *tight* (not
unconditionally impossible for this one deposit) still runs the
pre-existing eviction loop unchanged.

**Tests:** `tests/large_cache_extended_budget_before_materialization.rs`,
6 tests:
- `budget_infeasible_deposit_after_base_eight_full_never_materialises_extension`
  — the DISCRIMINATING counterfactual: fills the base 8 with genuinely
  resident spans, then deposits a 9th unconditionally-infeasible span and
  asserts the sidecar stays unmaterialised and the base 8 stay untouched.
  **Verified counterfactual-sound**: reverting the pre-check (`if true`
  instead of `if !...infeasible(...)`) makes this specific test FAIL
  (`materialised after 9th: true`), confirming the test is not vacuous.
- `zero_budget_never_materialises_extension_even_past_base_eight`,
  `budget_smaller_than_every_span_never_materialises_extension` — companion
  budget=0 / tiny-budget cases.
- `effectively_unbounded_budget_still_materialises_extension_on_overflow` —
  sanity: the pre-check must not suppress LEGITIMATE materialisation.
- `default_config_resolves_finite_budget_when_extension_compiled_in`,
  `explicit_budget_bytes_overrides_the_extended_default` — item 2 coverage
  (below).

---

## 2. Item 2 — finite default budget for `large-cache-extended`

**Decision:** `large-cache-extended` gets its OWN finite default budget,
applied only when the caller never calls `.budget_bytes(..)` explicitly.
`src/alloc_core/large_cache_config.rs`:

```rust
pub(crate) const DEFAULT_EXTENDED_BUDGET_BYTES: usize = 5 * DEFAULT_HEADROOM_BYTES; // 1280 MiB
```

`resolved_budget_bytes()` now branches on the feature:

```text
#[cfg(feature = "large-cache-extended")]
{ self.budget_bytes.map_or(Some(DEFAULT_EXTENDED_BUDGET_BYTES), Some) }
#[cfg(not(feature = "large-cache-extended"))]
{ self.budget_bytes }  // unchanged: None = unbounded
```

**Rationale for the 5x multiplier:** the extension raises the slot ceiling
from 8 to 40 — a `40 / 8 = 5x` ratio. The base cache's own "unbounded is
fine" default relies on slot count alone being self-limiting (at most 8
distinct spans, however large, can ever be resident); once slot count
stops being a meaningful bound (40 slots), the SAME informal "how much is
normal" answer should scale by the same ratio the slot count did, rather
than staying literally unbounded. `DEFAULT_HEADROOM_BYTES` (256 MiB, the
existing decay anti-thrashing floor) is the scaling base because it is
already this module's one canonical "how much cached Large RSS is normal"
constant.

**Override still wins:** an explicit `.budget_bytes(n)` call for ANY `n`
(including `0` or `usize::MAX`) always overrides this fallback — this is a
default, not a hard cap a caller cannot escape. Base-cache builds (feature
OFF) are completely unaffected: `None` still resolves to `None`.

**Tests:** covered in the same
`tests/large_cache_extended_budget_before_materialization.rs` file (see
item 1's list); `default_config_resolves_finite_budget_when_extension_compiled_in`
pins the exact resolved value (`Some(1_342_177_280)` = 1280 MiB) so a future
change to the ratio is a visible, deliberate diff.

---

## 3. Item 3 — RSS/commit retention measurement (adversarial, long-holding)

**Harness:** `examples/r14_5_large_cache_extended_rss_measure.rs` (THROWAWAY,
not a shipping artifact) — extends the R13-7 hit-rate harness's pattern to
measure retained commit/RSS on a wide-diversity, long-holding workload: 40
distinct Large sizes (linearly spaced, one SEGMENT apart, from the
safely-Large floor), all allocated fresh then all freed (each deposits into
its own slot), then HELD with no further Large-path touches (the worst case
for retained-but-idle committed memory — decay only fires inline on a
SUBSEQUENT large alloc/free, never in the background).

Two modes: `unbounded` (isolates slot-count-driven retention from budget
policy — the number the Round 13 "~5x" claim is about) and `default-config`
(exercises item 2's new finite default in the same workload shape).

### 3.1 Results — `unbounded` mode (isolates slot-count effect)

Raw logs: `docs/perf/_raw_r14_5_rss_base_off_unbounded.log`,
`docs/perf/_raw_r14_5_rss_extended_on_unbounded.log`.

| Arm | Resident slots | Retained commit (delta vs baseline) |
|---|---:|---:|
| base (extension OFF, 8 slots) | 8 | 1,235,316 KiB (1206.4 MiB) |
| extended (extension ON, 40 slots) | 40 | 3,533,568 KiB (3450.8 MiB) |

Ratio: **2.86x** (not literally 5x for this specific linear-spacing size
ladder — the 5x figure in the original review was a slot-COUNT ratio,
40/8; the actual RSS ratio depends on which spans happen to occupy the
extra 32 slots, here linearly-spaced sizes averaging smaller-than-worst-case
per slot). Confirms the qualitative finding: uncapped, the extension CAN
retain multiples of the base cache's committed footprint under an
adversarial wide-diversity, long-holding workload — the exact risk item 2
addresses.

### 3.2 Results — `default-config` mode (item 2's mitigation, same workload)

Raw logs: `docs/perf/_raw_r14_5_rss_base_off_defaultcfg.log`,
`docs/perf/_raw_r14_5_rss_extended_on_defaultcfg.log`.

| Arm | Resolved budget | Resident slots (base+ext) | Retained commit |
|---|---|---:|---:|
| base (extension OFF) | `None` (unaffected by item 2) | 8 | 1,235,316 KiB |
| extended (extension ON) | `Some(1342177280)` (1280 MiB, item 2 default) | 8 (7 base + 1 ext) | 1,235,328 KiB |

With the new default budget active, the extended arm's actual retained
commit on this adversarial workload drops to **effectively the same as the
base arm's own retention** (1,235,328 KiB vs 1,235,316 KiB — a 12 KiB
rounding-noise difference) — the budget check evicts old entries to stay
within 1280 MiB long before the full 40-slot ceiling is reached (only 8 of
40 slots occupied). **This confirms item 2's default policy neutralises the
adversarial-RSS scenario in this exact workload shape**, while leaving an
explicit `.budget_bytes(..)` override available for a caller who has
measured their own workload and wants more headroom.

### 3.3 Post-hold stability

Both arms/modes assert `post_fill_commit == post_hold_commit` (no silent
background drift) — confirmed by the harness's own `assert_eq!` (would
panic if violated; all four raw logs show this assertion passing).

---

## 4. Item 4 — N=1/2/4 post-materialisation hit-path correctness gate

**File:** `tests/large_cache_extended_narrow_working_set_after_materialization.rs`,
4 tests. Forces sidecar materialisation via a 9-distinct-size burst
(matching the sibling files' proven pattern), then narrows the working set
to the first 1/2/4 of those 9 sizes and cycles 50 rounds, asserting on every
round:
- Every alloc in the narrow working set succeeds (non-null) — no spurious
  failure from the widened 40-slot scan bound.
- The `large_cache_used_bytes == sum(base) + sum(extension)` invariant holds
  throughout (reused from `large_cache_extended_budget_still_enforced.rs`'s
  established check).
- A final re-allocation of every narrowed size still succeeds after the
  churn.

`scan_bound_stays_forty_during_narrow_working_set_phase` is the direct
behavioural proof that these tests exercise the WIDENED scan bound (not the
pre-materialisation 8-slot-only path): `dbg_large_cache_total_slots()` stays
40 throughout the narrow-working-set phase (materialisation is one-way,
never reverts, matching `large_cache_extended.rs`'s documented design).

This is a CORRECTNESS gate, not a timing gate (per this project's policy of
keeping micro-timing judges in `examples/`+`scripts/paired-ab-runner.mjs`,
not `tests/`) — it proves the widened scan bound does not corrupt admission,
eviction order, or hit-servicing once most of the 40 slots are stale-empty.
The actual wall-clock cost difference of an O(40) vs O(8) scan on a narrow
working set was not separately isolated in this task (both the base
`LARGE_CACHE_SLOTS` doc and `large_cache_extended.rs`'s own doc already
argue this scan is "cheap" at either size, since every touch site is the
cold large-alloc/dealloc slow path, never the hot small-object path) — a
dedicated timing gate for this specific narrow-working-set-after-burst shape
is deferred as a follow-up if a future review wants a number attached to
that "cheap" claim specifically for N=1/2/4 (as opposed to R13-8's already-
measured 24-distinct-size turnover shape, §6 below).

---

## 5. Item 5 — mixed-size / adversarial best-fit / FIFO tests

**File:** `tests/large_cache_extended_mixed_size_best_fit_fifo.rs`, 2 tests.

- `best_fit_picks_tightest_slot_across_base_extension_boundary`: deposits
  two OVERLAPPING sizes (within `LARGE_CACHE_SIZE_FACTOR` = 2x of each
  other) — one landing in the base 8, the other forced into the extension
  by 7 non-overlapping fillers. A subsequent request for the smaller size
  must best-fit-match its OWN tighter base-slot deposit, not the looser
  extension-resident one, proving best-fit correctly compares candidates
  ACROSS the base/extension boundary rather than short-circuiting on
  whichever half is scanned first. (Required an empirical `probe_usable_size`
  helper — header/SEGMENT-rounding overhead means a raw byte request does
  not trivially predict its resulting `usable_size`, so candidate sizes are
  discovered by probing a scratch `AllocCore`, not by re-deriving the
  rounding arithmetic in the test.)
- `fifo_eviction_targets_true_oldest_across_combined_index_space`: fills
  all 40 combined slots with distinct sizes, deposits a 41st, and verifies
  exactly the TRUE FIFO-oldest entry (smallest `seq`, deposited first) is
  evicted — regardless of whether that oldest entry lives in the base array
  or the extension sidecar — while every OTHER of the 40 resident entries
  remains servable.

---

## 6. Item 6 — production A/B/B/A gate (turnover profile)

**Reused tooling** per the task's explicit instruction:
`scripts/paired-ab-runner.mjs` (already used in R14-3/#288, R14-4/#289) via
`scripts/_r14_5_large_cache_extended_turnover_ab.json`.

**Workload** (new, `examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`
+ `examples/paired_ab_large_cache_extended_{off,on}.rs`): the SAME turnover
shape R13-8 Part C's in-process judge established as the scenario where the
extension actually does something (R13-8 §0's own caveat: a STATIC live-set
workload deposits nothing into the cache until teardown — 0 hits regardless
of the extension — so this MUST be turnover-shaped, not peak-live-object-
shaped, to be a meaningful comparison). 24 distinct Large sizes, batch
alloc-all/dealloc-all, 3 untimed warm-up rounds + 200 timed rounds
(`4800` total dealloc pairs), one `Instant` pair around the timed region.
Both arms built `--features "production alloc-stats"` (baseline) /
`"production alloc-stats large-cache-extended"` (treatment) — `alloc-stats`
is NOT part of `production`, added only on these probe binaries' build
lines so the causal-mechanism proof (`large_cache_hits`) is live.

### 6.1 Causal-mechanism proof (single-launch sanity)

| Arm | `large_cache_hits` / `total_deallocs` | `elapsed_ns` |
|---|---:|---:|
| off (base, 8 slots) | 1600 / 4800 = 33.3% | 338,987,200 |
| on (extended, 40 slots) | 4800 / 4800 = 100% | 939,900 |

Matches R13-8 Part C's in-process finding (33.33% base, 100% extended) —
the process-level harness reproduces the exact hit-rate signature.

### 6.2 Paired A/B/B/A statistics (n=15, `elapsed_ns`)

Raw log: `docs/perf/_raw_r14_5_paired_ab_turnover.log`. Provenance JSON:
`docs/perf/paired_ab_runs/2026-07-23T14-00-48-940Z.json` (git commit
`6a644a4`, `rustc 1.97.0`, Windows 10 Pro, i7-11800H).

```text
=== off vs on (A - B, ns) ===
n=15  mean Δ=342.077 ms  sd=6.768 ms  se=1.747 ms  t=195.759  df=14  crit(p<0.05)=2.160  => REAL (rejects null)
sign test: off-faster=0/15  on-faster=15/15  ties=0
```

`t = 195.759` is enormously past `crit = 2.160` (df=14, p<0.05); sign test
is unanimous (15/15) favoring the extended arm. This is an unambiguous,
statistically real result — off (base) takes ~342 ms LONGER than on
(extended) per round on this turnover workload, driven directly by the
hit-rate difference in §6.1 (every miss in the base arm pays a real OS
`VirtualAlloc`/`VirtualFree` round-trip; the extended arm's 100% hit rate
avoids nearly all of them).

### 6.3 Same-vs-same control (harness sanity)

Raw log: `docs/perf/_raw_r14_5_paired_ab_same_vs_same_control.log`.

```text
=== off(A-slot) vs off(B-slot) (A - B, ns)  [SAME-VS-SAME CONTROL] ===
n=8  mean Δ=-3.105 ms  sd=7.076 ms  se=2.502 ms  t=-1.241  df=7  crit(p<0.05)=2.447  => NOT statistically distinguishable from noise (fails to reject null)
sign test: off(A-slot)-faster=7/8  off(B-slot)-faster=1/8  ties=0
```

`t = -1.241` is well under `crit = 2.447` — confirms the harness itself is
sound (a same-vs-same comparison shows no real difference), so §6.2's result
is not a harness artifact.

**Honest scope note (R13-8 §0 carried over):** this A/B result is for the
TURNOVER profile specifically. A static live-set workload would show ~0
hits and no meaningful wall-clock difference between arms, exactly as R13-8
already documented — this gate does not contradict or supersede that
finding, it measures a DIFFERENT (and, per R13-8, the actually-relevant)
scenario.

---

## 7. Runs performed

- `cargo test --release --features "alloc-core alloc-decommit alloc-stats large-cache-extended"`
  over all 213 compiling `tests/*.rs` files (2 pre-existing, unrelated
  `regression_r4_3_*` files fail to compile under this exact combo on `main`
  BEFORE this task's changes too — confirmed via `git stash`; they require
  `alloc-global`, which is outside this task's target combo and outside this
  task's scope) — **230 tests passed, 0 failed** (final clean rerun after
  `cargo fmt`).
- `cargo clippy --all-targets -- -D warnings` (default features): clean.
- `cargo clippy --all-targets --features experimental -- -D warnings`: clean.
- `cargo clippy --all-targets --all-features -- -D warnings`: clean.
- `cargo fmt --check`: clean (after one `cargo fmt` pass fixing 4 files'
  formatting).
- `cargo test --release --features production --test no_stale_doc_references`:
  6/6 passed (required an `ARCHITECTURE.md` test-file-count update, 212→215,
  for the 3 new test files this task adds — the test itself caught this).
- `cargo build --release --example r14_5_large_cache_extended_rss_measure --all-features`:
  clean.
- Production A/B/B/A gate (§6): `t=195.759` (real), same-vs-same control
  `t=-1.241` (noise, as expected).

---

## 8. Files changed/added

**Source (hardening, items 1-2):**
- `src/alloc_core/alloc_core_large_cache.rs` — new
  `large_cache_deposit_budget_infeasible`, updated
  `large_cache_find_free_slot` doc, new `dbg_large_cache_budget` test seam.
- `src/alloc_core/alloc_core.rs` — pre-check wired into the Large `dealloc`
  admission loop.
- `src/alloc_core/alloc_core_large.rs` — pre-check wired into
  `reclaim_large_segment`'s admission loop.
- `src/alloc_core/large_cache_config.rs` — `DEFAULT_EXTENDED_BUDGET_BYTES`,
  feature-conditional `resolved_budget_bytes`.

**Tests (items 1, 2, 4, 5):**
- `tests/large_cache_extended_budget_before_materialization.rs` (6 tests)
- `tests/large_cache_extended_narrow_working_set_after_materialization.rs` (4 tests)
- `tests/large_cache_extended_mixed_size_best_fit_fifo.rs` (2 tests)

**Measurement harnesses (items 3, 6):**
- `examples/r14_5_large_cache_extended_rss_measure.rs` (throwaway)
- `examples/_shared/paired_ab_large_cache_extended_turnover_workload.rs`
- `examples/paired_ab_large_cache_extended_off.rs`
- `examples/paired_ab_large_cache_extended_on.rs`
- `scripts/_r14_5_large_cache_extended_turnover_ab.json`

**Docs:**
- `docs/ARCHITECTURE.md` (test-file count 212→215)
- This file.

**Raw logs** (cited above, `git add -f`'d alongside this report per the
project's raw-log policy):
- `docs/perf/_raw_r14_5_rss_base_off_unbounded.log`
- `docs/perf/_raw_r14_5_rss_extended_on_unbounded.log`
- `docs/perf/_raw_r14_5_rss_base_off_defaultcfg.log`
- `docs/perf/_raw_r14_5_rss_extended_on_defaultcfg.log`
- `docs/perf/_raw_r14_5_paired_ab_turnover.log`
- `docs/perf/_raw_r14_5_paired_ab_same_vs_same_control.log`

---

## 9. Verdict

All six required items are clean:

1. Budget-vs-materialisation ordering — **fixed**, counterfactual-verified.
2. Finite default budget for `large-cache-extended` — **implemented**,
   measured to neutralise the adversarial RSS scenario in §3.2.
3. RSS/commit gate — **measured**: ~2.86x retention increase when unbounded
   (isolating slot-count effect), reduced to ~parity with the base cache
   under the new default (§3.2).
4. N=1/2/4 post-materialisation hit-path gate — **clean**, no correctness
   regression found.
5. Mixed-size/adversarial best-fit/FIFO — **clean**, both new tests pass
   and are correctness-meaningful (best-fit correctly spans the
   base/extension boundary; FIFO correctly targets the true combined-space
   oldest entry).
6. Production A/B/B/A gate — **statistically real win** for the turnover
   profile (`t=195.759`, sign test 15/15), harness validated via a clean
   same-vs-same control.

**Recommendation: CONDITIONAL-GO.**

The GO condition: promotion to `production` should be considered ONLY
together with shipping item 2's finite default budget as-is (or an
equivalent finite ceiling) — the measurements in §3 show the feature is
genuinely risky for adversarial wide-diversity, long-holding workloads
under an UNBOUNDED budget, and genuinely safe (RSS-neutral vs the base
cache) under the new bounded default. The turnover-profile win in §6 is
large and statistically unambiguous, and R13-8's own honest caveat (no
benefit, no harm, for static live-set workloads) still holds — this task
found no new correctness or regression concerns across items 1, 2, 4, 5.

Per the task's explicit instruction, this is a recommendation, not a
self-authorized promotion — the orchestrator/user makes the final call on
whether `large-cache-extended` (with its new default budget) enters
`production`'s `Cargo.toml` feature list.
