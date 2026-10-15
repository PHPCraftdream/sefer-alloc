# Round 20 manifest — PRF-01 C'' and C‴ gates, PRF-03 closure, PRF-04 ReuseFirst, gate and evidence infrastructure

**Scope:** Round 20 starts after `022d282caf375737a888a9762db4a0d741c3af37` (the R19 closeout that added the R19 manifest) and ends at `726580a0313c90a9e33eb3d3fbcd2cb877f061a0` (PR #28). Most work landed through pull requests, so the table lists the main line (`--first-parent`): one row per direct commit or merged PR. It is derived by this command; no SHA is transcribed by hand:

```sh
git log --first-parent --reverse --format="%H|%cI|%s" 022d282caf375737a888a9762db4a0d741c3af37..726580a0313c90a9e33eb3d3fbcd2cb877f061a0
```

33 main-line entries. For a merge, the subject is the PR title (`gh pr view <n> --json title`). The closeout commit that adds this file falls outside the range.

## §1. Commits and classification

| # | SHA | Commit time (`%cI`) | PR | Prefix (R30-12) | Subject (PR title for merges) | Category and R20 effect |
|---:|---|---|---|---|---|---|
| 1 | `ec6bf5c21f8cee615e1c703e6d5e4c73c0f65fe2` | `2026-10-12T14:23:03+02:00` | — | `ci` | ci: enforce at most 7 entries per src directory and 1000 lines per code file | ci |
| 2 | `ad30ebc0b02d37fc497635ddba93f2884e845286` | `2026-10-12T15:20:08+02:00` | — | `docs` | docs: record the three reviewer-arbiter answers on PRF-01/03/04 and correct the PRF-03 R3 reading | docs |
| 3 | `2f207d638e116072226b8b63f5780c2bd72c92a9` | `2026-10-12T15:57:49+02:00` | — | `docs` | docs: record the @ox assessment of the three PRF answers and make card 82 current-state | docs |
| 4 | `8d6ae58d6d42658a94b10dc64c70dfb9393bf742` | `2026-10-12T16:43:41+02:00` | — | `docs` | docs: R20 control-geometry rule for Ir gates (N/2N slope neutrality, separate construction cell) | docs |
| 5 | `c39af51306f8c1293bd9379b7c7d2acd533d58fc` | `2026-10-12T17:47:01+02:00` | — | `bench` | bench: close PRF-03 (item 86) on the Large-hit activation measurement; src probe stays on a branch | bench |
| 6 | `e2a5099af3f8ea7ef2075efba039abc4a015a118` | `2026-10-12T17:53:36+02:00` | — | `checkpoint` | checkpoint: 2026-10-12-1751 | docs — session checkpoint |
| 7 | `e8c4607a7b93c1cef69081647802dcad50e69537` | `2026-10-12T21:59:15+02:00` | — | `docs` | docs: record the two R20 reviewer answers (PRF-01 budget_K, rule item 3a) with the orchestrator disposition | docs |
| 8 | `d8289462655534d46adb3a84129f41604c1b2aaa` | `2026-10-13T06:40:34+02:00` | #6 | `ci` | ci: dispatch-only r20-gates workflow (run the R20 gate runners on GitHub) | ci — INFRASTRUCTURE: r20-gates workflow |
| 9 | `ec3c165e0a0c335cbcda8d4dd2c4292dead8e9e8` | `2026-10-13T06:40:39+02:00` | #7 | `fix(ci)` | fix(ci): make the weekly cargo-hack powerset and fuzz jobs runnable, and fix the E0425 the powerset unmasked | fix(ci) — weekly powerset/fuzz jobs runnable; E0425 fixed |
| 10 | `3eed527ffe6037fc81448276ce4ad304f5134acd` | `2026-10-13T06:41:11+02:00` | #9 | `ci` | ci: run the child-spawning r8 fallback-free test alone in the G1/G3 steps (fixes grep flake) | ci — r8 fallback-free test isolated (grep flake) |
| 11 | `c197c70c0971228aad5cce3166eb14c6b2708652` | `2026-10-13T06:41:18+02:00` | #3 | `bench` | bench: R20 PRF-04 claim-path Ir gate (harness, preregistration, run01 evidence) | bench — PRF-04 V4′ claim-path gate (GO on Ir; V4′ not merged, superseded by ReuseFirst) |
| 12 | `d8e73f55028c9b8fe9e6538dce9c50db628eea4b` | `2026-10-13T08:22:31+02:00` | #8 | `bench` | DRAFT bench: PRF-01 v2 gate (prereg, judge, runner, controls cells) — unmeasured | bench — PRF-01 v2 gate harness |
| 13 | `a31d70a1b278e0b486d795eeb081113f007b0294` | `2026-10-13T11:08:40+02:00` | #11 | `docs` | docs: close correctness cards 107 and 188 on green main CI, narrow 190 and 195 | docs — correctness cards 107/188 closed, 190/195 narrowed |
| 14 | `52d5e6bc1271fc0bbf66b2d65b94e58143c356a1` | `2026-10-13T11:08:45+02:00` | #12 | `fix` | fix: gate three root test targets on the features they require (card 95, r3_1 E0599) + CI guard | fix — test targets gated on their required features (card 95) + CI guard |
| 15 | `a98fc34f78f03438444feb04c6cde7568205b49c` | `2026-10-13T16:39:00+02:00` | #14 | `docs` | docs: evidence-registry readiness plan for the PRF-01 C'' and PRF-04 V4' candidates | docs — evidence-registry readiness plan |
| 16 | `91aa0e9a2d9c56269c010dcf826112c2d07535c6` | `2026-10-13T17:39:51+02:00` | #10 | `bench` | bench: PRF-01 runner works on native Linux (publicText), re-frozen before any measurement | bench — PRF-01 runner on native Linux |
| 17 | `faef4e220b0ecaf6c8cf506129d6621fcb29bb4e` | `2026-10-13T18:15:31+02:00` | #15 | `ci` | ci: tiered runs - changed-path job groups, PR run cancellation, nightly full tier | ci — INFRASTRUCTURE: tiered CI runs |
| 18 | `e70e7b04ed31540ba0ffbab6e1306b47ac85f4a7` | `2026-10-13T18:34:08+02:00` | #17 | `bench` | bench: PRF-01 runner cleanliness check ignores its own out-dir (re-freeze 10) | bench — PRF-01 runner cleanliness fix (re-freeze 10) |
| 19 | `09e05067461b12bcc5c0de65baeacc3bb8359343` | `2026-10-13T18:34:18+02:00` | #16 | `test(proc-probe)` | test(proc-probe): fix flaky peak_rss monotonicity assertion in re-export test | test — proc-probe peak_rss flake fixed |
| 20 | `493ccae004039ba9cfe8998aa53025a5eb41cd1e` | `2026-10-13T18:56:34+02:00` | #13 | `docs` | docs: reviewer answer on an absolute-Ir instrument for PRF-04 (owner decision input) | docs — reviewer answer (absolute-Ir instrument) |
| 21 | `d5c8b97052b26f5388c1ca7b9ae1b750534d6145` | `2026-10-13T19:31:06+02:00` | #18 | `bench` | bench: official PRF-01 C'' gate run (v2) — NO-GO (producer price, construction cost) | bench — PRF-01 C'' official gate: NO-GO |
| 22 | `37c4ac18afa04dbb390d5a41dc8e50d81e9e5adc` | `2026-10-13T20:54:35+02:00` | #19 | `docs` | docs: ox design — PRF-04 heap-slot reuse behind a user flag | docs — PRF-04 flag design (not built) |
| 23 | `7c8db57c7b1e498a343cb606b6210c64500de9f9` | `2026-10-13T21:09:19+02:00` | #20 | `docs` | docs: ox design — PRF-01 remote-free discovery behind a user flag | docs — PRF-01 flag design (not built) |
| 24 | `b4194df452b8fa0381cf7ca067bcfa15e7c3924a` | `2026-10-14T01:12:06+02:00` | #21 | `ci` | ci: r20 gate runs as PR checks (label gate-run) | ci — INFRASTRUCTURE: gate runs as PR checks |
| 25 | `969e4ac39b51bc00f9cd66ad98590b7a75459e89` | `2026-10-14T01:12:15+02:00` | #22 | `docs` | docs: C‴ contract amendment + state-machine chain + item 82 card | docs — C‴ contract amendment (later REJECTED) |
| 26 | `939e37d6eb93d1f053229f5248ccea02a2e10b89` | `2026-10-14T05:24:14+02:00` | #25 | `ci` | ci: evidence-registry re-verification runner (executable recipes, PR-check) | ci — INFRASTRUCTURE: evidence re-verification runner |
| 27 | `2e927ee9fc43dd58f2799f2e6b628f789647b6c5` | `2026-10-14T06:13:02+02:00` | #23 | `test(loom)` | test(loom): actual-source Loom model of the sidecar protocol | test(loom) — actual-source sidecar Loom model |
| 28 | `179e47e644aff3786f4da75fb12d85eff53a0047` | `2026-10-14T06:43:28+02:00` | #24 | `perf(runtime)` | perf(runtime): strict trim from payload start (P1-3) | perf(runtime) — SHIPPED: strict trim from payload start (Ir not measured) |
| 29 | `32fe6cf76cb01e288f08ff6459a4e1dacac38f0d` | `2026-10-14T06:52:19+02:00` | #26 | `ci(r20-gates)` | ci(r20-gates): skip cleanly when the gate-run label outlives the gate config | ci — r20-gates summary skip fix |
| 30 | `02806ba73e0716d0989844c41ea7a3300c51570d` | `2026-10-14T15:40:30+02:00` | #27 | `bench(prf01)` | bench(prf01): C‴ default-path gate harness (prereg/judge/runner/verifier, pre-freeze) (#27) | bench — C‴ gate harness |
| 31 | `3cc870a9b09353a96cf8c5250d9f479936f5e954` | `2026-10-14T22:14:03+02:00` | #30 | `bench(prf04)` | bench(prf04): hint-spill gate harness (prereg/judge/runner/verifier, frozen, candidate TBD) (#30) | bench — PRF-04 heap-reuse gate harness |
| 32 | `2b9ed7357166d0ea3b7c9a4765d3c2e9ab91d95f` | `2026-10-14T22:25:13+02:00` | #31 | `bench(prf01)` | bench(prf01): C‴ default-path gate is a NO-GO; close item 82, no flag fallback (#31) | bench — C‴ default-path gate: NO-GO; item 82 closed |
| 33 | `726580a0313c90a9e33eb3d3fbcd2cb877f061a0` | `2026-10-15T07:38:41+02:00` | #28 | `perf(runtime)` | perf(runtime): PRF-04 ReuseFirst hint-spill as the default heap-slot selection (candidate, gated) (#28) | perf(runtime) — SHIPPED: ReuseFirst default claim path (v2 gate GO) |

## §2. Net `production` default-feature impact

The `production` feature composition is **unchanged**: `git diff 022d282c 726580a0 -- Cargo.toml` adds only bench and example targets, and no `production` line changes. No feature was added to the default set. No dependency versions or `Cargo.lock` entries changed.

Two runtime behaviour changes ship inside the existing composition, both without a flag:
1. **ReuseFirst** (PR #28, `perf(runtime)`). Releasing a heap spills the reuse hint it displaces into a FREE-slot bitmap. A claim checks hint → bitmap → bump → scan.
2. **Strict trim from payload start** (PR #24, `perf(runtime)`). Strict trim and bounded maintenance cut sidecar words from the payload start, not from word 0.

## §3. Measured deltas

- **PRF-04 ReuseFirst**, deterministic Ir under Callgrind, run 38682260617, `docs/perf/R20_PRF04_HEAP_REUSE_RUN02_REPORT.md`.
  - Waves: 12962/558283 (L1) and 82133/740979 (L2) of main.
  - L2 price: +7 Ir per cold claim, +7 per recycled pair, +12 per release. Each is under 1% of its window.
  - Native latency and RSS were not measured.
- **Strict trim from payload start:** Ir **not measured**. The planned P1-3m measurement used the C‴ runner and was dropped together with the C‴ line by owner decision on 2026-10-15. Its cells (`r20_prf01_strict_trim_payload`, `r20_prf01_bounded_maintenance`) exist, but no before/after pair was run.
- **No wall-clock or RSS claim this round.**

## §4. Verdict per item

| Item | Verdict | Evidence |
|---|---|---|
| PRF-01 C'' (summary bit on 0 → non-0) | NO-GO — producer +13.03 Ir/free (budget +6), construction +641 Ir | `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01_REPORT.md` |
| PRF-01 C‴ (summary in the dead prefix) | NO-GO — producer +8.125 / +16 Ir/free (budgets +6 / +10), construction +42 Ir; owner rejected C‴ and the flag fallback | `docs/perf/R20_PRF01_C3_RUN01_REPORT.md`; perf item 82 closed |
| PRF-03 retained Large route | CLOSED — 0 Large requests in the repository benches; redesign has no shown value | `docs/perf/R20_PRF03_LARGE_HIT_ACTIVATION.md`; perf item 86 |
| PRF-04 V4′ claim path | GO on Ir, not merged — superseded by ReuseFirst | `docs/perf/R20_PRF04_CLAIM_PATH_GATE.md` |
| PRF-04 ReuseFirst | **SHIPPED** — v2 gate GO (deterministic; native deferred); run01 INVALID under v1 | `docs/perf/R20_PRF04_HEAP_REUSE_RUN02_REPORT.md`; perf item 87 closed |
| Strict trim from payload start (P1-3) | SHIPPED — correctness/Gate S; Ir not measured | PR #24 |
| r20-gates as PR checks, tiered CI, evidence re-verification runner | INFRASTRUCTURE | PRs #6, #15, #21, #25, #26 |
| `mimalloc_bootstrap_proxy` Ir nondeterminism | OPEN — perf item 89 | `docs/perf/R20_PRF01_C3_RUN01_REPORT.md` §3 |
| Native host (nanosecond clocks, quiet bare metal) | OPEN — hardware prerequisite | — |

## §5. Raw logs committed this round

15 `_raw_*.log` files were added: 12 under `docs/evidence/` (E0 re-verification receipts) and 3 under `docs/perf/`. Together they are 133 572 bytes (`git diff --name-only --diff-filter=A 022d282c 726580a0 -- 'docs/perf/_raw_*' 'docs/evidence/_raw_*' | xargs du -cb`). Gate run data beyond that is committed as gzip tier-2 files inside the `R20_*_RUN0*` directories. The remainder is tier 3, kept in the Actions artifacts and anchored by `receiptHashes`.
