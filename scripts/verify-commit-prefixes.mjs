// Commit-prefix lint for CLAUDE.md's R30-12 taxonomy (task #482, R31-5c —
// the third of three independent parts task #468 split into: #480
// (structural gate-report checks, commit `fabe6b4`), #481 (semantic
// gate-report checks, not yet done), #482 (THIS script — a lint over commit
// SUBJECT LINES, not over docs/perf/ report bodies at all; deliberately a
// separate script from scripts/verify-gate-report.mjs, a different class of
// artifact).
//
// THE RULE THIS ENFORCES (CLAUDE.md "Active rules", the bullet starting "A
// commit subject line's conventional-commit prefix must state whether
// runtime behavior actually changed", R30-12/task #461): a bare `perf(...)`
// prefix is reserved for a commit that actually changes what ships.
// Five-prefix taxonomy:
//   - `perf(runtime)` — production's feature composition, a default
//     constant/config, or an always-on hot-path algorithm actually changed.
//   - `perf(opt-in)`  — a non-default feature/profile's CODE changed (real
//     code, but behind a flag a user must opt into).
//   - `bench`         — ONLY a judge/probe/gate-report/benchmark harness
//     changed; no shipping or opt-in algorithm code changed at all.
//   - `docs(config)`  — an existing tuning/config option got documented;
//     no code changed at all.
//   - `fix(perf)`     — shipping or opt-in code changed to restore a
//     documented invariant / close a latent correctness defect, but NO
//     speedup is measured or claimed (R33-13/task #518; precedent: 5df56d3).
//     A perf-family prefix (direction-1 applies: it MUST touch src/Cargo.toml,
//     same as perf-runtime/perf-opt-in — a fix(perf) touching nothing outside
//     docs/examples/benches/tests/scripts should be bench/docs instead).
// A bare `perf(...)` or `perf:` subject on a commit whose diff touches
// nothing outside docs/examples/benches/tests/scripts is exactly the
// R30-12 finding (`79aad56`, `894e9e3`, `7c2c62d` were all real Round-29
// examples of this) — a reader skimming `git log` alone, with no bullet tag
// and no round header in view, is misled into believing the allocator got
// faster.
//
// NOT RETROACTIVE, same posture as every other non-retroactive rule this
// project's CLAUDE.md states (raw-log-truncation, summary-CSV,
// immutable-source-identity, derived-tables, entry-point, same-workload-
// regime — R30-12 itself says so explicitly: "no historical commit message
// is retagged... it governs new commits going forward only"). This script
// therefore NEVER walks past R30_12_RULE_COMMIT (the commit that added the
// rule to CLAUDE.md, `3f7db16`) — any range crossing that boundary is
// silently clipped at it, so history that predates the rule (e.g.
// `894e9e3 perf(docs): measure the large-cache headroom's idle-RSS
// floor...`, a real Round-29 commit that is CITED BY NAME in the rule's own
// text as the violation that motivated it) is never flagged by a script
// that did not exist when it landed.
//
// COMMENT-ONLY REFINEMENT (task #1117, lint half): Both direction checks are
// refined to distinguish comment-only src/ changes from real code changes.
// Two real defects this refinement catches:
//
// - `09f4d16` (docs(vmem): prefix) changed the public `Display` string of
//   `VmemError` — real shipping code under a docs prefix. The original script
//   printed a direction-2 WARNING and exited 0; the warning was skimmed past.
//
// - `b11d8be` (fix(perf)) had an entirely doc-comment-only `src/` delta —
//   direction 1 is PATH-based (`crates/aligned-vmem/src/…` counts as shipping
//   code) so it passed, though nothing but `///` comments changed.
//
// The fix adds a diff-content check: for perf-runtime / perf-opt-in /
// fix-perf commits, at least one CHANGED line (added or removed, ignoring the
// leading +/- of the diff hunk and blank diff lines) in a non-measurement-only
// path must NOT match the comment/attribute patterns (see hasNonCommentChange).
//
// For docs/docs-config prefixes, direction 2 is upgraded to ERROR when the
// src/ delta contains at least one NON-COMMENT changed line — this catches the
// `09f4d16` shape. A docs commit whose src/ delta is comment-only is legitimate
// and must NOT error (that's most docs(vmem) commits). For bench prefixes,
// direction 2 remains at WARNING in all cases — the bench-internals accessor
// pattern (`#[cfg(feature = "bench-internals")]` / `#[doc(hidden)] pub(crate) static
// ..._FAILURES: AtomicU64`) is a legitimate diagnostic-only pattern and is
// explicitly documented in the file header (commits 88592d7, d772d99).
//
// Two independent directions, both heuristic tripwires, not a precise
// classifier:
//
//   (1) perf(...)-prefixed or bare `perf:` commit whose diff touches
//       NOTHING outside docs/, examples/, benches/, tests/, scripts/ ->
//       FAIL. This is the direction CLAUDE.md's R30-12 rule text is
//       actually about — a measurement-only commit dressed as a runtime win.
//       A bare `perf:` (no explicit `(runtime)`/`(opt-in)` subscope) is
//       ALSO a FAIL even when the commit does touch src/ or Cargo.toml,
//       because R30-12's sanctioned taxonomy has no bare `perf:` member —
//       every real perf commit must say WHICH kind it is.
//
//       REFINEMENT (task #1117): even when the commit does touch src/ or
//       Cargo.toml, FAIL if every changed line in those paths is comment-only
//       (matches `^\s*(///|//!|//)`). This catches the `b11d8be` shape — a
//       fix(perf) commit whose src/ delta is entirely `///` doc comments.
//
//   (2) `bench(...)`/`docs(...)`/`build(...)`/bare `build:`-prefixed commit
//       whose diff DOES touch something outside docs/, examples/, benches/,
//       tests/, scripts/ (i.e. touches src/ or Cargo.toml) -> ERROR for
//       docs/docs-config, WARNING for bench and for build (direction 2).
//       This is a hidden-runtime-change-under-an-innocuous-prefix shape —
//       the opposite direction. `build` was added task #1168 (OX3/F7); see
//       the "BUILD: EXTENSION" note below for why it is WARNING-only, not
//       ERROR like docs.
//
//       For docs/docs-config: upgraded to ERROR after a wide-range scan showed
//       no false positives on historically-legitimate commits; the `09f4d16`
//       shape (a docs(vmem) commit that changed the public `Display` string of
//       `VmemError`) is exactly the real defect this catches. A docs commit whose
//       src/ delta is comment-only (matches hasNonCommentChange's patterns) is
//       NOT an error — that's most docs(vmem) commits, which legitimately fix
//       doc-comment content without changing behavior.
//
//       For bench: stays at WARNING because legitimate bench-internals
//       diagnostic accessor patterns exist (e.g. `#[cfg(feature =
//       "bench-internals")]` / `#[doc(hidden)] pub(crate) static ..._FAILURES:
//       AtomicU64` — see commits 88592d7, d772d99). A bench commit whose src/
//       delta is comment-only is also just a warning, same as docs.
//
// BUILD: EXTENSION (task #1168, OX3/F7): direction 2 now ALSO covers bare
// `build:` and scoped `build(...)`  — a THIRD, separate branch from the
// docs/bench one above, always WARNING (never promoted to ERROR).
//
// Why this branch exists at all: `39279b5` (docs(vmem): prefix, task #1160)
// and `cecdeec` (bare build: prefix, task #1164) make the EXACT SAME
// comment-only claim-strengthening edit to the same two files
// (`crates/aligned-vmem/src/api/decommit.rs`,
// `crates/aligned-vmem/src/api/reserve_aligned_huge.rs`) — both change a
// doc comment's claim about what CI evidence proves, on a publishable
// (docs.rs-facing) surface, with zero non-comment src/ delta. Direction 2
// flagged `39279b5` (docs prefix) but was silent on `cecdeec` (build
// prefix) purely because `build` was outside BENCH_OR_DOCS_RE — the same
// defect class direction 2 exists to catch, just reachable through a
// prefix the original check never examined.
//
// Why `build` gets its OWN WARNING-only branch instead of being folded into
// the docs/bench branch at ERROR severity: `build` is a standard
// conventional-commit TYPE for tooling/CI/dependency/structural work, not a
// "no shipping code changed" claim the way `docs(...)`/`bench(...)` are
// within R30-12's own five-prefix taxonomy (`build` isn't even IN that
// taxonomy). A wide-range scan of every `build:`/`build(...)` commit since
// the R30-12 rule commit (60 commits, `git log
// 3f7db1629d389c18ae987120f4094aaccf04f81f..cecdeec` filtered to `^build`)
// splits into 44 / 6 / 10. The 44 touch NOTHING outside docs/examples/
// benches/tests/scripts/.github/ (`outside.length === 0` under this
// script's own isMeasurementOnlyPath). The 10 DO touch something outside
// those prefixes with REAL (non-comment) content — all ten, enumerated in
// full (task #1185): `a4b8e50`'s module-per-file split, `c75aa59`'s
// crate-directory rename, `d72b6d7`'s version bump, `5daa90c`'s
// forced-page call-site regression test + guard rule, dependency
// hoists/removals (`56d0764`, `57c4510`), and — worth naming because they
// are the least obvious members of this bucket — `503f703`/`f3020fd`,
// whose only outside-prefix path is `.gitignore`, and `eb6935b`/`fabe6b4`,
// whose only outside-prefix path is `package.json`.
// Those four touch NO `src/` or `Cargo.toml` path at all (verified per
// commit, task #1178), yet each adds one non-comment line to a repo-root
// file that no measurement-only prefix covers, which is exactly why they
// land here and not among the 44. None of the 10 is a
// claim-strengthening defect; promoting this branch to ERROR the way docs
// is would have made all 10 false positives, in direct tension with the
// "must not start noise on legacy commits" requirement direction 2 has
// always had to clear before being tightened. Only 6 of the 60 have a
// comment-only delta in that outside-prefix set (the actual defect shape);
// 5 of those 6 are the same family as `cecdeec` — a doc-comment claim
// revised on `crates/aligned-vmem/src/**` — and the sixth, `88c5f3c`, is a
// same-shape comment-only revision but on `crates/aligned-vmem/Cargo.toml`
// (25 added lines, all `#`-prefixed TOML comments) rather than `src/**`.
// WARNING, not ERROR, keeps this branch visible to a reviewer without
// blocking the much larger set of legitimate build: commits that touch
// src/ (or Cargo.toml, or any other outside-prefix path) for real reasons
// unrelated to claim strength.
//
// Why adding this branch cannot have changed the historical FAILURE set in
// EITHER direction (task #1178/OX4): the `kind === 'build'` branch pushes
// only to `warnings`, so it cannot have ADDED a failure — but that alone
// does not rule out having REMOVED one, since a branch that intercepts
// commits before they reach a stricter check could silence a failure they
// would otherwise have hit. The branch cannot have done that either, for a
// structural reason: `classifySubject` tries the `build` branch (via
// `BUILD_RE`) only AFTER every perf/fix-perf/bench/docs branch above it has
// already failed to match, and immediately before the final catch-all
// `return 'other'` — see the `if (BUILD_RE.test(subject)) return 'build';`
// line right before that `return 'other';` below. Every `build:`/
// `build(...)` commit therefore classified as `'other'` before this branch
// existed (`BUILD_RE` cannot also match `perf`/`fix(perf)`/`bench`/`docs`
// prefixes, so no build: commit was ever being caught by one of THOSE
// branches instead), and `'other'` is the one classification the main loop
// below has never run a single check against — see its own trailing
// comment ("not a perf/bench/docs(config) prefix at all; out of this
// lint's scope entirely"). A classification with zero checks cannot
// produce a failure, so the set of build: commits this branch now examines
// contributed zero failures before it existed. Adding a WARNING-only
// branch between "no check ran" and "no check ran" leaves the FAILURE set
// exactly as it was — not because of what the new branch pushes to, but
// because of what it intercepts commits FROM.
//
// Direction (1) is FAIL (exit 1): unlike a brand-new rule with zero
// real-world track record, R30-12 already has three independently-verified
// real violations in this project's own history (cited in CLAUDE.md's own
// rule text) that a script exactly this shape would have caught — the
// heuristic (path-prefix classification of `git show --stat`) is the same
// register of "cheap, already-proven-useful text scan" that
// scripts/verify-perf-gate-stubs.mjs and scripts/verify-gate-report.mjs
// both already hard-fail on, not a speculative new check.
//
// Usage:
//   node scripts/verify-commit-prefixes.mjs                  # @{u}..HEAD if
//                                                              # an upstream is
//                                                              # configured,
//                                                              # else the last
//                                                              # DEFAULT_LOOKBACK
//                                                              # commits
//   node scripts/verify-commit-prefixes.mjs <base>..<head>    # explicit range
//                                                              # (CI: PR base..head)
//   npm run check:commit-prefixes
//
// Deliberately ZERO cargo invocations — pure `git log`/`git show --stat`
// text scanning, same design register as verify-gate-report.mjs and
// verify-perf-gate-stubs.mjs (see both files' own header comments).
//
// PATH QUOTING (task #1218): `git` C-quotes any path containing non-ASCII
// bytes (core.quotepath defaults to true: the path is wrapped in double
// quotes with every non-ASCII byte octal-escaped), and this repo hit that
// for real on two Cyrillic-named review files (commits 2074646, 105cf53).
// Before task #1218 both consumers of git path output were blind to that
// form, in compounding ways. `changedPaths` passed the quoted line through
// raw, so `"docs/reviews/…"` failed isMeasurementOnlyPath's ^-or-/
// segment boundary (a `"` sits in front of `docs/`) and a file plainly
// under docs/ was classified OUTSIDE the measurement-only prefixes — a
// false direction-2 finding. And hasNonCommentChange's header regex
// /^diff --git a\/\S+ b\/(\S+)/ does not match git's quoted header form
// (`diff --git "a/…" "b/…"`, the a/ and b/ prefixes INSIDE the quotes), so
// every changed line of such a file was silently attributed to the
// PREVIOUSLY-SEEN file — which made hasNonCommentChange return false and
// DOWNGRADED that false positive from ERROR to WARNING: the visible
// symptom was a non-blocking false warning, the invisible one was that
// the guard never inspected the file's contents at all.
//
// The fix is three deliberate parts, not one config flag:
//   (a) git() below passes -c core.quotepath=false (plus
//       -c diff.noprefix=false and -c diff.mnemonicPrefix=false, pinning
//       the `a/… b/…` header shape against the user's gitconfig) so
//       non-ASCII paths arrive raw — the live condition in this repo;
//   (b) unquoteGitPath()/unescapeCStyle() decode git's C-style quoting,
//       because quotepath=false does NOT disable quoting for a path
//       containing a literal `"`, a backslash, or a control byte — git
//       quotes those unconditionally (see unquoteGitPath's doc comment);
//   (c) diffGitBPath() accepts the quoted header form and RESETS file
//       attribution to null on any unparseable header, so an unrecognized
//       shape is skipped outright rather than silently counted against
//       the previous file — the exact misattribution shape bug 2 was.
//
// STILL NOT HANDLED, deliberately: (1) a path whose bytes are not valid
// UTF-8 — execFileSync(encoding: 'utf8') has already replaced those bytes
// with U+FFFD before this script sees them; both parse sites decode
// identically so classification still agrees, but the printed path is
// lossy; (2) git's `diff --git a/x b/x` header is inherently ambiguous
// for an UNQUOTED path containing the byte sequence ` b/` — the split
// takes the LAST ` b/` (git itself cannot round-trip that shape); no
// space-containing path exists in this repo's history (verified over the
// full post-rule range 3f7db16..HEAD at fix time).

import { execFileSync } from 'node:child_process';
import { REPO_ROOT } from './lib.mjs';

// The commit that added the R30-12 rule to CLAUDE.md — this script never
// lints a commit at or before this SHA (the rule is explicitly non-
// retroactive; see this file's header comment). Full 40-hex SHA, verified
// via `git log -1 --format=%H 3f7db16` before hardcoding.
const R30_12_RULE_COMMIT = '3f7db1629d389c18ae987120f4094aaccf04f81f';

// Commits that ALREADY EXISTED when task #1114/#1117 strengthened the two
// checks below (direction 1 gained a "the src/ delta must contain a
// non-comment line" content test; direction 2's docs-prefix warning became an
// ERROR). CLAUDE.md's R30-12 is explicitly non-retroactive — "no historical
// commit message is retagged or amended by this rule; it governs new commits
// going forward only" — and the R14-10 raw-log rule and the R24-6
// `dbg_push_to_ring` decision are two more precedents in the same file for
// declining exactly this kind of retroactive cleanup. Without this list the
// mandatory pre-push gate would be permanently RED on landed history that
// cannot be fixed, which is worse than useless: a gate nobody can make green
// is a gate everybody learns to ignore.
//
// ONE ENTRY IS NOT A GENUINE MIS-SLOT ON LANDED HISTORY (task #1238):
// `2f9d7b9` is a heuristic FALSE-POSITIVE exemption. It did not pre-exist
// the check (it failed it on the day it was committed) — but unlike
// `c766951`, the other entry that post-dates the check, it is NOT a true
// positive with an honest alternative prefix the commit should have
// carried: its entire non-comment src/ delta is prose inside a string
// literal (the entry's reason line has the diff facts). It is exempted
// rather than fixing hasNonCommentChange to see string state, because that
// fix's failure mode is a SILENT miss — a real code line classified as
// prose, i.e. the exact `09f4d16` defect passing green — while a false
// FAILURE is loud and forces a human look; a guard made cleverer that
// claims a property it does not have is this repository's most-reproduced
// guard defect (task #1126's own subject: "both guards hardened last wave
// claimed properties they did not have"). It is also UNPUSHED, so unlike
// the landed entries its owner may still reword the commit by rebase; the
// exemption records that decision point, it does not foreclose it.
//
// SELF-CLEANING, deliberately: an entry that no longer fails is itself a
// FAILURE (see the check after the scan). An exception that has silently
// stopped applying is how a suppression list rots into a blanket.
//
// TWO mechanical guards on the list itself (task #1123):
//   (1) every entry's SHA must RESOLVE to a commit (`git cat-file -e`),
//       checked on EVERY run regardless of the scanned range — a typo'd
//       entry would otherwise never be reported, because the stale check
//       only fires for SHAs inside the scanned range and the default range
//       legitimately excludes landed commits (fb7dac8 sits in origin/main,
//       outside `@{u}..HEAD`, so on the default range its absence from
//       every index was silent for exactly this reason). An existence check
//       is the right tool for the typo class because it is range-
//       independent and cheap; the in-range stale check remains the tool
//       for the behavioral class (an entry that stopped suppressing).
//       These failures are deliberately NOT exemptible — an entry key must
//       not be able to grandfather away the report of its own nonexistence.
//   (2) an entry OUTSIDE the scanned range is listed in the output
//       (informational, not a failure) so nothing about the list is silent.
//
// Each entry states WHY, because "grandfathered" without a reason is the
// shape this campaign keeps finding and correcting. Every entry also names
// its durable record — the sole record must never be this suppression list
// itself (fb7dac8 was in exactly that state until task #1123 added it to
// item 78).
const GRANDFATHERED = new Map([
  [
    'fb7dac8',
    'LANDED (in origin/main) before this check existed. docs(vmem) prefix on a ' +
      'commit whose src/ delta includes a changed assert! panic-message string. ' +
      'Recorded, not amended: rewriting pushed history is what R30-12 forbids. ' +
      'Durable record: docs/CORRECTNESS_OPEN_ITEMS.md item 78, sub-card 5 ' +
      '(task #1123 — until then this entry appeared in NO index; its sole ' +
      'record was this list itself).',
  ],
  [
    '09f4d16',
    'docs(vmem) prefix on a commit that changed VmemError\'s public Display ' +
      'string. Recorded in docs/CORRECTNESS_OPEN_ITEMS.md item 78 (task #1117) ' +
      'with the full evidence, including that this script WARNED at the time ' +
      'and the warning was skimmed past — which is why the warning is now an ' +
      'error for every commit after this list.',
  ],
  [
    'b11d8be',
    'fix(perf) prefix on a commit whose entire src/ delta is doc comments. ' +
      'Recorded in item 78 (task #1117).',
  ],
  [
    'c766951',
    'fix(perf) prefix on a commit with NO src/ path at all (docs/ + tests/ ' +
      'only). Caught by this very check within an hour of the check landing — ' +
      'the correct slot was bench: or docs(...). Durable record: item 78, ' +
      'sub-card 4 (added task #1123 — the record commit c766951 itself landed ' +
      'BEFORE the lint commit, and the card was never updated when this fourth ' +
      'entry appeared).',
  ],
  [
    '2f9d7b9',
    'NOT a genuine mis-slot on landed history — the first FALSE-POSITIVE ' +
      'exemption (task #1238): unlike c766951, the other entry that ' +
      'post-dates this check, there is NO honest alternative prefix the ' +
      'commit should have carried. docs: prefix on a task-#1227 citation ' +
      'repair whose ENTIRE non-comment src/ delta is two string-literal ' +
      'continuation lines inside the deliberately compile-failing MIPS-only ' +
      'compile_error! in src/os/unix.rs (the docs/CORRECTNESS_OPEN_ITEMS.md ' +
      'citation gaining "item 62", plus the re-wrap); the other three src/ ' +
      'files are ///-comment-only (re-verified per file). hasNonCommentChange ' +
      'cannot see string state — a --unified=0 diff carries no opening quote ' +
      '— and is kept deliberately dumb: reconstructing quote state without ' +
      'parsing Rust (raw strings, escapes, char-vs-lifetime, comments that ' +
      'themselves contain quotes) would trade this loud false positive for a ' +
      'silent miss — the 09f4d16 defect class passing green; guards made ' +
      'cleverer claiming properties they did not have is this repository\'s ' +
      'most-reproduced guard defect. fix(perf), the one prefix whose checks ' +
      'this commit would pass, would assert a shipping-code fix that did not ' +
      'happen. UNPUSHED, unlike the four landed entries: the owner may still ' +
      'reword it by rebase — this exemption records that decision point, it ' +
      'does not foreclose it. Durable record: item 78, sub-card 6 ' +
      '(task #1238).',
  ],
  [
    'e25ec74',
    'NOT a genuine mis-slot — a second FALSE-POSITIVE exemption (task ' +
      '#1335), same class as 2f9d7b9 through a different macro: docs(numa-shim): ' +
      'prefix on a task-#1333 doc-honesty commit whose ENTIRE crates/numa-shim/' +
      'src/lib.rs delta is //!/// doc-comment lines (already correctly ' +
      'recognized as comment by hasNonCommentChange). The one non-comment ' +
      'changed line in the whole commit is Cargo.toml\'s `description = "..."` ' +
      'key-value line — genuine TOML content, not a #-comment, so the regex ' +
      'correctly does not suppress it — but it is crates.io-facing PACKAGE ' +
      'METADATA PROSE (the same "zero C library dependencies" -> "zero ' +
      'third-party C/C++ dependencies" wording correction the doc comments ' +
      'already carry, kept in sync per this crate\'s own convention), not ' +
      'shipping/opt-in code; Cargo.toml\'s version field is untouched. No ' +
      'honest alternative prefix fits better — docs(numa-shim): is correct. ' +
      'Not taught to the guard as a new exception path (a "Cargo.toml ' +
      'description field" special case would be exactly the guard-cleverness ' +
      '2f9d7b9\'s own entry warns against — see that entry\'s reasoning, ' +
      'which applies unchanged here). UNPUSHED: the owner may still reword it ' +
      'by rebase — this exemption records that decision point, it does not ' +
      'foreclose it. Durable record: item 78, sub-card 7 (task #1335).',
  ],
  [
    'eaa3310',
    'NOT a genuine mis-slot — a third FALSE-POSITIVE exemption (task ' +
      '#1505), same class as e25ec74: docs(size-classes): prefix on a ' +
      'task-#1473 commit whose ENTIRE diff is one line, ' +
      'crates/size-classes/Cargo.toml\'s `description = "..."` key-value ' +
      'string, reworded from a changelog-fragment-shaped parenthetical to ' +
      'a self-contained sentence. Crates.io-facing PACKAGE METADATA PROSE, ' +
      'not shipping/opt-in code; `version` untouched. No honest alternative ' +
      'prefix fits better. Not taught to the guard as a new exception path ' +
      '(same guard-cleverness risk e25ec74\'s entry already warns against). ' +
      'UNPUSHED: the owner may still reword it by rebase — this exemption ' +
      'records that decision point, it does not foreclose it. Found by an ' +
      'independent oxx prepublish review of size-classes, which also found ' +
      'the current origin/main landing SHA is CI-red for an unrelated ' +
      'reason (clippy::large_const_arrays, already fixed locally, never ' +
      'pushed). Durable record: item 78, sub-card 8 (task #1505).',
  ],
  [
    'abf2061',
    'NOT a genuine mis-slot -- a fourth FALSE-POSITIVE exemption (task ' +
      '#1514), same class as eaa3310/e25ec74: docs(size-classes): prefix ' +
      'on a task-#1514 commit (itself fixing oxx P4-3) whose ENTIRE diff ' +
      'is one line, crates/size-classes/Cargo.toml\'s `description = "..."` ' +
      'value, shortened for crates.io search-result truncation. Same ' +
      'reasoning as eaa3310\'s entry applies verbatim. UNPUSHED. Durable ' +
      'record: item 78, sub-card 9 (task #1514).',
  ],
  [
    '66f47d2',
    'A GENUINE mis-slot, same defect class as 09f4d16/fb7dac8 (sub-cards ' +
      '1/5) -- NOT a heuristic false positive like every other entry in ' +
      'this list. The commit\'s one non-comment src/ line is a real ' +
      'Display::fmt format-string change on InvalidAlign (task #1509): ' +
      '"class_for: align ..." -> "try_class_for: align ...". docs(...) ' +
      'requires "no code changed at all"; the honest prefix would have ' +
      'been a plain fix(size-classes):. UNLIKE 09f4d16/fb7dac8 this is ' +
      'UNPUSHED, so a rebase to reword it was available -- deliberately ' +
      'not taken: rewriting local history for one string-literal wording ' +
      'fix is exactly the kind of git-history action this repo\'s own ' +
      'conventions reserve for explicit owner request, and InvalidAlign ' +
      'has zero real consumers (size-classes has never been published, ' +
      'task #660), so the "real Display change" risk 09f4d16\'s entry ' +
      'warns about does not apply here the way it did for an already-' +
      'shipped VmemError. First entry in this list that is a genuine ' +
      'mis-slot rather than a guard heuristic gap -- recorded as such, ' +
      'not relabeled as a false positive to fit the pattern. Durable ' +
      'record: item 78, sub-card 10 (task #1514).',
  ],
  [
    '4c332ab',
    'A GENUINE mis-slot, same defect class as 09f4d16/fb7dac8/66f47d2 ' +
      '(sub-cards 1/5/10) -- NOT a heuristic false positive. docs(size-classes) ' +
      'prefix on a size-classes round-3 review commit whose src/lib.rs delta ' +
      'bundles three genuine comment-only fixes with one real internal-' +
      'implementation change: the round-3 review\'s P3-5 finding removed the ' +
      'now-redundant `small_max` struct field (its only two readers, ' +
      '`small_max()` and `Debug`, now read `self.table[N - 1]` directly). ' +
      'Behavior-preserving (small_max() returns the same value; Debug\'s ' +
      'printed output unchanged, confirmed by the pre-existing ' +
      'debug_impl_prints_a_summary_not_the_raw_tables test staying green) and ' +
      'no speedup measured or claimed, so the honest prefix would have been ' +
      'fix(perf), matching the 5df56d3 PerClass repr(C) precedent CLAUDE.md ' +
      'itself cites for that slot. PUSHED, like sub-cards 1/5 -- un-amendable ' +
      'per R30-12\'s non-retroactive posture. Durable record: item 78, ' +
      'sub-card 11 (task #1584).',
  ],
]);

// A local run with no explicit range and no configured upstream falls back
// to the last DEFAULT_LOOKBACK commits. Chosen by looking at this repo's own
// cadence (`git log --oneline -60` at the time this script was written shows
// ~30-60 commits per multi-day round, several per hour during an active
// session) — 40 is comfortably more than one task's worth of commits
// (almost every task here lands as 1-2 commits) without being so large that
// a local ad-hoc run walks deep into already-pushed, already-reviewed
// history for no reason.
const DEFAULT_LOOKBACK = 40;

// Paths outside of which a `perf(...)` commit is expected to have SOME
// change for direction (1) — and inside of which alone it should NOT
// (`bench`/`docs(...)` should stay confined to these). Prefix-matched
// against each changed path's repo-relative POSIX form.
const MEASUREMENT_ONLY_PREFIXES = [
  'docs/',
  'examples/',
  'benches/',
  'tests/',
  'scripts/',
  '.github/', // CI workflows are infrastructure, not shipping code
];

// Repo-ROOT doc files (not under any prefix above, since they have no
// directory component at all) that are unambiguously documentation —
// CHANGELOG.md/README.md/CLAUDE.md are the three this repo actually edits
// routinely alongside a docs-only or bench-only commit (verified: every
// commit in this script's own non-vacuity range that touched a root file
// touched exactly one of these three). Exact basename match, not a prefix.
// Also includes any CHANGELOG.md file anywhere in the repo (e.g. in crate
// subdirectories) as these are always documentation.
//
// `.gitignore` (task #1514, oxx prepublish review of size-classes, found
// via the very FAILURE this addition prevents): a different category from
// the other three -- not documentation, but the same underlying property
// that earns the exemption (a file whose content can, BY DEFINITION, never
// be shipping/behavioral code -- its only possible content is git
// ignore-glob patterns). Unlike the deliberately-NOT-taught Cargo.toml
// `description` case (see the eaa3310/e25ec74 grandfather entries' own
// reasoning), a WHOLE-FILE basename exemption here carries none of that
// guard-cleverness risk: there is no other field/section of `.gitignore`
// that could hide real behavior change, the way `dependencies`/`version`
// coexist with `description` in Cargo.toml.
// `bench-iters.txt` (task #1531 follow-up): the root-level calibration
// manifest `bench-scale-tool` writes JIT-calibrated iteration counts to --
// generated data FROM running a benchmark harness, never hand-authored
// shipping/behavioral code, same category as CHANGELOG.md/.gitignore above.
const MEASUREMENT_ONLY_ROOT_FILES = new Set([
  'CHANGELOG.md',
  'README.md',
  'CLAUDE.md',
  '.gitignore',
  'bench-iters.txt',
]);

function git(args) {
  // Task #1218: three -c overrides, all pinning the OUTPUT SHAPE this
  // script's parsers assume, against the invoking user's gitconfig:
  //   - core.quotepath=false  — stop non-ASCII bytes from C-quoting a path
  //     (part (a) of the fix; see the PATH QUOTING header note).
  //   - diff.noprefix=false / diff.mnemonicPrefix=false — guarantee the
  //     `diff --git a/<src> b/<dst>` header shape (a user-global
  //     diff.noprefix=true would emit `diff --git src dst` and break
  //     diffGitBPath's unquoted branch; the OLD code failed on that shape
  //     too, by misattribution).
  // These do NOT make unquoteGitPath()/diffGitBPath() redundant — git
  // still quotes a path containing `"`, a backslash, or a control byte
  // even with quotepath=false (parts (b)/(c) of the fix).
  return execFileSync('git', ['-c', 'core.quotepath=false', '-c', 'diff.noprefix=false', '-c', 'diff.mnemonicPrefix=false', ...args], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
  });
}

function resolveRange(explicitRange) {
  if (explicitRange) return explicitRange;
  try {
    git(['rev-parse', '--verify', '--quiet', '@{u}']);
    return '@{u}..HEAD';
  } catch {
    return `HEAD~${DEFAULT_LOOKBACK}..HEAD`;
  }
}

/** Clip a `base..head` (or bare `base`, meaning `base..HEAD`) range so it
 * never walks past R30_12_RULE_COMMIT — i.e. effectively
 * `max(base, R30_12_RULE_COMMIT)..head`. Implemented by listing commits in
 * the requested range and dropping every one at-or-before the rule commit,
 * rather than trying to rewrite the range expression itself (simpler, and
 * works uniformly whether `base` is a SHA, a ref, or `@{u}`). */
function listShasInRange(range) {
  const out = git(['log', '--format=%H', range]).trim();
  return out ? out.split(/\r?\n/) : [];
}

function isAtOrBeforeRuleCommit(sha) {
  try {
    // --is-ancestor exits 0 if `sha` is an ancestor of (or equal to) the
    // rule commit.
    execFileSync(
      'git',
      ['merge-base', '--is-ancestor', sha, R30_12_RULE_COMMIT],
      { cwd: REPO_ROOT },
    );
    return true;
  } catch {
    return false;
  }
}

function toPosix(p) {
  return p.replace(/\\/g, '/');
}

/** Undo git's C-style path quoting (git's quote_c_style). A path is
 * wrapped in double quotes and backslash-escaped whenever it contains a
 * `"`, a backslash, or a control byte — and, under the default
 * core.quotepath=true, every non-ASCII byte as well. core.quotepath=false
 * (set on this script's git() helper, task #1218) removes ONLY the
 * non-ASCII trigger; the other three quote unconditionally, which is why
 * this function exists alongside the flag. Escapes handled: \a \b \f \n
 * \r \t \v \\ \" and \NNN octal byte escapes. Octal escapes are decoded
 * at the BYTE level and re-decoded as UTF-8, because quotepath=true
 * escapes each byte of a multi-byte character separately (\320\262 is two
 * escapes of ONE Cyrillic character). An unquoted input (the common case)
 * passes through unchanged. */
function unquoteGitPath(path) {
  // A path containing a `"` is ALWAYS quoted by git, so real unquoted
  // output can never START with one — a leading `"` is a reliable quote
  // marker, and the closing quote is the final character (any earlier
  // `"` is escaped as `\"`).
  if (path.length < 2 || !path.startsWith('"') || !path.endsWith('"')) {
    return path;
  }
  return unescapeCStyle(path.slice(1, -1));
}

/** Decode the escape sequences inside a git-quoted path's body (between
 * the surrounding double quotes). Collects decoded BYTES, then decodes
 * the byte string as UTF-8 in one shot; invalid sequences become U+FFFD,
 * the same replacement execFileSync's utf8 decoding already applies to
 * raw output, so quoted and unquoted spellings of the same path land on
 * identical strings. */
function unescapeCStyle(body) {
  if (!body.includes('\\')) return body;
  const SIMPLE_ESCAPES = {
    a: 0x07, b: 0x08, f: 0x0c, n: 0x0a, r: 0x0d, t: 0x09, v: 0x0b,
    '\\': 0x5c, '"': 0x22,
  };
  const bytes = [];
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch !== '\\') {
      for (const b of Buffer.from(ch, 'utf8')) bytes.push(b);
      continue;
    }
    const esc = body[++i];
    if (esc === undefined) {
      bytes.push(0x5c); // lone trailing backslash (malformed): keep literal
      break;
    }
    if (esc in SIMPLE_ESCAPES) {
      bytes.push(SIMPLE_ESCAPES[esc]);
      continue;
    }
    if (/[0-7]/.test(esc)) {
      // Git always emits exactly three octal digits; accept 1-3.
      let oct = esc;
      while (oct.length < 3 && /[0-7]/.test(body[i + 1] ?? '')) oct += body[++i];
      bytes.push(parseInt(oct, 8) & 0xff);
      continue;
    }
    // Unknown escape (git never emits one): keep the character as-is.
    for (const b of Buffer.from(esc, 'utf8')) bytes.push(b);
  }
  return Buffer.from(bytes).toString('utf8');
}

/** Parse the b-side path out of a `diff --git <src> <dst>` header line,
 * tolerating the C-quoted form on either side — git quotes a path there
 * under the same conditions as unquoteGitPath, with the a/ or b/ prefix
 * INSIDE the quotes (`diff --git "a/…" "b/…"`). Returns the b-side path
 * (unquoted, prefix stripped) or null when the line is not a parseable
 * header; callers must treat null as "unknown file" (skip its body), NEVER
 * as "keep attributing to the previous file" — that misattribution is the
 * exact shape of task #1218's bug 2. */
function diffGitBPath(line) {
  const PREFIX = 'diff --git ';
  if (!line.startsWith(PREFIX)) return null;
  const rest = line.slice(PREFIX.length);

  if (rest.startsWith('"')) {
    // C-quoted src: find its UNESCAPED closing quote, then require
    // ` "<quoted dst>` after it.
    let close = -1;
    for (let i = 1; i < rest.length; i++) {
      if (rest[i] === '\\') {
        i++; // skip the escaped character
        continue;
      }
      if (rest[i] === '"') {
        close = i;
        break;
      }
    }
    if (close === -1) return null;
    const tail = rest.slice(close + 1);
    if (tail.length < 3 || !tail.startsWith(' "') || !tail.endsWith('"')) {
      return null;
    }
    const dst = unescapeCStyle(tail.slice(2, -1));
    return dst.startsWith('b/') ? dst.slice(2) : dst;
  }

  // Unquoted form. Greedy first group: the split lands on the LAST ` b/`,
  // which is correct for space-containing paths unless the path itself
  // contains ` b/` (see the header note — git's format is ambiguous
  // there, and no such path exists in this repo's history). The old
  // `\S+` regex could not span a space AT ALL and captured a truncated
  // path for such files.
  const m = rest.match(/^a\/(.*) b\/(.*)$/);
  return m ? m[2] : null;
}

function isMeasurementOnlyPath(path) {
  const p = toPosix(path);
  // Check if the basename is a measurement-only file (e.g. CHANGELOG.md anywhere in the tree)
  const basename = p.split('/').pop();
  if (MEASUREMENT_ONLY_ROOT_FILES.has(basename)) return true;

  // Root-level prefix check (e.g. "docs/", "examples/")
  if (MEASUREMENT_ONLY_PREFIXES.some((prefix) => p.startsWith(prefix))) return true;

  // Segment-aware check: match if a segment boundary precedes a measurement-only directory
  // (e.g. "crates/vmem/examples/x.rs" or "crates/aligned-vmem/benches/y.rs")
  // This fixes false positive on commit 2d86fcf where src changes were in crates/vmem/examples/
  // The regex ensures a real / boundary before the dir name, not just substring match
  // (so "src/examples_support.rs" or "crates/foo/tests_util/" do NOT match)
  const SEGMENT_AWARE_RE = /(^|\/)(docs|examples|benches|tests|scripts|\.github)\//;
  return SEGMENT_AWARE_RE.test(p);
}

/** Returns the list of changed paths for one commit (`git show --stat`,
 * parsed) — repo-relative, POSIX-separated. Uses `--name-only` for a
 * trivially parseable one-path-per-line format rather than scraping the
 * human `--stat` summary table (this script's header comment describes the
 * check as scanning `--stat`; `--name-only` is `--stat`'s machine-readable
 * sibling flag on the same `git show`, not a different command). */
/** Returns true if the commit has at least one non-comment changed line
 * in the given paths. Parses `git show --unified=0 --format=` to get the diff
 * body and checks each changed line (ignoring the leading +/- and blank lines)
 * against comment/attribute patterns. A changed line is considered a COMMENT/
 * metadata line if it matches any of:
 *   - `^\s*(///|//!|//)` — Rust doc/comment lines
 *   - `^\s*#(?!\[)` — TOML/YAML/shell # comments (not Rust attributes) —
 *     fixes false positives on commits c76e91f, fcb96ba
 *   - `^\s*#\[[^\n]*\]\s*$` — bare Rust attribute lines like `#[inline]` or
 *     `#[cfg(...)]` (metadata alone, not an algorithm/behavior delta) —
 *     fixes false positive on commit 338d50f
 * Any line not matching these patterns is considered a real code change. */
function hasNonCommentChange(sha, paths) {
  const nonMeasurementPaths = paths.filter((p) => !isMeasurementOnlyPath(p));
  if (nonMeasurementPaths.length === 0) return false;

  // Get the full diff with no context lines
  const diff = git(['show', '--unified=0', '--format=', sha]);
  const lines = diff.split(/\r?\n/);

  // Track which file we're currently in the diff for
  let currentFile = null;
  let hasNonComment = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // File header: `diff --git a/<src> b/<dst>`, where git C-quotes each
    // side (with the a/ or b/ prefix INSIDE the quotes) when the path
    // contains `"`, a backslash, or a control byte — even with
    // core.quotepath=false. Task #1218: the old
    // /^diff --git a\/\S+ b\/(\S+)/ did not match that quoted form, so
    // every changed line of a quoted-path file was silently attributed to
    // the PREVIOUSLY-SEEN file (`\S+` could not span a space in an
    // unquoted path either). An unparseable header now RESETS currentFile
    // to null — its body is skipped outright, never counted against the
    // previous file.
    if (line.startsWith('diff --git')) {
      const bPath = diffGitBPath(line);
      currentFile = bPath === null ? null : toPosix(bPath);
      continue;
    }

    // Skip if we're not in a file we care about, or if it's a hunk header
    if (!currentFile || !nonMeasurementPaths.includes(currentFile)) continue;
    if (line.startsWith('@@')) continue;
    if (line.startsWith('---') || line.startsWith('+++')) continue;

    // Check for actual changed lines (added or removed, not context)
    if (line.startsWith('+') || line.startsWith('-')) {
      // Remove the leading +/- to get the actual content
      const content = line.slice(1);

      // Skip empty lines
      if (!content.trim()) continue;

      // Check if it's a COMMENT/metadata line
      const isComment =
        /^\s*(\/\/\/|\/\/!|\/\/)/.test(content) || // Rust doc/comments
        /^\s*#(?!\[)/.test(content) || // TOML/YAML/shell # comments (not attributes) - fixes c76e91f, fcb96ba
        /^\s*#\[[^\n]*\]\s*$/.test(content); // Bare Rust attributes like #[inline] - fixes 338d50f

      if (!isComment) {
        hasNonComment = true;
        break;
      }
    }
  }

  return hasNonComment;
}

function changedPaths(sha) {
  const out = git(['show', '--name-only', '--format=', sha]).trim();
  // Task #1218: a line may still be C-quoted even with
  // core.quotepath=false (a path containing `"`, a backslash, or a
  // control byte is quoted unconditionally). Before the fix, the quoted
  // form passed through raw, so a file plainly under docs/ (e.g. the
  // Cyrillic review file of 105cf53, spelled
  // "docs/reviews/…-\320\241….md") failed isMeasurementOnlyPath's
  // ^-or-/ segment boundary — the leading `"` sits in front of `docs/` —
  // and was classified as OUTSIDE the measurement-only prefixes.
  return out
    ? out.split(/\r?\n/).filter(Boolean).map((l) => toPosix(unquoteGitPath(l)))
    : [];
}

const PERF_BARE_RE = /^perf:/;
const PERF_SCOPED_RE = /^perf\(([^)]*)\)!?:/;
const BENCH_OR_DOCS_RE = /^(bench|docs)\(([^)]*)\)!?:/;
const BENCH_BARE_RE = /^bench:/;
const DOCS_BARE_RE = /^docs:/;
const FIX_PERF_RE = /^fix\(perf\)!?:/;
// `build:` / `build(scope):` — task #1168 (OX3/F7): NOT part of R30-12's
// five-prefix perf taxonomy at all (build is a standard conventional-commit
// type for tooling/CI/dependency/structural changes, unrelated to the
// perf-vs-measurement axis R30-12 governs). Classified separately from
// 'docs-other'/'bench' below — see the 'build' branch in the direction-2
// switch for why it gets its own WARNING-only treatment rather than being
// folded into docs' ERROR tier.
const BUILD_RE = /^build(\(([^)]*)\))?!?:/;

/** Classify one commit's subject prefix. Returns one of:
 *   'perf-runtime' | 'perf-opt-in' | 'perf-other-scope' | 'perf-bare' |
 *   'bench' | 'docs-config' | 'docs-other' | 'fix-perf' | 'build' | 'other' */
function classifySubject(subject) {
  const scoped = PERF_SCOPED_RE.exec(subject);
  if (scoped) {
    const scope = scoped[1].trim().toLowerCase();
    if (scope === 'runtime') return 'perf-runtime';
    if (scope === 'opt-in') return 'perf-opt-in';
    return 'perf-other-scope'; // e.g. perf(large-cache): — pre-R30-12 style
  }
  if (PERF_BARE_RE.test(subject)) return 'perf-bare';
  // fix(perf) — the fifth R30-12 slot (R33-13/task #518): a shipping/opt-in
  // code fix with no speedup claimed, in perf-sensitive code. Checked before
  // the generic fix(...) fallthrough (which is 'other', out of this lint's
  // scope) so the direction-1 check below can verify it touches src/.
  if (FIX_PERF_RE.test(subject)) return 'fix-perf';
  const bd = BENCH_OR_DOCS_RE.exec(subject);
  if (bd) {
    if (bd[1] === 'bench') return 'bench';
    return bd[2].trim().toLowerCase() === 'config' ? 'docs-config' : 'docs-other';
  }
  if (BENCH_BARE_RE.test(subject)) return 'bench';
  // A bare `docs:` (no parenthesized scope) is symmetric with `bench:` above
  // and with the scoped `docs(x):` non-"config" case — classified the same
  // as 'docs-other' so it hits the exact same direction-2 path-check branch
  // below instead of silently falling through to 'other' (which direction-2
  // never examines).
  if (DOCS_BARE_RE.test(subject)) return 'docs-other';
  // build: / build(scope): — task #1168. Checked after every perf/bench/docs
  // branch (none of those prefixes start with "build") and before the final
  // 'other' fallthrough.
  if (BUILD_RE.test(subject)) return 'build';
  return 'other';
}

function main() {
  const argRange = process.argv[2];
  const range = resolveRange(argRange);

  let shas;
  try {
    shas = listShasInRange(range);
  } catch (err) {
    console.error(`[verify-commit-prefixes] failed to resolve range "${range}": ${err.message}`);
    process.exit(2);
  }

  const clipped = shas.filter((sha) => !isAtOrBeforeRuleCommit(sha));
  const droppedCount = shas.length - clipped.length;

  console.log(
    `[verify-commit-prefixes] range: ${range}  (${shas.length} commit(s) total` +
      (droppedCount > 0
        ? `, ${droppedCount} dropped as at-or-before the R30-12 rule commit ` +
          `${R30_12_RULE_COMMIT.slice(0, 7)} — not retroactive, per CLAUDE.md`
        : '') +
      `)`,
  );

  // Guard (1) from the GRANDFATHERED header comment: every entry's SHA must
  // resolve to a commit, on every run, independent of the scanned range.
  // Kept OUTSIDE `failures` so an entry key cannot grandfather away the
  // report of its own nonexistence. This loop runs BEFORE the empty-range
  // early exit below: the default range `@{u}..HEAD` is empty on a clean
  // pushed tree — the state `npm run check` sits in most of the time — so
  // placing this check after that exit meant the "on EVERY run regardless
  // of the scanned range" property the header claims silently stopped
  // holding the moment every commit landed (a typo'd key outside the
  // scanned range would then never be reported, the exact case this guard
  // exists for).
  const structuralFailures = []; // not exemptible by construction
  for (const sha of GRANDFATHERED.keys()) {
    try {
      execFileSync('git', ['cat-file', '-e', `${sha}^{commit}`], {
        cwd: REPO_ROOT,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch {
      structuralFailures.push(
        `grandfather entry \`${sha}\` does not resolve to a commit ` +
          `(git cat-file -e) — a typo'd suppression key. This check is ` +
          `range-independent precisely because a typo'd entry outside the ` +
          `scanned range would otherwise never be reported.`,
      );
    }
  }

  if (clipped.length === 0) {
    if (structuralFailures.length > 0) {
      console.log(`\n[verify-commit-prefixes] ${structuralFailures.length} FAILURE(s):`);
      for (const t of structuralFailures) console.log(`  - ${t}`);
      // Only structural (grandfather-key) failures are reachable in the
      // empty-range branch — the taxonomy pointer would send the reader to
      // the wrong rule (a typo'd SHA has nothing to do with R30-12).
      console.log(
        `\n[verify-commit-prefixes] FAILED — a GRANDFATHERED key does not resolve to a ` +
          `commit; fix or remove the entry in the GRANDFATHERED list near the top of ` +
          `scripts/verify-commit-prefixes.mjs. This is a suppression-list integrity ` +
          `failure, not a commit-prefix issue — CLAUDE.md's R30-12 taxonomy does not apply.`,
      );
      process.exit(1);
    }
    console.log('\n[verify-commit-prefixes] PASS — no commit in range to lint');
    process.exit(0);
  }

  // Failures carry their commit SHA as a STRUCTURED field (task #1123):
  // the grandfather matching below used to re-parse `\b[0-9a-f]{7,40}\b`
  // out of the failure STRING, which binds to the FIRST such word — a
  // message containing an ordinary word made of hex letters (`defaced`,
  // `effaced`, `acceded`) before the SHA would silently rebind the
  // exemption. Objects, not strings, make that class impossible.
  let failures = [];
  const warnings = [];

  for (const sha of clipped) {
    const subject = git(['log', '-1', '--format=%s', sha]).trim();
    const kind = classifySubject(subject);
    const short = sha.slice(0, 7);

    if (kind === 'perf-bare' || kind === 'perf-other-scope') {
      failures.push({
        sha,
        text: `${short} "${subject}" — bare/unscoped "perf(...)"/"perf:" is not a ` +
          `sanctioned R30-12 prefix; use perf(runtime): or perf(opt-in): (or ` +
          `bench:/docs(config): if this is measurement-only / docs-only).`,
        taxonomy: true,
      });
      continue;
    }

    if (kind === 'perf-runtime' || kind === 'perf-opt-in' || kind === 'fix-perf') {
      const paths = changedPaths(sha);
      const outside = paths.filter((p) => !isMeasurementOnlyPath(p));
      if (outside.length === 0) {
        const claim = kind === 'perf-runtime'
          ? 'a production-default runtime change'
          : kind === 'perf-opt-in'
            ? 'an opt-in runtime change'
            : 'a shipping/opt-in code fix in perf-sensitive code';
        failures.push({
          sha,
          text: `${short} "${subject}" — prefix claims ${claim}, but every changed path is under docs/examples/benches/tests/scripts/ ` +
            `(${paths.length} path(s): ${paths.slice(0, 6).join(', ')}${paths.length > 6 ? ', …' : ''}); ` +
            `use bench: or docs(config): instead if no shipping/opt-in code actually changed.`,
          taxonomy: true,
        });
        continue;
      }

      // REFINEMENT (task #1117): check if the src/ delta is comment-only
      if (!hasNonCommentChange(sha, outside)) {
        const claim = kind === 'perf-runtime'
          ? 'a production-default runtime change'
          : kind === 'perf-opt-in'
            ? 'an opt-in runtime change'
            : 'a shipping/opt-in code fix in perf-sensitive code';
        failures.push({
          sha,
          text: `${short} "${subject}" — prefix claims ${claim}, but every changed line in src/ is comment-only ` +
            `(matches \\s*(///|//!|//)); use bench:/docs(config): (or docs(...)) instead.`,
          taxonomy: true,
        });
        continue;
      }
      continue;
    }

    if (kind === 'bench' || kind === 'docs-config' || kind === 'docs-other') {
      const paths = changedPaths(sha);
      const outside = paths.filter((p) => !isMeasurementOnlyPath(p));
      // Cargo.toml is the one repo-root file that is never under any
      // MEASUREMENT_ONLY_PREFIXES prefix by construction — call it out by
      // name when it's the trigger, since a feature-composition change in
      // Cargo.toml is exactly the R30-12 "production default changed" case.
      if (outside.length > 0) {
        // REFINEMENT (task #1117): split verdict by prefix type
        const hasNonComment = hasNonCommentChange(sha, outside);
        const isDocs = kind === 'docs-config' || kind === 'docs-other';

        if (isDocs && hasNonComment) {
          failures.push({
            sha,
            text: `DIRECTION-2 NON-COMMENT src/ CHANGE: ${short} "${subject}" — prefix reads as measurement/docs-only, ` +
              `but ${outside.length} changed path(s) fall outside docs/examples/benches/tests/scripts/ ` +
              `with at least one non-comment changed line: ${outside.slice(0, 6).join(', ')}${outside.length > 6 ? ', …' : ''} — ` +
              `this shape previously shipped a real Display change (09f4d16). Verify no shipping/opt-in behavior actually changed ` +
              `(a bench-internals-gated diagnostic-only accessor in src/ is a known legitimate exception; ` +
              `a real algorithm/default change is not).`,
            taxonomy: true,
          });
        } else {
          warnings.push(
            `${short} "${subject}" — prefix reads as measurement/docs-only, but ${outside.length} ` +
              `changed path(s) fall outside docs/examples/benches/tests/scripts/ ` +
              `(${hasNonComment ? 'with non-comment' : 'comment-only'} src/ delta): ` +
              `${outside.slice(0, 6).join(', ')}${outside.length > 6 ? ', …' : ''} — verify no ` +
              `shipping/opt-in behavior actually changed.`,
          );
        }
      }
      continue;
    }

    if (kind === 'build') {
      // task #1168 (OX3/F7): `build:`/`build(scope):` gets its OWN direction-2
      // branch, deliberately separate from the docs/bench branch above and
      // deliberately WARNING-ONLY (never promoted to ERROR the way docs is).
      // Rationale, checked against this repo's own history before writing
      // this branch (60 build:/build(...) commits since the R30-12 rule
      // commit, cited exactly in this file's own header comment below):
      // `build` is a standard conventional-commit TYPE for tooling/CI/
      // dependency/structural work, not a claim of "no shipping code
      // changed" the way `docs(...)`/`bench(...)` are within R30-12's own
      // taxonomy — 10 of those 60 commits legitimately carry REAL
      // (non-comment) content OUTSIDE the measurement-only prefixes and are
      // not mis-scoped. SIX of the ten reach `src/` or a `Cargo.toml`
      // (`5daa90c`, `a4b8e50`'s module-per-file split, `c75aa59`'s
      // crate-directory rename, `d72b6d7`'s version bump, and the
      // dependency hoist/removal pair `56d0764`/`57c4510`); the other FOUR
      // touch NEITHER — `503f703`/`f3020fd` reach only `.gitignore`, and
      // `eb6935b`/`fabe6b4` only `package.json` (task #1185: this sentence
      // previously said all 10 "touch src/ or Cargo.toml", false for
      // exactly those four; the header comment below enumerates all ten).
      // Promoting this branch to ERROR the way docs is would
      // have flagged all 10 as false positives on legitimate history. But a
      // comment-only src/ delta under `build:` is exactly the same
      // hidden-claim-strengthening shape direction 2 exists to catch
      // (confirmed on this repo's own history: `39279b5` under `docs(vmem):`
      // and `cecdeec` under bare `build:` are the SAME comment-only change
      // to the SAME two files, `crates/aligned-vmem/src/api/decommit.rs` and
      // `crates/aligned-vmem/src/api/reserve_aligned_huge.rs` — task #1160
      // vs task #1164 — and only the docs-prefixed one was ever flagged
      // before this branch existed). So: always WARNING (both comment-only
      // and non-comment paths), never ERROR — visible to a reviewer without
      // blocking the many legitimate build: commits that touch src/ for
      // real.
      const paths = changedPaths(sha);
      const outside = paths.filter((p) => !isMeasurementOnlyPath(p));
      if (outside.length > 0) {
        const hasNonComment = hasNonCommentChange(sha, outside);
        warnings.push(
          `${short} "${subject}" — "build:" prefix is not part of R30-12's perf taxonomy, but ${outside.length} ` +
            `changed path(s) fall outside docs/examples/benches/tests/scripts/ ` +
            `(${hasNonComment ? 'with non-comment' : 'comment-only'} src/ delta): ` +
            `${outside.slice(0, 6).join(', ')}${outside.length > 6 ? ', …' : ''} — ` +
            `if this is a claim/evidence-strengthening comment change on a publishable surface ` +
            `(the 39279b5/cecdeec shape — task #1168), verify the claim is no stronger than what ` +
            `was actually run/observed; a real build/tooling/dependency change (module splits, ` +
            `renames, version bumps) is the common legitimate case and needs no action.`,
        );
      }
      continue;
    }

    // 'other' — not a perf/bench/docs(config)/build prefix at all; out of
    // this lint's scope entirely (e.g. fix:/feat:/refactor:/test:).
    // `build:`/`build(...)` is classified as 'build' above (task #1168,
    // OX3/F7) and handled by its own branch, not this one — this comment
    // itself named `build:` as an 'other' example until task #1178
    // (OX4/L2) caught that it went stale the moment `bf7b6b2` added the
    // `build` branch immediately above it, in the same commit.
  }

  console.log(`[verify-commit-prefixes] linted ${clipped.length} commit(s)`);

  if (warnings.length > 0) {
    console.log(`\n[verify-commit-prefixes] ${warnings.length} WARNING(s) (direction 2 — comment-only src/ delta):`);
    for (const w of warnings) console.log(`  - ${w}`);
  }

  // Grandfathering (task #1117; SHA binding made structured in task
  // #1123): drop failures whose commit SHA is on the pre-existing list, and
  // FAIL if a listed SHA stopped failing — a suppression entry that no
  // longer suppresses anything is how an exception list rots into a
  // blanket, so it must be removed consciously. The SHA comes from the
  // failure OBJECT's `sha` field, never re-parsed from the message text.
  const failingShas = new Set(failures.map((f) => f.sha.slice(0, 7)));
  const exempted = [];
  const staleExemptions = [];
  for (const [sha, why] of GRANDFATHERED) {
    if (failingShas.has(sha)) exempted.push(`${sha} — ${why}`);
    else staleExemptions.push(sha);
  }
  if (exempted.length > 0) {
    console.log(
      `\n[verify-commit-prefixes] ${exempted.length} grandfathered commit(s) — ` +
        `recorded in docs/CORRECTNESS_OPEN_ITEMS.md item 78, NOT amended (R30-12 is ` +
        `non-retroactive); each entry's reason line states why it is exempt — genuine ` +
        `mis-slot on landed history, or a heuristic false-positive exemption (task #1238):`,
    );
    for (const e of exempted) console.log(`  - ${e}`);
  }
  failures = failures.filter((f) => !GRANDFATHERED.has(f.sha.slice(0, 7)));
  if (staleExemptions.length > 0) {
    // Behavioral staleness is only decidable when the scanned range
    // actually covered those commits; a narrow range legitimately excludes
    // them. Guard (2) from the GRANDFATHERED header comment: entries the
    // range did NOT cover are still LISTED, so nothing is silent.
    const scanned = new Set(clipped.map((s) => s.slice(0, 7)));
    const reallyStale = staleExemptions.filter((s) => scanned.has(s));
    const uncovered = staleExemptions.filter((s) => !scanned.has(s));
    if (uncovered.length > 0) {
      console.log(
        `\n[verify-commit-prefixes] ${uncovered.length} grandfathered entry(ies) outside the scanned range ` +
          `(${uncovered.join(', ')}): not stale-checked this run — the range did not ` +
          `cover them (their SHAs were still existence-checked above).`,
      );
    }
    if (reallyStale.length > 0) {
      console.log(
        `\n[verify-commit-prefixes] STALE EXEMPTION(S): ${reallyStale.join(', ')} — ` +
          `listed in GRANDFATHERED but no longer failing. Remove the entry (and its ` +
          `item-78 line if the record is now moot) rather than leaving a suppression ` +
          `that suppresses nothing.`,
      );
      failures.push({
        sha: reallyStale[0],
        text: `stale grandfather entries: ${reallyStale.join(', ')}`,
      });
    }
  }
  failures.push(...structuralFailures.map((text) => ({ sha: null, text })));

  if (failures.length > 0) {
    console.log(`\n[verify-commit-prefixes] ${failures.length} FAILURE(s):`);
    for (const f of failures) console.log(`  - ${f.text}`);
    // The taxonomy pointer applies ONLY to prefix failures (marked
    // `taxonomy: true` at their push sites). A run whose failures are all
    // structural (grandfather-key) / stale-exemption ones would otherwise
    // get a trailing pointer to R30-12's prefix taxonomy — a rule that has
    // nothing to do with a typo'd suppression key or a rotten entry.
    if (failures.some((f) => f.taxonomy)) {
      console.log(
        `\n[verify-commit-prefixes] FAILED — see CLAUDE.md's R30-12 rule ("Active rules" section) ` +
          `for the full five-prefix taxonomy (perf(runtime) / perf(opt-in) / bench / docs(config) / fix(perf)).`,
      );
    } else {
      console.log(
        `\n[verify-commit-prefixes] FAILED — GRANDFATHERED list integrity failure(s) above; ` +
          `fix or remove the offending entry in scripts/verify-commit-prefixes.mjs ` +
          `(not a commit-prefix-taxonomy issue).`,
      );
    }
    process.exit(1);
  }

  console.log(`\n[verify-commit-prefixes] PASS${warnings.length > 0 ? ' (with warnings above)' : ''}`);
  process.exit(0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in self-test — runs FIRST, always (task #1218; same posture as
// scripts/verify-vmem-page-constant-call-sites.mjs's phase-1 fixture
// self-test: a parser that has never been proven against the exact byte
// shapes it exists to handle is unproven — and the regression this one
// pins made the guard claim a property it did not have, reporting a file
// under docs/ as outside docs/ while simultaneously never inspecting that
// file's diff at all). The 105cf53-derived fixtures are byte-exact copies
// of what git printed for that commit's Cyrillic-named review file under
// default core.quotepath=true; the residual-form fixtures (quote mark,
// control byte, backslash, space) are constructed per git's quote_c_style
// rules — the forms that stay quoted EVEN WITH core.quotepath=false. No
// such path exists in this repo's history yet; the parser must not be
// blind to the class merely because the repo has been lucky so far (it
// was not lucky twice: 2074646, 105cf53). Deliberately quiet on pass
// (one summary line — this runs before every lint, including check-all
// step 37 and CI) and loud on failure (expected/got per fixture, exit 1
// before any commit is scanned).
// ─────────────────────────────────────────────────────────────────────────────
function runQuotingSelfTest() {
  const ESCAPED =
    'docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-' +
    '\\320\\241\\320\\276\\320\\273-\\320\\272\\320\\276\\320\\264\\320\\265\\320\\272\\321\\201.md';
  const QUOTED = `"${ESCAPED}"`;
  const CYRILLIC =
    'docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md';
  const QUOTED_HEADER = `diff --git "a/${ESCAPED}" "b/${ESCAPED}"`;

  const cases = [
    {
      name: 'unquote decodes the quotepath=true octal form (105cf53, byte-exact)',
      actual: unquoteGitPath(QUOTED),
      expected: CYRILLIC,
    },
    {
      name: 'unquote decodes a residual quote-mark path (still quoted with quotepath=false)',
      actual: unquoteGitPath('"we\\"ird.md"'),
      expected: 'we"ird.md',
    },
    {
      name: 'unquote decodes residual control-byte/backslash escapes',
      actual: unquoteGitPath('"ta\\tb\\\\sec.md"'),
      expected: 'ta\tb\\sec.md',
    },
    {
      name: 'unquote passes an unquoted path through unchanged',
      actual: unquoteGitPath('docs/CORRECTNESS_OPEN_ITEMS.md'),
      expected: 'docs/CORRECTNESS_OPEN_ITEMS.md',
    },
    {
      name: 'BUG 1: the unquoted 105cf53 path classifies as measurement-only',
      actual: isMeasurementOnlyPath(unquoteGitPath(QUOTED)),
      expected: true,
    },
    {
      name: 'BUG 1 mechanism: the STILL-quoted form does NOT classify — unquoting is load-bearing',
      actual: isMeasurementOnlyPath(QUOTED),
      expected: false,
    },
    {
      name: 'BUG 2: the quoted diff --git header yields the b-side path (105cf53, byte-exact)',
      actual: diffGitBPath(QUOTED_HEADER),
      expected: CYRILLIC,
    },
    {
      name: 'header: unquoted form still parses (regression guard)',
      actual: diffGitBPath('diff --git a/src/lib.rs b/src/lib.rs'),
      expected: 'src/lib.rs',
    },
    {
      name: 'header: space-containing unquoted path parses (the old \\S+ truncated it)',
      actual: diffGitBPath('diff --git a/docs/my review.md b/docs/my review.md'),
      expected: 'docs/my review.md',
    },
    {
      name: 'header: quoted path with an escaped quote mark parses',
      actual: diffGitBPath('diff --git "a/docs/we\\"ird.md" "b/docs/we\\"ird.md"'),
      expected: 'docs/we"ird.md',
    },
    {
      name: 'header: quoted rename strips the b/ prefix',
      actual: diffGitBPath('diff --git "a/old.md" "b/new.md"'),
      expected: 'new.md',
    },
    {
      name: 'header: unparseable header returns null (skip the body, never misattribute)',
      actual: diffGitBPath('diff --git "a/unclosed b/x'),
      expected: null,
    },
    {
      name: 'header: non-header line returns null',
      actual: diffGitBPath('+++ b/src/lib.rs'),
      expected: null,
    },
  ];

  const failed = cases.filter((c) => c.actual !== c.expected);
  if (failed.length > 0) {
    console.error(
      `\n[verify-commit-prefixes] self-test FAILED (${cases.length - failed.length}/${cases.length} git-path-quoting fixtures):`,
    );
    for (const c of failed) {
      console.error(`  FAIL: ${c.name}`);
      console.error(`    expected: ${JSON.stringify(c.expected)}`);
      console.error(`    actual:   ${JSON.stringify(c.actual)}`);
    }
    console.error(
      '\n[verify-commit-prefixes] the git-path-quoting parser (task #1218) disagrees ' +
        'with reality; fix the parser, not the fixtures — the 105cf53 ones are ' +
        'byte-exact shapes git printed. No commit was scanned.',
    );
    process.exit(1);
  }
  console.log(
    `[verify-commit-prefixes] self-test OK (${cases.length}/${cases.length} git-path-quoting fixtures, task #1218)`,
  );
}

runQuotingSelfTest();

main();
