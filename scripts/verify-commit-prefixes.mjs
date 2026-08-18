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
//   (2) `bench(...)`/`docs(...)`-prefixed commit whose diff DOES touch
//       something outside docs/, examples/, benches/, tests/, scripts/
//       (i.e. touches src/ or Cargo.toml) -> ERROR for docs/docs-config,
//       WARNING for bench (direction 2). This is a hidden-runtime-change-under-
//       an-innocuous-prefix shape — the opposite direction.
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
// SELF-CLEANING, deliberately: an entry that no longer fails is itself a
// FAILURE (see the check after the scan). An exception that has silently
// stopped applying is how a suppression list rots into a blanket.
//
// Each entry states WHY, because "grandfathered" without a reason is the
// shape this campaign keeps finding and correcting.
const GRANDFATHERED = new Map([
  [
    'fb7dac8',
    'LANDED (in origin/main) before this check existed. docs(vmem) prefix on a ' +
      'commit whose src/ delta includes a changed assert! panic-message string. ' +
      'Recorded, not amended: rewriting pushed history is what R30-12 forbids.',
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
      'the correct slot was bench: or docs(...). Recorded in item 78.',
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
const MEASUREMENT_ONLY_ROOT_FILES = new Set(['CHANGELOG.md', 'README.md', 'CLAUDE.md']);

function git(args) {
  return execFileSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8' });
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

    // Check for file header: "diff --git a/path b/path"
    const fileMatch = line.match(/^diff --git a\/\S+ b\/(\S+)/);
    if (fileMatch) {
      currentFile = toPosix(fileMatch[1]);
      continue;
    }

    // Skip if we're not in a file we care about, or if it's a hunk header
    if (!currentFile || !nonMeasurementPaths.includes(currentFile)) continue;
    if (line.startsWith('@@') || line.startsWith('diff --git')) continue;
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
  return out ? out.split(/\r?\n/).filter(Boolean).map(toPosix) : [];
}

const PERF_BARE_RE = /^perf:/;
const PERF_SCOPED_RE = /^perf\(([^)]*)\)!?:/;
const BENCH_OR_DOCS_RE = /^(bench|docs)\(([^)]*)\)!?:/;
const BENCH_BARE_RE = /^bench:/;
const DOCS_BARE_RE = /^docs:/;
const FIX_PERF_RE = /^fix\(perf\)!?:/;

/** Classify one commit's subject prefix. Returns one of:
 *   'perf-runtime' | 'perf-opt-in' | 'perf-other-scope' | 'perf-bare' |
 *   'bench' | 'docs-config' | 'docs-other' | 'fix-perf' | 'other' */
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

  if (clipped.length === 0) {
    console.log('\n[verify-commit-prefixes] PASS — no commit in range to lint');
    process.exit(0);
  }

  let failures = [];
  const warnings = [];

  for (const sha of clipped) {
    const subject = git(['log', '-1', '--format=%s', sha]).trim();
    const kind = classifySubject(subject);
    const short = sha.slice(0, 7);

    if (kind === 'perf-bare' || kind === 'perf-other-scope') {
      failures.push(
        `${short} "${subject}" — bare/unscoped "perf(...)"/"perf:" is not a ` +
          `sanctioned R30-12 prefix; use perf(runtime): or perf(opt-in): (or ` +
          `bench:/docs(config): if this is measurement-only / docs-only).`,
      );
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
        failures.push(
          `${short} "${subject}" — prefix claims ${claim}, but every changed path is under docs/examples/benches/tests/scripts/ ` +
            `(${paths.length} path(s): ${paths.slice(0, 6).join(', ')}${paths.length > 6 ? ', …' : ''}); ` +
            `use bench: or docs(config): instead if no shipping/opt-in code actually changed.`,
        );
        continue;
      }

      // REFINEMENT (task #1117): check if the src/ delta is comment-only
      if (!hasNonCommentChange(sha, outside)) {
        const claim = kind === 'perf-runtime'
          ? 'a production-default runtime change'
          : kind === 'perf-opt-in'
            ? 'an opt-in runtime change'
            : 'a shipping/opt-in code fix in perf-sensitive code';
        failures.push(
          `${short} "${subject}" — prefix claims ${claim}, but every changed line in src/ is comment-only ` +
            `(matches \\s*(///|//!|//)); use bench:/docs(config): (or docs(...)) instead.`,
        );
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
          failures.push(
            `DIRECTION-2 NON-COMMENT src/ CHANGE: ${short} "${subject}" — prefix reads as measurement/docs-only, ` +
              `but ${outside.length} changed path(s) fall outside docs/examples/benches/tests/scripts/ ` +
              `with at least one non-comment changed line: ${outside.slice(0, 6).join(', ')}${outside.length > 6 ? ', …' : ''} — ` +
              `this shape previously shipped a real Display change (09f4d16). Verify no shipping/opt-in behavior actually changed ` +
              `(a bench-internals-gated diagnostic-only accessor in src/ is a known legitimate exception; ` +
              `a real algorithm/default change is not).`,
          );
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

    // 'other' — not a perf/bench/docs(config) prefix at all; out of this
    // lint's scope entirely (e.g. fix:/feat:/refactor:/test:/build:).
  }

  console.log(`[verify-commit-prefixes] linted ${clipped.length} commit(s)`);

  if (warnings.length > 0) {
    console.log(`\n[verify-commit-prefixes] ${warnings.length} WARNING(s) (direction 2 — comment-only src/ delta):`);
    for (const w of warnings) console.log(`  - ${w}`);
  }

  // Grandfathering (task #1117): drop failures whose SHA is on the
  // pre-existing list, and FAIL if a listed SHA stopped failing — a
  // suppression entry that no longer suppresses anything is how an exception
  // list rots into a blanket, so it must be removed consciously.
  const failingShas = new Set(
    failures.flatMap((f) => {
      const m = f.match(/\b([0-9a-f]{7,40})\b/);
      return m ? [m[1].slice(0, 7)] : [];
    }),
  );
  const exempted = [];
  const staleExemptions = [];
  for (const [sha, why] of GRANDFATHERED) {
    if (failingShas.has(sha)) exempted.push(`${sha} — ${why}`);
    else staleExemptions.push(sha);
  }
  if (exempted.length > 0) {
    console.log(
      `\n[verify-commit-prefixes] ${exempted.length} grandfathered commit(s) — ` +
        `pre-existing when this check was strengthened (task #1117); recorded in ` +
        `docs/CORRECTNESS_OPEN_ITEMS.md item 78, NOT amended (R30-12 is non-retroactive):`,
    );
    for (const e of exempted) console.log(`  - ${e}`);
  }
  failures = failures.filter((f) => {
    const m = f.match(/\b([0-9a-f]{7,40})\b/);
    return !(m && GRANDFATHERED.has(m[1].slice(0, 7)));
  });
  if (staleExemptions.length > 0) {
    // Only meaningful when the scanned range actually covered those commits;
    // a narrow range legitimately excludes them.
    const scanned = new Set(clipped.map((s) => s.slice(0, 7)));
    const reallyStale = staleExemptions.filter((s) => scanned.has(s));
    if (reallyStale.length > 0) {
      console.log(
        `\n[verify-commit-prefixes] STALE EXEMPTION(S): ${reallyStale.join(', ')} — ` +
          `listed in GRANDFATHERED but no longer failing. Remove the entry (and its ` +
          `item-78 line if the record is now moot) rather than leaving a suppression ` +
          `that suppresses nothing.`,
      );
      failures.push(`stale grandfather entries: ${reallyStale.join(', ')}`);
    }
  }

  if (failures.length > 0) {
    console.log(`\n[verify-commit-prefixes] ${failures.length} FAILURE(s) (direction 1 — R30-12 taxonomy violation):`);
    for (const f of failures) console.log(`  - ${f}`);
    console.log(
      `\n[verify-commit-prefixes] FAILED — see CLAUDE.md's R30-12 rule ("Active rules" section) ` +
        `for the full five-prefix taxonomy (perf(runtime) / perf(opt-in) / bench / docs(config) / fix(perf)).`,
    );
    process.exit(1);
  }

  console.log(`\n[verify-commit-prefixes] PASS${warnings.length > 0 ? ' (with warnings above)' : ''}`);
  process.exit(0);
}

main();
