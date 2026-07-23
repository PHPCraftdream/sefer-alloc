# Round 13 — production A/B, double-checked wave summary

**Task:** #280 (R13-10). This is a **SUMMARY** report — it re-states and
cross-references numbers already measured and gated by R13-6/R13-8/R13-9's
own documents; it does not re-measure anything new. Its purpose is the thing
R13-10's own brief asks for: one place that answers "what actually changed
in `production` this round, and what is the before/after evidence for it,"
in an A/B, double-checked shape (production BEFORE the whole Round 13 wave,
vs AFTER, with each side's own already-gated numbers restated once for a
stable double-check, not a fresh remeasurement). **Naming note
(R14-10/task #295):** an earlier version of this title/intro called this
shape "A/B/B/A," which is the name of a DIFFERENT, stricter protocol (genuine
alternating interleaved runs within one session — see
`docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` §2.2 for an actual
A/B/B/A judge); §2 below explains the distinction where it matters most.

**Date:** 2026-07-23. **Round 13 span:** `e2d84f7`..`da77b38` (R13-1 through
R13-12, tasks #271-#280 minus this document itself). **Baseline commit for
"before":** `f9a9f0d` (Round 12's own CHANGELOG entry, the last commit before
Round 13 started). **"After" commit:** `da77b38` (R13-9's `production`
promotion, the last `production`-affecting commit this round).

---

## 1. What actually changed in `production` this round

**Exactly one feature was promoted into `production` in Round 13:
`class-aware-dirty` (R13-9, task #279, commit `da77b38`).** Everything else
that shipped this round is either a correctness fix to code already inside
`production` (no feature-list change) or a new/extended opt-in feature that
was explicitly measured and NOT promoted.

```text
# Cargo.toml `production = [...]`, before Round 13 (post-Round12, f9a9f0d):
production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
              "alloc-segment-directory", "primordial-lazy-commit"]

# Cargo.toml `production = [...]`, after Round 13 (da77b38):
production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
              "alloc-segment-directory", "primordial-lazy-commit",
              "class-aware-dirty"]
```

## 2. Production A/B, double-checked — class-aware-dirty (the one promoted feature)

**Naming correction (R14-10/task #295):** this section was originally
titled "Production A/B/B/A," which overpromises — the genuine A/B/B/A
protocol (alternating interleaved runs within one measurement session,
guarding against monotonic host-state drift) is what R14-3's dedicated
process-level judge actually performs
(`docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` §2.2, 20 pairs of 4
alternating launches each). What THIS section does is narrower: **A** =
`production` before the wave (class-aware-dirty OFF), **B** = `production` +
`class-aware-dirty` (the treatment), with B's own numbers **restated a
second time from the same gate document's own two internal sections (§3 and
§9)** as a citation double-check, not a second independent measurement pass
— an "A/B, double-checked" shape, not an interleaved A/B/B/A run. All
numbers below are restated from
`docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` (measured 2026-07-23,
base revision `874650b`) — **not re-measured for this summary**, per this
document's own scope (a wave summary, not a new gate).

| Axis | A: `production` (class-aware-dirty OFF) | B: `production` + `class-aware-dirty` (ON) | B (double-check, same gate doc §3/§9) | A (unaffected, confirmed unchanged) |
|---|---|---|---|---|
| Remote fan-in wall-clock, N=8 producer classes, SUB-WINDOW `ns/owner_alloc` | 23,527.4 ns | **1,083.9 ns (21.71× faster on this sub-window metric)** | Re-measured on top of R13-1's latch fix, inside R12-7's own pre-latch 19.7-32.4× range — the latch does not erode the win | N/A — this axis does not exist with the feature off |
| Remote fan-in wall-clock, N=8, FULL ROUND (criterion mean, same harness) | ~20.6 ms | **~18.4 ms (~11% faster)** | **R14-3 correction (task #288):** the sub-window row above is NOT the round-level speedup — criterion's own full-round mean (same raw logs) moved only ~11% at N=8, ~1.6% at N=4; the sub-window's larger reduction reflects drain work moving into the unmeasured pre-alloc/recycle portion of the round, not disappearing. See `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`. | N/A |
| Remote fan-in, N=1→N=4 delta | +89.2% (722.7→1367.3 ns) | **+35.6% (722.1→979.2 ns) — flattened** | Consistent with R12-7's own re-measurement | — |
| iai, 12 non-remote single-thread benches (Ir) | baseline (see `IAI_BASELINE.md`) | **+0.00% to +0.02%** | Confirmed a fixed +5 Ir per bench regardless of workload shape — "feature compiled in, code path never reached," not a per-call cost | Confirmed unchanged — feature is remote-drain-only |
| iai, same 12 benches, Estimated Cycles | baseline | **+0.00% to +0.35%** | Within noise | — |
| Sidecar RSS, per materialised heap | 0 (feature absent) | **8.0 KiB (2 pages)** | Corrects R12-7's own doc, which cited the raw un-page-rounded 6.1 KiB `size_of` figure | — |
| CI feature-isolation row (`production alloc-stats`, no `numa-aware` — R14-10/#295: the `class-aware-dirty` token this row's name originally spelled out was dropped from `ci.yml` as a no-op once R13-9 promoted the feature into `production` itself) | — | **green** | Re-run personally on current HEAD, byte-for-byte the `.github/workflows/ci.yml` job | — |

**Verdict this round acted on:** GO (R13-9's own §7 recommendation),
user-confirmed via `AskUserQuestion` before the `Cargo.toml` edit. The
promotion comment at `production`'s definition in `Cargo.toml` cites these
same numbers at the point of use.

## 3. What stayed opt-in this round (measured, not promoted)

| Feature(s) | Task | Gate doc | Verdict | Why not promoted |
|---|---|---|---|---|
| `exact-span-large` + `large-reserved-capacity` | R13-6 (#276) | `docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` | **CONDITIONAL-GO** | iai `realloc_grow` (64B→4MiB, 16 doublings): **+102.3% instructions, +52.7% Estimated Cycles** — a real, deterministic regression on a doubling-cadence realloc workload (the pair's own fixed 2× `reserved_capacity` ceiling re-trips almost every doubling step). The RSS win (15.80×→1.06× at 260 KiB) is real and unregressed, and `large-reserved-capacity` does recover 2 of the 4 in-place-realloc legs `exact-span-large` alone loses — but the iai regression is large enough, and deterministic enough, that shipping unconditionally was not recommended. §7 of the gate doc lists the concrete follow-up (widen or make adaptive the `LARGE_RESERVED_CAP_GROWTH_FACTOR` 2× ceiling) that would need to land before revisiting GO. |
| `large-cache-extended` | R13-7 (#277) | no dedicated gate doc — numbers in the module doc (`src/alloc_core/large_cache_extended.rs`) + R13-8's judge (`docs/perf/R13_8...`, confirms it is irrelevant to a static live-set workload) | **not a promotion candidate this round** | The task brief for R13-7 scoped it as "extend the cache," not "gate it for production" — no A/B production gate was run against it, so there is no promotion evidence to act on either way. Its own turnover-workload judge (88.89%→100.00% hit rate, ~99× ns/op win on a genuine 9-distinct-size overflow workload) is real but narrow (R13-8 separately confirmed 0 hits on a static 256-2048 live-object workload — the cache only helps turnover-shaped access patterns). A future round could scope a real production gate for it the way R13-6 did for `exact-span-large`/`large-reserved-capacity`. |

## 4. Correctness bugs closed this round (P0/P1) — default-`production` runtime effect vs opt-in-only

**R14-10 (task #295) correction:** the original version of this section
described all five fixes below as "inside already-`production` code" without
qualification. That is true for exactly TWO of the five (R13-1, R13-12) —
R13-2's fix lives entirely behind `#[cfg(feature = "numa-aware")]` and
R13-3's behind `#[cfg(feature = "virgin-zero-skip")]`, and **neither
`numa-aware` nor `virgin-zero-skip` is in `production`'s feature list**
(`Cargo.toml`: `production = ["alloc-global", "alloc-xthread",
"alloc-decommit", "fastbin", "alloc-segment-directory",
"primordial-lazy-commit", "class-aware-dirty"]`). A default `--features
production` build compiles OUT both fixed code paths entirely — the fixes are
real and land correctly in code that ships (once a user separately opts into
those features), but they carry ZERO default-`production` runtime effect.
None of the five changed `Cargo.toml`'s feature list themselves; they are
listed here because the wave's "what changed for a `production` build"
question needs a complete, honestly-scoped answer, not just the one
feature-list line in §1.

| Task | Severity | Commit | Production runtime effect | Summary |
|---|---|---|---|---|
| R13-1 (#271) | P0 | `e2d84f7` | **Default `production`: YES** — gated on `class-aware-dirty`, which R13-9 promoted into `production` this same round, so every default `production` build ships this fix. | Closed a lost-signal gap in `class-aware-dirty`'s OOM-transition: a coarse-only latch ensures a sidecar-OOM push and a later successful materialisation can never silently diverge. Loom-verified (7 tests). Landed BEFORE R13-9's promotion — the promoted feature already includes this fix. |
| R13-2 (#272) | P1 | `a3434df` | **Default `production`: NO** — the entire fix (`active_bits_by_node`, the `clear_bit` correction) is `#[cfg(feature = "numa-aware")]`; `numa-aware` is opt-in, not in `production`. Only reachable if a user separately builds with `--features numa-aware`. | NUMA directory bucket-slot reuse: an `active_bits_by_node` counter frees a slot once every bit a node ever set returns to 0, preventing slot exhaustion under long-running bucket churn across 9+ nodes. Also fixed a second, independently-found defect (`clear_bit` was using the registering `node_bucket_mut` instead of read-only `node_bucket`). |
| R13-3 (#273) | P1 (upgraded from perf to a resource-retention defect) | `9886780` | **Default `production`: NO** — the fix (`PerClass::virgin_mask`, the rethreaded magazine path) is `#[cfg(feature = "virgin-zero-skip")]`; `virgin-zero-skip` is opt-in, not in `production`. Only reachable if a user separately builds with `--features virgin-zero-skip`. | Threaded virgin-zero-skip through the magazine (`PerClass::virgin_mask`) instead of bypassing it — this recovered not just the tcache fast path but also the drain prelude (`drain_heap_overflow`), which a calloc-only workload had been silently never running. A real resource-retention bug, not merely a missed optimisation. |
| R13-11 (#284, found mid-verification of R13-1) | P0 | `da037f2` | **N/A — test-only.** Root-caused to a bug in `class_aware_dirty_routing.rs`'s own test assertion, not in any `src/` production code path; fixes test-suite trustworthiness, not runtime behaviour. | A deterministic (not flaky) lost-wakeup test failure in `class_aware_dirty_routing.rs`, reproducible even on the original R12-7 commit — root-caused to a TEST bug (a small_cur refill-batch leftover masking the intended cross-thread-reclaim path), not a production defect. Fixed via a burn-down loop before R13-1's own measured assertion could be trusted. |
| R13-12 (#285, found mid-verification of R13-3) | P1 | `e7617d1` | **Default `production`: YES** — the two gated call sites are inside `drain_heap_overflow` (`alloc-xthread`) and the fix itself is `#[cfg(feature = "alloc-segment-directory")]`; both `alloc-xthread` and `alloc-segment-directory` are in `production`, so the compile-error class this closes was reachable from a `production`-adjacent feature combination (`alloc-xthread`+`fastbin`+`alloc-decommit` without `alloc-segment-directory`). | A genuine pre-existing compile error (`alloc-xthread`+`fastbin`+`alloc-decommit` without `alloc-segment-directory` → E0599 in `drain_heap_overflow`), confirmed via `git stash` to predate R13-3 entirely. Fixed by gating the two call sites, mirroring the existing pattern at every sibling call site. |

**Net: exactly TWO of these five fixes (R13-1, R13-12) have any effect on a
default `--features production` build; R13-2 and R13-3 are real, correct
fixes to opt-in feature code (worth having — they close genuine defects that
would bite any user of `numa-aware`/`virgin-zero-skip`), and R13-11 is a
test-only correction. §6 restates this distinction in the wave's net-effect
summary.**

## 5. Process/documentation corrections (not code, but part of the wave)

- R13-4 (#274, `6018cf8`): page-run's verdict corrected from "SUPERSEDED" to
  "DEFERRED — no demonstrated production victim yet" — both `exact-span-large`
  and `medium-classes-wide` are still opt-in, so `production` gets no RSS
  benefit from either yet (an external review had flagged the prior wording
  as overclaiming).
- R13-5 (#275, `0f3b608`): feature-isolated CI rows (the exact combinations
  that would have caught R13-11/R13-12's bugs earlier), `loom_class_aware_dirty.rs`
  wired into CI (was silently never running), plus a structural guard
  (`tests/no_stale_loom_files.rs`) against a repeat.
- R13-8 (#278, `874650b`): a judge on 256-2048 simultaneously-live 260 KiB-2 MiB
  objects found a real, 100%-reproducible `MAX_SEGMENTS` wall at exactly 1023
  live Large objects in every feature arm — this updates R13-4's "no
  demonstrated victim" verdict for THIS specific size band (though
  `exact-span-large` already closes the RSS/commit side of it, and there is
  no non-linear wall-clock cost approaching the wall).

## 6. Net effect on a default `--features production` build

- **One feature promoted** (`class-aware-dirty`): up to ~20× owner-allocation
  SUB-WINDOW throughput win on cross-thread remote-free-heavy workloads (N≥4
  concurrent producer classes sharing a heap) — the sub-window metric this
  wave's own gate doc headlined. The end-to-end, full-round wall-clock
  improvement for the same fixed amount of work is a low double-digit
  percentage (~11% at N=8, ~1.6% at N=4, criterion's own full-round mean —
  see the R14-3 correction, task #288,
  `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`), because most of the
  sub-window's apparent savings is deferred drain work moving into the
  unmeasured pre-alloc/recycle portion of the round rather than disappearing.
  Effectively zero cost (+0.00-0.02% Ir) everywhere else, ~8 KiB RSS sidecar
  per materialised heap. **Future-optimization note (R14-3, not implemented):**
  the deferred drain work behind the sub-window/full-round gap is a candidate
  for a future round to shrink in TOTAL (e.g. batching/amortizing reclaim —
  coalescing `sync_directory_for_segment_classes` per-segment instead of
  per-block, or batching the recycle-time drain), not merely move within the
  round as it does today.
- **Five correctness fixes shipped this round; the default-`production`
  runtime effect is TWO of them, not five (R14-10/#295 correction — see §4
  for the full per-fix table).** R13-1 (OOM-transition visibility latch,
  gated on `class-aware-dirty` which is now in `production`) and R13-12 (the
  `alloc-xthread`+`alloc-segment-directory` compile gap) genuinely affect
  every default `--features production` build. R13-2 (NUMA bucket-slot
  reuse) and R13-3 (calloc resource retention via virgin-zero-skip) are real,
  correctly-fixed defects, but their entire fix is gated behind
  `numa-aware`/`virgin-zero-skip` respectively — NEITHER feature is in
  `production`, so a default build compiles both fixed code paths out
  entirely and gets zero effect from them until a user separately opts in.
  R13-11 is a test-suite-only correction with no `src/` runtime code touched
  at all. None of the five changed what `production` users need to opt into.
- **No RSS/large-alloc win landed in `production` this round** —
  `exact-span-large`/`large-reserved-capacity` remain opt-in
  (CONDITIONAL-GO, blocked on the realloc regression), and `large-cache-extended`
  was not gated for production at all this round.
- **README.md's wall-clock table was refreshed as part of this same task**
  (R13-10/#280, raw log `docs/perf/_raw_r13_10_bench_table_full.log`) — see
  the README diff for the actual before/after numbers; it had gone stale
  across two consecutive `production` composition changes (R12-9/R12-11's
  Round 12 changes, then R13-9's `class-aware-dirty` promotion) before this
  refresh. Headline deltas vs the prior (2026-07-20, post-Round9) numbers:
  Churn+write 64 B moved from a 1.18× win to a 1.00×/within-noise tie, and
  Cold-direct 64 B moved from a 1.98× loss to a 1.00×/parity tie — both
  single-host wall-clock swings, not attributed to any Round13 code change
  (`class-aware-dirty` is remote-drain-only and iai-confirmed zero-cost on
  these single-thread paths, `R13_9...md` §1a/§3); Churn+write/non-writing
  256 B and 1024 B held their lead (2.71×→1.57× and 1.45×→1.71×
  respectively — both still clear wins, magnitude shifted within the host's
  documented ±15-20% noise band).

## 7. Reproduction

```bash
# The one production-affecting gate this round (already committed, not re-run here):
cat docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md

# The opt-in pair that stayed CONDITIONAL-GO:
cat docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md

# The canonical wall-clock table this document's §6 refers to:
npm run bench:table
```

## 8. Scope note

Per this task's own brief, this document is a **summary of a completed
wave**, not a new production gate — no `Cargo.toml` feature-list decision is
made or proposed here (that already happened, in R13-9/`da77b38`). Every
number in §2/§3 is a restatement, with citation, of a number some other
document already measured and gated; this document adds no new measurement
of its own beyond the README wall-clock refresh cross-referenced in §6.
