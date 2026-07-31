// STRUCTURAL + SEMANTIC checks over a `docs/perf/R*_*.md` gate report — the
// machine-checkable half of CLAUDE.md's "gate report tables must be
// derived, not hand-transcribed" rule (see CLAUDE.md's "Phased delivery"
// section, the bullet starting "A gate report's tables and headline ratios
// must be DERIVED, by one checked script...") plus the sibling
// "exact entry point under test" / "same workload regime" rules. R31-5
// (task #480/#481/#482) splits enforcement into three parts; THIS script
// carries parts (a) [structural, task #480] and (b) [semantic, task #481].
// The commit-prefix taxonomy lint is a SEPARATE script, task #482
// (`scripts/verify-commit-prefixes.mjs`) — deliberately not folded in here,
// since it scans commit subject lines, not report file content.
//
// Three structural checks (task #480), each independently reproducing a
// defect class a prior round's review caught by hand and that CLAUDE.md now
// states as a rule:
//
//   (a) COMPANION CSV EXISTS — if a report's own text cites a
//       `*_summary.csv` path (CLAUDE.md's summary-CSV rule, R14-10/task
//       #295), every cited path must exist on disk. A report that cites no
//       summary CSV at all is a DESIGN/FEASIBILITY doc under CLAUDE.md's own
//       scoping rule ("a design or feasibility report that cites no measured
//       numbers... does not [owe raw logs/CSV]") and this check SKIPs it,
//       rather than inventing a companion-file requirement CLAUDE.md itself
//       does not impose on non-measurement docs.
//
//   (b) VALID SHA, NO PLACEHOLDER — every cited companion CSV's commit/SHA
//       field(s) must be a real 40-hex-character git SHA, not a prose
//       placeholder. This is the exact defect R30-6's
//       `commit_sha=<see report header, this file is committed
//       alongside...>` (OPEN_ITEMS.md P2-10) and R29-13's 63-hex-character
//       (one short of valid sha256, a DIFFERENT but sibling defect class:
//       an invalid-length hash) both were — see CLAUDE.md's R29-6
//       immutable-source-identity rule and R30-9's gate-report-hygiene
//       rule. CSV schemas in this repo are NOT standardized (some use a
//       `commit`/`commit_sha`/`base_commit`/`landing_commit` header column,
//       some use a `# commit_sha=...` comment-header line — see
//       `docs/perf/R27_3_POOL_RETENTION_GATE_summary.csv` and
//       `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv` for the
//       comment-header style, `docs/perf/R31_0_..._summary.csv` for the
//       column style), so this check scans BOTH shapes rather than assuming
//       one fixed schema.
//
//   (c) CITED RAW LOGS EXIST — every `_raw_*.log` filename the report's text
//       mentions (in prose, in a table cell, or as a bare backtick-quoted
//       filename) must exist under `docs/perf/`. This is the raw-log-policy
//       rule (CLAUDE.md "Raw perf logs... are scratch by default" bullet):
//       "when a perf-gate report cites specific `_raw_*.log` filenames as
//       its evidence, [commit] those named files alongside the report so
//       the citation is reproducible from the commit, not just from a
//       re-run."
//
// Four semantic checks (task #481, R31-5b), each mechanizing a rule
// CLAUDE.md states in prose and a prior round's review caught only by hand:
//
//   (d) PROSE<->CSV HEADLINE CROSS-CHECK — a headline number named in the
//       report's own prose (near a keyword like "headline"/"hit rate"/
//       "retention"/"MDE", carrying a unit like "MiB"/"%"/"ns/op"/"Ir")
//       should reappear, at reasonable rounding, somewhere in the companion
//       CSV's own text. This is a DELIBERATELY LIGHTWEIGHT heuristic, not
//       NLP-grade parsing — precedent is `tests/no_stale_doc_references.rs`
//       ("a regex/text scan... good enough text scanning over a known,
//       hand-authored file shape"), the same register this whole script
//       already uses. See `checkProseCsvCrossCheck`'s own doc comment for
//       the documented limitations of this heuristic (false negatives from
//       derived/rounded prose numbers are expected and downgrade to WARN,
//       never silently pass as if verified).
//
//   (e) ALLOCATOR-LAYER-UNDER-TEST FIELD — mechanizes the CLAUDE.md rule
//       (commit `2d9eef2`, "A gate report must name the exact entry point
//       under test") that a report must state which layer
//       (`AllocCore`/`HeapCore`/`SeferAlloc`/real `#[global_allocator]`/real
//       downstream application) its verdict applies to. A report naming NO
//       layer is WARNed (not failed — a report answering a pure-substrate
//       question, e.g. a Stage-1 decomposition never meant to generalize to
//       the global-allocator chain, legitimately has no such field); a
//       report naming a layer BELOW `SeferAlloc` while its prose also uses
//       promotion/production-verdict language is WARNed as a stronger,
//       separate signal (the R30-3/R31-0 defect shape specifically).
//
//   (f) BETWEEN-ARM MECHANISM DELTA — a report comparing two or more arms
//       must carry (in its CSV or its own prose) the actual
//       treatment-minus-control mechanism delta, not just each arm's
//       activation count reported separately with the reader left to
//       subtract.
//
// Plus one identity-freshness check (also task #481, not lettered — it is a
// cross-cutting check over checks (b)+(c) together, not a new report
// dimension of its own):
//
//   POST-FACTO IDENTITY WARNING — warns when a report's cited commit SHA
//   postdates its own raw-log files' mtimes, a signal (not proof) that the
//   SHA was assembled after measurement rather than captured beforehand per
//   CLAUDE.md's R29-6 rule. See `scripts/capture-measurement-identity.mjs`,
//   this same task's companion helper, for the fix going forward: run it
//   immediately before a measurement's binaries are built, not after.
//
// All four semantic checks are WARN-level (their own status is 'WARN', not
// 'FAIL', and `verifyReport`'s `ok` computation only ever inspects checks
// a/b/c) — CLAUDE.md itself treats (e) as "flag, don't hard-fail" (a
// substrate-only report legitimately has no layer field), and
// (d)/(f)/identity-freshness share the same false-positive risk from a
// genuinely lightweight text heuristic. A human reading the WARN output
// still has to use judgment; this script's job is to make sure nothing
// goes unnoticed, not to auto-reject.
//
// Deliberately ZERO cargo invocations, ZERO network access, pure text/regex
// scanning of already-committed files — same design register as
// scripts/verify-perf-gate-stubs.mjs (see that file's own header for the
// precedent: "a regex/text scan, not a real Rust parser... good enough text
// scanning over a known, hand-authored file shape").
//
// Usage:
//   node scripts/verify-gate-report.mjs                 # scan every docs/perf/R*_*.md
//   node scripts/verify-gate-report.mjs <path> [<path>...]  # scan specific report(s)
//   node scripts/verify-gate-report.mjs <path> --new-only   # scan specific report(s),
//                                                            # refusing (exit 2) if any
//                                                            # target already carries a
//                                                            # documented retroactive
//                                                            # exemption below — use this
//                                                            # form to confirm a report you
//                                                            # are about to add is judged
//                                                            # with NO historical slack
//   npm run verify-gate-reports
//
// RETROACTIVE EXEMPTIONS — CLAUDE.md is explicit that several of the rules
// this script enforces are NOT retroactive (the raw-log-truncation rule, the
// summary-CSV rule, the immutable-source-identity rule, and the R30-9
// derived-tables rule all say so in their own text). `RETROACTIVE_EXEMPT`
// below lists reports that predate the relevant rule and are known, already
// tracked (via OPEN_ITEMS.md and/or a landing-commit follow-up), to fail one
// of these checks for a historical reason — so a fresh report author is not
// blocked by pre-existing debt this task is not scoped to clean up. A NEW
// report must not be added to this list to silence a real new defect.
//
// Use `--new-only` to skip the exemption list machinery altogether and only
// ever treat unexempted, unrecognized-by-git-history reports as scannable
// (in practice: run this script's default full-directory scan, but hard-fail
// if invoked against a path that's actually in RETROACTIVE_EXEMPT, rather
// than silently downgrading it to SKIP) — see the flag's own handling below.

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, basename, dirname } from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPO_ROOT } from './lib.mjs';

const PERF_DIR = resolve(REPO_ROOT, 'docs/perf');

// Each entry: reportBasename -> { reason, checks: ['a'|'b'|'c', ...] }
// `checks` lists exactly which of this script's checks the report is known
// to fail for a documented, already-tracked historical reason; any OTHER
// check still runs and must pass normally — an exemption is scoped to the
// specific known defect, not a blanket pass for the whole file.
// Keyed by REPORT basename (`.md` stem). Applies to checks (a) and (c),
// which are report-scoped (a report either cites a companion CSV / a raw
// log by name, or it doesn't).
const RETROACTIVE_EXEMPT = new Map([
  [
    'R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE',
    {
      reason:
        'OPEN_ITEMS.md P2-11: report basename carries a "_GATE" suffix its ' +
        'own companion CSV (R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv) ' +
        'does not, a pre-existing, already-tracked naming mismatch that ' +
        'predates this script and is not a new defect.',
      checks: ['a'],
    },
  ],
  [
    'R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST',
    {
      reason:
        '§5.2 prose reads "raw log: would be `docs/perf/_raw_r15_1_bench_table_head.log` ' +
        'if retained" — an explicit statement that this log was deliberately NOT ' +
        'committed (numbers transcribed directly into the doc\'s tables instead, ' +
        'per this project\'s raw-log policy: a log is only owed when a report cites ' +
        'it AS EVIDENCE, which this prose explicitly disclaims). This script\'s regex ' +
        'cannot distinguish a hypothetical filename from a real citation; a real new ' +
        'report should avoid this "would be X if retained" phrasing rather than rely ' +
        'on a second exemption of this kind.',
      checks: ['c'],
    },
  ],
  [
    'R10_7_BATCH_WARM_ARM',
    {
      reason:
        'OPEN_ITEMS.md item 37: report §5 cites 4 raw logs ' +
        '(_raw_r10_7_warm_arm.log, _raw_r10_7_tcache_isolated.log, ' +
        '_raw_r10_7_d_vs_f.log, _raw_r10_7_tcache_arm.log) that were never ' +
        'committed. Verified via `git merge-base --is-ancestor 9611a56 1a2dd7d` ' +
        '(exit 0): the report (created at 9611a56) predates the raw-log-is-' +
        'scratch-by-default policy (R13-10/task #280, commit 1a2dd7d) that ' +
        'first required a cited raw log to be committed alongside its report ' +
        '— pre-existing debt this script surfaced on its first CI run, not a ' +
        'new defect.',
      checks: ['c'],
    },
  ],
  [
    'R8_9_MEDIUM_CLASSES_VERDICT',
    {
      reason:
        'OPEN_ITEMS.md item 37 follow-up: report cites 4 raw logs ' +
        '(_raw_baseline_off.log, _raw_medium_on.log, ' +
        '_raw_baseline_off_reduced.log, _raw_medium_on_reduced.log) that were ' +
        'never committed. Verified via `git merge-base --is-ancestor 9afba66 ' +
        '1a2dd7d` (exit 0): the report (created at 9afba66) predates the ' +
        'raw-log-is-scratch-by-default policy (R13-10/task #280, commit ' +
        '1a2dd7d) — same pre-existing-debt class as R10_7_BATCH_WARM_ARM ' +
        'above, found present locally only as untracked scratch files (a ' +
        'clean checkout, as CI uses, does not have them).',
      checks: ['c'],
    },
  ],
  [
    'R9_3_MEDIUM_CLASSES_PRODUCTION_GATES',
    {
      reason:
        'OPEN_ITEMS.md item 37 follow-up: report cites 6 raw logs ' +
        '(_raw_iai_production.log, _raw_iai_medium.log, ' +
        '_raw_criterion_production.log, _raw_criterion_medium.log, ' +
        '_raw_firstalloc_production.log, _raw_firstalloc_medium.log) that ' +
        'were never committed. Verified via `git merge-base --is-ancestor ' +
        'c8f5f32 1a2dd7d` (exit 0): the report (created at c8f5f32) predates ' +
        'the raw-log-is-scratch-by-default policy (R13-10/task #280, commit ' +
        '1a2dd7d) — same pre-existing-debt class as R10_7_BATCH_WARM_ARM ' +
        'above, found present locally only as untracked scratch files (a ' +
        'clean checkout, as CI uses, does not have them).',
      checks: ['c'],
    },
  ],
]);

// Keyed by companion-CSV basename (the `_summary.csv` file itself, not the
// report). Applies to check (b) only — the defect lives IN the CSV's
// commit/SHA field, and more than one report can cite the same CSV (e.g.
// R14_4 and R18_9 both cite R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv), so
// keying by CSV avoids listing the same underlying defect twice.
//
// Every entry below is a short (7-char) abbreviated commit hash predating
// the R29-6 immutable-source-identity rule (commit `34f3702`, task #437,
// "docs(perf): archive OPEN_ITEMS.md's closure narratives; add
// immutable-provenance rule") that FIRST required a full 40-hex SHA instead
// of an abbreviated one. Verified for each CSV below via `git log
// --diff-filter=A --format=%H -- docs/perf/<report>.md | tail -1` followed
// by `git merge-base --is-ancestor <that SHA> 34f3702` (exit 0 = predates
// the rule) before being added here. CLAUDE.md's R29-6 rule text is
// explicit this is "not retroactive". A cutoff computed at RUN TIME via
// `git merge-base` was considered and rejected: `.github/workflows/ci.yml`'s
// `actions/checkout@v5` steps do not set `fetch-depth`, so CI runs on a
// shallow (depth-1) clone where `git merge-base`/`git log --diff-filter=A`
// against an old ancestor commit would not reliably resolve — a hardcoded,
// individually-verified list has no such environment dependency.
const RETROACTIVE_EXEMPT_CSV = new Map([
  [
    'R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB_summary.csv',
    'predates R29-6 (commit 34f3702); created at 6d85db4.',
  ],
  [
    'R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv',
    'predates R29-6 (commit 34f3702); cited by both R14_4 (created 6a644a4) ' +
      'and R18_9 (created 60633e3).',
  ],
  [
    'R15_1_MAX_SEGMENTS_DRAIN_SCAN_COST_summary.csv',
    'predates R29-6 (commit 34f3702); created at 7224670.',
  ],
  [
    'R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at ee5f2aa.',
  ],
  [
    'R21_2_OPT_H_STAGE1_HIT_RATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at b6af12d.',
  ],
  [
    'R22_15_MIMALLOC_IR_ARM_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at ff48029.',
  ],
  [
    'R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at 610f915.',
  ],
  [
    'R23_2_WARM_N_2N_MIMALLOC_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at 3cf2d66.',
  ],
  [
    'R24_11_TEARDOWN_RESIDUAL_ROOTCAUSE_summary.csv',
    'predates R29-6 (commit 34f3702); created at 9594570.',
  ],
  [
    'R24_3_FLUSH_MAGAZINE_CLASS_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at e530a9f. Also holds a ' +
      'deliberate non-SHA annotation ("3bc9c91 (HEAD; merged-code measured ' +
      'in working tree then rev...)") explaining an uncommitted-tree ' +
      'measurement — exactly the gap the R29-6 rule was written to close.',
  ],
  [
    'R24_5_COLD_ALLOC_FREE_SPLIT_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at 9a5b1f3.',
  ],
  [
    'R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at db35617 — R29-3 landed ' +
      'immediately before R29-6 in the same round.',
  ],
  [
    'R29_4_SEGMENT_STATE_RECONCILIATION_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at e4576d9 — R29-4 landed ' +
      'immediately before R29-6 in the same round.',
  ],
  [
    'R29_5_PROMOTION_FREQUENCY_GATE_summary.csv',
    'predates R29-6 (commit 34f3702); created at 79aad56 — R29-5 landed ' +
      'immediately before R29-6 in the same round.',
  ],
]);

// --------------------------------------------------------------------------
// Check (a): companion summary CSV
// --------------------------------------------------------------------------

const SUMMARY_CSV_CITATION_RE = /docs\/perf\/([A-Za-z0-9_.-]+_summary\.csv)/g;

/**
 * A report "owes" a companion CSV under this check iff its own text cites
 * at least one `docs/perf/..._summary.csv` path — a design/feasibility doc
 * that cites no measured numbers cites no such path and is not held to this
 * check (CLAUDE.md's own scoping: "a design or feasibility report that
 * cites no measured numbers... does not [owe raw logs/CSV]").
 */
function checkCompanionCsv(mdPath, text) {
  const cited = new Set();
  for (const m of text.matchAll(SUMMARY_CSV_CITATION_RE)) {
    cited.add(m[1]);
  }
  if (cited.size === 0) {
    return { status: 'SKIP', detail: 'report cites no *_summary.csv path (design/feasibility doc)' };
  }
  const missing = [...cited].filter((name) => !existsSync(resolve(PERF_DIR, name)));
  if (missing.length > 0) {
    return {
      status: 'FAIL',
      detail: `cited summary CSV(s) not found on disk: ${missing.join(', ')}`,
    };
  }
  return { status: 'PASS', detail: `${cited.size} cited summary CSV(s) all present: ${[...cited].join(', ')}` };
}

// --------------------------------------------------------------------------
// Check (b): valid SHA in the companion CSV, no prose placeholder
// --------------------------------------------------------------------------

const VALID_SHA_RE = /^[0-9a-f]{40}$/;
// Header-column style: a CSV column whose name looks like a commit/SHA field.
const SHA_COLUMN_NAME_RE = /(^|_)(commit|sha)(_sha)?$/i;
// Comment-header style (R27-3, R30-6): a `# commit_sha=<value>` line, value
// running to end of line or to the next whitespace-delimited annotation.
const SHA_COMMENT_RE = /^#\s*(\w*commit\w*)\s*=\s*(\S+)/i;

/**
 * Scan one CSV file's text for every commit/SHA-shaped field (both the
 * header-column style and the `#`-comment-header style this repo's CSVs
 * use — see this file's header comment for both real examples) and report
 * each as { fieldName, value, valid }.
 */
function extractShaFields(csvText) {
  const lines = csvText.split(/\r?\n/);
  const fields = [];

  // Comment-header style: any line starting with `#` naming a commit field.
  for (const line of lines) {
    const m = SHA_COMMENT_RE.exec(line.trim());
    if (m) {
      const rawValue = m[2].replace(/[),.;]+$/, '');
      fields.push({ fieldName: m[1], value: rawValue, valid: VALID_SHA_RE.test(rawValue) });
    }
  }

  // Header-column style: first non-comment line is the header row; any
  // matching column is checked on every data row.
  const headerLineIdx = lines.findIndex((l) => l.trim() && !l.trim().startsWith('#'));
  if (headerLineIdx !== -1) {
    const header = splitCsvLine(lines[headerLineIdx]);
    const shaColIdxs = header
      .map((name, idx) => ({ name, idx }))
      .filter(({ name }) => SHA_COLUMN_NAME_RE.test(name.trim()));
    if (shaColIdxs.length > 0) {
      for (let i = headerLineIdx + 1; i < lines.length; i++) {
        const line = lines[i];
        if (!line.trim() || line.trim().startsWith('#')) continue;
        const cols = splitCsvLine(line);
        for (const { name, idx } of shaColIdxs) {
          const value = (cols[idx] ?? '').trim();
          if (!value) continue; // e.g. an UNFILLED landing_commit sentinel column with a real word, not blank — handled by VALID_SHA_RE below; a genuinely blank cell is a ragged-CSV concern, not this check's job
          fields.push({ fieldName: name.trim(), value, valid: VALID_SHA_RE.test(value) });
        }
      }
    }
  }

  return fields;
}

/** Minimal CSV line splitter handling double-quoted fields (this repo's CSVs
 * only ever quote a features string like `"production alloc-stats"` — no
 * embedded commas-inside-quotes-containing-quotes complexity beyond that). */
function splitCsvLine(line) {
  const out = [];
  let cur = '';
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') {
      inQuotes = !inQuotes;
    } else if (c === ',' && !inQuotes) {
      out.push(cur);
      cur = '';
    } else {
      cur += c;
    }
  }
  out.push(cur);
  return out;
}

function checkValidSha(csvName, csvPath) {
  const text = readFileSync(csvPath, 'utf8');
  const fields = extractShaFields(text);
  if (fields.length === 0) {
    return {
      status: 'SKIP',
      detail: `${csvName}: no commit/SHA-shaped field found (neither a header column named ` +
        `commit/sha/*_commit/*_sha nor a "# commit_sha=..." comment line)`,
    };
  }
  const invalid = fields.filter((f) => !f.valid);
  // De-dup identical (fieldName, value, valid) triples so a per-row column
  // repeated across hundreds of data rows reports once, not once per row.
  const uniqueInvalid = [...new Map(invalid.map((f) => [`${f.fieldName}=${f.value}`, f])).values()];
  if (uniqueInvalid.length > 0) {
    return {
      status: 'FAIL',
      detail: `${csvName}: invalid SHA value(s): ` +
        uniqueInvalid.map((f) => `${f.fieldName}="${truncate(f.value, 60)}"`).join('; '),
    };
  }
  const uniqueValid = new Set(fields.map((f) => f.fieldName));
  return {
    status: 'PASS',
    detail: `${csvName}: ${fields.length} SHA field value(s) across column(s)/comment(s) ${
      [...uniqueValid].join(', ')
    } all valid 40-hex`,
  };
}

function truncate(s, n) {
  return s.length > n ? s.slice(0, n) + '…' : s;
}

// --------------------------------------------------------------------------
// Check (c): cited raw logs exist
// --------------------------------------------------------------------------

// Matches both `docs/perf/_raw_X.log` and bare `` `_raw_X.log` `` citation
// forms (both are used across existing reports — see this file's header
// comment). Captures just the filename; resolved against docs/perf/ below.
const RAW_LOG_CITATION_RE = /(?:docs\/perf\/)?(_raw_[A-Za-z0-9_./-]*\.log)/g;

function checkRawLogsExist(text) {
  const cited = new Set();
  for (const m of text.matchAll(RAW_LOG_CITATION_RE)) {
    // Guard against a truncation-marker pattern like "_raw_*.log" (a literal
    // glob in prose, e.g. this very script's own header comment style) —
    // reject any captured name containing a literal '*' or other non-
    // filename character.
    const name = m[1];
    if (/[*?]/.test(name)) continue;
    cited.add(name);
  }
  if (cited.size === 0) {
    return { status: 'SKIP', detail: 'report cites no _raw_*.log filename' };
  }
  const missing = [...cited].filter((name) => !existsSync(resolve(PERF_DIR, name)));
  if (missing.length > 0) {
    return {
      status: 'FAIL',
      detail: `cited raw log(s) not found under docs/perf/: ${missing.join(', ')}`,
    };
  }
  return { status: 'PASS', detail: `${cited.size} cited raw log(s) all present` };
}

// --------------------------------------------------------------------------
// Check (d): prose <-> CSV headline cross-check
// --------------------------------------------------------------------------
//
// LIMITATIONS OF THIS HEURISTIC (documented per the task brief's explicit
// request — read this before trusting or extending it):
//
//   1. It is a NUMBER-NEAR-KEYWORD-NEAR-UNIT text scan, not a claim parser.
//      It cannot tell "headline: 100.0% hit rate" from "not 12.5% but
//      100.0%" — both look like a number near "hit rate" with a "%" unit.
//   2. It only credits a prose number if the SAME number (at 2 significant
//      rounding tolerances — exact string match, or match to 1 decimal
//      place) appears ANYWHERE in the CSV text, in ANY column — it does not
//      map "this prose sentence" to "this specific CSV cell." A number that
//      coincidentally matches an unrelated CSV column (e.g. a row index or
//      an n_pairs count) can produce a false PASS.
//   3. It only scans for units this project's reports actually use
//      (MiB/KiB/GiB/%/ns per op/Ir/ms) near keywords this project's reports
//      actually use (headline/hit rate/retention/MDE/decommit/mechanism
//      delta/RSS) — a report phrasing its headline differently, or using a
//      unit outside this list, produces a false negative (nothing flagged),
//      not a false PASS. This asymmetry is deliberate: a missed check is
//      recoverable by a human reader; a false FAIL blocking `npm run check`
//      on a legitimate report is not — hence this check is WARN-level only.
//   4. Derived/rounded numbers (e.g. prose stating "~7x" where the CSV only
//      has the two raw operands, not the ratio) will not match and will be
//      WARNed even though the report is correct — a human must judge each
//      WARN, this check does not claim to auto-verify arithmetic (that is
//      CLAUDE.md's separate "assert the arithmetic in the generating
//      script" rule, a different, stronger guarantee this check does not
//      replace).
//
// Given these limits, check (d) is scoped deliberately narrow: it flags
// prose numbers that are ABSENT from the CSV entirely, as a cheap tripwire
// for the R29-3-style "quoted a number that matches neither raw source"
// defect class — it does not attempt to be a general fact-checker.

// Keywords this project's reports actually use next to a headline number
// (see docs/perf/R30_6_*, R31_1_*, R31_2_*, R31_3_* for the vocabulary this
// list is drawn from).
const HEADLINE_KEYWORD_RE =
  /\b(headline|hit[- ]rate|retention|MDE|decommit|mechanism delta|activation)\b/i;

// A number-with-unit token: an integer or decimal, optionally with a sign or
// leading `~`/`≈`, followed (with optional whitespace) by one of the units
// this project's reports actually use. Captures the numeric part only.
//
// Trailing boundary is `(?![A-Za-z])` rather than `\b`: `\b` only fires on a
// transition between a word char and a non-word char, so it correctly stops
// "MiBs" from matching "MiB" mid-word, but it ALSO silently fails to match
// after `%` — `%` is itself a non-word character, so "88.0% " has no
// word/non-word transition at that point and `\b` never matches, meaning
// EVERY percentage headline number was silently skipped before this fix
// (caught by this task's own scratch "break it, see the check fire" test —
// a report with a genuine "88.0%" headline number absent from its CSV
// produced a SKIP, not the expected WARN, until this was corrected).
// `(?![A-Za-z])` gives the same mid-word protection ("MiBs" still rejected)
// without excluding `%`.
const NUMBER_WITH_UNIT_RE =
  /[~≈]?[-+]?(\d[\d,]*\.?\d*)\s?(MiB|KiB|GiB|%|ns\/op|ns|Ir|ms)(?![A-Za-z])/g;

/**
 * Scan `text` for headline-shaped number+unit tokens: a NUMBER_WITH_UNIT
 * match within `windowChars` characters of a HEADLINE_KEYWORD_RE hit,
 * scanning line-by-line (a report's headline sentences are always within
 * one paragraph/line in this project's Markdown style — matching precedent:
 * this script's own SHA_COMMENT_RE and existing citation regexes are all
 * single-line scans, not cross-line).
 */
function extractHeadlineNumbers(text, windowChars = 200) {
  const found = [];
  for (const line of text.split(/\r?\n/)) {
    if (!HEADLINE_KEYWORD_RE.test(line)) continue;
    for (const m of line.matchAll(NUMBER_WITH_UNIT_RE)) {
      const idx = m.index ?? 0;
      const keywordMatch = HEADLINE_KEYWORD_RE.exec(line);
      HEADLINE_KEYWORD_RE.lastIndex = 0; // reset (test() above advances it as it's global-less but be safe)
      const keywordIdx = keywordMatch ? keywordMatch.index : 0;
      if (Math.abs(idx - keywordIdx) > windowChars) continue;
      const raw = m[1].replace(/,/g, '');
      const value = Number.parseFloat(raw);
      if (Number.isNaN(value)) continue;
      found.push({ value, unit: m[2], token: m[0].trim(), line: line.trim() });
    }
  }
  return found;
}

/** Every bare numeric token appearing anywhere in the CSV text, parsed as a
 * float, deduplicated. Used as the match pool for check (d) — deliberately
 * column-agnostic (see limitation #2 above).
 *
 * Parses CELL-BY-CELL via `splitCsvLine` (the same delimiter-aware splitter
 * check (b)/(f) already use), NOT a raw regex scan over the whole text: a
 * naive digit-run regex applied over unsplit CSV text treats the FIELD
 * DELIMITER comma the same as a thousands-separator comma, so adjacent
 * numeric cells like `...,64,1,67108864,...` collapse into one bogus token
 * (`"64,1,67108864"` -> stripped to `64167108864`) instead of three separate
 * values `64`, `1`, `67108864` — silently hiding the very value (a bare `64`
 * in a `headroom_mib` column) a headline number like "64 MiB" needs to
 * match against. Splitting into cells first, then scanning each cell's own
 * text for numeric tokens (a cell can itself contain more than one number,
 * e.g. a quoted features string), avoids this cross-column bleed entirely. */
function extractCsvNumbers(csvText) {
  const nums = new Set();
  for (const line of csvText.split(/\r?\n/)) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    for (const cell of splitCsvLine(line)) {
      for (const m of cell.matchAll(/-?\d[\d,]*\.?\d*/g)) {
        const raw = m[0].replace(/,/g, '');
        const value = Number.parseFloat(raw);
        if (!Number.isNaN(value)) nums.add(value);
      }
    }
  }
  return nums;
}

/** True if `value` matches some number in `pool` either exactly, rounded to
 * 1 decimal place, or rounded to the nearest integer — the "reasonable
 * rounding" tolerance the task brief asks for. */
function matchesWithRounding(value, pool) {
  if (pool.has(value)) return true;
  const r1 = Math.round(value * 10) / 10;
  const r0 = Math.round(value);
  for (const p of pool) {
    if (Math.round(p * 10) / 10 === r1) return true;
    if (Math.round(p) === r0) return true;
  }
  return false;
}

/**
 * Check (d): every headline-shaped prose number should reappear somewhere
 * in the companion CSV. SKIPs (same as check a) when no CSV is cited at
 * all. WARN-level only — see the limitations block above.
 */
function checkProseCsvCrossCheck(text, citedCsvNames) {
  if (citedCsvNames.size === 0) {
    return { status: 'SKIP', detail: 'report cites no *_summary.csv path (see check a)' };
  }
  const headlineNumbers = extractHeadlineNumbers(text);
  if (headlineNumbers.length === 0) {
    return { status: 'SKIP', detail: 'no headline-shaped number+unit token found near a headline keyword' };
  }
  const csvPool = new Set();
  const missingCsvs = [];
  for (const csvName of citedCsvNames) {
    const csvPath = resolve(PERF_DIR, csvName);
    if (!existsSync(csvPath)) {
      missingCsvs.push(csvName);
      continue;
    }
    for (const n of extractCsvNumbers(readFileSync(csvPath, 'utf8'))) csvPool.add(n);
  }
  if (csvPool.size === 0) {
    return {
      status: 'SKIP',
      detail: missingCsvs.length > 0
        ? `cited CSV(s) missing on disk (see check a): ${missingCsvs.join(', ')}`
        : 'cited CSV(s) contain no numeric tokens',
    };
  }
  const unmatched = headlineNumbers.filter((h) => !matchesWithRounding(h.value, csvPool));
  if (unmatched.length > 0) {
    const uniqueUnmatched = [...new Map(unmatched.map((h) => [h.token, h])).values()];
    return {
      status: 'WARN',
      detail:
        `${uniqueUnmatched.length} headline number(s) not found (even rounded) in the cited CSV — ` +
        `may be a derived/rounded figure (see this check's documented limitations), verify by hand: ` +
        uniqueUnmatched.map((h) => `"${h.token}" (from: "${truncate(h.line, 80)}")`).join('; '),
    };
  }
  return {
    status: 'PASS',
    detail: `${headlineNumbers.length} headline number(s) all found (within rounding) in the cited CSV`,
  };
}

// --------------------------------------------------------------------------
// Check (e): "Allocator layer under test" field
// --------------------------------------------------------------------------
//
// Mechanizes CLAUDE.md's "A gate report must name the exact entry point
// under test..." rule (added by commit `2d9eef2`, prose text search key:
// "the exact entry point under test"). The one report that has already
// adopted this convention voluntarily
// (`docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md`, `## 0.
// Allocator layer under test (CLAUDE.md's R30-8 rule)`) is the positive
// pattern this regex is built to recognize; `docs/perf/R31_2_...md` is the
// KNOWN negative case the task brief names — it states its layer
// (`#[global_allocator]`) only in flowing prose (§ header comment,
// "Feature set" line), not through a structured field, and is exactly the
// "flag, don't hard-fail" scenario this check is designed to produce a
// non-vacuous WARN for.

const ALLOWED_LAYERS = ['AllocCore', 'HeapCore', 'SeferAlloc', '#[global_allocator]', 'downstream application'];

// A structured field: a Markdown heading or bold-label line mentioning
// "allocator layer" or "entry point under test" (case-insensitive) —
// matches both "## 0. Allocator layer under test" (R31-3's own heading) and
// a bold inline label style ("**Allocator layer under test:** ...").
const LAYER_FIELD_HEADING_RE = /(?:^#{1,6}\s.*|^\*\*)[^\n]*?(allocator layer|entry point under test)/im;

// Promotion/production-verdict language — the specific combination this
// check additionally flags when paired with a sub-SeferAlloc layer.
const PROMOTION_LANGUAGE_RE =
  /\b(promote|promotion)\b|production feature composition|\bGO\b.{0,40}production|production.{0,40}\bGO\b|\bNO-GO\b.{0,40}production|production.{0,40}\bNO-GO\b/i;

/**
 * Check (e): does the report declare its allocator layer via a structured
 * field, and (if it names a layer below SeferAlloc) does it also use
 * promotion/production-verdict language that the R30-3/R31-0 defect shape
 * warns against? Always WARN, never FAIL — CLAUDE.md's own rule text
 * treats a substrate-only report as legitimate.
 */
function checkAllocatorLayerField(text) {
  const headingMatch = LAYER_FIELD_HEADING_RE.exec(text);
  if (!headingMatch) {
    return {
      status: 'WARN',
      detail:
        'no structured "Allocator layer under test" / "entry point under test" field found ' +
        '(CLAUDE.md commit 2d9eef2\'s rule; a substrate-only report legitimately has none — ' +
        'verify by hand whether this report\'s verdict is meant to generalize to a real ' +
        '#[global_allocator] chain)',
    };
  }
  // Look at a window after the heading for which of the allowed layer names
  // are actually mentioned (R31-3's own field names TWO layers explicitly,
  // since different sections of that report drive different entry points —
  // so this collects ALL layers mentioned, not just the first).
  const windowText = text.slice(headingMatch.index, headingMatch.index + 800);
  const namedLayers = ALLOWED_LAYERS.filter((layer) => windowText.includes(layer));
  if (namedLayers.length === 0) {
    return {
      status: 'WARN',
      detail:
        'a layer field heading was found but names none of the five allowed values ' +
        `(${ALLOWED_LAYERS.join(' / ')}) within the following ~800 characters — verify by hand`,
    };
  }
  // The R30-3/R31-0 defect shape: layer named is strictly below SeferAlloc
  // (bare AllocCore or HeapCore without also mentioning SeferAlloc/the real
  // global allocator/a downstream app) AND the report's prose elsewhere
  // uses promotion/production-verdict language.
  const namesOnlySubSeferAlloc =
    namedLayers.length > 0 &&
    namedLayers.every((l) => l === 'AllocCore' || l === 'HeapCore');
  if (namesOnlySubSeferAlloc && PROMOTION_LANGUAGE_RE.test(text)) {
    return {
      status: 'WARN',
      detail:
        `layer field names only sub-SeferAlloc layer(s) (${namedLayers.join(', ')}) but report text ` +
        'also uses promotion/production-verdict language (promote/production feature composition/' +
        'GO or NO-GO near "production") — this is the exact R30-3/R31-0 defect shape ' +
        '(measuring below the layer a feature actually ships at); verify by hand',
    };
  }
  return {
    status: 'PASS',
    detail: `layer field present, names: ${namedLayers.join(', ')}`,
  };
}

// --------------------------------------------------------------------------
// Check (f): between-arm mechanism delta
// --------------------------------------------------------------------------
//
// Applies only to a "comparative" report — one whose CSV or prose names two
// or more arms/comparisons AND uses mechanism/activation vocabulary (the
// same HEADLINE_KEYWORD_RE-adjacent vocabulary check (d) already scans
// for). A report with no comparison structure at all (e.g. a single-arm
// Stage-1 decomposition) is SKIPped — this check does not invent a
// two-arm requirement CLAUDE.md's rule does not impose on every report.

const MECHANISM_KEYWORD_RE = /\b(mechanism delta|decommit|activation|hit[- ]rate|retention)\b/i;
// A delta-shaped CSV column name.
const DELTA_COLUMN_NAME_RE = /delta/i;
// Prose stating a delta explicitly: "delta" or "Δ" near a number, or the
// literal word "ZERO"/"zero" describing a delta (this project's own
// R31-2 report phrases its null result as "mechanism delta ... ZERO").
const PROSE_DELTA_RE = /(?:mechanism\s+)?delta[^.\n]{0,60}?(?:is|stays|=|:)?\s*(ZERO|zero|-?\d[\d,.]*)/i;

/**
 * Check (f): a comparative report must carry an explicit between-arm
 * mechanism delta, not just per-arm counts left for the reader to subtract.
 */
function checkMechanismDelta(text, citedCsvNames) {
  if (!MECHANISM_KEYWORD_RE.test(text)) {
    return { status: 'SKIP', detail: 'no mechanism/activation vocabulary found — not a mechanism-comparison report' };
  }
  // Heuristic for "compares two or more arms": the report's own headline
  // table markup contains a "vs"/"vs." comparison label, OR the CSV (if
  // any) has a `comparison`/`arm` column with more than one distinct value.
  const hasVsLanguage = /\bvs\.?\s/i.test(text);
  let csvHasMultipleArms = false;
  for (const csvName of citedCsvNames) {
    const csvPath = resolve(PERF_DIR, csvName);
    if (!existsSync(csvPath)) continue;
    const csvText = readFileSync(csvPath, 'utf8');
    const lines = csvText.split(/\r?\n/).filter((l) => l.trim() && !l.trim().startsWith('#'));
    if (lines.length < 2) continue;
    const header = splitCsvLine(lines[0]);
    const armColIdx = header.findIndex((h) => /^(comparison|arm)$/i.test(h.trim()));
    if (armColIdx === -1) continue;
    const values = new Set(lines.slice(1).map((l) => splitCsvLine(l)[armColIdx]?.trim()).filter(Boolean));
    if (values.size > 1) csvHasMultipleArms = true;
  }
  if (!hasVsLanguage && !csvHasMultipleArms) {
    return { status: 'SKIP', detail: 'no evidence of a two-or-more-arm comparison (no "vs" language, no multi-value comparison/arm CSV column)' };
  }

  // Look for a delta column in any cited CSV.
  let csvHasDeltaColumn = false;
  for (const csvName of citedCsvNames) {
    const csvPath = resolve(PERF_DIR, csvName);
    if (!existsSync(csvPath)) continue;
    const csvText = readFileSync(csvPath, 'utf8');
    const lines = csvText.split(/\r?\n/).filter((l) => l.trim() && !l.trim().startsWith('#'));
    if (lines.length === 0) continue;
    const header = splitCsvLine(lines[0]);
    if (header.some((h) => DELTA_COLUMN_NAME_RE.test(h.trim()))) csvHasDeltaColumn = true;
  }
  if (csvHasDeltaColumn) {
    return { status: 'PASS', detail: 'comparative report; a delta-shaped column found in the cited CSV' };
  }
  if (PROSE_DELTA_RE.test(text)) {
    return { status: 'PASS', detail: 'comparative report; prose explicitly states a between-arm delta' };
  }
  return {
    status: 'WARN',
    detail:
      'comparative report (mechanism/activation vocabulary + multi-arm comparison detected) but ' +
      'neither a delta-shaped CSV column nor explicit prose delta statement found — verify the ' +
      'reader is not left to subtract per-arm counts by hand',
  };
}

// --------------------------------------------------------------------------
// Post-facto identity check: cited SHA newer than its own raw logs
// --------------------------------------------------------------------------
//
// Signal (not proof) that a report's cited commit SHA was assembled AFTER
// measurement rather than captured beforehand per CLAUDE.md's R29-6 rule —
// mirrors the concern `scripts/capture-measurement-identity.mjs`'s header
// comment explains in full. A report's raw logs are written at measurement
// time (mtime on disk); if the cited SHA's own commit timestamp is LATER
// than every one of that report's raw logs' mtimes, the SHA cannot have
// been the tree that produced those logs — it was filled in afterward. This
// is exactly the sanctioned "chicken-and-egg" landing-commit pattern several
// reports already document explicitly in prose (R30-6, R30-7, R31-1, R31-2
// all state it), so this check is WARN-level ONLY and is expected to fire
// on those already-known, already-explained cases — it exists to catch a
// FUTURE report that does the same thing WITHOUT the explaining prose, not
// to relitigate the already-documented ones.

function checkShaNewerThanRawLogs(text, citedRawLogNames) {
  if (citedRawLogNames.size === 0) {
    return { status: 'SKIP', detail: 'no cited raw logs to compare a SHA timestamp against (see check c)' };
  }
  // Reuse whichever 40-hex SHA(s) the report's own prose names as its
  // "measured on" / "base revision" commit — scan prose directly (not the
  // CSV) since this is about the report's OWN stated identity, not every
  // SHA a companion CSV happens to carry (e.g. R31-2's CSV repeats the same
  // base SHA on every row — scanning prose avoids treating that repetition
  // as several independent claims).
  const shaMatches = [...text.matchAll(/\b[0-9a-f]{40}\b/g)].map((m) => m[0]);
  if (shaMatches.length === 0) {
    return { status: 'SKIP', detail: 'no 40-hex SHA found in report prose' };
  }
  const rawLogMtimes = [...citedRawLogNames]
    .map((name) => resolve(PERF_DIR, name))
    .filter((p) => existsSync(p))
    .map((p) => statSync(p).mtime);
  if (rawLogMtimes.length === 0) {
    return { status: 'SKIP', detail: 'cited raw log(s) missing on disk (see check c)' };
  }
  const earliestRawLogMtime = new Date(Math.min(...rawLogMtimes.map((d) => d.getTime())));

  const laterShas = [];
  for (const sha of new Set(shaMatches)) {
    const r = spawnSync('git', ['log', '-1', '--format=%cI', sha], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
    });
    if (r.status !== 0 || !r.stdout.trim()) continue; // SHA not resolvable (e.g. not yet pushed, or a non-commit hex string) — not this check's concern
    const commitDate = new Date(r.stdout.trim());
    if (commitDate.getTime() > earliestRawLogMtime.getTime()) {
      laterShas.push({ sha, commitDate: commitDate.toISOString() });
    }
  }
  if (laterShas.length === 0) {
    return { status: 'PASS', detail: `all cited SHA(s) predate or match the earliest cited raw log's mtime (${earliestRawLogMtime.toISOString()})` };
  }
  return {
    status: 'WARN',
    detail:
      `${laterShas.length} cited SHA(s) postdate the earliest cited raw log's mtime ` +
      `(${earliestRawLogMtime.toISOString()}) — possible post-facto identity assembly (see ` +
      `scripts/capture-measurement-identity.mjs for the fix going forward; this is EXPECTED and ` +
      `already documented in prose for the known landing-commit chicken-and-egg pattern several ` +
      `reports use, e.g. R30-6/R30-7/R31-1/R31-2): ` +
      laterShas.map((s) => `${s.sha.slice(0, 12)}… @ ${s.commitDate}`).join('; '),
  };
}

// --------------------------------------------------------------------------
// Per-report driver
// --------------------------------------------------------------------------

/**
 * Run every check against one report. Returns
 * `{ mdPath, results: { a, b, c, d, e, f, identity }, ok }` where each
 * result is `{ status: 'PASS'|'FAIL'|'WARN'|'SKIP', detail }[]` (checks (b)
 * can yield more than one result — one per cited companion CSV file).
 * Checks (d)/(e)/(f)/identity are WARN-level only and never affect `ok`.
 */
function verifyReport(mdPath) {
  const name = basename(mdPath, '.md');
  const text = readFileSync(mdPath, 'utf8');
  const exemption = RETROACTIVE_EXEMPT.get(name);
  const exemptChecks = new Set(exemption?.checks ?? []);

  const results = { a: [], b: [], c: [], d: [], e: [], f: [], identity: [] };

  const aResult = checkCompanionCsv(mdPath, text);
  results.a.push(aResult);

  // Check (b) runs against every companion CSV cited in the report (usually
  // the one check (a) already found) — re-derive the cited set rather than
  // threading it through, since (a) may SKIP while a CSV still happens to
  // exist on disk with the report's exact conventional name.
  const citedCsvNames = new Set();
  for (const m of text.matchAll(SUMMARY_CSV_CITATION_RE)) citedCsvNames.add(m[1]);
  if (citedCsvNames.size === 0) {
    results.b.push({ status: 'SKIP', detail: 'no companion CSV cited (see check a)' });
  } else {
    for (const csvName of citedCsvNames) {
      const csvPath = resolve(PERF_DIR, csvName);
      if (!existsSync(csvPath)) {
        results.b.push({ status: 'SKIP', detail: `${csvName}: file missing (already reported by check a)` });
        continue;
      }
      let r = checkValidSha(csvName, csvPath);
      const csvExemptReason = RETROACTIVE_EXEMPT_CSV.get(csvName);
      if (r.status === 'FAIL' && csvExemptReason) {
        r = { status: 'EXEMPT', detail: `${r.detail} (retroactive exemption: ${csvExemptReason})` };
      }
      results.b.push(r);
    }
  }

  const cResult = checkRawLogsExist(text);
  results.c.push(cResult);

  // Re-derive the cited raw-log name set the same way check (c) did, for
  // reuse by the identity-freshness check below.
  const citedRawLogNames = new Set();
  for (const m of text.matchAll(RAW_LOG_CITATION_RE)) {
    const rawName = m[1];
    if (/[*?]/.test(rawName)) continue;
    citedRawLogNames.add(rawName);
  }

  results.d.push(checkProseCsvCrossCheck(text, citedCsvNames));
  results.e.push(checkAllocatorLayerField(text));
  results.f.push(checkMechanismDelta(text, citedCsvNames));
  results.identity.push(checkShaNewerThanRawLogs(text, citedRawLogNames));

  // Apply report-keyed exemptions (checks a/c): a FAIL in an exempted check
  // is downgraded to a reported, non-blocking EXEMPT status — never
  // silently dropped.
  for (const key of ['a', 'c']) {
    if (!exemptChecks.has(key)) continue;
    results[key] = results[key].map((r) =>
      r.status === 'FAIL'
        ? { status: 'EXEMPT', detail: `${r.detail} (retroactive exemption: ${exemption.reason})` }
        : r,
    );
  }

  // Checks (d)/(e)/(f)/identity are WARN-level only by construction (their
  // functions never return FAIL) — `ok` therefore only depends on (a)/(b)/(c),
  // unchanged from before this task.
  const ok = ['a', 'b', 'c'].every((key) => results[key].every((r) => r.status !== 'FAIL'));
  return { mdPath, name, results, ok };
}

// --------------------------------------------------------------------------
// CLI
// --------------------------------------------------------------------------

function discoverReports() {
  return readdirSync(PERF_DIR)
    .filter((f) => /^R\d.*\.md$/.test(f))
    .map((f) => resolve(PERF_DIR, f))
    .sort();
}

const rawArgs = process.argv.slice(2);
const newOnly = rawArgs.includes('--new-only');
const pathArgs = rawArgs.filter((a) => a !== '--new-only');

const targets = pathArgs.length > 0 ? pathArgs.map((p) => resolve(REPO_ROOT, p)) : discoverReports();

if (newOnly) {
  // A report is "already exempted" if it is directly in RETROACTIVE_EXEMPT
  // (checks a/c) OR if it cites a companion CSV that is in
  // RETROACTIVE_EXEMPT_CSV (check b) — --new-only means "refuse to scan
  // anything with a documented historical exemption", regardless of which
  // check the exemption applies to.
  const blocked = targets.filter((p) => {
    const name = basename(p, '.md');
    if (RETROACTIVE_EXEMPT.has(name)) return true;
    if (!existsSync(p)) return false;
    const text = readFileSync(p, 'utf8');
    for (const m of text.matchAll(SUMMARY_CSV_CITATION_RE)) {
      if (RETROACTIVE_EXEMPT_CSV.has(m[1])) return true;
    }
    return false;
  });
  if (blocked.length > 0) {
    console.error(
      `[verify-gate-report] --new-only: refusing to scan report(s) with a documented ` +
        `retroactive exemption: ${blocked.map((p) => basename(p)).join(', ')}`,
    );
    process.exit(2);
  }
}

console.log(`[verify-gate-report] scanning ${targets.length} report(s) under ${dirname(targets[0] ?? PERF_DIR)}\n`);

let allOk = true;
let anyWarn = false;
for (const mdPath of targets) {
  if (!existsSync(mdPath)) {
    console.log(`FAIL  ${basename(mdPath)} — file not found`);
    allOk = false;
    continue;
  }
  const { name, results, ok } = verifyReport(mdPath);
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}.md`);
  for (const [key, label] of [
    ['a', '(a) companion CSV exists'],
    ['b', '(b) valid SHA / no placeholder'],
    ['c', '(c) cited raw logs exist'],
    ['d', '(d) prose<->CSV headline cross-check [WARN-only]'],
    ['e', '(e) allocator layer under test field [WARN-only]'],
    ['f', '(f) between-arm mechanism delta [WARN-only]'],
    ['identity', '(identity) SHA vs raw-log mtime freshness [WARN-only]'],
  ]) {
    for (const r of results[key]) {
      console.log(`      ${label}: ${r.status} — ${r.detail}`);
      if (r.status === 'WARN') anyWarn = true;
    }
  }
  if (!ok) allOk = false;
}

console.log(`\n[verify-gate-report] ${allOk ? 'ALL GREEN' : 'FAILED'} (${targets.length} report(s) scanned)` +
  (anyWarn ? ' — one or more WARN-level semantic checks fired; see above (does not fail the run)' : ''));
process.exit(allOk ? 0 : 1);
