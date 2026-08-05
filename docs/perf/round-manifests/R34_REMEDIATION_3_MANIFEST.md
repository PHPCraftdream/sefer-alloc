# Round 34 remediation wave 3 manifest — commit classification & verdict

**Generated 2026-08-05 — task #576/H6.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale.

This file covers **wave 3**: remediation of the closing review of wave 2
(`docs/reviews/2026-08-05-sol-remediation-readonly-review.md`, findings
H1-H8), tasks #571-578 — sequenced strictly in priority order per this
session's explicit governing instruction ("fix all red/broken things
first, then review findings, then a new review"): H1→H2→H3→H4→H5→H7→H8→H6.

**This manifest is NOT the final word on wave 3's commit count.** Per this
task's own instructions (H6 is deliberately sequenced near the end but
still needs to commit itself, and the wave's closing checkpoint +
CHANGELOG-entry + markdown commit + `@oh` review have not landed yet at
the time this file is written), at least 2 more commits will land after
this one: this task's own commit, and the wave-closing
checkpoint/CHANGELOG commit the `/babygoal` ritual requires. Rather than
risk a fourth silent under-count, this file states that explicitly instead
of pretending to be final — the reader should re-run the derivation
command below against current `HEAD` if an exact final count matters.

## §1. Commit classification (verbatim from `git log`, as of this commit)

Reproduce: `git log --reverse --format="%H %s" 2a7f1e6..HEAD` (upper bound
`HEAD` will include more commits after this one lands — see caveat above).

| # | SHA (full) | Commit prefix | Subject (truncated) | Task | Category |
|---|------------|---------------|---------------------|------|----------|
| 1 | `8b9ed1041eff7ccbda38df1f47580ea1adc553cb` | `fix` | resolve two compilation failures blocking npm run check (H1) | #571 | **fix — production-source** (test `#![cfg]` gate + a `#[cfg]`-scoped compile-time assert; `src/registry/heap_core.rs` + `tests/r34_18_heap_core_stack_pressure_pin.rs`) |
| 2 | `25d6ac4d23b4859b726724424e5912dc54fe0bf0` | `fix(perf)` | close H2 — gate the remaining 31 AllocCore::dbg_* methods behind internals | #572 | **fix(perf) — visibility/cfg-gating** (same class as wave 2's `dbb4016`, see task #578/H8) |
| 3 | `baa91cc9b7ff2b8d859fc6b3a7eb6f42a626d5c6` | `docs` | close H3 — genuinely consolidate CHANGELOG.md under one [0.3.0] header | #573 | **docs-only** |
| 4 | `6e5c06732f135d9dbbf542d14fcd619475d82ede` | `docs` | close H4 — fix 10 pre-rebase SHA citations invalidated by G1's rebase | #574 | **docs-only** |
| 5 | `d48d7bafac0911af69ada200ecc53a54fadcdf43` | `docs` | close H5 — cross-file Sol-F5/Sol-F6's residuals into the correctness index | #575 | **docs-only** (2 one-line `src/` doc-comment cross-references, no behavior change) |
| 6 | `5c17cc37872eaa44cf4732251063435743c1cf86` | `docs` | close H7 — add a BREAKING CHANGE entry for the internals surface narrowing | #577 | **docs-only** |
| 7 | `800ee8668b2971a81d6f707ef525373ddcf82c0f` | `docs` | close H8 — record the decision to leave dbb4016's fix(perf) prefix as-is | #578 | **docs-only** |

*(H6/task #576's own commit — the one creating this file — is not listed
in its own table; it will appear as row 8 in a future re-derivation if one
is ever performed.)*

### Aggregate counts (as of this commit, incomplete — see caveat above)

| Category | Count | Commits |
|----------|-------|---------|
| **fix — production-source** (compile-fix, no algorithm/behavior change) | 1 | 8b9ed10 |
| **fix(perf) — visibility/cfg-gating** | 1 | 25d6ac4 |
| **docs-only** | 5 | baa91cc, 6e5c067, d48d7ba, 5c17cc3, 800ee86 |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED**. H2's `internals`-gating extension (25d6ac4), like wave 2's
`dbb4016`, narrows what compiles WITHOUT the opt-in `internals` feature —
it does not add or change any `production`-default behavior. H1's compile
fix (8b9ed10) restores buildability under feature combinations that were
red before it (a stack-pressure test gate + a compile-time size assert's
own `#[cfg]` scope) — no shipping algorithm changed.

## §2. Known follow-up

Once the wave's closing checkpoint/CHANGELOG/`@oh`-review sequence lands,
re-run `git log --reverse --format="%H %s" 2a7f1e6..<final-HEAD>` and
extend this table with the remaining rows (this task's own commit, the
closing commit(s), and anything a follow-up review finds) — a bounded,
single-wave extension, not a repeat of the cross-wave under-count pattern
`R34_MANIFEST.md` accumulated three times before this file's scope
redefinition.
