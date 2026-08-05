# Round 34 remediation wave 3 manifest — commit classification & verdict

**Generated 2026-08-05 — task #576/H6.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale.

This file covers **wave 3**: remediation of the closing review of wave 2
(`docs/reviews/2026-08-05-sol-remediation-readonly-review.md`, findings
H1-H8), tasks #571-578 — sequenced strictly in priority order per this
session's explicit governing instruction ("fix all red/broken things
first, then review findings, then a new review"): H1→H2→H3→H4→H5→H7→H8→H6.

**FINAL — closed 2026-08-05.** Extended twice past H6's own original
commit (`db63aed`, row 8): once to absorb 3 real fallout-fix commits
`npm run check`'s own full run surfaced AFTER H6 landed (rows 9-11), and
once more (task #576/H6's own follow-up, filed as finding F7 of
`docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`) to add
the manifest's own extension commit and the wave's closing
checkpoint/CHANGELOG commits (rows 12-14) — completing the manifest to the
exact 14-commit span that review itself independently cited as wave 3's
scope (`2a7f1e6..85dacfc`, verified via `git log --oneline 2a7f1e6..85dacfc
| wc -l` = 14). This is the bounded, single-wave extension pattern §2's
original text anticipated; no further extension is expected — wave 3 ends
at `85dacfc`, and any commit after it belongs to the NEXT wave's own
manifest (`R34_REMEDIATION_4_MANIFEST.md`, covering the fixes for this
same review's F1-F10 findings).

**Convention for future waves** (per F7's own suggested fix): the wave's
LAST commit should update its own manifest file, listing itself, in the
SAME commit — this avoids the "one more commit still to land" residual
this file carried through two rounds of extension.

## §1. Commit classification (verbatim from `git log`, FINAL)

Reproduce: `git log --reverse --format="%H %s" 2a7f1e6..85dacfc` (14
commits, the closed, final wave-3 span).

| # | SHA (full) | Commit prefix | Subject (truncated) | Task | Category |
|---|------------|---------------|---------------------|------|----------|
| 1 | `8b9ed1041eff7ccbda38df1f47580ea1adc553cb` | `fix` | resolve two compilation failures blocking npm run check (H1) | #571 | **fix — production-source** (test `#![cfg]` gate + a `#[cfg]`-scoped compile-time assert; `src/registry/heap_core.rs` + `tests/r34_18_heap_core_stack_pressure_pin.rs`) |
| 2 | `25d6ac4d23b4859b726724424e5912dc54fe0bf0` | `fix(perf)` | close H2 — gate the remaining 31 AllocCore::dbg_* methods behind internals | #572 | **fix(perf) — visibility/cfg-gating** (same class as wave 2's `dbb4016`, see task #578/H8) |
| 3 | `baa91cc9b7ff2b8d859fc6b3a7eb6f42a626d5c6` | `docs` | close H3 — genuinely consolidate CHANGELOG.md under one [0.3.0] header | #573 | **docs-only** |
| 4 | `6e5c06732f135d9dbbf542d14fcd619475d82ede` | `docs` | close H4 — fix 10 pre-rebase SHA citations invalidated by G1's rebase | #574 | **docs-only** |
| 5 | `d48d7bafac0911af69ada200ecc53a54fadcdf43` | `docs` | close H5 — cross-file Sol-F5/Sol-F6's residuals into the correctness index | #575 | **docs-only** (2 one-line `src/` doc-comment cross-references, no behavior change) |
| 6 | `5c17cc37872eaa44cf4732251063435743c1cf86` | `docs` | close H7 — add a BREAKING CHANGE entry for the internals surface narrowing | #577 | **docs-only** |
| 7 | `800ee8668b2971a81d6f707ef525373ddcf82c0f` | `docs` | close H8 — record the decision to leave dbb4016's fix(perf) prefix as-is | #578 | **docs-only** |
| 8 | `db63aed887012c589ea327e947b7d850fadefd35` | `docs` | close H6 — redefine R34_MANIFEST.md's scope instead of a 3rd extension | #576 | **docs-only** (this file's own creation) |
| 9 | `e886ea42a46ef7d6a614c25504a6c173125825fe` | `fix(perf)` | close H2's remaining fallout — 6 examples + 1 bench need internals | — | **fix(perf) — visibility/cfg-gating** (same class as row 2; found by `npm run check`'s own `--all-features` matrix step, not caught by H2's own `src/`-only sweep) |
| 10 | `0d23e7fa95dd355ad1e891f872ad7c4866900ad6` | `fix` | correct H1's stack-pressure runtime test — was vacuous under --all-features | — | **fix — test-only** (mirrors the compile-time guard's own `#[cfg]` exclusion; no shipping/opt-in code changed) |
| 11 | `2f16ba658e618e5b5501fe167a803cb5342925a6` | `fix` | serialize a pre-existing flaky test on process-wide tier1 counters | — | **fix — test-only** (pre-existing flake, unrelated to this wave's own changes, found by the same full-matrix run) |
| 12 | `28663e43f4dc1a6b9ed9680ac4512c42dd7dd591` | `docs` | extend R34_REMEDIATION_3_MANIFEST.md with H6's own commit + 3 fallout fixes | #576 | **docs-only** (this file's own first extension) |
| 13 | `b57f988ec049229aab89fe421e30bc328f954216` | `docs` | CHANGELOG entry for wave 3 release-readiness remediation (tasks #571-578) | — | **docs-only** (wave-closing CHANGELOG entry) |
| 14 | `85dacfc300784cb45ce61c9cfba76dd1a0820870` | `docs` | commit session checkpoints (wave 3 mid-work + wave 3 closing) | — | **docs-only** (wave-closing checkpoints) |

*Row 9's SHA is the FINAL, amended SHA (`git commit --amend` folded a
second required-features gap found immediately after the first fix into
the same logical commit, since nothing had stacked on it yet at amend
time) — verified via `git rev-parse e886ea4` against this table.*

### Aggregate counts (FINAL — 14 of 14 commits)

| Category | Count | Commits |
|----------|-------|---------|
| **fix — production-source** (compile-fix, no algorithm/behavior change) | 1 | 8b9ed10 |
| **fix(perf) — visibility/cfg-gating** | 2 | 25d6ac4, e886ea4 |
| **fix — test-only** | 2 | 0d23e7f, 2f16ba6 |
| **docs-only** | 9 | baa91cc, 6e5c067, d48d7ba, 5c17cc3, 800ee86, db63aed, 28663e4, b57f988, 85dacfc |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED**. H2's `internals`-gating extension (25d6ac4 + its fallout
fix), like wave 2's `dbb4016`, narrows what compiles WITHOUT the opt-in
`internals` feature — it does not add or change any `production`-default
behavior. H1's compile fix (8b9ed10 + its own follow-up 0d23e7f) restores
buildability under feature combinations that were red before it — no
shipping algorithm changed. `2f16ba6`'s flake fix serializes two
pre-existing tests reading process-wide diagnostic counters, unrelated to
any shipping behavior.

## §2. Zero-trust discovery: `npm run check`'s own full-matrix run caught
real gaps H2/H1's own per-task verification missed

Both `e886ea4`/row 9 and `0d23e7f`/row 10 exist because running the FULL
`npm run check` pipeline (all 6 clippy rows, all 4 test-feature combos,
including `--all-features`) AFTER H1-H8 individually landed surfaced two
real compile/test failures that each task's OWN narrower verification
(scoped to `production internals` or similar) did not exercise:
`examples`/`benches` are separate Cargo targets with independent
`required-features`, invisible to a library-only `cargo check`; and
`--all-features` uniquely combines `experimental`/`pinning`/
`bench-internals`/`batch-api`, a combination no single H-task's own
verification step happened to reach. `2f16ba6`/row 11 is a third,
unrelated discovery from the same full-matrix run: a pre-existing flaky
test this session did not cause but that the run surfaced regardless.
This is the concrete value of running the FULL gate before closing a
wave, not just each task's own scoped verification — consistent with
CLAUDE.md's own "npm run check before every push" rule.
