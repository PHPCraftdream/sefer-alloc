# Checkpoint — 2026-07-28 15:15 [round25-complete]

## CORRECTION (2026-07-28, R26-2, task #411)

The R25-5 "wins on BOTH latency AND RSS simultaneously (no tradeoff)" claim in
this checkpoint's Session summary is only half-confirmed: the RSS/commit axis is
invalidated (the probe ran all arms sequentially in one `--release` process, and
the registry's slot reuse + first-claim-wins config in
`src/registry/heap_registry.rs` silently overrides mismatched configs on
recycled slots, with the `debug_assert!` compiled out under `--release`), so RSS
rows labelled cap=8/16/32 may have executed under cap=4. The latency/decommit
axis (cap 4→8 eliminates the 20-decommit residual) is unaffected — it uses
`AllocCore` directly and self-verifies the resolved cap. See
`docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §8 and
`docs/reviews/2026-07-28-r25-readonly-review.md` for full detail; RSS
remeasurement is tracked as task #410 (R26-1) and R25-6's reopened
adaptive-budget work as task #418 (R26-9).

## Session summary
Continuation of the zero-trust review/fix cycle on sefer-alloc (100%-Rust
allocator). This session picked up after Round 24 was fully shipped
(9 commits, `14a86ce`..`9594570`, plus a docs/CI-fix wave through `bc7b8c8`).
The user asked me to study a fresh independent read-only review of Round 24
(`docs/reviews/2026-07-28-r24-readonly-review.md`) and file tasks — every
concrete claim in it was personally re-verified against current source
before anything was filed, per this project's zero-trust convention. Two
claims turned out to be materially wrong on inspection: the review's P4 NUMA
recommendation cited a pre-fix report without checking whether R11-6 (14
rounds earlier) had already resolved the cliff, and my own just-written
Round 24 CHANGELOG entry (not the review's fault) repeated a self-
contradictory "production composition changed once" claim. The review's P0
soundness finding, by contrast, was real and confirmed: `dbg_overflow_
bitmap_clear_pass` was a genuinely unsound safe `pub fn`.

Filed 10 Round 25 tasks (#395-404) with explicit priority (P0/P1/P2/P3/
Conditional) and `blockedBy` dependencies matching each task's own stated
trigger condition. Work proceeded through the queue with a mix of tools
depending on `crush` provider availability:

- **R25-1/R25-2** (commits `667cfe7`/`ef16ced`): dispatched via the actual
  `crush` CLI, but the `zai` provider hit its peak-hours window (08:00-12:00
  local) mid-dispatch for R25-1. Per the user's explicit direction ("поставь
  будильник на 12... используй @sh агентов"), scheduled a one-shot cron
  alarm for 12:03 to auto-resume `crush` later, and switched to the `Agent`
  tool (subagent_type `sh`) for R25-1/R25-3/R25-4/R25-5 in the interim.
  R25-2 was small/well-diagnosed enough to fix directly, no delegation.
- **R25-3 through R25-10**: R25-3/R25-4/R25-5 via `Agent` (peak-hours
  window still open); R25-6/R25-9 closed directly (no dispatch — their
  conditional gates were evaluated and found not-met from already-available
  evidence, so spinning up an agent would have been speculative busywork);
  R25-7/R25-8/R25-10 via the real `crush` CLI once the 12:03 alarm fired and
  peak-hours closed, per the user's later `/babygoal` re-invocation
  explicitly asking to continue with `/crush` agents.
- **A `/babygoal` (not `/goal`) was used for the crush-resume phase** — the
  user interrupted an initial `/goal` invocation and re-issued `/babygoal`
  with the same text, so the session's tracking mechanism is the TaskList +
  a `babysit` cron (armed at `15m`, job id `c125053c`), not a Stop-hook
  condition. The babysit cron self-deleted correctly once the TaskList
  emptied at Round 25's completion (confirmed via a real babysit tick that
  found `pending + in_progress == 0` and called `CronDelete` itself).

**All 10 Round 25 tasks landed, each with full personal zero-trust review
before committing** (every diff read line-by-line, every measured number
independently re-verified against raw logs — this caught and fixed a real
evidentiary bug in R25-5's first draft — full test suites re-run personally,
not trusted from agent claims):

- **R25-1** (`667cfe7`): P0 soundness fix. `dbg_overflow_bitmap_clear_pass`
  was a safe `pub fn` writing allocator metadata through an unvalidated raw
  pointer, reachable under plain `production`. Fixed: `unsafe fn` +
  documented `# Safety` contract + `bench-internals` gating, matching the
  R24-6 pattern. One real caller updated, not deleted.
- **R25-2** (`ef16ced`): fixed my own self-contradictory CHANGELOG wording
  from the prior session.
- **R25-3** (`0465c97`): NO-GO. `FLUSH_N` sweep (4/8/12/16) — `FLUSH_N=16`
  wins on raw Ir but triggers a 20x refill-thrash regression on a boundary-
  stress workload. Third NO-GO in the magazine-overflow region this round
  cluster.
- **R25-4** (`a3cca54`): new isolated `HeapCore`-level correctness oracle
  for `dealloc_batch`'s multi-flush path — exact `live_count` delta
  assertion, mutation-counterfactual personally re-run and confirmed.
- **R25-5** (`8cad0b7`): `pool_segments` 4->8 wins on BOTH latency AND RSS
  simultaneously (no tradeoff). **A real evidentiary bug caught during
  review**: the delegated session's report/CSV cited numbers from an
  uncommitted run, not the actual committed raw log — corrected cell-by-cell
  before committing, verified against my own independent re-run of the
  probe (identical decommit signal: 20/0/0/0).
  **Also caught a real regression this task and R25-3 both introduced**:
  their new example probes lacked `Cargo.toml [[example]] required-features`
  entries, breaking `cargo test --features production` (the exact pre-push
  gate command) with `E0601` — fixed in the R25-7 commit once discovered.
- **R25-6** (`6a75874`): closed without a design attempt — R25-5's own data
  disproved its trigger condition (no RSS-vs-latency tradeoff exists to
  manage).
- **R25-7** (`2148efc`): confirmed `STAGE_CAP=64` clean at every N up to
  1024 (R24-8's original evidence only covered N=16/64). Also fixed the
  R25-3/R25-5 `Cargo.toml` regression discovered during this task's own
  verification pass.
- **R25-8** (`1b5479e`): design-only, CONDITIONAL-GO but explicitly
  excluding the magazine-overflow region that motivated it (LIFO, not
  offset-contiguous — the design's own precondition fails there). Gated on
  a `dealloc_batch` consumer that doesn't exist today.
- **R25-9** (`a6fbaf2`): closed — the review's NUMA-directory cliff citation
  was stale; R11-6 (14 rounds earlier) already fixed it. Verified against
  current source (`alloc_core_small.rs:554-571`'s own "R11-6 UPDATE"
  comment) before writing this conclusion.
- **R25-10** (`52fafe0`): codified the R25-1 lesson as a standing CLAUDE.md
  "Active rules" entry (3 enforceable sub-rules) plus a new ZERO-TRUST
  review checklist item, so the same bug class can't recur unnoticed.

**Immediately before this checkpoint**, the user asked me to repeat the
session's standard wrap-up sequence (CHANGELOG update, checkpoint, commit
all markdown, push, "наладь ci"). I have just finished writing a new
"### Round 25" CHANGELOG.md section (inserted above "### Round 24",
following the same Runtime-improvements/Measurement-tooling split
convention Round 24 established) — **0 runtime improvements this round**
(every task was a correctness fix, measurement, design, or docs/process
work) — and am about to run `/checkpoint` (this write), then commit all
outstanding markdown, push, and investigate CI health the same way I did
after Round 24 (which found two real bugs in `scripts/lib.mjs`/`scripts/
iai.mjs` last time — worth checking whether anything similar has crept in
since).

## Active goal
No `/goal` Stop hook is currently armed (the earlier `/goal` invocation was
interrupted by the user in favor of `/babygoal`, and `/babygoal` does not
use `/goal` machinery). The `babysit` cron that was tracking Round 25's
TaskList already self-deleted once the queue emptied. The user's current
wrap-up request (changelog + checkpoint + commit + push + CI) is being
tracked via ordinary TaskList items (#405-409), not a goal/babysit
mechanism.

## TaskList
### in_progress
- #406 Run /checkpoint (this write)

### pending
- #407 Commit all outstanding markdown files (CHANGELOG.md update, this
  checkpoint, `docs/reviews/2026-07-28-r24-readonly-review.md` — still
  untracked since the review was first delivered)
- #408 Investigate and fix CI ("наладь ci") — not yet started
- #409 Push to origin/main — local is 11 commits ahead of the last known
  push (Round 24's CI-fix wave at `bc7b8c8`)

### recently completed
- #405 Update CHANGELOG.md for Round 25
- #395-404 (all Round 25 tasks — see this checkpoint's Session summary /
  CHANGELOG.md's new "### Round 25" section for full detail)

## Decisions
- **Skipped agent dispatch for R25-6 and R25-9** rather than spinning up a
  `crush`/`Agent` session to formally "investigate" something the already-
  available evidence had already resolved (R25-5's data for R25-6; a direct
  source-code grep for R25-9). Judged that dispatching an agent against a
  disproven or already-answered premise would be speculative busywork, not
  genuine investigation — a deliberate deviation from "always delegate,"
  applied only when the answer was already independently verifiable in
  under a few tool calls.
- **Fixed the R25-3/R25-5 `Cargo.toml` `required-features` regression
  myself, inline, during R25-7's zero-trust review**, rather than filing it
  as a separate task — it was a small, mechanical, already-diagnosed fix
  (a missing `[[example]]` block matching an established, well-precedented
  pattern) directly blocking the exact command (`cargo test --features
  production`) I was already running to verify R25-7 itself.
- **Corrected R25-5's report/CSV numbers directly during review** (rather
  than re-delegating to the same session or discarding the finding)
  once a cell-by-cell comparison against the committed raw log revealed a
  mismatch on the noisy wall-clock/RSS axis (the deterministic decommit-
  count axis matched exactly throughout) — the qualitative conclusion
  (GO-CANDIDATE, no tradeoff) was unaffected by the correction, so this was
  a citation-accuracy fix, not a re-investigation.

## Open questions
None from the user's side. Operationally: task #408 ("наладь ci") has no
specific symptom named yet — same open-ended framing as the prior Round 24
instance of this same request, which required actually running `npm run
check` locally to surface two real bugs GitHub Actions itself didn't catch
(they were specific to the local Node-based pre-push mirror, not the real
CI runners). The next session/turn should start there again rather than
assuming "CI" means only the GitHub Actions dashboard.

## Repo state
```
 M CHANGELOG.md
?? .claude/
?? docs/checkpoints/2026-07-28-round25-complete.md
?? docs/reviews/2026-07-28-r24-readonly-review.md
```
```
a6fbaf2 docs(perf): close R25-9 -- the NUMA directory cliff it targets was already fixed in R11-6 (task #403)
1b5479e docs(perf): run-encoded free batch design -- CONDITIONAL-GO, but not for the region that motivated it (R25-8, task #402)
52fafe0 docs(claude): codify the benchmark-only dbg_* hook safety rule R25-1 fixed one instance of (R25-10, task #404)
2148efc bench(perf): STAGE_CAP=64 confirmed clean at every measured N up to 1024 (R25-7, task #401)
6a75874 docs(perf): close R25-6 without a design attempt -- its conditional gate was not met (task #400)
```
Local `main` is 11 commits ahead of the last known push (Round 24's CI-fix
wave at `bc7b8c8`) — push is now explicitly requested (task #409) and will
happen once #407's commit lands.
