# Checkpoint — 2026-07-23 23:50 [r14-complete]

## Session summary
This session (continuation of the same long-running session that completed
Round 13) planned and executed Round 14 — the follow-up queue against THREE
independent reviews of the Round 13 wave (two inline reviews I performed
personally by reading and verifying code, one written to a file by an `@fx`
sub-agent). Synthesis of all three is recorded in
`docs/reviews/2026-07-23-r14-plan.md` and
`docs/reviews/2026-07-23-r14-reviews-synthesis.md`. The queue was decomposed
into tasks #286-#295 (R14-1..R14-10) plus three closing tasks (#296
CHANGELOG, #297 this checkpoint, #298 `@fh` review), executed under
`/babygoal`'s TaskList-driven model with a babysit cron re-armed at session
resume. Every task followed the session's standing zero-trust discipline:
delegate to `@sh` sub-agents, personally read every diff line-by-line,
personally re-run the tests myself, personally reproduce red/green
counterfactuals for safety-relevant changes, run clippy across default/
experimental/`--all-features` + fmt, check `no_stale_doc_references`, and
only mark a task `completed` after finishing personal verification.

**All 10 planned tasks (#286-#295) are completed and committed**, plus two
genuine defects found and fixed mid-verification (their own hotfix tasks,
not folded silently into the tasks that surfaced them):

- **R14-1** (`06a5be6`, #286, P0): `LargeCacheExtension` typed-init via
  explicit `ptr::write` (was relying on an unspecified all-zero-bytes-as-
  `None` layout guarantee, found by all three reviews); `deref_*` converted
  to `unsafe fn`.
- **R14-2** (`ef4db50`, #287, P0/P1): `sidecar_oom_latch` ordering
  divergence closed (production promoted `Relaxed`→`Acquire`, loom model
  verified to match byte-for-byte, new loom test for the exact
  OOM-trip/publish/consumer-visit race); latch-reset-on-recycle
  investigated and explicitly declined this round (documented hazard: no
  `STATE_LIVE` gate on the remote-write path).
- **R14-3** (`6d85db4`, #288, P1): honest sub-window vs. full-round
  reframing of `class-aware-dirty`'s "21.71×" headline (full round only
  ~11% at N=8) — reworded everywhere, bench now prints both axes, new
  fixed-work process-level A/B/B/A judge built on `paired-ab-runner.mjs`.
- **R14-4** (`3fde9f9`/`9fcde2e`/`6a644a4`, #289, P1, largest task): Stage 2
  Small/medium→Large realloc promotion implementing the approved
  R11-3 design. Gate verdict: **CONDITIONAL-GO mechanism / RED on R10-2's
  specific kill-gate** (iai clean, but R10-2's exact workload
  oversubscribes the base Large cache) — NOT promoted.
- **R14-5** (`c0ccbc4`, #290, P1): `large-cache-extended` hardened (budget-
  before-materialize ordering, finite 1280 MiB default budget, N=1/2/4
  post-materialization gates, adversarial best-fit/FIFO tests). Gate:
  **CONDITIONAL-GO** — not promoted.
- **R14-6** (`8265b1c`, #291, P1): `large-reserved-capacity` growth factor
  2×→4×, REVERSING R13-6's +102.3% Ir regression into **−22.44% Ir /
  −36.17% cycles**, RSS win unchanged (15.80×→1.06×). Gate: **clean GO**
  (recommendation only) — not promoted, decision left to user.
- **R14-7** (`b117257`/`ffb82bc`, #292, P1/P2): `MAX_SEGMENTS` raised
  1024→4096 (README-documented first, then measured cheap on every axis,
  then raised) — the ONE unconditional change this round (no feature
  gate). Design-only expandable-table doc written as a future-round
  candidate if the new ceiling is ever hit.
- **R14-8** (`8a432d7`, #293, P2): corrected false "compiled out under
  NUMA" claims for `class-aware-dirty`'s drain path — confirmed empirically
  wrong via a new probe test before fixing the comments.
- **R14-9** (`4204b38`+4 more, #294, P2): unified owner-only sidecar
  primitive (`src/alloc_core/sidecar.rs`) replacing three independently
  hand-rolled reserve/init/deref implementations; `PerClassDirty`
  deliberately exempted (different concern, documented why).
- **R14-10** (`f49cddc`, #295, P2 process): nine-item wave hygiene sweep —
  `git diff --check` clean + `.gitattributes` for raw logs, honest
  production-vs-opt-in CHANGELOG/wave-summary framing, pinned-worktree
  bench-profile protocol, machine-readable CSV policy, `cargo-hack
  --feature-powerset --depth 2` **adopted** as a weekly CI job (308 checks,
  too much per-PR), README noise disclosure, four P3 micro-fixes.

**Two hotfixes found mid-verification, each its own numbered task:**
- **#299** (`ee1c14e`): two real CI failures on `main`, both surfaced by
  Round 13's own `class-aware-dirty` promotion (not this round's work) —
  the `r9_6` waste-ratio test's own "not reachable from CI" claim went
  false after R13-9, and a pre-existing `batch-api` feature-gate gap in a
  hardened-tier test. Confirmed via a temporary git worktree at the
  pre-Round-14 commit before fixing.
- **#302** (`6cc46f1`): `tests/r14_4_promotion_move_leg_reduction.rs` failing
  under `--all-features`, found by `npm run check` during R14-10. Confirmed
  via temporary worktree that this predates R14-10 (reproduces at R14-4's
  own commit `9fcde2e`) — root-caused to `exact-span-large`'s own
  documented zero-headroom trade-off interacting with R14-4's own
  zero-padding design decision, not a new bug. Fixed the TEST (scoped the
  pointer-identity oracle to headroom-available configs), not production
  code, with a documented red/green counterfactual proving the relaxed
  assertion isn't vacuous.

**CHANGELOG updated** (#296, `e4cb683`) — full Round 14 entry matching the
established Round 12/13 format.

**This checkpoint (#297)** is being written now, as the next step in the
sequence. **Not yet done:** #300 (commit the three still-untracked planning/
review docs from this session + push to origin + watch CI), #298 (launch
`@fh` review agent — per explicit user request to use `@fh` this round
instead of `@fm`), #301 (file and execute follow-up tasks from the `@fh`
review — per explicit user request: "после завершения всех задач... заведи
новые таски по результатам ревью и выполняй их" / "Post-Wave-C wrap-up:
CHANGELOG, checkpoint, commit docs, push, then @fh review + follow-up
tasks"). This is a NEW addition to this session's standing pattern: after
Round 13 the wave ended at the `@fm` review; this time the user has
explicitly asked for a further iteration — file tasks from the review
findings and KEEP GOING, not stop after reporting them.

## Active goal
A `/goal` Stop hook is active (re-armed after a session interruption earlier
in this continuation) with condition: "реализуй задачи с помощью @sh, между
задачами делай коммиты, покрой их тестами, в конце обнови чендйнжлог, сделай
/checkoint и запусти @fm ревью агента" — the Stop-hook feedback mechanism has
been firing throughout this session, correctly identifying remaining steps.
Note the goal text says `@fm` but the user's later explicit message overrode
this to `@fh` for this round — task #298 has been updated accordingly. The
goal auto-clears once its condition is judged met; given the user's follow-up
request to continue with new tasks after the review, the practical
completion point is now #301, not #298.

A babysit cron is also armed (session-only, ticking every 15 min per the
established pattern), independently monitoring the TaskList.

## TaskList
### in_progress
- #297 R14: взять чекпойнт сессии по итогам Round 14 (/checkpoint) — this task, being closed by this very write

### pending
- #300 R14: закоммитить оставшиеся плановые доки Round 14 + пуш в origin (blockedBy: #297, resolves once this checkpoint completes)
- #298 R14: запустить @fh ревью-агента по итогам Round 14 (blockedBy: #300)
- #301 R14: завести и выполнить follow-up задачи по итогам @fh ревью (blockedBy: #298)

### recently completed
- #296 R14: обновить CHANGELOG.md за Round 14
- #302 HOTFIX: r14_4_promotion_move_leg_reduction под --all-features
- #295 R14-10: wave hygiene
- #294 R14-9: unified sidecar primitive
- #293 R14-8: NUMA doc corrections
- #292 R14-7: MAX_SEGMENTS raise
- #291 R14-6: large-reserved-capacity growth factor 4x
- #290 R14-5: large-cache-extended hardening
- #289 R14-4: Stage 2 realloc promotion
- #288 R14-3: class-aware-dirty honest framing

## Decisions
- Chose to raise `MAX_SEGMENTS` (R14-7) rather than write a design-only doc,
  after measuring the raise as cheap on every axis (static footprint,
  RSS, scan-path cost) — the only unconditional (non-feature-gated) code
  change this round.
- R14-6's `large-reserved-capacity` growth-factor fix got a clean GO
  recommendation but was NOT auto-promoted — left for explicit user
  decision, matching this session's standing "promotion needs AskUserQuestion"
  discipline (not yet asked, since the wave isn't finished).
- R14-4's realloc promotion and R14-5's cache hardening both landed
  CONDITIONAL-GO, not clean GO — neither promoted, no promotion question
  asked (matches the established pattern: only prompt on CLEAN gates).
- Chose NOT to migrate `PerClassDirty` onto the new R14-9 sidecar primitive
  — a deliberate scope decision (different concurrency shape, CAS-publish
  vs. owner-only), documented rather than forced into a one-size-fits-all
  abstraction.
- Both hotfixes (#299, #302) were root-caused via a temporary `git worktree`
  at the relevant historical commit BEFORE writing any fix — establishing
  "predates this round" as a verified fact, not an assumption, in both
  cases.
- User explicitly requested `@fh` (not `@fm`) for this round's review agent,
  and explicitly requested that follow-up tasks from that review be filed
  AND executed, not just reported — task #298 and a new task #301 were
  adjusted/added accordingly mid-session.

## Open questions
- None from the user's side at this exact moment — the immediate next
  mechanical steps (#300 push, #298 review, #301 follow-up) are already
  queued and unblocked in sequence. The one standing question for a FUTURE
  point in the sequence: whether to promote R14-6's clean-GO
  `large-reserved-capacity` growth-factor fix into any user-facing
  recommendation — not yet reached, since R14-4/R14-5's non-clean gates on
  the same feature family (Large allocation path) may be relevant context
  to present together.

## Repo state
```
?? docs/agent_reviews_us/2026-07-23-r13-wave-review-fx.md
?? docs/reviews/2026-07-23-r14-plan.md
?? docs/reviews/2026-07-23-r14-reviews-synthesis.md
```
(Three untracked planning/review docs from this session — not yet committed;
task #300 will commit them alongside this checkpoint file.)

```
e4cb683 docs(changelog): document Round 14 -- sidecar hardening, medium realloc promotion, exact-span/large-cache gates, MAX_SEGMENTS raise, unified sidecar primitive (task #296)
6cc46f1 test(alloc-core): scope R14-4 OPT-G pointer-identity oracle to grow headroom (hotfix, task #302)
f49cddc chore(process): Round 13 wave hygiene -- diff --check, honest production framing, bench-profile pinning, cargo-hack (R14-10, task #295)
f344f62 docs(readme): refresh unsafe-inventory counts for the sidecar primitive (R14-9, task #294)
c568fcc docs(alloc-core): document why PerClassDirty is exempt from the sidecar unsafe-fn deref pattern (R14-9, task #294)
```

Local `main` is 19 commits ahead of `origin/main` (Round 14's full range,
`06a5be6`..`e4cb683`) — NOT yet pushed. Task #300 will run `npm run check`
then push and watch the resulting CI run, per this session's established
"push burned us once with red CI, always check first" discipline.
