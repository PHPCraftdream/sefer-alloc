#!/usr/bin/env node
// CI job duration comparison for the P0.2 test-merge pilot (docs/perf/P0_2_TEST_MERGE_PILOT.md).
// Compares `gh run view <id> --json jobs` dumps (one JSON file per run, one --before and one
// --after directory) and reports, per affected job, the median seconds spent in `cargo test`
// steps before vs. after a change. Derived, never typed: every median, delta and count is
// re-verified by an independent in-script assertion before anything is printed.
//
// Usage:
//   node scripts/ci-job-durations.mjs --before <dir> --after <dir> [--csv <out.csv>]
//       [--step-pattern <regex>] [--jobs <n1,n2,...>] [--min-runs 3]
//
// Exit code is 0 on success even if verdicts are "grew" — this script reports, it does not
// decide GO/NO-GO. Exit 1 on corrupt input JSON (the offending file is named) or on any
// failed internal assertion. Node >= 24, zero npm dependencies, no network.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

/** Classic median: middle element for odd n, mean of the two middle elements for even n. */
export function median(xs) {
  if (xs.length === 0) throw new Error('median of empty list');
  const s = [...xs].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 === 1 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

/** Independent second median computation (different index math) used for the assertions. */
function medianAlt(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  const lo = Math.ceil(n / 2) - 1; // lower middle (same as floor((n-1)/2))
  const hi = Math.floor(n / 2);
  return lo === hi ? s[lo] : (s[lo] + s[hi]) / 2;
}

/** (completed_at - started_at) in seconds; null when either timestamp is missing/invalid. */
export function durationSeconds(startedAt, completedAt) {
  if (!startedAt || !completedAt) return null;
  const start = Date.parse(startedAt);
  const end = Date.parse(completedAt);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  const d = (end - start) / 1000;
  return Number.isFinite(d) && d >= 0 ? d : null;
}

/** Sum of step durations whose name matches `pattern`; null when no step matches. */
export function cargoTestStepSeconds(job, pattern) {
  let sum = null;
  const raws = [];
  for (const step of job.steps ?? []) {
    if (!pattern.test(step.name ?? '')) continue;
    const d = durationSeconds(step.started_at, step.completed_at);
    if (d === null) continue;
    raws.push(d);
    sum = (sum ?? 0) + d;
  }
  return sum === null ? null : { seconds: sum, raws };
}

/**
 * Read every *.json file in `dir`; each must parse as `{jobs: [...]}`.
 * Returns Map(jobName -> [{ cargoTestSeconds, metric, jobSeconds }]) — one entry per run.
 * A step/job with missing or invalid timestamps contributes a `skipped` count.
 */
export function collectRuns(dir, pattern) {
  const perJob = new Map(); // job name -> array of per-run records
  let skipped = 0;
  const files = readdirSync(dir).filter((f) => f.endsWith('.json')).sort();
  for (const file of files) {
    const path = join(dir, file);
    let doc;
    try {
      doc = JSON.parse(readFileSync(path, 'utf8'));
    } catch (e) {
      throw new Error(`failed to parse ${path}: ${e.message}`);
    }
    if (!Array.isArray(doc?.jobs)) throw new Error(`${path}: missing "jobs" array`);
    for (const job of doc.jobs) {
      const name = String(job.name ?? '');
      if (!perJob.has(name)) perJob.set(name, []);
      const stepSum = cargoTestStepSeconds(job, pattern);
      const jobSeconds = durationSeconds(job.started_at, job.completed_at);
      let rec;
      if (stepSum !== null) {
        rec = { seconds: stepSum.seconds, metric: 'steps', raws: stepSum.raws, jobSeconds };
      } else if (jobSeconds !== null) {
        rec = { seconds: jobSeconds, metric: 'job', raws: null, jobSeconds };
      } else {
        rec = null;
        skipped += 1;
      }
      if (rec) perJob.get(name).push(rec);
    }
  }
  return { perJob, skipped, files: files.length };
}

/** Verdict for one delta value: fell <= -5, grew > 0, flat otherwise. */
export function verdictOf(deltaPct) {
  if (deltaPct <= -5) return 'fell';
  if (deltaPct > 0) return 'grew';
  return 'flat';
}

/** Delta percent, rounded to 2 decimals for display. */
export function deltaPct(before, after) {
  return (after - before) / before * 100;
}

/**
 * Build the report rows. `jobs` (array of names) restricts the affected set; when empty,
 * every job present on both sides is reported and rows whose metric came from the whole
 * job duration in every run on a side are marked distinctly (metric column "job").
 */
export function computeRows({ before, after, jobs = [], minRuns = 3 }) {
  const names = jobs.length > 0
    ? jobs
    : [...before.perJob.keys()].filter((n) => after.perJob.has(n)).sort();
  const rows = [];
  for (const name of names) {
    const bRuns = (before.perJob.get(name) ?? []).filter((r) => jobs.length === 0 || r.seconds !== null);
    const aRuns = (after.perJob.get(name) ?? []).filter((r) => jobs.length === 0 || r.seconds !== null);
    if (bRuns.length === 0 && aRuns.length === 0) continue; // job absent on both sides
    const side = (runs) => {
      if (runs.length === 0) return { n: 0, med: null, min: null, max: null, metric: '-' };
      const secs = runs.map((r) => r.seconds);
      return {
        n: secs.length,
        med: median(secs),
        min: Math.min(...secs),
        max: Math.max(...secs),
        metric: runs.every((r) => r.metric === 'job') ? 'job' : 'steps',
      };
    };
    const b = side(bRuns);
    const a = side(aRuns);
    const insufficient = b.n < minRuns || a.n < minRuns;
    let d = null;
    let verdict = 'insufficient-runs';
    if (!insufficient) {
      d = deltaPct(b.med, a.med);
      verdict = verdictOf(d);
    }
    rows.push({ job: name, metric: a.metric !== '-' ? a.metric : b.metric, b, a, delta: d, verdict, insufficient });
  }
  return rows;
}

/** Assert every derived value in `rows` against independent recomputation. Hard errors on mismatch. */
export function assertRows(rows, { before, after }) {
  for (const row of rows) {
    for (const [side, runs] of [['before', before.perJob.get(row.job) ?? []], ['after', after.perJob.get(row.job) ?? []]]) {
      const secs = runs.map((r) => r.seconds);
      assertEqual(row[side === 'before' ? 'b' : 'a'].n, secs.length, `${row.job} ${side} n`);
      if (secs.length === 0) continue;
      const med = row[side === 'before' ? 'b' : 'a'].med;
      assertEqual(med, medianAlt(secs), `${row.job} ${side} median (alt computation)`);
      assertEqual(med, median(secs), `${row.job} ${side} median (recompute)`);
      // sum-of-steps re-verified from raw step durations
      for (const r of runs) {
        if (r.raws) {
          assertEqual(r.seconds, r.raws.reduce((x, y) => x + y, 0), `${row.job} ${side} step sum`);
        } else {
          assertEqual(r.seconds, r.jobSeconds, `${row.job} ${side} whole-job duration`);
        }
      }
    }
    if (!row.insufficient) {
      const expected = deltaPct(row.b.med, row.a.med);
      assertEqual(row.delta, expected, `${row.job} delta_pct`);
      assertEqual(Math.abs(row.delta - (row.a.med - row.b.med) / row.b.med * 100), 0, `${row.job} delta formula`);
      assertEqual(row.verdict, verdictOf(row.delta), `${row.job} verdict`);
    } else {
      assertEqual(row.verdict, 'insufficient-runs', `${row.job} insufficient verdict`);
    }
  }
}

function assertEqual(got, want, what) {
  if (typeof got === 'number' && typeof want === 'number') {
    if (Math.abs(got - want) > 1e-9) throw new Error(`assertion failed: ${what}: got ${got}, want ${want}`);
  } else if (got !== want) {
    throw new Error(`assertion failed: ${what}: got ${got}, want ${want}`);
  }
}

const F2 = (x) => x.toFixed(2);

/** Markdown table + summary line. */
export function renderMd(rows) {
  const head = '| job | metric | n_before | median_before_s | n_after | median_after_s | delta_pct | verdict |';
  const sep = '|---|---|---|---|---|---|---|---|';
  const lines = rows.map((r) =>
    `| ${r.job} | ${r.metric} | ${r.b.n} | ${r.b.med === null ? '-' : F2(r.b.med)} | ${r.a.n} | ${r.a.med === null ? '-' : F2(r.a.med)} | ${r.delta === null ? '-' : F2(r.delta)} | ${r.verdict} |`);
  const counts = { fell: 0, flat: 0, grew: 0 };
  for (const r of rows) if (r.verdict in counts) counts[r.verdict] += 1;
  const summary = `summary: ${counts.fell} fell, ${counts.flat} flat, ${counts.grew} grew (${rows.length} jobs)`;
  return [head, sep, ...lines, '', summary].join('\n');
}

/** CSV with the exact same computed values as the Markdown table. */
export function renderCsv(rows) {
  const esc = (s) => `"${String(s).replaceAll('"', '""')}"`;
  const lines = ['job,metric,n_before,median_before_s,n_after,median_after_s,delta_pct,verdict'];
  for (const r of rows) {
    lines.push([esc(r.job), r.metric, r.b.n, r.b.med === null ? '' : F2(r.b.med), r.a.n, r.a.med === null ? '' : F2(r.a.med), r.delta === null ? '' : F2(r.delta), r.verdict].join(','));
  }
  return lines.join('\n') + '\n';
}

function arg(args, name) {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
}

function main(argv) {
  const beforeDir = arg(argv, '--before');
  const afterDir = arg(argv, '--after');
  if (!beforeDir || !afterDir) {
    console.error('usage: ci-job-durations.mjs --before <dir> --after <dir> [--csv out.csv] [--step-pattern re] [--jobs n1,n2] [--min-runs 3]');
    process.exit(2);
  }
  const pattern = new RegExp(arg(argv, '--step-pattern') ?? 'cargo test');
  const jobs = (arg(argv, '--jobs') ?? '').split(',').map((s) => s.trim()).filter(Boolean);
  const minRuns = Number(arg(argv, '--min-runs') ?? 3);
  const before = collectRuns(beforeDir, pattern);
  const after = collectRuns(afterDir, pattern);
  const rows = computeRows({ before, after, jobs, minRuns });
  assertRows(rows, { before, after }); // everything asserted BEFORE printing
  if (before.skipped + after.skipped > 0) {
    console.log(`skipped (missing/invalid timestamps): ${before.skipped + after.skipped} job record(s)`);
  }
  console.log(renderMd(rows));
  const csvPath = arg(argv, '--csv');
  if (csvPath) writeFileSync(csvPath, renderCsv(rows));
}

if (process.argv[1]?.replace(/\\/g, '/').endsWith('scripts/ci-job-durations.mjs')) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`ci-job-durations: ${e.message}`);
    process.exit(1);
  }
}
