// STRUCTURAL checks over a `docs/perf/R*_*.md` gate report — the machine-
// checkable half of CLAUDE.md's "gate report tables must be derived, not
// hand-transcribed" rule (see CLAUDE.md's "Phased delivery" section, the
// bullet starting "A gate report's tables and headline ratios must be
// DERIVED, by one checked script..."). R31-5 (task #480/#481/#482) splits
// that rule's enforcement into three parts; THIS script is part (a):
// structural checks only. Semantic checks (prose<->CSV cross-check, the
// allocator-layer-named field, mechanism-delta evidence) are task #481
// (R31-5b); the commit-prefix taxonomy lint is task #482 (R31-5c) — both
// extend this same file's CHECKS array rather than becoming separate
// scripts, so a single `npm run verify-gate-reports` stays the one command
// that runs every mechanized gate-report rule.
//
// Three structural checks, each independently reproducing a defect class a
// prior round's review caught by hand and that CLAUDE.md now states as a
// rule:
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

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve, basename, dirname } from 'node:path';
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
// Per-report driver
// --------------------------------------------------------------------------

/**
 * Run every structural check against one report. Returns
 * `{ mdPath, results: { a, b, c }, ok }` where each result is
 * `{ status: 'PASS'|'FAIL'|'SKIP', detail }[]` (check (b) can yield more
 * than one result — one per cited companion CSV file).
 */
function verifyReport(mdPath) {
  const name = basename(mdPath, '.md');
  const text = readFileSync(mdPath, 'utf8');
  const exemption = RETROACTIVE_EXEMPT.get(name);
  const exemptChecks = new Set(exemption?.checks ?? []);

  const results = { a: [], b: [], c: [] };

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
  ]) {
    for (const r of results[key]) {
      console.log(`      ${label}: ${r.status} — ${r.detail}`);
    }
  }
  if (!ok) allOk = false;
}

console.log(`\n[verify-gate-report] ${allOk ? 'ALL GREEN' : 'FAILED'} (${targets.length} report(s) scanned)`);
process.exit(allOk ? 0 : 1);
