# Round 34 remediation wave 1 manifest — commit classification & verdict

**Generated 2026-08-05 — task #576/H6**
(`docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H6).

## Why this file exists, and why it is separate from `R34_MANIFEST.md`

`R34_MANIFEST.md` covers Round 34 proper (`40241b0..c5db553`, 43 commits,
R34-1 through R34-27). That span is closed and will not grow further — its
own commit table is final and does not need re-deriving.

The work that followed Round 34's close is a chain of independent
review-and-remediate waves, each triggered by an `@oh`-style readonly
review of the PRIOR wave's own work (per this session's governing
`/babygoal` instruction: implement → checkpoint → review → repeat). By
task #576/H6, `R34_MANIFEST.md` had already been extended ONCE (task #550,
38→43 commits, itself correcting an original under-count) and was about to
need a THIRD extension to absorb 31 more post-closing commits across three
separate waves — the same staleness pattern recurring for a third time.

**Decision: redefine scope, do not extend again.** `R34_MANIFEST.md` is
frozen at Round 34 proper. Each remediation wave gets its OWN manifest
file, numbered by wave, so no single file's derivation command
(`git log --reverse ... <lower>..<upper>`) needs to keep silently drifting
out of date as new waves are appended — a reader who wants "the Round 34
proper" commits reads `R34_MANIFEST.md`; a reader who wants "what did wave
N fix" reads `R34_REMEDIATION_N_MANIFEST.md`. This is the more honest and
less staleness-prone option of the two considered (the other being a
fourth-and-onward perpetual re-extension of one ever-growing file).

This file covers **wave 1**: remediation of the closing `@oh`-style review
of Round 34 itself
(`docs/reviews/2026-08-05-round34-readonly-review.md`, findings F1-F8),
tasks #547-554.

## §1. Commit classification (verbatim from `git log`)

Reproduce: `git log --reverse --format="%H %s" c5db553..4f45eee`

| # | SHA (full) | Commit prefix | Subject (truncated) | Task | Category |
|---|------------|---------------|---------------------|------|----------|
| 1 | `4d52cfbd1a9a6e69593427bd10acaea57433f4b9` | `test` | add non-vacuous CI row for the `internals` boundary guard (F1) | #547 | **test-only** |
| 2 | `5e75032b35621299663234f68c593d2d399d4d1f` | `docs(config)` | re-derive OPEN_ITEMS.md [L]12 verdict off the corrected realloc ratio (F2) | #548 | **docs-only** |
| 3 | `55f831768c4ae2ac2d9e1645554f1fe9de94e103` | `docs` | close CORRECTNESS_OPEN_ITEMS item 15 (F-2) resolved-negative (F3) | #549 | **docs-only** |
| 4 | `80463d2596cae4b11d6d644f21cefb8e17398bc8` | `docs` | correct Round-34 manifest span + CHANGELOG R34-3 attribution (F5+F6) | #550 | **docs-only** |
| 5 | `358be4e25f279b8976c5fd83342525cd9ea4c3c5` | `docs(config)` | gzip-compress r34_23_runs tier-2 violator, index the deviation (F7) | #551 | **docs-only** |
| 6 | `a7d73958b52a5ae54bc853b1fce026a8201e3592` | `docs` | drop stale line-number citation; correct ALLOC_BENCH.md realloc figures (F4+F8) | #552 | **docs-only** |
| 7 | `4f45eeed50e6217b409437e3b00c807eb893fe78` | `docs` | CHANGELOG entry for F1-F8 remediation + session checkpoints | #554 | **docs-only** (wave-closing) |

### Aggregate counts

| Category | Count | Commits |
|----------|-------|---------|
| **test-only** | 1 | 4d52cfb |
| **docs-only** | 6 | 5e75032, 55f8317, 80463d2, 358be4e, a7d7395, 4f45eee |
| **fix(perf) / feat(api) / opt-in-source / bench-only** | 0 | — |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED**. No `src/` runtime behavior changed in this wave — every
commit is a doc correction, an open-items-index closure, an artifact
compression/indexing fix, or a new CI test row (F1's non-vacuous-guard
test exercises an EXISTING compile-time boundary, it does not add or
change any shipping code path).

## §2. Note on SHA stability — the G1 rebase

The table above already shows POST-rebase SHAs. Task #555 (G1, executed
during wave 2, below) reworded 3 commit MESSAGES via a scripted, verified
`git rebase -i`; two of the three reworded commits are actually inside
THIS wave's span, not wave 2's: `73817ee`→`5e75032` (row 2 above) and
`d46c349`→`a7d7395` (row 6 above) — the rebase reached back one wave to
fix pre-existing taxonomy-violating messages. The third
(`a4dc38e`→`ff496c6`) is a genuine wave 2 commit; see
`R34_REMEDIATION_2_MANIFEST.md` §2. Content (tree) was verified
byte-identical before/after the rebase; only the two commits' MESSAGES
changed prefix — see task #574/H4's `docs/CORRECTNESS_OPEN_ITEMS.md`/
`CHANGELOG.md` fixes for the trail of docs that cited the pre-rebase SHAs
before this was caught.
