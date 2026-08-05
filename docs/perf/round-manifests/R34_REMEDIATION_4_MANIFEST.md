# Round 34 remediation wave 4 manifest — commit classification & verdict

**Generated 2026-08-05 — task #585/I7.** See `R34_REMEDIATION_1_MANIFEST.md`
for the scope-redefinition rationale.

This file covers **wave 4**: remediation of the closing review of wave 3
(`docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`,
findings F1-F10 — 1×P1, 3×P2, 3×P3, 3×P4), tasks filed as I1-I10 (#579-588).

**NOT YET FINAL — this wave is still in progress at the time this file is
written** (I7/this task's own commit will land after this file, and the
wave's closing checkpoint/CHANGELOG/`@oh`-review sequence has not started).
Per the convention wave 3's manifest established (its own §"Convention for
future waves"): **this wave's LAST commit should update THIS file, listing
itself, in the same commit** — avoiding the "one more commit still to
land" residual wave 3's manifest carried through two rounds of extension
before finally closing.

## §1. Commit classification (verbatim from `git log`, as of this commit —
INCOMPLETE, see caveat above)

Reproduce: `git log --reverse --format="%H %s" 85dacfc..HEAD` (upper bound
will grow — this task's own commit and the wave-closing sequence are not
yet included).

| # | SHA (full) | Commit prefix | Subject (truncated) | Finding | Category |
|---|------------|---------------|---------------------|---------|----------|
| 1 | `3d57a266729d04e4b6c1d0889033e41be850c770` | `fix(perf)` | close F1 — HeapCore stack-pressure budget didn't cover production medium-classes | F1 [P1] | **fix(perf) — production-source** (a genuine CI-red compile failure under `production medium-classes`; `src/registry/heap_core.rs` + `tests/r34_18_heap_core_stack_pressure_pin.rs`) |
| 2 | `b1a9b7b05fa81e74ab80e4006002d5d8a8e022d3` | `test` | close F4+F10 — 39 test files missing internals gate; tighten the verify script | F4+F10 [P2/P4] | **test-only** (mechanical `#![cfg]` fixes across 39 `tests/*.rs` files + a script tightening; no `src/` behavior change) |
| 3 | `ba180719d56902a4f2e9d4b397f63c7f94a97f08` | `docs` | close F3+F8 — move BREAKING CHANGE heading out of the Round-34 bullet list | F3+F8 [P2/P4] | **docs-only** |
| 4 | `7a9b7c7120a18b662e0b62805836f04df944660d` | `fix(perf)` | close F5 — gate SeferAlloc::dbg_trim_current_thread behind internals | F5 [P3] | **fix(perf) — visibility/cfg-gating** (same class as wave 3's `25d6ac4`/`e886ea4`) |
| 5 | `addc63dddde11bc6146dba35290b27b4d4eb82df` | `docs` | close F6 — renumber colliding CORRECTNESS_OPEN_ITEMS.md items 17/18 | F6 [P3] | **docs-only** |
| 6 | `04c2f7425aa18d627ebc3cc05f74b8047c61bf30` | `docs` | close F2 — fix the remaining 8 of 13 orphaned SHA citations H4 missed | F2 [P2] | **docs-only** |
| 7 | `650b818742ab534ea8bdf40dfaf406e841657099` | `docs(config)` | close F9 — scripts/check-all.mjs's stale "5 clippy rows" count | F9 [P4] | **docs-only** |

### Aggregate counts (as of this commit, incomplete — see caveat above)

| Category | Count | Commits |
|----------|-------|---------|
| **fix(perf) — production-source** | 1 | 3d57a26 |
| **fix(perf) — visibility/cfg-gating** | 1 | 7a9b7c7 |
| **test-only** | 1 | b1a9b7b |
| **docs-only** | 4 | ba18071, addc63d, 04c2f74, 650b818 |

**Net default-feature impact:** `production`'s feature composition is
**UNCHANGED**. F1's fix (`3d57a26`) is the only one touching a compile-time
budget an actual shipping `production medium-classes` composition hits —
it RAISES a stack-pressure assert's ceiling (8192 B -> 9216 B) to cover a
composition that was already shipping and already this size; it does not
change any algorithm or add/remove a code path. F5's fix narrows
`dbg_trim_current_thread`'s reachability under the non-default `internals`
feature, the same visibility-only class as wave 3's H2.

## §2. Zero-trust discovery: this wave's own findings, and what its own
remediation additionally found

This wave closes 10 findings (F1-F10) from an independent review of wave
3's work — the third review in this session's chain (wave1-review ->
wave1-fix -> wave2-review(x2) -> wave2-fix -> wave3-review -> wave3-fix ==
THIS wave). Notably, F1 (the one P1 finding) was itself a genuine CI-red
regression that predated this whole session (R34-18, Round 34) but was
inside H1's own remit to fully close — H1 only narrowed WHEN the assert
fires (excluding 4 named experimental features) without checking whether
OTHER shipping feature combinations also exceeded the original 8192 B
budget; `production medium-classes` (a real, CI-tested shipping opt-in)
did, at 8408 B. Fixed by raising the budget to cover the TRUE global
maximum (`--all-features` = 8840 B, the union of every feature this crate
has, hence provably the largest any composition can reach) and removing
the fragile per-feature exclusion list entirely — the assert is now
unconditional across every possible composition, closing this entire bug
class rather than patching the exclusion list a third time.
