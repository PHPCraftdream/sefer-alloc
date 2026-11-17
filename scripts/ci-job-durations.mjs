#!/usr/bin/env node
// CI job duration comparison for the P0.2 test-merge pilot (docs/perf/P0_2_TEST_MERGE_PILOT.md).
// Compares GitHub Actions job dumps (one JSON file per run, one --before and one
// --after directory) and reports, per affected job, the median seconds spent in `cargo test`
// steps before vs. after a change. Derived, never typed: every median, delta and count is
// re-verified by an independent in-script assertion before anything is printed.
//
// Both dump formats are accepted:
//   - `gh run view <id> --json jobs` (camelCase: startedAt / completedAt), and
//   - `gh api repos/<owner>/<repo>/actions/runs/<id>/jobs?per_page=100` (snake_case: started_at / completed_at).
//
// Usage:
//   node scripts/ci-job-durations.mjs --before <dir> --after <dir> [--csv <out.csv>]
//       [--step-pattern <regex>] [--jobs <n1,n2,...>] [--min-runs 3]
//       [--family "<label>=<before-job-regex>=><after-shard-regex>"] ...
//
// --family (repeatable) aggregates an unsharded before job (before-job-regex) with its
// sharded after jobs (after-shard-regex, matches >=1 shard per run). For each run the
// family value is (i) the SUM of the shards' per-job cargo-test/whole-job durations and
// (ii) the MAX over shards (critical path); both are reported as separate table rows
// (metrics "family-sum" / "family-crit"). With any --family/--jobs given, the script
// EXITS 1 when an affected row has fewer than --min-runs samples on either side.
// A summary line prints the criterion-(b) verdict (all fell >= 5% AND none grew).
//
// Exit code is 0 on success even if verdicts are "grew" — this script reports, it does not
// decide GO/NO-GO. Exit 1 on corrupt input JSON or input files with no valid job records
// (the offending file is named), or on any failed internal assertion. Node >= 24, zero npm
// dependencies, no network.
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

export function normalizeJob(job) {
  const out = { ...job, started_at: job.started_at ?? job.startedAt, completed_at: job.completed_at ?? job.completedAt };
  if (Array.isArray(job.steps)) {
    out.steps = job.steps.map((s) => ({ ...s, started_at: s.started_at ?? s.startedAt, completed_at: s.completed_at ?? s.completedAt }));
  }
  return out;
}

/**
 * Read every *.json file in `dir`; each must parse as `{jobs: [...]}`.
 * Both snake_case (gh api .../jobs) and camelCase (gh run view --json jobs) records are
 * accepted: timestamps are normalized to started_at/completed_at at read time.
 * Returns Map(jobName -> [{ cargoTestSeconds, metric, jobSeconds }]) — one entry per run.
 * A step/job with missing or invalid timestamps contributes a `skipped` count.
 * Throws when a file contains jobs but none yielded a valid record, or `jobs` is empty.
 */
export function collectRuns(dir, pattern) {
  const perJob = new Map(); // job name -> array of per-run records
  const perRun = []; // one entry per file: { file, records: Map(job name -> rec) }
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
    if (doc.jobs.length === 0) throw new Error(`${path}: empty "jobs" array`);
    let valid = 0;
    const runRecords = new Map();
    for (const raw of doc.jobs) {
      const job = normalizeJob(raw);
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
      }
      if (rec) perJob.get(name).push(rec);
      else skipped += 1;
      if (rec) { valid += 1; runRecords.set(name, rec); }
    }
    if (valid === 0) throw new Error(`${path}: no valid job record(s) — all ${doc.jobs.length} record(s) skipped (missing/invalid timestamps)`);
    perRun.push({ file, records: runRecords });
  }
  return { perJob, perRun, skipped, files: files.length, fileNames: files };
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

/**
 * Parse --family entries: "<label>=<before-job-regex>=><after-shard-regex>".
 * Returns [{ label, beforeRe, afterRe }]; throws on a malformed entry.
 */
export function parseFamilies(entries) {
  return entries.map((e) => {
    const m = /^(.+?)=([^=]+)=>(.+)$/.exec(e);
    if (!m) throw new Error(`bad --family entry (want "label=beforeRe=>afterRe"): ${e}`);
    return { label: m[1], beforeRe: new RegExp(m[2]), afterRe: new RegExp(m[3]) };
  });
}

/**
 * Aggregate one side's perRun records for a family regex. For every run file, all jobs
 * whose name matches `re` contribute their per-job cargo-test/whole-job duration; a run
 * with zero matching jobs is omitted (job/family absent in that run). Returns an array of
 * per-run records { seconds, metric, jobSeconds } plus the contributing shard names.
 */
export function familySideRecords(side, re) {
  const sums = [];
  const crits = [];
  for (const run of side.perRun) {
    const matches = [...run.records].filter(([name]) => re.test(name));
    if (matches.length === 0) continue;
    const secs = matches.map(([, rec]) => rec.seconds);
    const metric = matches.every(([, rec]) => rec.metric === 'job') ? 'job' : 'steps';
    const shards = matches.map(([name]) => name).sort();
    sums.push({ seconds: secs.reduce((x, y) => x + y, 0), metric, shards });
    crits.push({ seconds: Math.max(...secs), metric, shards });
  }
  return { sums, crits };
}

/**
 * Build collectRuns-shaped synthetic perJob maps for the family rows so the ordinary
 * computeRows/assertRows/render pipeline can be reused unchanged. Every family yields two
 * affected rows: "<label>" (metric family-sum: median of per-run shard SUMS) and
 * "<label> (critical path)" (metric family-crit: median of per-run shard MAXES).
 */
export function computeFamilyRows({ before, after, families, minRuns }) {
  const bPerJob = new Map();
  const aPerJob = new Map();
  const detail = []; // { label, bSumRuns, aSumRuns, bCritRuns, aCritRuns }
  for (const fam of families) {
    const b = familySideRecords(before, fam.beforeRe);
    const a = familySideRecords(after, fam.afterRe);
    const mk = (recs) => recs.map((r) => ({ seconds: r.seconds, metric: r.metric, raws: null, jobSeconds: r.seconds }));
    bPerJob.set(fam.label, mk(b.sums));
    bPerJob.set(`${fam.label} (critical path)`, mk(b.crits));
    aPerJob.set(fam.label, mk(a.sums));
    aPerJob.set(`${fam.label} (critical path)`, mk(a.crits));
    detail.push({ label: fam.label, beforeRe: fam.beforeRe, afterRe: fam.afterRe, bSumRuns: b.sums, aSumRuns: a.sums, bCritRuns: b.crits, aCritRuns: a.crits });
  }
  const bf = { perJob: bPerJob };
  const af = { perJob: aPerJob };
  const names = families.flatMap((f) => [f.label, `${f.label} (critical path)`]);
  const rows = computeRows({ before: bf, after: af, jobs: names, minRuns });
  assertRows(rows, { before: bf, after: af });
  assertFamilies(detail, { before, after });
  return { rows, detail };
}

/** Independent re-aggregation: recompute every family sum/max straight from perRun records. */
export function assertFamilies(detail, { before, after }) {
  for (const d of detail) {
    for (const [side, sideData, sums, crits] of [
      ['before', before, d.bSumRuns, d.bCritRuns],
      ['after', after, d.aSumRuns, d.aCritRuns],
    ]) {
      const matchedSums = [];
      const matchedCrits = [];
      for (const run of sideData.perRun) {
        const secs = [...run.records].filter(([name]) => (side === 'before' ? d.beforeRe : d.afterRe).test(name)).map(([, rec]) => rec.seconds);
        if (secs.length === 0) continue;
        matchedSums.push(secs.reduce((x, y) => x + y, 0));
        matchedCrits.push(Math.max(...secs));
      }
      assertEqual(sums.length, matchedSums.length, `${d.label} ${side} run count`);
      assertEqual(sums.map((r) => r.seconds).join(','), matchedSums.join(','), `${d.label} ${side} per-run sums`);
      assertEqual(crits.map((r) => r.seconds).join(','), matchedCrits.join(','), `${d.label} ${side} per-run critical paths`);
      // each sum must exceed or equal its critical path, and exceed it for >1 shard
      for (const [s, c] of sums.map((s, i) => [s.seconds, crits[i].seconds])) {
        if (!(s >= c)) throw new Error(`assertion failed: ${d.label} ${side} sum ${s} < crit ${c}`);
      }
    }
  }
}

function arg(args, name) {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
}

function main(argv) {
  const beforeDir = arg(argv, '--before');
  const afterDir = arg(argv, '--after');
  if (!beforeDir || !afterDir) {
    console.error('usage: ci-job-durations.mjs --before <dir> --after <dir> [--csv out.csv] [--step-pattern re] [--jobs n1,n2] [--family "label=beforeRe=>afterRe"]... [--min-runs 3]');
    process.exit(2);
  }
  const pattern = new RegExp(arg(argv, '--step-pattern') ?? 'cargo test');
  const jobs = (arg(argv, '--jobs') ?? '').split(',').map((s) => s.trim()).filter(Boolean);
  const familyEntries = [];
  for (let i = 0; i < argv.length; i++) if (argv[i] === '--family') familyEntries.push(argv[i + 1]);
  const families = parseFamilies(familyEntries);
  const minRuns = Number(arg(argv, '--min-runs') ?? 3);
  const before = collectRuns(beforeDir, pattern);
  const after = collectRuns(afterDir, pattern);
  const rows = computeRows({ before, after, jobs, minRuns });
  assertRows(rows, { before, after }); // everything asserted BEFORE printing
  let famDetail = null;
  if (families.length > 0) {
    const fam = computeFamilyRows({ before, after, families, minRuns });
    famDetail = fam.detail;
    rows.push(...fam.rows);
  }
  // contributing run ids (dump file names) per side, for every reported row
  for (const r of rows) {
    const ids = (side) => (side === 'before' ? before.fileNames : after.fileNames).join(', ');
    console.log(`runs ${r.job} | before: ${ids('before')} | after: ${ids('after')}`);
  }
  if (famDetail) {
    for (const d of famDetail) {
      console.log(`family ${d.label} shards (after, per run): ${d.aSumRuns.map((r) => `[${r.shards.join(' + ')}]`).join(' ')}`);
    }
  }
  if (before.skipped + after.skipped > 0) {
    console.log(`skipped (missing/invalid timestamps): ${before.skipped + after.skipped} job record(s)`);
  }
  console.log(renderMd(rows));
  if (jobs.length > 0 || families.length > 0) {
    const decisive = rows.filter((r) => r.verdict in { fell: 1, flat: 1, grew: 1 });
    const grew = decisive.filter((r) => r.verdict === 'grew');
    const holds = grew.length === 0 && decisive.every((r) => r.delta <= -5);
    console.log(`criterion (b) (all fell >= 5% AND none grew): ${holds ? 'HOLDS' : 'DOES NOT HOLD'} — grew: ${grew.map((r) => r.job).join(', ') || 'none'}`);
  }
  const csvPath = arg(argv, '--csv');
  if (csvPath) writeFileSync(csvPath, renderCsv(rows));
  if (jobs.length > 0 || families.length > 0) {
    const insufficient = rows.filter((r) => r.insufficient);
    if (insufficient.length > 0) {
      console.error(`insufficient samples (< ${minRuns} on a side): ${insufficient.map((r) => r.job).join(', ')}`);
      process.exit(1);
    }
  }
}

if (process.argv[1]?.replace(/\\/g, '/').endsWith('scripts/ci-job-durations.mjs')) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`ci-job-durations: ${e.message}`);
    process.exit(1);
  }
}
