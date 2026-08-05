# Round 34 remediation wave 2 manifest — commit classification & verdict

**Generated 2026-08-05 — task #576/H6.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale (why this is a separate file rather
than a further extension of `R34_MANIFEST.md`).

This file covers **wave 2**: remediation of two parallel independent
reviews of wave 1's own work —
`docs/reviews/2026-08-05-sol-release-readonly-review.md` (a deep
release-readiness audit, findings Sol-F1 through Sol-F7) and
`docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md` (a
re-verification of wave 1, findings G1-G7 plus a bonus) — tasks #555-570,
plus 3 directly-adjacent cleanup commits landed in the same closing pass
(the CHANGELOG-consolidation CI guard, a fabricated-URL fix caught during
that guard's own review, and a stale-clippy-row-count fix caught while
reading a third review found during closing,
`docs/reviews/2026-08-05-fallback-release-readonly-review.md`).

## §1. Commit classification (verbatim from `git log`)

Reproduce: `git log --reverse --format="%H %s" 4f45eee..2a7f1e6`

| # | SHA (full) | Commit prefix | Subject (truncated) | Task | Category |
|---|------------|---------------|---------------------|------|----------|
| 1 | `d17eec38963de7d03efd8261e2daf7b3dab3c338` | `docs(config)` | correct Cargo.toml's false blanket no-panic/no-abort contract (Sol-F3) | #565 | **docs-only** |
| 2 | `dbb40163c86f914ea663760743168c9c8b473846` | `fix(perf)` | AllocCore::dbg_* inherent methods now genuinely require `internals` (Sol-F1) | #563 | **fix(perf) — visibility/cfg-gating** (no algorithm changed, see task #578/H8's decision to leave this prefix as-is) |
| 3 | `61905260ab204d2c7b3b1747401d7b1b2ace3850` | `docs` | correct R34-11 release narrative — remove false unbounded-spin claim (Sol-F4) | #566 | **docs-only** |
| 4 | `ff496c6bafe9cd77bd9c62ab804bf27395d8e9f8` | `docs` | tighten DrainHeadPublish's panic-safety contract wording (Sol-F5) | #567 | **docs-only** |
| 5 | `1f1015adf0cb381e8ea03fbf87161afc3ea2447d` | `docs` | narrow InitStateGuard doc claim to its actual guarantee (Sol-F6) | #568 | **docs-only** |
| 6 | `45c45be2974fc87ef61eee2e4a49a818dec28d83` | `docs` | state RemoteFreeRing cached_head's wrap/preemption assumption explicitly (Sol-F7) | #569 | **docs-only** |
| 7 | `0302a1420ba74c1ce5c2815d7db2df430b74c813` | `docs` | append pointer note to R34-24's stale "38 commits" CHANGELOG bullet (G2) | #556 | **docs-only** |
| 8 | `c7bf4ccd2aa86924f3a527ba134186d1456fb996` | `docs` | add raw-log count + total size census to R34_MANIFEST.md (G3) | #557 | **docs-only** |
| 9 | `8f6c3c16a871ca4de65eaa2fe5704d59e9498d4d` | `docs` | add correction note for stale ~40x realloc figure in OPEN_ITEMS_ARCHIVE.md (G4) | #558 | **docs-only** |
| 10 | `acdb1dcaed98781d693c831e8186921ad7cab7fa` | `docs` | fix OPEN_ITEMS.md item-40 collision (G6) | #560 | **docs-only** |
| 11 | `5047d4dbd7b246dbb85c6745e21f22204835956d` | `docs` | file CORRECTNESS_OPEN_ITEMS.md entry for pre-existing Round-34 commit-prefix violations (G1-bonus) | #562 | **docs-only** |
| 12 | `9d7bd9ebd11fcf8b9992fc75f1a4dc043ce206e8` | `docs` | fix item-number collisions 13 and 38 in OPEN_ITEMS.md (G6-followup) | #570 | **docs-only** |
| 13 | `8edd8c872b4c79d0ddd603ad694db57eda0bfccc` | `docs` | consolidate CHANGELOG — remove false 0.3.0 release date, merge [Unreleased] into single section (Sol-F2, part 1 — later found structurally incomplete by H3/task #573, corrected in wave 3) | #564 | **docs-only** |
| 14 | `a6484ca0766e6e592a686c5273875d3c9bb8dd63` | `build` | add CHANGELOG consolidation guard to release workflow (Sol-F2, part 2) | #564 | **build-tooling** |
| 15 | `9c5ea6437f140b863b467b55fd6881b26124d344` | `build` | remove fabricated GitHub issue URL from release.yml guard comment (self-caught during #564's own review) | — | **docs-fix** (comment-only) |
| 16 | `eb66af62efdc5071aa9bac7fd0888dd69e51724f` | `docs(config)` | fix stale "5 clippy rows" CI comment, now 6 (caught reading `docs/reviews/2026-08-05-fallback-release-readonly-review.md` during closing) | — | **docs-only** |
| 17 | `2a7f1e6e1e8f2834e02fefb26883b378c5371868` | `docs` | CHANGELOG entry for release-readiness remediation wave + session checkpoint (tasks #555-570) | #554-closing | **docs-only** (wave-closing) |

### Aggregate counts

| Category | Count | Commits |
|----------|-------|---------|
| **fix(perf) — visibility/cfg-gating** | 1 | dbb4016 |
| **build-tooling / docs-fix (release.yml)** | 3 | a6484ca, 9c5ea64, eb66af6 |
| **docs-only** | 13 | d17eec3, 6190526, ff496c6, 1f1015a, 45c45be, 0302a14, c7bf4cc, 8f6c3c1, acdb1dc, 5047d4d, 9d7bd9e, 8edd8c8, 2a7f1e6 |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED**. `dbb4016`'s `internals`-gating change narrows what compiles
WITHOUT the opt-in `internals` feature (which is not part of `production`)
— it removes reachable surface under a non-default feature combination, it
does not add or change any `production`-default behavior.

## §2. SHA stability note — the G1 rebase

Task #555 (G1), itself executed during this wave, performed a scripted,
verified `git rebase -i` REWORDING 3 commits' messages — but its 3 REWORDED
COMMITS are not all inside this wave's own span: `73817ee`→`5e75032` and
`d46c349`→`a7d7395` are wave 1 commits (`R34_REMEDIATION_1_MANIFEST.md`
items 2 and 6 — the rebase reached back one wave to reword pre-existing
taxonomy-violating commit messages), while `a4dc38e`→`ff496c6` (item 4
above, Sol-F5) IS this wave's own commit. The tables in both manifests
already reflect the POST-rebase SHAs. See task #574/H4's
`docs/CORRECTNESS_OPEN_ITEMS.md`/`CHANGELOG.md` fixes for the trail of
docs that cited the pre-rebase SHAs before this was caught.
