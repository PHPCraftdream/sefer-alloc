# Round 34 remediation wave 5 manifest — commit classification & verdict

**Generated 2026-08-06 — task #629/S6.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale (this file continues that same
per-wave manifest series; "wave 5" here is the release-readiness sprint,
not a Round-34-numbered sub-wave, but it lands directly on top of wave 4's
closing commit and follows the identical convention).

This file covers **wave 5**: the release-readiness sprint executed against
`docs/plans/2026-08-05-release-execution-map.md`, itself built from three
independent readonly reviews of 0.3.0's release readiness —
`docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`,
`docs/reviews/2026-08-05-release-readiness-gap-audit.md`, and
`docs/reviews/2026-08-05-fh-release-readiness-verification-review.md`
(tasks K1-K18, L1-L8, M1-M2, filed as #596-623) — PLUS the fallout from a
closing readonly review of the sprint's own work
(`docs/reviews/2026-08-06-sprint-closing-readonly-review.md`, findings
S1-S12, filed as #624-633), the same "run the full gate, review the wave
that closed, fix what it finds" pattern established by wave 3's own §2 and
continued by wave 4's own §2.

**Explicit scope note:** the crates.io publish-DAG resolution (K3, K4, K9,
L2, L3, L5 — tasks #598, #599, #604, #615, #616, #618) was deferred to a
dedicated pre-release pass per the user's explicit mid-sprint instruction
("по крейтам — временно ссылаемся на свои папки; перед релизом будем
публиковать"). Those tasks remain `pending` on the TaskList and are NOT
part of this wave's closed-commit set below. K7 (release-candidate freeze)
remains `pending`, correctly blocked on that deferred DAG work. S8 (task
#631) was likewise explicitly deferred to post-release per the closing
review's own recommendation.

## §1. Commit classification (verbatim from `git log`, FINAL)

Reproduce: `git log --reverse --format="%H %s" 42d4206..HEAD` (this table's
own upper bound is the wave's true closing SHA — this file's own last row
cannot literally cite its own commit SHA, the same self-referential-hash
problem wave 4's manifest documented in its §1 note: amending the file to
embed its own hash changes the hash it just embedded. Resolve the last
row's SHA via `git log -1 --format=%H -- <this file's path>`).

| # | SHA (full) | Commit prefix | Subject (truncated) | Finding | Category |
|---|------------|---------------|---------------------|---------|----------|
| 1 | `90d1e78833be6b19e9717d0d27b31c5b1d0677c5` | `docs` | commit all remaining readonly review reports from this session | — | **docs-only** (session housekeeping — commits the hs-new-waves and gap-audit review reports per explicit user request) |
| 2 | `a5291c9e16477484b39bc7ee1876d878454aa929` | `docs` | close K1 — document that R1's self-SHA finding was already fixed | K1 | **docs-only** |
| 3 | `2853d351693f1506292594ce9db80d8fcb2ba404` | `fix` | close L8 — serialize a CI-red flaky test on DBG_INJECT_CHUNK_OOM | L8 | **fix — test-only** (`tests/regression_free_path_chunk_oom_graceful.rs` gains a `TEST_LOCK` mutex; no `src/` change) |
| 4 | `9df72c07b6266e1d794e7abc32b7b67c20f1c439` | `bench` | fix doc-comment attribute breaking iai-callgrind library_benchmark | L1/N1 | **bench — tooling-only** (`benches/macro_multiseg_steady_state.rs`: `///` doc comments converted to `//` line comments — `iai-callgrind`'s `#[library_benchmark]` silently rejects any non-`bench`/`benches` attribute; no benchmarked algorithm changed) |
| 5 | `e43e17f37b992fd2b253b1fa9f141998292745b5` | `build(ci)` | key release.yml guards on dry_run, scope CHANGELOG guard to root crate | K5+L4 | **build(ci) — workflow-config-only** (`.github/workflows/release.yml` guard `if:` conditions; no `src/` or `production` change) |
| 6 | `1c06b868220771d4b859cb1d89faad7e99a0fab0` | `build(ci)` | gate non-dry publish on main CI success for the same SHA | K8 | **build(ci) — workflow-config-only** (adds a CI-green gate step to `.github/workflows/release.yml`) |
| 7 | `503f70384db40a6381cb6b3771531acfd885b6bf` | `build` | close L6 — gitignore .claude/, local tool state was blocking cargo package | L6 | **build — repo-config-only** (`.gitignore` only) |
| 8 | `f43600d1fc4f546bcfdcb69e1273147ae27e2bc8` | `docs` | close K2 — correct two overstated wave-4 CHANGELOG bullets, add append-only correction + known-limitations note | K2 | **docs-only** (`CHANGELOG.md` only) |
| 9 | `9129ba789add5fe96221843adaf0d152ad859903` | `docs` | mark CORRECTNESS_OPEN_ITEMS.md items 16/19 resolved (K2/f43600d) | K2 | **docs-only** |
| 10 | `c9c7341f4df42d0357d416c777b1a2dcff2b7e20` | `docs(claude)` | fix stale 'all five clippy rows' → six in npm run check | K14 | **docs-only** (`CLAUDE.md` only) |
| 11 | `63914973ee73bd0069724e94487c895c45f9bfc1` | `docs(security)` | fix stale feature vocabulary and dead email promise | K13 | **docs-only** (`SECURITY.md`, `README.md`; flagged by `verify-commit-prefixes.mjs` as a direction-2 warning since `SECURITY.md` falls outside `docs/examples/benches/tests/scripts/` — reviewed: prose-only edit, no shipping/opt-in behavior changed) |
| 12 | `5db488c7b9939919714f472441c2db9a5227cabb` | `docs(contributing)` | fix stale paths, test/fuzz names, and mandatory commands | K12 | **docs-only** (`CONTRIBUTING.md` only; same direction-2 warning class as row 11, same review outcome) |
| 13 | `b8d62358ea137773448deef18bd90ba79e419404` | `docs` | close K6 — commit-prefix debt now published history, do not rebase | K6 | **docs-only** (`docs/CORRECTNESS_OPEN_ITEMS.md` only) |
| 14 | `e74a7b15f76f0abb31c6f349296b3fa6372e0313` | `docs` | CHANGELOG entry for the release-readiness sprint (tasks #596-621) | — | **docs-only** (wave-closing CHANGELOG entry) |
| 15 | `9fee2ef6fd41731e816665f55b17b7fc91fc46dd` | `docs` | commit the release-readiness map, fh review, and sprint checkpoint | — | **docs-only** (`docs/plans/2026-08-05-release-execution-map.md`, `docs/reviews/2026-08-05-fh-release-readiness-verification-review.md`, `docs/checkpoints/2026-08-06-0015.md`) |
| 16 | `e078c3f507884def3d079f846a132b750f775679` | `docs` | close S3+S9 — fix silent loom/miri no-ops and stale "plain production" claim | S3+S9 | **docs-only** (`CONTRIBUTING.md`, `CLAUDE.md`, `scripts/check-all.mjs` header comment — no executable step changed, only the comment/instructions describing which commands to run) |
| 17 | `1ff5a7950fb5480e9e4b4543633794e1da1c1d99` | `docs` | close S1+S2+S4+S7 — correct sprint-CHANGELOG overclaims, file README debt | S1+S2+S4+S7 | **docs-only** (`CHANGELOG.md`, `docs/CORRECTNESS_OPEN_ITEMS.md`) |
| 18 | `fbeb3156ce1860bb2e815aef86f458bcd6373881` | `docs` | close S5 — move BREAKING CHANGE heading past three more orphaned subsections | S5 | **docs-only** (`CHANGELOG.md` only) |
| 19 | *(this commit — see the self-referential-hash note above §1)* | `docs` | add wave-5 round manifest for the release-readiness sprint | S6 | **docs-only** (this file's own finalization) |

### Aggregate counts (FINAL — 19 of 19 commits)

| Category | Count | Commits |
|----------|-------|---------|
| **fix — test-only** | 1 | 2853d35 |
| **bench — tooling-only** | 1 | 9df72c0 |
| **build(ci) — workflow-config-only** | 2 | e43e17f, 1c06b86 |
| **build — repo-config-only** | 1 | 503f703 |
| **docs-only** | 14 | 90d1e78, a5291c9, f43600d, 9129ba7, c9c7341, 6391497, 5db488c, b8d6235, e74a7b1, 9fee2ef, e078c3f, 1ff5a79, fbeb315, *(this commit)* |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED** across all 19 commits — no row touches `Cargo.toml`'s
`[features]` section or any algorithm/default constant in `src/`. This is
a pure release-readiness/documentation/CI-config sprint: two rows fix
CI-red conditions (row 3's flaky-test serialization, row 4's benchmark
attribute fix), two rows harden `.github/workflows/release.yml`'s guard
logic (rows 5-6), one row is a repo-config fix (`.gitignore`, row 7), and
the remaining fourteen rows are documentation corrections — CHANGELOG
overclaim fixes, stale-reference fixes in `CONTRIBUTING.md`/`SECURITY.md`/
`CLAUDE.md`, and `CORRECTNESS_OPEN_ITEMS.md` bookkeeping. `node
scripts/verify-commit-prefixes.mjs 42d4206..HEAD` reports PASS (2
direction-2 warnings on rows 11-12 for touching root `.md` files outside
`docs/examples/benches/tests/scripts/` — both reviewed and confirmed
prose-only, no shipping/opt-in behavior changed).

## §2. Zero-trust discovery: this wave's own remediation work was itself
independently reviewed, and that review found real overclaims and a
recurring structural bug

This wave closes the P0/P1/P2 items from three independent release-
readiness reviews (rows 1-15) plus a fourth-order closing review's own
findings (rows 16-18) — continuing the same review chain wave 3 and wave 4
established: review → fix → review → fix → review. After landing rows 1-15
and running `npm run check` (all green) plus writing a checkpoint and
CHANGELOG entry, a closing `@oh`-style readonly review
(`docs/reviews/2026-08-06-sprint-closing-readonly-review.md`) independently
re-verified all 15 commits (confirmed accurate) and found 12 new findings
(S1-S12) — no broken code, but:

- **CHANGELOG overclaims (S1, S2):** the K5 bullet claimed manual publish
  "now clears the same checks as a tag push" — false, since
  `workflow_dispatch`'s version guard has nothing to compare against and
  the CHANGELOG guard skips non-root crates entirely, so a member-crate
  manual publish clears zero checks. The K8 bullet claimed a dry-run
  follow-up "can validate this" — false, the CI-success guard is
  structurally skipped on dry-run, so a dry-run run cannot exercise it.
  Both fixed in row 17 (`1ff5a79`) with inline corrections.
- **Silent no-op commands (S3):** `CONTRIBUTING.md`'s documented loom
  command was missing `RUSTFLAGS="--cfg loom"` — every `tests/loom_*.rs`
  file is `#![cfg(loom)]`, so without that flag the binary builds empty and
  "passes" vacuously (0 tests, exit 0) instead of failing loudly. The
  adjacent miri command used a bare positional filter (`-- region_invariants`,
  a substring match over test *function* names, none of which contain that
  substring) instead of `--test region_invariants` (the binary selector),
  silently running zero tests while still reporting green. Both fixed in
  row 16 (`e078c3f`).
- **A recurring heading-hierarchy bug (S5):** the exact same bug class as
  wave 4's F3+F8 (a `###` heading outranks `####`, silently re-parenting
  everything after it) recurred one level upstream — three more `####`
  CHANGELOG subsections were added in this wave's own row 8/14 work
  *after* wave 4's F3+F8 fix had already moved the `### BREAKING CHANGE`
  heading once, landing before that heading again and reproducing the bug.
  Fixed a second time in row 18 (`fbeb315`), with an append-only correction
  note inside the BREAKING CHANGE block documenting the second move.
- **A stale documentation claim (S9):** `CLAUDE.md` and
  `scripts/check-all.mjs`'s header comment both referenced a
  `cargo test --features production` step that does not exist in
  `npm run check`'s actual step list (no bare `--features production` test
  step exists — `production` alone appears only as a clippy row). Fixed in
  row 16 alongside S3.
- **A tracked documentation debt item (S4):** filed as
  `docs/CORRECTNESS_OPEN_ITEMS.md` item 24 (row 17) — README.md's "each is
  a real crates.io crate" claim is currently false pending the deferred
  publish-DAG pass (K3/#598).

This is the same zero-trust-catches-what-self-verification-misses pattern
that has recurred throughout this session's whole review chain (wave 3 §2,
wave 4 §2, and now wave 5 §2 here): each wave's own fix and each wave's own
CHANGELOG narration introduces at least one gap that only an INDEPENDENT
read catches — the entire reason this chain keeps running an external
review after every wave rather than trusting a wave's own internal
verification as sufficient.

S8 (task #631) and the remaining structural `CORRECTNESS_OPEN_ITEMS.md`
renumbering (M2/#623) were explicitly deferred to a post-release pass, not
part of this wave's closed set — see this file's scope note above and
`docs/plans/2026-08-05-release-execution-map.md` for the full deferred-item
rationale.
