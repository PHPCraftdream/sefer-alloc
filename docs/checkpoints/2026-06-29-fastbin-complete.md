# Checkpoint — 2026-06-29 fastbin-complete

## Session summary

Massive multi-day perf campaign on `sefer-alloc`. Two consecutive arcs:

**Arc 1 (the inline campaign, #101 + #102):** flamegraph-guided removal of
all per-op overhead from the hot path. Started at 1.87× slower than mimalloc
on `bench_direct_alloc/SeferMalloc/16B`, closed to 1.43× via `#[inline]` ->
`#[inline(always)]` on the dispatch chain (`HeapCore::alloc`,
`AllocCore::alloc/alloc_small/pop_free/dealloc_small`, all `Node::*` /
`SegmentMeta::*` / `BinTable::*` / `AllocBitmap::*` view-fn wrappers,
`SegmentTable::contains_base` hash helpers, `current_for_alloc` outer frame).
Final commits: `3547c79` (#101), `10439a1` + `bdfca78` (#102). Inline-seam
fully exhausted by the end — every trivial wrapper on the hot path is
`#[inline(always)]`.

**Arc 2 (the fast-bin/tcache project, #103 → P0-P8):** designed and built a
per-thread per-class magazine cache, gated behind a new `fastbin` cargo
feature (default-on in `production`). Seven sequential phases via `@o46l`
sub-agents with zero-trust review between each:
- P0 (#104) added the churn microbench + larson/mstress baseline.
- P1 (#105) added `AllocCore::refill_class` / `flush_class` thin batch APIs.
- P2 (#106) wired the `Tcache` struct + magazine fast paths.
- P3 (#107, RISK PHASE) added the M2 magazine double-free guard. **Zero-trust
  review caught a real hole**: the agent's initial implementation handled
  in-magazine double-free but missed the flushed-then-double-freed case
  (block on BinTable with stale TCACHE_KEY still in word1). Counterfactually
  verified, fixed with a bitmap-check fallback layer, new strong T3 test
  added.
- P4 (#108) hoisted `stamp_segment_owner` from per-alloc into refill (IDEA 3
  absorbed). Closes mstress T=1 regression from P2.
- P5 (#109, RISK PHASE) reconciled decommit + live_count with the D1
  invariant ("magazine-resident block counts as live"). Added `dbg_flush_all`
  test seam + T4 counterfactual. Soak xthread 40M ops alloc=free balanced
  confirmed cross-thread safety.
- P6 (#110) ran the constants sweep (CAP/REFILL/FLUSH), produced the
  win/loss ledger (W=1 mstress T=4, L=4 all bulk regressions documented as
  design worst case, N=9 neutrals). Decided KEEP fastbin default-on.
- P7 (#111) added bulk-mode bypass: per-class refill-streak counter, when
  streak ≥ BULK_THRESHOLD=3 (= 48 allocs) the magazine fast path is
  bypassed. **Reviewer caught the agent's reported larson T=1 = 13.6M as a
  machine-state artifact** — re-measurement confirmed 21.3M (parity with
  P6). Bulk 16B closed from 2.87× → 2.25× slower (-0.62× ratio win).
- P8 (#112, RISK PHASE) attempted IDEA 2: BinTable bitmap → in-block word1
  key. **Implementation was correct (165/0 tests, 43M-op soak), but the
  hypothesis was refuted by re-profiling**: on the MT macro-bench the
  bitmap is < 1% of runtime (not the 12% predicted by the single-thread
  bulk microbench). **REVERTED via `git restore`**. Honest negative-result
  write-up committed (`5f10134`) + meta methodological lesson recorded
  in `docs/PROFILE_FLAMEGRAPHS.md §7` (`cd619ad`): re-profile the workload
  you're optimizing, don't generalize across different bench shapes.

Final perf vs mimalloc (recorded in `README.md`, `docs/MALLOC_BENCH.md`,
`docs/FASTBIN_DESIGN.md`):
- **Large alloc/free OPT-E (4-64 MiB):** 16-39× faster (the headline win).
- **Churn 16/64/1024 B:** 1.7-7.3× faster.
- **MT larson/mstress T≥2:** 1.2-1.3× faster.
- **larson/mstress T=1:** 1.3× slower (structural cost of safety guards;
  flamegraph §7.3 shows the cost is distributed, not in any single function).
- **Bulk 16-256 B:** 1.4-2.3× slower (documented magazine worst case; P7
  closed it by ~0.5× ratio vs P6).

10 commits on `main` ahead of `origin`. User asked for `/checkpoint` at
the end — nothing in flight, all task lists empty, babysit cron deleted.

## Active goal

none (no `/goal` Stop hook armed; the babygoal cycle for the fastbin project
self-completed when TaskList emptied)

## TaskList

(empty — TaskList was emptied as P7 + P8 closed out; babysit cron
`4e13e8eb` was explicitly deleted via `CronDelete`)

### recently completed
- #112 P8 — IDEA 2: M2 BinTable bitmap → in-block key (REVERTED, hypothesis refuted)
- #111 P7 — bulk-mode bypass: streak detection + temporary magazine skip
- #110 P6 — tune CAP/REFILL/FLUSH on the churn bench + honest perf write-up
- #109 P5 — decommit / live_count reconciliation + soak adjustments
- #108 P4 — stamp hoist into refill (absorbs IDEA 3)
- #107 P3 — M2 magazine double-free guard (key + scan + bitmap fallback)
- #106 P2 — Tcache in HeapCore + alloc/free fast paths behind `fastbin` feature
- #105 P1 — AllocCore refill_class + flush_class batch APIs
- #104 P0 — churn microbench + larson/mstress baseline
- #103 Fast-bin / tcache design doc (umbrella)

## Decisions

- **P8 REVERTED, NOT committed as code.** Implementation was correct
  but hypothesis was refuted by re-profiling on the MT workload (bitmap
  < 1% of MT runtime, not 12% as the §1 single-thread microbench
  predicted). The marginal bulk-16B bonus (0.45× ratio) did not justify
  the new 2⁻⁶⁴ false-positive surface (user writes key into word1 ->
  silent leak, not UB). Lesson recorded in PROFILE_FLAMEGRAPHS.md §7.5.
- **`fastbin` STAYS default-on in `production`** per P6 ledger
  (W=1 mstress T=4, L=4 all on synthetic bulk, N=9 — bulk losses are
  documented design trade-off, the real-world wins dominate).
- **The 1.3× larson/mstress T=1 gap is accepted as the documented cost
  of safety guarantees** (M2 double-free no-op vs mimalloc UB,
  foreign-pointer guard, cross-thread routing readiness,
  `forbid(unsafe_code)`). Closing further requires structural changes
  (IDEA 4 / TLS pointer bypass) that are deferred — flagged for ops
  who specifically need single-thread perf.
- **Measurement methodology established for any future perf work:**
  capture the 6-cell ratio table (larson T=1/2/4, mstress T=1/2/4,
  churn 256B) per commit; treat ±0.3× ratio as noise floor; >=0.3×
  movement is signal. Re-profile the SPECIFIC workload you're
  optimizing — different bench shapes have completely different
  hot-path profiles.

## Open questions

(none open — P8's "where can we still gain?" was answered in
PROFILE_FLAMEGRAPHS.md §7.4 and the choice is the user's whether to
ever pursue IDEA 4 or accept the gap)

## Repo state

```
(working tree clean — only untracked checkpoint files in docs/checkpoints/)
```

```
cd619ad docs(profile): §7 post-fastbin re-investigation + methodological lesson
5f10134 docs(fastbin): P8 investigation — IDEA 2 reverted (hypothesis refuted) (#112)
e9f4716 feat(fastbin): P7 bulk-mode bypass — closes the bulk-regression LOSS (#111)
0ba1e14 docs: refresh perf tables across README + MALLOC_BENCH after #101/#102/#103
c5ed150 docs(fastbin): P6 FINAL — sweep, ledger, production decision (P6/#110/#103)
393b88a test(fastbin): decommit/live_count reconciliation + dbg_flush_all + T4 (P5/#109/#103)
d2171d4 perf(fastbin): stamp hoist into refill (P4/#108/#103, absorbs IDEA 3)
3c3884d feat(fastbin): M2 magazine double-free guard (key+scan + bitmap) (#107, P3/#103)
d965ca9 feat(fastbin): per-thread tcache magazine in HeapCore (#106, P2/#103)
93e44cf docs(fastbin): P1 measurement snapshot + measurement methodology for P2-P6
```

(plus `7451650` P1, `7401cec` P0, `bdfca78`/`10439a1`/`3547c79` #101/#102
further back — total 14 commits this session arc; 3 commits ahead of
origin since the `0ba1e14` push: `e9f4716`, `5f10134`, `cd619ad`.)
