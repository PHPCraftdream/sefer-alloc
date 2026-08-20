# sefer-alloc — project conventions

Core instructions, mandatory for all code in this repository. They
**override** default behavior.

## File and module structure

- **One file — one export.** Each source file defines exactly one public item
  (type, trait, function). The file name matches the export. This rule is about
  *one responsibility per file*, not a literal count of `pub` tokens; the
  following categories are sanctioned exceptions (they keep a single focused
  responsibility even though the file exposes more than one public item):
  1. **doc-hidden test-only forwarders** — items that are `pub` solely because
     their enclosing module is `#[doc(hidden)]`, exposing a test hook so an
     integration test in `tests/` can reach an otherwise-internal surface (the
     established "test-only export pattern"; see the `#[doc(hidden)]` notes in
     `src/lib.rs`, `src/alloc_core/mod.rs`, `src/registry/mod.rs`,
     `src/registry/tagged_ptr.rs`). These are not stable public API.
  2. **protocol-constant clusters attached to their one primary type** — a set
     of `pub` protocol constants that belong to a single owning type and live
     with it (e.g. `RemoteFreeRing` with its `RING_CAP` / `DBG_RING_OVERFLOW`
     constants). The constants are that type's protocol, not independent
     exports in the sense of the rule.
  3. **single-file seam crates in `crates/`** — for a crate that is one file
     (e.g. `crates/numa-shim/src/lib.rs`, `crates/malloc-bench-rs/src/lib.rs`), "the whole crate is one module"; it
     publishing several public items is normal, because the crate as a whole is
     the single focused library — that is its one responsibility.
  4. **`#[cfg(kani)]` proof harnesses in `src/`** (e.g. `src/kani_proofs.rs`) —
     Kani proof harnesses need `pub(crate)` internals (e.g.
     `crate::alloc_core::node::Node`, `crate::concurrent::hand::AtomicSlot`)
     that are invisible from `tests/` (integration tests see only `pub`), so
     they legitimately live in `src/` behind `#[cfg(kani)]` rather than in the
     `tests/` tree.
- **`mod.rs` — reexports only, no code.** The `mod.rs` file contains
  exclusively `mod`/`pub mod`/`pub use` declarations. No logic, types,
  functions, or tests belong in `mod.rs` — it only wires modules together.

## Tests

- **Put tests in a separate folder from the start.** Do not leave tests inline
  in the module file (`#[cfg(test)] mod tests` inside `src/*.rs`). Tests live in
  `tests/` (integration) with a mirrored structure; new code is written with
  tests in separate files from the very beginning, not extracted later.
- **No doctests.** Never add a runnable rustdoc code example (` ```rust `,
  ` ```compile_fail `, ` ```no_run `, or a bare ` ``` ` fence) to a doc comment
  in `src/**/*.rs` — `cargo test --doc` compiles and runs every one of them as
  its own separate test binary, and that per-example compile overhead is too
  slow across a crate this size. Illustrative snippets in doc comments must use
  a non-executed fence (` ```text `) or plain prose; the runnable version of the
  example belongs in `tests/` as a real test. Pre-existing doctests are tracked
  debt for migration (see `docs/reviews/2026-07-12-round2-remediation-plan.md`),
  not a precedent for adding more.

## Phased delivery

- **Round start: check BOTH open-items indexes.** Before forming a new
  round's task queue, read `docs/perf/OPEN_ITEMS.md` end-to-end AND its
  sibling `docs/CORRECTNESS_OPEN_ITEMS.md` end-to-end, and decide, for each
  item a prior round's gate report / commit message / review flagged as open
  / deferred / follow-up, whether this round closes it, defers it (with a
  one-line reason appended in the relevant index), or leaves it — none may be
  silently ignored. `docs/perf/OPEN_ITEMS.md` covers perf gate reports and
  perf design docs only (see its own `## Scope`); `docs/CORRECTNESS_OPEN_ITEMS.md`
  covers correctness bugs, flaky tests, and CI-coverage gaps flagged from ANY
  source (commit messages, code comments, reviews) — the two are
  deliberately separate files with separate scopes, not one merged index.
  When a gate report / commit / review newly flags an open item, add it to
  the appropriate index in the same commit; when one is closed, move it to
  that index's "Recently resolved" trail (R18-8/task #336: R14-4's
  explicitly-marked-open "re-run `scripts/r10_2_medium_gate.mjs` once R14-5
  lands" item hung unnoticed through rounds 15–17 and was caught only by an
  accidental external re-read; R22-3/task #354: R19-1's commit message
  flagged a flaky test and a clippy dead-code combo as follow-ups that then
  existed in NEITHER index — because at the time there was no correctness
  sibling for `OPEN_ITEMS.md`'s deliberately perf-only scope to defer to —
  and the flaky item was independently reproduced twice more before this
  gap was noticed and `docs/CORRECTNESS_OPEN_ITEMS.md` was created to close
  it; the in-session TaskList does not survive a session boundary, so a
  fresh session inherits no memory of prior rounds' flagged-open items —
  these indexes do).
- **OPEN_ITEMS indexes are CURRENT-STATE, not archives (R34-24/task #543).**
  `docs/perf/OPEN_ITEMS.md` and `docs/CORRECTNESS_OPEN_ITEMS.md` must read as
  a current-state checklist at the start of every round: every open item
  carries a current-state card (Status / Current-number-or-verdict / Next
  trigger / Evidence) as its FIRST visible block, and a closed / null /
  rejected item must NOT look active due to a stale header, tier placement,
  or missing Status-card update. The structural rule (established
  R29-6/task #437 for the perf index, which created
  `docs/perf/OPEN_ITEMS_ARCHIVE.md` and moved the long dated closure
  narratives there): **when an item is closed, its full closure narrative
  moves to the archive file (or, for the correctness index which has no
  archive file yet, to its "Recently resolved" section); the main index
  keeps only a one-line pointer in its own "Recently resolved" section.**
  A closed item that still sits in an active tier (`[A]`, `[D]`, `[L]`,
  `[T]`) with no Status-card update is a structural defect — the round
  that closes it MUST update the card to `Status: CLOSED` and move the
  narrative in the SAME commit, not leave the header implying the item is
  still open. If `docs/CORRECTNESS_OPEN_ITEMS.md` grows past ~1,000 lines,
  the same split applies (create `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`).
  This rule formalizes the R29-6 split as a standing convention for BOTH
  indexes; the full migration of older inline closure narratives is not
  required by this rule (the R29-6 split already handles the perf index's
  worst offenders; future closures follow the convention going forward).
  **A single-main-file + single-archive-file pair is the DEFAULT shape this
  rule describes, not the only sanctioned one.** When even the OPEN portion
  of a main index grows large enough that a single archive split no longer
  keeps round-start reading fast (`docs/CORRECTNESS_OPEN_ITEMS.md` split
  2026-08-20, task #1217, reversing item 86's 2026-08-19 deferral of
  exactly this split — see that item's card in
  `docs/correctness-open-items/TRACKED_process_record.md`), splitting the
  OPEN portion itself by an axis the index already uses (here, its own
  `[A]`/`[T]` tier key) into a small folder —
  `docs/correctness-open-items/{ACTIVE,TRACKED,RESOLVED,ARCHIVE}.md` — is
  an equally sanctioned instance of this rule, provided the top-level
  filename (`docs/CORRECTNESS_OPEN_ITEMS.md`) is KEPT as a thin index/
  table-of-contents rather than deleted, so every existing ``
  `docs/CORRECTNESS_OPEN_ITEMS.md` item N `` citation across the codebase
  keeps resolving unchanged. This is not a rewrite of the rule above — the
  main-index-survives-as-pointer / archive-holds-full-narrative structure
  is identical; only the OPEN portion additionally splits by tier when
  doing so is the difference between the mandatory round-start read
  covering the FULL open-item content in one or two short files versus one
  long one. **A tier file that itself later grows past the threshold
  splits again, by an axis it already exposes, one level deeper — and if
  the first-chosen axis turns out wrong for citation stability, the split
  can be REDONE along a different axis the same day.** The `[T]` tier from
  the split immediately above hit the threshold the same day (task #1221,
  2026-08-20): every card in it was already `[T]`, so tier was exhausted as
  a further axis, and it first split by ITEM-NUMBER RANGE into
  `TRACKED_005_008.md` / `TRACKED_009_018.md` / `TRACKED_019_043.md` /
  `TRACKED_044_093.md` — chosen because every citation of this index is by
  item number, never by topic or line, so a number-range filename needs no
  new taxonomy and stays a one-hop lookup. **The owner rejected
  balancing-by-line-count and asked for a THEMATIC split instead, the same
  day (task #1222)**: the four number-range files were replaced by nine
  files grouped by what each card is actually ABOUT (derived by reading all
  70 cards, not assumed) —
  `docs/correctness-open-items/TRACKED_hook_safety.md`,
  `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`,
  `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`,
  `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`,
  `TRACKED_process_record.md`, and a small explicitly-named residual,
  `TRACKED_misc.md`, for the two cards that fit no other category (neither
  `TRACKED.md` nor the four `TRACKED_NNN_NNN.md` files exist any longer).
  A thematic filename is NOT a one-hop lookup by item number the way the
  number-range filenames were — the cost this reopens is paid by
  `docs/CORRECTNESS_OPEN_ITEMS.md` itself carrying a complete,
  mechanically-generated item-N → file lookup table covering all 70
  numbers (including the `59a`/`59b` sub-items), so a citation by number
  stays a two-hop (not one-hop) but still mechanical and always-correct
  lookup. See `docs/CORRECTNESS_OPEN_ITEMS.md`'s own "Structure" section
  for the full criterion-by-criterion breakdown and the table.
- **Every phase is delivered with tests** — code without tests is not considered
  a completed phase.
- **Between phases: run tests and commit.** Before moving to the next phase,
  run `cargo test` (and miri/loom where applicable), confirm everything is
  green, and commit that phase. These are explicitly sanctioned commits between
  phases (the general prohibition "do not commit without being asked" is lifted
  by the user for phase boundaries). Push — only on a separate explicit request.
- **After each phase — ZERO-TRUST review.** Before committing a phase
  (especially if the code was written by a sub-agent): personally read the
  entire diff line by line; rerun the tests yourself (do not trust the agent's
  claim "tests passed"); verify the tests are not vacuous (would they fail
  without the fix — counterfactual); run an adversarial audit by rust-intel
  categories (rust-cc-audit / code-review); look for out-of-scope edits,
  TODO/placeholder, half-wired features, and any new safe `pub fn` that
  accepts a raw pointer and touches allocator metadata — a benchmark-only
  `dbg_*` hook of that shape is a soundness hole by construction (see the
  benchmark-hook rule in "Active rules"; this is exactly how the R25-1 bug was
  missed). Commit — only after personal verification. An agent's statement is
  a claim, not a receipt.
- **After each wave, if the `production` feature composition changed:**
  re-run `npm run bench:table` + `npm run iai` and commit the updated
  README.md / `docs/perf/IAI_BASELINE.md` numbers in the same PR — do not
  defer the canonical-table refresh to a later round (R13-10/task #280: the
  README wall-clock table had gone stale across two consecutive rounds of
  `production` changes before this rule existed). Cite the raw logs the
  refresh was measured from (see the raw-log policy below).
- **A wall-clock gate must report both the sub-window metric and the
  full-round criterion time for the same harness** — not the sub-window
  figure alone. If a harness times an internal region narrower than the
  whole benchmarked round (e.g. skipping setup/teardown inside the timed
  iteration), the report must also cite criterion's own full-round mean for
  that same run, and any material gap between the two axes is itself a
  result requiring explanation, not a detail to omit (R14-3/task #288: the
  `class-aware-dirty` gate's "21.71×" headline was a sub-window figure whose
  full-round improvement was actually ~11%, discovered only because three
  independent reviews asked for the missing axis).
- **Raw perf logs (`docs/perf/_raw_*.log`) are scratch by default** —
  `.gitignore`d (R13-10/task #280), never committed just because a `cargo
  bench`/`npm run iai` invocation happened to write one. The one exception:
  when a perf-gate report (`docs/perf/*.md`) cites specific `_raw_*.log`
  filenames as its evidence, `git add -f` those named files alongside the
  report so the citation is reproducible from the commit, not just from a
  re-run — see `docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md`
  and `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` for the
  established pattern.
  - **Boundary rule for what counts as an artifact requiring raw logs +
    summary CSV** (R22-14/task #365): **any report whose verdict (GO /
    NO-GO / NULL / CONDITIONAL-GO) rests on measured numbers owes raw logs +
    a summary CSV, regardless of whether the measurement came from
    criterion/iai, a `paired-ab-runner.mjs` process-level judge, or an
    ad-hoc probe built for a single one-off question; a design or
    feasibility report that cites no measured numbers (pure reasoning,
    closed-form derivation, or a qualitative code-reading conclusion) does
    not.** The test is "does the verdict rest on a number obtained by
    *running something*", not the measurement's pedigree or permanence —
    `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md`'s criterion-driven
    ns/op numbers and `R21_2_OPT_H_STAGE1_HIT_RATE.md`'s 0/320 and 0/20
    attempt/hit counts from a throwaway diagnostic-counter probe are the
    same category under this rule, even though one is a permanent
    benchmark harness and the other was a single-use wrapper never meant
    to be a reporting pipeline: both are "I ran code and read off a
    number, and the recommendation depends on that number." This closes a
    real gap the rule left open until now: R21-2 published 0/320 and 0/20
    as its explicit decision basis but committed neither a raw log nor a
    summary CSV — only prose reconstruction instructions for a probe
    binary its own text said was "not committed" — leaving it ambiguous
    whether R21-2 even owed logs under the pre-R22-14 wording, which named
    only "a perf-gate report" without defining that phrase. Applied
    retroactively to `R21_2_OPT_H_STAGE1_HIT_RATE.md` in the same commit
    that added this rule: the throwaway wrapper was promoted to a small
    permanent committed example (`examples/r21_2_opt_h_stage1_probe.rs`,
    observation-only, reusing the existing `OPT_H_ATTEMPTS`/`OPT_H_HITS`
    counters and the existing `paired_ab_medium_workload.rs`/
    `paired_ab_hot_buffer_workload.rs` shared harnesses — no new counters,
    no behavior change), and its output committed as
    `docs/perf/_raw_r21_2_stage1_measurement.log` +
    `docs/perf/R21_2_OPT_H_STAGE1_HIT_RATE_summary.csv`, reproducing (not
    re-deriving) the exact already-published 0/320 and 0/20 result. This
    was the cheaper and more honest of the two options weighed (re-run and
    commit vs. add an explicit in-report exemption note): the measurement
    was genuinely reproducible from already-existing test infrastructure
    (the counters and both harness files pre-date this rule and were never
    the throwaway part — only the one-off `main()` wrapper around them
    was), so there was no real reason to leave a documented reproducibility
    gap standing when closing it cost one small `examples/` file. A report
    that truly cannot be regenerated from anything committed (e.g. it
    depended on since-deleted throwaway code, or external state that no
    longer exists) would instead take the exemption-note route — add a
    line to the report itself stating why it falls outside this rule —
    rather than inventing new measurement code after the fact to manufacture
    a raw log; that path was not needed here.
  - **A cited raw log may be truncated to its relevant section, with an
    explicit truncation marker** (R14-10/task #295) — the log does not have
    to be committed in full just because one section of it is the cited
    evidence. Round 13+14 committed ~35 raw-log files, including full
    bench-table sweeps of ~1900+ lines each, growing `docs/perf/` by
    megabytes per wave with no bound; a gate report needs its citation to be
    reproducible, not the entire uncurated stdout. When truncating: keep the
    cited section(s) verbatim, add a `# TRUNCATED — see <describe what was
    cut> — full output reproducible via <exact command>` marker at each cut
    point, and keep enough surrounding context (headers, summary lines) that
    the section is self-explanatory without the removed parts. Do not
    truncate retroactively — this applies to newly-committed logs going
    forward; logs already committed stay as-is (re-truncating them later
    would itself need the same zero-trust diff review a normal doc edit
    gets, for no evidentiary gain).
- **A perf-gate report citing raw logs should also emit a machine-readable
  summary alongside them** (R14-10/task #295) — a compact CSV or JSON file
  next to the `_raw_*.log` files it summarizes, holding the fields a script
  would need to track a metric over time without re-parsing prose or
  criterion/iai stdout: commit SHA, active feature set, CPU/OS
  identification, sample count, and the gate's own key numbers (the exact
  fields depend on the gate — a wall-clock gate cites ns/op figures, an iai
  gate cites `Ir`/`Estimated Cycles` deltas). This does NOT replace the raw
  logs or the prose report — it is a small companion
  (`docs/perf/<REPORT_NAME>_summary.csv`, same base name as the report it
  summarizes) that makes the report's own numbers grep/diff-able across
  rounds. Not retroactive — existing gate docs are not required to grow a
  summary file after the fact; new perf-gate reports going forward should
  include one. See `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB_summary.csv`
  for a concrete example (companion to
  `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`'s §2 tables).
- **A perf-gate report measuring an uncommitted tree must record an
  IMMUTABLE source identity, not just "base SHA + uncommitted changes"**
  (R29-6/task #437). Several gate reports (e.g. `docs/perf/R27_3_POOL_RETENTION_GATE.md`,
  `docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md`) honestly record their measured
  source as "`main` @ `<base SHA>` + this task's uncommitted working tree" —
  honest about what was measured, but weaker than an immutable reference for
  later reproduction: once the working tree changes further (or the task's
  diff is discarded without committing), there is no way to recover exactly
  what was measured, only the base SHA plus a description of intent. Going
  forward, a report measuring an uncommitted tree must cite ONE of:
  1. a temporary measurement commit SHA (even on a throwaway/scratch branch
     that is later deleted — the SHA stays resolvable via `git reflog` or
     `git fsck --unreachable` for as long as it isn't garbage-collected, and
     is trivially made permanent by pushing the branch if needed);
  2. a git tree object SHA (`git write-tree`), which snapshots the exact
     file contents without requiring a commit or a branch;
  3. a hash of the exact patch over the base (`git diff | sha256sum`),
     verifiable later by re-applying the patch to the cited base SHA and
     re-hashing; or
  4. the built executable's hash (e.g. `sha256sum target/release/<bin>`)
     plus complete feature/config metadata (feature set, target triple,
     rustc version) — the weakest of the four (does not reconstruct source,
     only proves which binary ran), acceptable only when the source-level
     options above are unavailable.
  **Not retroactive** — same convention as the summary-CSV rule immediately
  above: existing gate docs (including R27-3/R27-4) are not required to
  grow an immutable identity after the fact and keep their text as the
  honest historical record of what they actually did; this applies to NEW
  perf-gate reports measuring an uncommitted tree going forward only.
- **Artifact storage policy for cited raw logs — hybrid with a per-file
  size ceiling (R34-24/task #543).** CLAUDE.md's existing R14-10 rule
  ("Raw perf logs are scratch by default" + "force-add when cited") was
  written when individual raw logs were a few KiB each. At this repo's
  current scale (257 committed `_raw_*.log` files across 34 rounds; an
  independent bench review flagged `docs/perf` alone as 115 files /
  ~73,000 added lines in a single wave), the force-add-when-cited
  convention is in tension with the need to bound aggregate repository
  growth. The resolution is a **hybrid with a per-file size ceiling** —
  not a wholesale move of raw logs to external CI artifacts (which would
  break the "reproducible from the commit" property the existing rules
  deliberately preserve), nor an unbounded continuation of the status quo.
  Three tiers, keyed on the raw log's uncompressed byte size:
  1. **Under 200 KiB (default — no change):** a cited raw log is
     force-added to Git verbatim, exactly as the existing R14-10
     convention requires. As of R34-24, ALL 256 committed `_raw_*.log`
     files fall under this ceiling (the largest is 145 KiB —
     `docs/perf/_raw_r34_10_sparse_decay_gate.log`; verify via `find
     docs/perf -name '_raw_*.log' -size +200k`), so the existing convention
     is undisturbed for every currently-committed log matching that naming
     pattern. One raw JSON artifact committed under a different naming
     convention (`docs/perf/r34_23_runs/*.json`, ~258 KiB, task #542) is
     the first real tier-2 case — see point 2 below.
  2. **200 KiB – 2 MiB:** the raw log MUST be either (a) truncated per
     the R14-10 truncation rule (keep the cited section verbatim + a
     `# TRUNCATED` marker + reproduction command, bringing the file under
     the ceiling), or (b) gzip-compressed to `_raw_*.log.gz` before
     committing (text benchmark logs compress ~8-12×; the `.gz` stays IN
     the commit, preserving reproducibility, just compressed). Choose (a)
     when truncation loses no citable evidence (a single criterion output
     line); choose (b) when the full log is genuinely needed (a
     multi-thousand-line paired-A/B JSON dump).
  3. **Over 2 MiB:** do NOT commit the raw log to Git at all, even
     compressed. Instead commit, alongside the report: the summary CSV
     (always required), a one-line manifest entry recording the log's
     SHA-256 content hash (`sha256sum _raw_*.log`), the exact reproduction
     command, and a minimal excerpt (the specific lines cited by the
     report, under the 200 KiB ceiling). Store the full log as a GitHub
     Actions build artifact or external object store, referenced by its
     content hash for retrieval — the reviewer's proposed approach,
     reserved for the rare case where a log is too large for any in-Git
     option to be practical.
  **The summary CSV + reproduction commands + measurement identity are
  ALWAYS committed to Git regardless of tier** — these are the irreducible
  reproducibility surface, already required by the existing rules. The
  tier choice affects only the raw log's storage FORM, not whether the
  report's evidence is reproducible from the commit. Aggregate growth is
  tracked per-round by the round-manifest artifact (next bullet), which
  records the count and total size of raw logs committed that round —
  complementing the R14-10 per-file truncation rule, which bounds
  individual log size but not the aggregate.
  **Not retroactive** — same convention as the raw-log-truncation,
  summary-CSV, and immutable-source-identity rules above: existing
  committed raw logs stay as-is; this rule governs NEW cited raw logs
  going forward.
- **Bench-profile pinning: a pinned-commit/worktree protocol, not named
  `production-rN` Cargo feature bundles** (R14-10/task #295). `production`'s
  own composition is expected to keep changing across rounds (that is the
  whole point of the promotion gate) — a gate report or README table
  refreshed under one round's `production` is NOT reproducible against a
  later round's `production` by re-running `--features production` on
  current `HEAD`, because the feature list underneath that flag moved. Two
  options were weighed: (a) freeze each round's composition into a
  permanent Cargo.toml feature alias (`production-r12`, `production-r13`,
  ...) so `--features production-r13` always reproduces Round 13's exact
  set, or (b) pin the commit instead of the feature flag — check out (or
  `git worktree add`) the specific SHA a report was measured at, and re-run
  plain `--features production` there. **(b) was chosen**: (a) permanently
  grows the Cargo.toml feature matrix and the CI feature-powerset by one
  entry per round forever (every future promotion doubles the aliases that
  need maintaining, and an alias silently drifts out of sync with its
  intended historical meaning the moment anyone edits it), while (b) needs
  zero new Cargo.toml surface — git already durably pins every historical
  composition by commit SHA, and `git log -- Cargo.toml` or
  `git show <sha>:Cargo.toml` recovers the exact `production` list that was
  active at measurement time with no additional bookkeeping. Protocol for a
  gate doc that needs to reproduce a PRIOR round's numbers: cite the
  measurement commit SHA in the doc (already required practice — see
  existing gate docs' "measured on commit ..." lines), and to re-run it,
  `git worktree add ../sefer-alloc-<label> <sha>` (or `git checkout <sha>`
  in a scratch clone — never the main worktree, per the shared-workspace
  git-safety rule elsewhere in this file) then run the same `npm run
  bench:table` / `npm run iai` invocation there. No new Cargo feature is
  needed for this — `--features production` in that worktree IS that
  round's `production` by construction.
- **`cargo-hack` feature-powerset CI — ADOPTED, as a weekly + on-demand job,
  not a per-PR job** (R14-10/task #295 evaluation). Motivation: R13-12/task
  #285 was a real pre-existing E0599 compile error reachable only by
  `alloc-xthread`+`fastbin`+`alloc-decommit` WITHOUT `alloc-segment-directory`
  — a combination neither `production`, `--all-features`, nor any hand-written
  `test-feature-isolation` row in `ci.yml` ever exercised (both
  `production` and `--all-features` always turn `alloc-segment-directory` on
  alongside the other three), and it lived unnoticed for an entire round.
  Hand-maintained feature-isolation rows do not scale to catching this CLASS
  of bug — each new row only covers the ONE combination someone thought to
  write down; `class-aware-dirty` joining `production` in the same round
  (R13-9) also made one existing row (`production class-aware-dirty
  alloc-stats`) a silent duplicate of another (`production alloc-stats`),
  the same underlying problem in miniature (hand-written rows drift out of
  sync with feature-list changes with no automatic signal). **Evaluation:**
  `cargo hack check --feature-powerset --depth 2 --no-dev-deps` against this
  crate's ~26 top-level features resolves to **308** `cargo check`
  invocations (measured locally via `cargo hack ... --dry-run` before
  installing cargo-hack via `cargo install cargo-hack --locked`; the CI job
  itself uses `taiki-e/install-action@v2`, the prebuilt-binary installer
  already used for `cargo-deny` in this same workflow, not a from-source
  build). `check`-only (not `build`/`test`) keeps each invocation cheap
  (typecheck only), but 308 of them is real added CI-minutes cost — too much
  to add to the per-PR path without materially slowing every PR. **Decision:**
  scheduled weekly (`schedule: cron '0 6 * * 1'`, the same trigger already
  wired into `ci.yml` for the `numa-real-kernel` job) plus `workflow_dispatch`
  for on-demand runs — see the `feature-powerset` job in `.github/workflows/ci.yml`.
  This closes the actual gap (a bug that survived a full round undetected)
  on a bounded weekly cadence without taxing every push/PR with ~300 extra
  check invocations; `workflow_dispatch` lets a human force a run before a
  promotion decision if desired.
- **Per-round manifest artifact (R34-24/task #543).** Every round that
  lands ≥ 1 commit produces a manifest at
  `docs/perf/round-manifests/R{N}_MANIFEST.md` recording, in one
  machine-checkable place: (1) every commit in the round, classified by
  its R30-12 taxonomy category (`perf(runtime)` / `perf(opt-in)` / `bench`
  / `docs(config)` / `fix(perf)` / `feat` / `test` / `fix` / `build`),
  with the commit list DERIVED from `git log` (cite the exact range
  command that reproduces the table — no hand-transcription of SHAs); (2)
  the round's net `production` default-feature impact (composition changed
  / unchanged; new features added); (3) measured wall-clock / Ir / RSS
  deltas, or an explicit "no `perf(runtime)`/`perf(opt-in)` commits this
  round" statement; and (4) the final verdict per item (GO / NO-GO / NULL
  / CORRECTION / INFRASTRUCTURE / SHIPPED). This makes the R30-12
  commit-prefix taxonomy **self-checking at the round level** — a reviewer
  scanning the manifest's "net default-feature impact" line and the
  production-source commit list can confirm that no opt-in or
  measurement-only result is framed as a default speedup, without opening
  every commit body or cross-referencing the CHANGELOG header (an
  independent bench review explicitly flagged this risk: "opt-in results
  must not be presented as acceleration of ordinary `--features
  production`"). The manifest also records the count and total size of
  raw-log files committed that round, making aggregate `docs/perf/` growth
  visible per-round (complementing the artifact-storage-policy and R14-10
  per-file rules). See `docs/perf/round-manifests/R34_MANIFEST.md` for the
  reference example. **Not retroactive** — rounds that predate this rule
  are not required to grow a manifest after the fact.

## Speed: short scenario by default

- **Tests and benchmarks must run as fast as possible.** Long runs slow down
  the cycle too much.
- **Benchmarks (criterion):** fast profile — `sample_size(10)` + short
  `warm_up_time`/`measurement_time` (the entire suite in a few seconds). Numbers
  are rough, but the relative order of containers is visible.
- **proptest:** modest number of cases by default (around 64) — this is a
  smoke-check for conformance, not exhaustive fuzzing.
- **miri:** run on specific invariant tests (`region_invariants`) and a tiny
  bounded proptest, not the full suite.
- **Heavy/exhaustive runs (large N, many cases, CPU-hours of fuzz,
  multi-arch) — that is Phase 5 hardening**, not the everyday cycle.

## Before every push: `npm run check`

- **Run `npm run check` before pushing, every time.** It runs the fast subset
  of what CI runs — `cargo fmt --check`, `clippy -D warnings` across all six
  CI clippy rows (default / `experimental` / `--all-features` / `hardened
  medium-classes internals` / `production` / `production internals` —
  GENERATED from `scripts/check-matrix.mjs`'s `PER_PR_ROWS` since R30-5/task
  #454, not hand-written; byte-identical to ci.yml's clippy job, pinned by
  `tests/ci_clippy_matrix_consistency.rs`),
  `cargo test` across the main feature combinations (`production internals`,
  `production alloc-stats bench-internals internals`, `pinning`,
  `--all-features` — there is no bare `--features production` test step;
  `production` alone appears only as a clippy row above), then
  `npm run iai` (the deterministic judge) — and fails fast at the first red
  step (`scripts/check-all.mjs`). It does NOT replace CI (CI additionally runs
  miri, loom, TSan, multi-arch, no_std, MSRV) but it catches the most common
  drift class before a push, not after.
- **Why this rule exists:** a push in this session shipped 17 commits with a
  red CI (rustfmt drift accumulated across several phases, plus two CI
  workflow jobs still pointing at test files/features deleted by an earlier
  task) — discovered only by watching the Actions run *after* pushing.
  `npm run check` is the command that would have caught all of it first.
- **Then confirm CI went green — do not assume it.** `npm run check` and CI
  are NOT identical: CI additionally runs miri/loom/TSan/multi-arch/no_std/MSRV
  on a toolchain and OS this repo does NOT pin (there is no
  `rust-toolchain.toml`), so a clean local run can still go red in CI for
  reasons the local gate cannot reproduce. The local gate is the *first* line
  of defense; CI is the *last*. After every push, confirm CI ran green on
  the **landing SHA** — the SHA that actually appeared on GitHub's `main`
  after the push, NOT the commit you created locally before pushing. Read
  it from the remote, not from your local HEAD: `git fetch && git rev-parse
  origin/main` gives the real landing SHA (they can differ if the push was
  rebased, amended, or force-updated mid-transit). Open the GitHub Actions
  run for THAT SHA specifically and confirm it is green before starting
  the next task. This step was absent
  from this section until R33-2 (task #507), and `main` consequently stayed
  red on all five clippy rows for up to 70 commits (Round 31 → Round 33): the
  five failures fixed in `e526517befbf5a0cd0ca1a7ee62f9d84ffe509ee`
  (R33-1/task #506) were NOT a coverage gap — `npm run check` has run all five
  ci.yml clippy rows since R30-5 (task #454), byte-identically, pinned by
  `tests/ci_clippy_matrix_consistency.rs` — they were pushed without `npm run
  check` having been run (three of the five were rustc *compile errors*
  E0601/E0599/E0432, not clippy lints, so no toolchain-drift explanation is
  even possible; the other two — `doc_lazy_continuation`, `int_plus_one` — are
  long-stable clippy lints), and CI's red signal then went unwatched. This repo
  enforces the pre-push rule by discipline only — there are no git hooks
  (`.git/hooks/` holds only samples; `core.hooksPath` unset), no
  husky/lint-staged, and no required status check blocks a direct push to
  `main` (commits land directly on `main`; CI runs async *after* the push). The
  airtight version of this gate — GitHub branch protection requiring the
  `clippy` check to pass before merge — is repo-settings-side, outside any file
  a commit can touch, and is recommended; short of that, the local rule plus
  the post-push CI confirmation above is the realistic ceiling, and both must
  actually be done.
- **`npm run bench:table`** — the companion canonical wall-clock comparison
  table (SeferAlloc vs mimalloc vs System, fixed ns/op units, fixed bench
  set) for whenever comparative numbers are asked for. Exists because ad-hoc
  benchmark tables varied in units/subset/format run to run, once causing a
  spurious "20ns → 40ns regression" that was actually just µs-per-batch vs
  ns-per-op confusion.

## Active rules (from the plan/methodology)

- `#![forbid(unsafe_code)]` for the upper world; `unsafe` is allowed only in
  named seam modules that lift it with `#![allow(unsafe_code)]`, each with a
  single documented reason to hold `unsafe`. The seams are inventoried in
  README §"Where unsafe lives — the complete list" and mirrored in the
  `src/lib.rs` header. There are two tiers of confined `unsafe`, both captured
  by a single self-verifying command (never a hardcoded count):
  `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
  — **tier 1** is the `#![...]` (module-level) matches: named seam modules
  where `unsafe` is permitted anywhere inside the file; **tier 2** is the
  `#[...]` (item-level) matches: individual `unsafe fn` declarations (and the
  scoped `unsafe {}` blocks at their internal call sites) in files that are
  otherwise safe code, each carrying its own `# Safety` doc and per-site
  `// SAFETY:` comment. Both tiers are comment-proof: `^\s*#!?\[` requires the
  line to begin with optional whitespace then the attribute, so `//` comments
  that merely mention the attribute do not match (the unanchored
  `grep -rln 'allow(unsafe_code)' ...` form has false positives, e.g. in
  `src/lib.rs` and `src/registry/heap_overflow.rs`). Any formal audit
  compares against this command's output, and an `unsafe` token not covered by
  a tier-1 module or a tier-2 item-level allow is a hard compile error in every
  feature configuration. The sanctioned exception categories (doc-hidden
  test-only forwarders, protocol-constant clusters, single-file seam crates,
  kani proofs — listed in the "File and module structure" section above) apply
  to tier 1; tier 2 has its own rule: a single documented reason to hold
  `unsafe` applies to each item-scoped site individually, not just to seam
  modules.
- **Benchmark-only `dbg_*` hooks that touch allocator metadata through a raw
  pointer are `unsafe fn` + `bench-internals`-gated, full stop**
  (R25-1/task #395: `HeapCore::dbg_overflow_bitmap_clear_pass` in
  `src/registry/heap_core_diag.rs` was a *safe* `pub fn` that derived a segment
  base from an arbitrary caller pointer via the bitmask
  `os::segment_base_of_ptr` — zero validation — then wrote allocator metadata
  (`clear_magazine`) at the derived offset; gated only on `alloc-global +
  fastbin`, both already in plain `--features production`, so 100%-safe
  downstream code could trigger undefined behavior through it. A
  benchmark-only measurement hook mentally filed as "just for measurement," so
  it was never held to the production safety bar until the R24 readonly review
  (`docs/reviews/2026-07-28-r24-readonly-review.md`) caught it a full round
  after it landed). Three rules, each independently enforceable:
  1. Any `dbg_*`/measurement hook whose safety depends on a raw pointer
     actually referencing a live, owned, mapped allocation MUST be
     `pub unsafe fn` with a documented `# Safety` contract — "measurement-only"
     is not an exemption the moment the function is reachable as `pub` outside
     `#[cfg(test)]`. `dbg_dealloc_own_thread_with_base` /
     `dbg_overflow_bitmap_clear_pass` (both in `src/registry/heap_core_diag.rs`)
     are the positive pattern to follow.
  2. Any hook with no production caller MUST default to gating behind the
     `bench-internals` feature, not `alloc-global`/`fastbin`/other
     production-composition features — otherwise the hook's `#[cfg]` is
     satisfied by `production` alone and silently widens the safe public
     surface of a production build. The one sanctioned exception is
     `dbg_push_to_ring` (R6-MS-4): ~20 test files across the `alloc-xthread`
     suite already call it, so re-gating would reproduce a 130+-file diff for a
     documentation-precision concern rather than a regression — its R24-6
     doc-only justification note (README §"Where unsafe lives") is the
     resolution for that one, not a precedent to extend.
  3. When a hook's target experiment is rejected (NO-GO), re-evaluate the hook
     itself in the SAME task that lands the NO-GO verdict — do not leave it
     behind as a dangling artifact. This is exactly how the R25-1 bug survived
     a full round: R24-3's NO-GO (task #381) reverted the
     `flush_magazine_class` prototype but left the R24-2-era
     `dbg_overflow_bitmap_clear_pass` hook it depended on in place under the
     wider gate, undiscovered until an independent review caught it.
- **A benchmark/report that sweeps a runtime configuration value across
  multiple arms (e.g. `pool_segments`, cache sizes, thread counts fed through
  `with_config`/similar) MUST report, per arm, the evidence that the arm
  actually ran under its labelled configuration — not just the labelled value:
  (1) the REQUESTED config value, (2) the RESOLVED config value read back from
  the allocator's own diagnostic surface (not assumed), (3) any
  config-conflict-delta counter that surface exposes (e.g.
  `config_conflicts_total()`), and (4) the process/thread identity boundary
  the arm ran under (same-process-sequential vs subprocess-isolated) — because
  whether cross-arm state can leak determines whether (1)–(3) alone are
  sufficient. A config-sweep row missing any of these is not usable as
  GO/NO-GO evidence.**
  (R26-4/task #413: R25-5 (`docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md`, task #399)
  swept `pool_segments` = 4/8/16/32 on an RSS/commit axis by calling
  `SeferAlloc::with_config(...)` on freshly spawned threads SEQUENTIALLY in ONE
  process, and reported per-arm RSS labelled by the REQUESTED value only — no
  resolved-value self-check, no config-conflict-delta, implicitly
  same-process-sequential. It passed review and shipped a conclusion ("cap 4→8
  wins on BOTH latency AND RSS") that reached `CHANGELOG.md`, `OPEN_ITEMS.md`,
  and a session checkpoint. An independent review
  (`docs/reviews/2026-07-28-r25-readonly-review.md`) then found the RSS axis
  invalid: `HeapRegistry`'s slot lifecycle is first-claim-wins for a slot's
  whole process lifetime (`claim_with_config` re-claim of an
  already-materialised slot keeps the OLD config silently,
  `src/registry/heap_registry.rs:209` / ~247-300; `recycle` returns slots to
  `free_slots`, `:342`; `pick_slot` pops recycled slots first, `:316-322`), so
  arm N+1's threads could silently reuse arm N's already-configured slot and
  run under arm N's OLD `pool_segments` — rows labelled cap=8/16/32 may have
  actually executed under cap=4. The one signal that would have caught this
  (`CONFIG_CONFLICTS` counter, `heap_registry.rs:263`) was never read; the loud
  signal (`debug_assert!` at `:285`) is compiled out of `--release`, which is
  how R25-5's probe ran. Corrected in R26-2 (task #411, commit `5285e14`) and
  remeasured in R26-1 (task #410, commit `779474e`,
  `docs/perf/R26_1_POOL_CAP_RSS_SUBPROCESS_GATE.md`) using exactly the four
  pieces of evidence above — subprocess-per-arm isolation (structural: a fresh
  process has an empty registry, so cross-arm reuse is impossible by
  construction) plus a per-arm hard-assert of resolved cap via the new safe
  `HeapCore::dbg_pool_cap()` accessor (`src/registry/heap_core_diag.rs:259`)
  and `config_conflicts_total()` delta == 0. R26-1's corrected finding was
  materially different — the RSS "win" did not reproduce (RSS-neutral, not
  RSS-beneficial) — the concrete cost: a production-default-change
  recommendation rested on invalid data for one full round.)
- **Generalizing the rule immediately above: proving the arm ran under its
  labelled CONFIG is not the same as proving it exercised its labelled
  MECHANISM — a benchmark/report judging a feature or code path MUST also
  report, per arm, the evidence that the arm actually took the intended
  code path/counter activity it claims to measure (a path-activation
  oracle), not just that its config resolved correctly. A judge whose
  report cites no per-arm mechanism-activation evidence is not usable as
  GO/NO-GO evidence, exactly as the R26-4 rule already says for config
  evidence.** Concrete mechanism classes this covers (all either already
  countered in `src/` or trivially addable — do not assume a class applies
  without checking the counter actually exists first): virgin bump-carves
  vs. recycled free-list pops (`AllocCore::dbg_small_zero_pass_count`,
  `src/alloc_core/alloc_core_core_diag.rs`); large-cache hits vs. misses
  (`AllocCore`/`HeapCore::dbg_large_cache_hits`,
  `src/alloc_core/alloc_core.rs` / `src/registry/heap_core_diag.rs`);
  decommit/release/reserve call counts; promotion events; directory hits
  vs. fallback scans; and pool cap actually resolved AND victim actually
  activated — this last pair is the boundary case where R26-4's own
  config-evidence and this rule's mechanism-evidence overlap (a resolved
  `pool_segments` value is config evidence; whether the arm's workload
  actually drove an eviction/reuse of that pool is mechanism evidence), not
  a new item beyond what R26-4 already requires.
  (R29-16/task #447 → R30-3/task #452: R29-16's `virgin` wall-clock scenario
  in `benches/r29_16_virgin_zero_skip_calloc_wallclock.rs` freed its whole
  batch inside the SAME `b.iter()` closure Criterion calls thousands of
  times per sample, so `alloc_small_with_virgin`'s free-list-pop-first
  dispatch order (`src/alloc_core/alloc_core_small.rs:274-297`) meant only
  the very first call per sample was a genuine bump-carve — every call after
  that measured the RECYCLED path under the "virgin" label. R26-4's rule did
  not catch this: nothing about the CONFIG was wrong (the `virgin-zero-skip`
  feature flag was correctly compiled on/off as labelled per arm) — the CODE
  PATH exercised was wrong. This shipped a "21.4x speed win" framing that
  survived an entire round's per-task zero-trust review before a post-round
  independent review caught it (the underlying 21.4x Ir ratio itself stayed
  valid evidence that zeroing work was skipped — R30-4/task #453 §(a)
  confirmed that ratio, not a Valgrind artifact — but it was never a
  validated wall-clock claim). Rebuilt correctly in R30-3/task #452 (commit
  `d8f467b`,
  `benches/r30_3_virgin_zero_skip_native_gate.rs`), which added exactly the
  missing piece: a path-activation oracle built on the pre-existing
  `AllocCore::dbg_small_zero_pass_count()` counter, hard-asserting
  `min_activation_pct >= 95%` for a sample's calls before trusting that
  sample's timing (`MIN_ACTIVATION_PCT` in that file). **This oracle caught
  a SECOND real bug during R30-3's own development, before any number
  shipped**: a first attempt at `VIRGIN_BATCH = 16` measured only 6.25%
  virgin-path activation and was rejected by the gate — traced to
  `carve_block_with_refill`'s unconditional 31-block refill batch
  (Phase 9 amortisation) diluting same-class virgin activation to
  roughly 1-in-32 regardless of nominal batch size; see
  `docs/perf/R30_3_VIRGIN_ZERO_SKIP_NATIVE_GATE.md` §3 and the
  `VIRGIN_BATCH` constant's doc comment in
  `benches/r30_3_virgin_zero_skip_native_gate.rs` for the concrete
  mechanism.) This is establishing existing good practice as a mandatory
  rule, not inventing a novel requirement from scratch — two reports already
  adopted it VOLUNTARILY, with no rule requiring it: **R29-13/task #444**
  (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §1.3) hard-asserted
  `used_post_teardown_max > 0` (admission proven) per arm before trusting a
  cell's retention numbers; **R30-6/task #455**
  (`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §1.3) independently
  built the same style of oracle one step further — `admissions_ok` AND
  `hits_ok` (both hard-asserted), all 36 arms passing — explicitly citing
  R30-3's pattern as precedent.
  **Not retroactive** — same convention as this file's other non-retroactive
  rules (the raw-log-truncation rule and the immutable-source-identity rule
  both state this explicitly): existing gate reports that predate this rule
  are not required to be retrofitted with a path-activation oracle after the
  fact. R30-3 (`benches/r30_3_virgin_zero_skip_native_gate.rs`, task #452)
  and R30-6 (`examples/r30_6_large_cache_headroom_ab_gate.rs`, task #455)
  are the first two REAL applications and already comply going forward.
  R30-3's judge is the reference example for what a compliant judge looks
  like — read its own module-doc "4. Path-activation oracle" section
  (`benches/r30_3_virgin_zero_skip_native_gate.rs`, point 4 of its 8-point
  design) for the concrete oracle-design register future work should match.
- **A gate report's tables and headline ratios must be DERIVED, by one
  checked script, from raw per-sample data written before any prose is
  written — not hand-transcribed or hand-computed.** This COMPLEMENTS,
  does not replace, the raw-log-policy and summary-CSV-policy bullets
  above (which govern WHICH artifacts a report owes and when they must be
  committed); this rule governs HOW those artifacts must be produced, so a
  report's prose cannot silently disagree with its own underlying numbers.
  Concrete requirements, all mechanically checkable:
  1. **Probes/examples write raw PER-SAMPLE structured data (JSON/CSV) as
     their primary output first** — the numbers a report's tables are built
     from must exist as a machine-readable artifact before any table or
     headline sentence is typed.
  2. **The summary CSV and the report's Markdown tables are DERIVED from
     that raw data by one checked script, not retyped by hand.** This is
     the direct fix for a transcription-typo class of bug, not a
     hypothetical: R29-3's own landing commit (`db35617`, task #434)
     records, in its own commit message, that zero-trust review caught the
     report's "Run 1 (primary, cited as evidence)" prose citing numbers
     that matched NEITHER of the two actually-saved raw logs (implying a
     third, unsaved run had been quoted by hand) — re-cited to the real
     content of `_raw_r29_3_decomposition_run1.log`, "with the CSV's own
     sub-component transcription typos also fixed against the raw log" in
     the same pass, the aggregate numbers having been correct even though
     the sub-components typed under them were not.
  3. **Statistic names are printed by the code that computes them, not
     typed independently into prose later.** `examples/r29_3_decomposition_gate.rs`
     computes `total.elapsed() / N` — an arithmetic mean — and both the
     code's own `println!` and the report's prose call it "median";
     confirmed CONFIRMED by R30-4/task #453 (commit `575f3a8`, finding c).
     If the label string lives next to the computation instead of being
     retyped in Markdown afterward, a mean cannot drift into being called a
     median.
  4. **Every percentage in a report must name its numerator AND
     denominator inline.** A bare percentage with no stated denominator is
     not an acceptable report format going forward. R29-5's headline
     "0.054%" (33/60,722, over ALL allocations) coexisted with a far higher
     and more decision-relevant 82.5% (33/40, over just the PROMOTABLE
     population) that the report's own §1 data supported but its §0
     headline table never stated — confirmed PARTIAL by R30-4 (finding h):
     not an arithmetic error, a framing gap that naming both figures'
     denominators inline would have foreclosed.
  5. **Absolute retention and delta/incremental retention must be labeled
     distinctly wherever a report compares two such quantities.**
     `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §5 compared an
     absolute ~238 MiB/heap floor against R27-3's cap8-minus-cap4 DELTA
     (~8 MiB) as if both were the same kind of quantity, yielding an
     inflated "30x" headline — confirmed PARTIAL by R30-4 (finding g); no
     single unambiguous replacement ratio existed once the category
     mismatch was named, only like-for-like absolute-to-absolute
     comparisons computed fresh from each report's own tables (~7.6x/~10.3x).
  6. **A script that computes a headline ratio must assert the arithmetic
     it prints, not just print a hand-computed string** — e.g.
     `assert(headroom_a / headroom_b == stated_ratio)` (or the language
     equivalent) alongside the `println!`/table-cell write, so a wrong
     ratio is a FAILING CHECK in the generating script, never a published
     claim a human transcribed wrong by hand. This is the most mechanically
     enforceable point here. The same `R29_13_LARGE_CACHE_RETENTION_GATE.md`
     §5 headline also asserted "32x the small pool's 16 MiB cap" from
     256/16, which is 16, not 32 — a plain arithmetic error, part of the
     same PARTIAL finding g above, that a one-line in-script assertion
     would have caught before the report was ever written.
  7. **An immutable source identity (per the R29-6 rule in "Phased
     delivery" above) must be produced BEFORE measurement, not assembled
     after the fact from a stated recipe.** `R29_13_LARGE_CACHE_RETENTION_GATE.md`'s
     cited provenance hash was found to be 63 hex characters — one short of a
     valid sha256 digest's required 64 — and a reconstruction attempt
     against the report's own stated recipe (`sha256(git diff -- ...; cat
     ...)`) over its cited base SHA produced a genuinely DIFFERENT hash,
     confirming the citation cannot be reproduced, not just mistyped;
     confirmed CONFIRMED by R30-4 (finding e, commit `575f3a8`). This was
     the first real-world test of the R29-6 immutable-provenance rule and
     it failed on exactly the pattern this point targets: a hash
     hand-assembled from a recipe applied to an already-mutated,
     never-preserved working tree cannot be recomputed later, no matter how
     precisely the recipe is restated. Capture or compute the identity
     (temp commit SHA / `git write-tree` / patch hash / binary hash — the
     four forms the R29-6 rule already lists) from something that exists
     AT measurement time.
  **Not retroactive** — same convention as the raw-log-truncation,
  summary-CSV, and immutable-source-identity rules above: R29-3, R29-5,
  R29-13, and R29-16 (or any other pre-existing report) are not required to
  be regenerated through a checked-script pipeline after the fact — they
  were already corrected append-only by R30-4/task #453 where needed, a
  separate, already-completed remediation. This rule governs NEW gate
  reports going forward.
- **A gate report must name the exact entry point under test —
  `AllocCore::…` vs `HeapCore::…` vs a real `#[global_allocator]` — and
  state why that layer is the one the promotion/decision applies to; a
  judge measuring below the layer a feature actually ships at is not usable
  as promotion evidence.** This is the third instance of one meta-pattern,
  each closed only after it had already shipped a wrong or misleading
  verdict: R25-5 measured the wrong **config** (→ the R26-4 rule above);
  R29-16 measured the wrong **code path** (→ the R30-8 rule above); R30-3
  measured the wrong **layer** (→ this rule). R30-3 (task #452) built a
  judge that satisfied every rule that existed at the time — it had a
  path-activation oracle (R30-8's own requirement), it honestly reported
  ~3% activation, it correctly diagnosed the mechanism
  (`carve_block_with_refill`'s 31-block free-list dilution) — and still
  shipped a wrong NO-GO verdict, because it measured `AllocCore::alloc_zeroed`
  (bypassing the magazine) instead of `HeapCore::alloc_zeroed` (the actual
  chain `SeferAlloc`'s `#[global_allocator]` uses, which retains virginity
  across an entire magazine refill via `PerClass::virgin_mask` —
  `src/registry/heap_core_alloc.rs`). Caught and reopened in R31-0 (task
  #471, commit `dece4a7`). Filed as an owned item in
  `docs/perf/OPEN_ITEMS.md` (item 32) so a fresh round inherits this without
  reading a review doc first, per this file's own "Round start: check BOTH
  open-items indexes" convention.
- **Cost and benefit must be measured in the SAME workload regime — a
  fourth instance of the R26-4/R30-8/entry-point meta-pattern above, this
  time on the regime axis.** Do not combine (a) a parity/benefit result
  measured where the smaller capacity is never exceeded with (b) a
  cost/RSS-savings result measured where it IS exceeded, into one Pareto
  claim ("small setting X gives large setting Y's benefit at N× less
  cost"). The same arm must cross the policy boundary and measure cost,
  benefit, and latency together. R30-6's hit-rate workload (`8 × 6 MiB`,
  described as "48 MiB/burst") actually rounds up through whole-`SEGMENT`
  (4 MiB) allocation rounding (`AllocCore::alloc_large`,
  `src/alloc_core/alloc_core_large.rs:190-192`: 6 MiB → 2 segments = 8 MiB
  usable span/object, × 8 objects = 64 MiB) to land EXACTLY on the 64 MiB
  headroom boundary it was testing — confirmed against R30-6's own
  committed CSV (`burst1_used_max_bytes = 67108864` = exactly 64 MiB in all
  36 rows) by R31-12 (task #476). The 64-vs-256 MiB arm therefore could not
  structurally differ (both arms fit the whole burst), so the resulting
  "8/8 vs 8/8" parity was guaranteed by construction, not discovered
  empirically. R30-6's headline nonetheless combined that parity with
  R29-13's (task #444) ~7× retention difference — measured under a
  different regime (large fill + forced drain) — into one Pareto claim
  ("64 MiB gives 256 MiB's hit rate at 7× less RSS"), which then propagated
  into the public `Profile` rustdoc. Corrected by R31-1 (task #464,
  `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md`), which
  measured genuinely beyond the 64 MiB boundary and found the tie breaks
  (a real, reproducible 12.5-point hit-rate loss). See also
  `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` (the
  slot-count/`budget_bytes` axis), which — paired with R31-1 — is the
  positive example of two reports correctly keeping their regimes
  separate instead of merging them into a single claim.
  **Not retroactive** — same convention as this file's other
  non-retroactive evidence rules: R30-6 and R29-13 are not required to be
  regenerated; R31-1/R31-12 already corrected the live claim append-only.
  This rule governs NEW gate reports going forward.
- **A doc-build/lint gate must run in the EXACT feature set that actually
  ships, not merely a superset of it — a fifth instance of the
  R26-4/R30-8/entry-point/regime meta-pattern above, this time on the
  FEATURE-SET axis.** `--all-features` (or any other single fixed
  combination) is not a stand-in for "the configuration users will see" the
  moment a crate's `package.metadata.docs.rs.features` — the feature set
  docs.rs actually builds with — is narrower than `--all-features`, or a
  crate's own README advertises a default feature set narrower still.
  Passing under a wider set proves nothing about the narrower one, exactly
  as R25-5 passing under a same-process sweep proved nothing about the
  cross-arm-reuse regime it never tested. (`crates/aligned-vmem`, task
  #1142: `Cargo.toml`'s `[package.metadata.docs.rs]` deliberately pins
  `features = ["lazy-commit", "huge-pages", "fault-injection"]`, excluding
  `bench-internals` — the docs.rs render is the crates.io landing page's
  companion, so this narrower set is what a `cargo add aligned-vmem` user
  actually sees on docs.rs, not `--all-features`. Four intra-doc links
  (`crate::bench_internals::huge_decommit_attempts`, in `reservation.rs`'s
  `decommit`/`try_decommit`/`decommit_lazy` and `api/decommit.rs`'s free
  `decommit` — the crate's most-read pages) pointed inside the excluded
  `bench_internals` module. `.github/workflows/ci.yml`'s `aligned-vmem-gates`
  job ran exactly one doc-lint row, `RUSTDOCFLAGS="-D warnings" cargo doc
  -p aligned-vmem --all-features --no-deps` — the ONE feature set in which
  all four links resolve, because `--all-features` turns `bench-internals`
  ON alongside the three docs.rs features. So the gate was green on every
  commit while the actual published configuration would have rendered four
  broken links on docs.rs, undetected, because nothing in CI ever built
  the crate in that configuration. A plain default-feature build was *also*
  not the right oracle here — it independently carries two UNRELATED dead
  links (`reserve_aligned_huge`, gated on `huge-pages`, which IS in the
  docs.rs set and so is legitimately live there but dead under bare
  defaults), so "build under default features" would have flagged the
  wrong two links for the wrong reason while still missing that the real
  four were fine under defaults purely because `bench_internals` doesn't
  exist there either — a coincidence, not evidence the docs.rs set was
  ever tested.) **The mechanically checkable requirement:** any crate
  whose `Cargo.toml` declares `[package.metadata.docs.rs] features = [...]`
  narrower than `--all-features` MUST have a CI doc-lint row that builds
  with EXACTLY that list, `RUSTDOCFLAGS="-D warnings"`, in addition to (not
  instead of) the existing `--all-features` row — the two catch different
  defect classes (an item unreachable outside the full set vs. one
  unreachable outside the published set) and neither subsumes the other.
  Derive the feature list from `Cargo.toml` at CI time (e.g. `cargo
  metadata --no-deps --format-version 1 | jq -r
  '.packages[]|select(.name=="<crate>")|.metadata.docs.rs.features|
  join(",")'`) rather than hand-transcribing a second copy into the
  workflow file — a hand-copied list is exactly the kind of drift surface
  the `cargo-hack` feature-powerset rule above already rejected for a
  different reason, and here it would additionally reintroduce this same
  gate-blindness the moment someone edited one copy and not the other.
  **Not retroactive** — same convention as this file's other
  non-retroactive evidence rules: this governs new/edited CI doc-lint
  coverage going forward; a crate that gains its first
  `package.metadata.docs.rs.features` narrower than `--all-features` after
  this rule lands should get the matching CI row in the same change.
- **A commit subject line's conventional-commit prefix must state whether
  runtime behavior actually changed — `perf(...)` is reserved for commits
  that change what ships, not for measurement work that only tells you
  about what already ships** (R30-12/task #461). This is the same
  honesty-in-reporting discipline the Round-27/28/29 header line "**Runtime
  improvements this round: 0**" already applies at the ROUND level, pushed
  one level down to the individual COMMIT SUBJECT — because `git log`
  read alone, without opening every commit body or cross-referencing that
  round's CHANGELOG header, is a real reading path for reviewers, and a bare
  `perf(...)` prefix on measurement-only work misleads exactly that reader
  into concluding the allocator got faster when it did not. Concretely
  verified before writing this rule: `79aad56` ("perf(docs): measure
  medium->Large promotion frequency..."), `894e9e3` ("perf(docs): measure
  the large-cache headroom's idle-RSS floor..."), and `7c2c62d` ("perf(docs):
  calloc-shaped isolation gate...") are all Round 29 commits that add a new
  `bench-internals`-gated diagnostic and/or a new example/bench file with
  its own `required-features` — none touches `production`'s feature
  composition in `Cargo.toml`, and each commit's own body says so explicitly
  ("No production default changed" / equivalent). Round 27, 28, and 29 were
  in fact ALL "Runtime improvements this round: 0" rounds (their CHANGELOG
  headers, verified: line 82 "Runtime improvements this round: 0. Every task
  is a correction, a measurement..."; line 67 "...Both tasks are
  measurement-only / test-only..."; line 36 "...Every task is a correctness
  fix, a measurement..." — production's composition unchanged in all three),
  yet the commits landing inside them used the bare `perf(...)` prefix, which
  conventionally signals a runtime-performance change. Five-prefix taxonomy,
  applied GOING FORWARD only:
  - `perf(runtime)` — a shipping algorithm or a PRODUCTION DEFAULT actually
    changed: `production`'s feature composition changed, or a default
    constant/config value changed, or a hot-path algorithm changed while
    remaining in `production`'s always-on scope.
  - `perf(opt-in)` — a non-default feature/profile's CODE changed (e.g.
    something gated behind `virgin-zero-skip`, `numa-aware`, or a `Profile`
    variant's config) — real code changed, but a user has to opt in to reach
    it.
  - `bench` — ONLY a judge/probe/gate-report/benchmark harness changed; no
    shipping or opt-in algorithm code changed at all. Chosen over the
    alternative `measurement` prefix because `bench` already has 19 commits'
    worth of precedent in this project's own history
    (`git log --oneline --all | grep -E '^[a-f0-9]+ bench\('`), going all the
    way back to `465e3ba`/`92f3288` and including exactly this
    measurement-only-verdict shape already (e.g. `0465c97 bench(perf):
    FLUSH_N=4/8/12/16 sweep is a NO-GO...`, `e530a9f bench(perf):
    flush_magazine_class merge is a NO-GO...`); `measurement(...)` has ZERO
    prior commits in this project's history. Reusing the existing, already-
    understood prefix closes the gap without introducing a second synonym
    for reviewers to disambiguate.
  - `docs(config)` — an existing tuning/config option was documented (e.g.
    the README profile-comparison table R30-7/task #456 added) but no code
    changed at all.
  - `fix(perf)` — shipping or opt-in code changed to restore a documented
    invariant, close a latent correctness/consistency defect, or correct a
    previously-incorrect assumption, but NO speedup is measured or claimed
    and no runtime algorithm or default's OBSERVABLE behavior changed (only
    its internal correctness/consistency did). Distinguished from
    `perf(runtime)`/`perf(opt-in)` (which claim a measured or intended speed
    change) and from a bare `fix(...)` with no `perf` scope (which would
    undersell that the change lives in the SAME hot-path / measurement-
    sensitive code the `perf(...)` family already tracks). Precedent:
    `5df56d3` (R32-5/task #496 — `PerClass` gains `#[repr(C)]` to restore a
    documented one-cache-line magazine layout; measured at exactly 0 Ir
    delta, `docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md` §6, which
    states "`fix(perf)`, not `perf(runtime)`/`perf(opt-in)`: no runtime
    algorithm or default changed, and no measurable speedup is claimed —
    this is a layout-correctness fix restoring a documented invariant").
    This slot inherits the rule's existing non-retroactive posture
    ("Explicitly NOT a history rewrite" — below): `5df56d3` is cited as
    precedent, not retagged; the slot governs new commits going forward
    only.
  This is an ADDITIVE convention: it extends, and does not replace,
  CHANGELOG.md's already-working bullet-tag convention (`[measurement]`,
  `[correctness fix]`, `[process fix]`, `[docs]`, `[CI]`, etc. — see the
  `#### Measurement, correctness & tooling` sections throughout
  `CHANGELOG.md`). The bullet tags already solve this exact honesty problem
  one level UP, at the CHANGELOG-entry level; this rule fills the gap one
  level DOWN, at the raw `git log` subject-line level, where a skimming
  reader has neither a bullet tag nor a round header in view. The same
  measurement-vs-runtime-vs-opt-in distinction applies to new gate-report
  titles/headers under `docs/perf/R*_....md` going forward too — a report's
  title or opening summary line should make clear whether it describes
  measurement-only work or an actual runtime/opt-in code change; existing
  report titles are not required to be renamed for this.
  **Explicitly NOT a history rewrite** — no historical commit message is
  retagged or amended by this rule; it governs new commits going forward
  only, the same non-retroactive posture this file already takes elsewhere:
  the R14-10 raw-log-truncation rule above ("Do not truncate retroactively —
  this applies to newly-committed logs going forward; logs already committed
  stay as-is") and the R24-6 `dbg_push_to_ring` decision (left under its
  wider gate rather than reproducing a 130+-file diff "for a documentation-
  precision concern rather than a regression" — see the benchmark-hook rule
  above) are both precedents already in this file for declining exactly this
  kind of retroactive cleanup.
- Do not bump project or dependency versions without an explicit request.
- Verification-first: every invariant (I1–I7) is covered by proptest and/or
  unit test; the core is run under miri.
