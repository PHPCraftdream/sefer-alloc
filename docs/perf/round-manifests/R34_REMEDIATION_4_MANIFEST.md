# Round 34 remediation wave 4 manifest — commit classification & verdict

**Generated 2026-08-05 — task #585/I7.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale.

This file covers **wave 4**: remediation of the closing review of wave 3
(`docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`,
findings F1-F10 — 1×P1, 3×P2, 3×P3, 3×P4), tasks filed as I1-I10 (#579-588),
PLUS the fallout wave-4's own post-landing `npm run check` full-matrix rerun
and an independent readonly review of wave 4 itself
(`docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`, findings
F1-F6) surfaced — the same "run the FULL gate, review the wave that closed,
fix what it finds" pattern established by wave 3's own §2 below.

**Correction note (K1, standalone release-readiness gap audit,
`docs/reviews/2026-08-05-release-readiness-gap-audit.md` finding R1):** an
intermediate state of this file (at landing commit `7c8628a`, the audit's
own reviewed HEAD) cited a stale self-referential SHA `9d62bf6` for row 13
— an orphaned pre-amend sibling commit, not an ancestor of the eventual
HEAD (the classic self-referential-hash problem: a commit cannot literally
embed its own hash without a further amend invalidating it). This was
already corrected in the SAME finalizing commit's second amend
(`6550d68`, landed BEFORE the gap-audit report was written) by replacing
the literal SHA with a non-circular self-reference — see row 13 and the §1
note below. No further action needed; recorded here so the audit's R1
citation doesn't read as still-open against this file.

**FINAL — closed 2026-08-05.** Extended once past the original 7-row table
(task #585/I7's own commit, `782b92e`) to absorb: the flaky-test fix `npm
run check` surfaced post-landing (`60ad847`), the wave-4 CHANGELOG entry
(`5e1c92d`), the HS readonly review commit (`888c6a9`), and its own
remediation (`2426dcc`) — plus this manifest's own finalizing commit,
listed below per the convention wave 3's manifest established (its own
§"Convention for future waves"): **the wave's LAST commit updates THIS
file, listing itself, in the same commit.**

## §1. Commit classification (verbatim from `git log`, FINAL)

Reproduce: `git log --reverse --format="%H %s" 85dacfc..HEAD` (this table's
own upper bound is the wave's true closing SHA — this file's own row 13
cannot literally cite its own commit SHA, a self-referential-hash problem:
amending the file to embed its own hash changes the hash it just embedded.
Resolve row 13's SHA via `git log -1 --format=%H -- <this file's path>`).

| # | SHA (full) | Commit prefix | Subject (truncated) | Finding | Category |
|---|------------|---------------|---------------------|---------|----------|
| 1 | `3d57a266729d04e4b6c1d0889033e41be850c770` | `fix(perf)` | close F1 — HeapCore stack-pressure budget didn't cover production medium-classes | F1 [P1] | **fix(perf) — production-source** (a genuine CI-red compile failure under `production medium-classes`; `src/registry/heap_core.rs` + `tests/r34_18_heap_core_stack_pressure_pin.rs`) |
| 2 | `b1a9b7b05fa81e74ab80e4006002d5d8a8e022d3` | `test` | close F4+F10 — 39 test files missing internals gate; tighten the verify script | F4+F10 [P2/P4] | **test-only** (mechanical `#![cfg]` fixes across 39 `tests/*.rs` files + a script tightening; no `src/` behavior change — one residual gap in this fix itself is F1 of the HS review, row 11 below) |
| 3 | `ba180719d56902a4f2e9d4b397f63c7f94a97f08` | `docs` | close F3+F8 — move BREAKING CHANGE heading out of the Round-34 bullet list | F3+F8 [P2/P4] | **docs-only** |
| 4 | `7a9b7c7120a18b662e0b62805836f04df944660d` | `fix(perf)` | close F5 — gate SeferAlloc::dbg_trim_current_thread behind internals | F5 [P3] | **fix(perf) — visibility/cfg-gating** (same class as wave 3's `25d6ac4`/`e886ea4`) |
| 5 | `addc63dddde11bc6146dba35290b27b4d4eb82df` | `docs` | close F6 — renumber colliding CORRECTNESS_OPEN_ITEMS.md items 17/18 | F6 [P3] | **docs-only** |
| 6 | `04c2f7425aa18d627ebc3cc05f74b8047c61bf30` | `docs` | close F2 — fix the remaining 8 of 13 orphaned SHA citations H4 missed | F2 [P2] | **docs-only** |
| 7 | `650b818742ab534ea8bdf40dfaf406e841657099` | `docs(config)` | close F9 — scripts/check-all.mjs's stale "5 clippy rows" count | F9 [P4] | **docs-only** |
| 8 | `782b92eba2eb2bec5a97fd3e2cd4879481023598` | `docs` | close F7 — finalize wave 3's manifest, start wave 4's | F7 [P3] | **docs-only** |
| 9 | `60ad8474a2d62438eefa628e580f83c132da25fb` | `fix` | serialize a pre-existing flaky test on process-wide trim counters | — | **fix — test-only** (`tests/r31_10_trim_current_thread_api.rs`; a pre-existing flake `npm run check`'s own post-landing `--all-features` rerun surfaced, task #589) |
| 10 | `5e1c92d9d9f04cffc691afc596835de4828c000b` | `docs` | CHANGELOG entry for wave 4 release-readiness remediation (tasks #579-589) | — | **docs-only** (wave-closing CHANGELOG entry) |
| 11 | `2426dcc0e4d9bcb868a5b781843a25fe544cd15c` | `fix` | close F1 — exhaustive-verify scanner false-PASS on a doc-comment mention of #![cfg] | HS-F1+F2+F5+F6 | **fix — test/tooling-only** (`tests/medium_classes_correctness.rs` + `tests/medium_classes_wide_correctness.rs` gained real `internals` cfg; `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` rewritten to a line-anchored parser; `scripts/check-all.mjs` step-numbering fixed; `src/registry/heap_core.rs`'s doc comment precision-corrected — no shipping/opt-in algorithm changed) |
| 12 | `888c6a9d9c024f92da3033d838c97bfa3261cda7` | `docs` | add HS readonly review of wave-4 (85dacfc..60ad847) | — | **docs-only** (review artifact, committed per explicit user request — see this manifest's own commit for the departure from this session's default "review reports stay uncommitted" convention) |
| 13 | *(this commit — see the self-referential-hash note above §1)* | `docs` | finalize wave-4 manifest + commit closing checkpoint | — | **docs-only** (this file's own finalization; also commits `docs/checkpoints/2026-08-05-2211.md`) |

### Aggregate counts (FINAL — 13 of 13 commits)

| Category | Count | Commits |
|----------|-------|---------|
| **fix(perf) — production-source** | 1 | 3d57a26 |
| **fix(perf) — visibility/cfg-gating** | 1 | 7a9b7c7 |
| **fix — test-only** | 1 | 60ad847 |
| **fix — test/tooling-only** | 1 | 2426dcc |
| **test-only** | 1 | b1a9b7b |
| **docs-only** | 8 | ba18071, addc63d, 04c2f74, 650b818, 782b92e, 5e1c92d, 888c6a9, *(this commit)* |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED** across all 13 commits. F1's fix (`3d57a26`) raises a
stack-pressure assert's ceiling (8192 B → 9216 B) to cover a composition
that was already shipping and already this size — no algorithm or code path
added/changed. F5's fix (`7a9b7c7`) narrows `dbg_trim_current_thread`'s
reachability under the non-default `internals` feature, the same
visibility-only class as wave 3's H2. Row 9's flaky-test fix and row 11's
scanner/test-cfg fix are both test/tooling-only — no `production`-default
behavior changed by either.

## §2. Zero-trust discovery: this wave's own remediation work was itself
independently reviewed, and that review found a real defect in the
remediation

This wave closes 10 findings (F1-F10) from an independent review of wave 3's
work — the third review in this session's chain (wave1-review ->
wave1-fix -> wave2-review(x2) -> wave2-fix -> wave3-review -> wave3-fix ->
THIS wave). After landing all 10 (rows 1-8), an `npm run check` full-matrix
rerun surfaced one more pre-existing flaky test unrelated to any of this
wave's own changes (row 9, task #589) — the SAME "run the full gate before
closing a wave" discipline wave 3's own §2 established value for.

Separately, and more significantly: an independent readonly review of wave
4's own 9 commits (`docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`,
appearing mid-session from an `@oh`-style review agent launched earlier)
found a REAL, REPRODUCIBLE compile failure inside wave 4's own I4/F4 fix
(row 2, `b1a9b7b`) — the mechanical edit that added `internals` to 39 test
files' `#![cfg]` had, in two of those files
(`tests/medium_classes_correctness.rs`,
`tests/medium_classes_wide_correctness.rs`), inserted the `internals`
mention into a DOC COMMENT instead of the real crate-level attribute below
it. The new scanner that same commit added (specifically built to catch
exactly this class of gap) had its own bug: it matched `#![cfg(...)]`
against the whole raw file text, so the doc-comment mention produced a
false PASS. Confirmed via `cargo check --features "production
medium-classes" --test medium_classes_correctness`: 20 genuine E0599
errors. Fixed in row 11 (`2426dcc`), verified non-vacuous via a
counterfactual (stashing the test-file fix and re-running the fixed
scanner correctly reproduces both violations). This is the same
zero-trust-catches-what-self-verification-misses pattern that has recurred
throughout this session's whole review chain — each wave's own fix
introduces at least one gap that only an INDEPENDENT read catches, which is
the entire reason this chain keeps running an external review after every
wave rather than trusting a wave's own internal verification as sufficient.
