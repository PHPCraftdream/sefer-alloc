// Test suite for scripts/ci-job-durations.mjs (P0.2 test-merge pilot measurement script).
//
// Strategy: ground truth by construction. Synthetic `gh run view --json jobs` dumps are
// written into mkdtempSync(tmpdir)/ci-job-durations-* trees (removed in finally) and the
// script is exercised both at the function level and via the real CLI (spawnSync, script
// run directly). No CI data exists in-repo; everything here is synthetic. RED
// counterfactual included: corrupting one input duration must change the computed median.
//
// Node >= 24, ESM, zero npm dependencies. No cargo, no network, no writes outside os.tmpdir().

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import {
  assertRows, cargoTestStepSeconds, collectRuns, computeRows, deltaPct,
  durationSeconds, median, renderCsv, renderMd, verdictOf,
} from '../scripts/ci-job-durations.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = join(repoRoot, 'scripts', 'ci-job-durations.mjs');

/** Temp workspace with a fresh run dump dir per side. Returns {dir, paths}. */
function makeWorkspace() {
  const dir = mkdtempSync(join(tmpdir(), 'ci-job-durations-'));
  const before = mkdirSync(join(dir, 'before'), { recursive: true });
  const after = mkdirSync(join(dir, 'after'), { recursive: true });
  return { dir, before, after };
}

/** One synthetic run dump file: `steps` = [[name, durSeconds], ...]. */
function runFile(side, fileName, jobName, steps, opts = {}) {
  const base = 1730000000000;
  const t = (sec) => new Date(base + (opts.offset ?? 0) * 60000 + sec * 1000).toISOString();
  const job = {
    name: jobName,
    started_at: t(0),
    completed_at: t(opts.jobDur ?? 100),
    conclusion: 'success',
    steps: steps.map(([name, dur], i) => ({
      name, number: i + 1, started_at: t(0), completed_at: t(dur), conclusion: 'success',
    })),
  };
  if (opts.noJobTimestamps) { delete job.started_at; delete job.completed_at; }
  writeFileSync(join(side, fileName), JSON.stringify({ jobs: [job] }));
}

// ------------------------------------------------------------- unit-level checks

test('median: odd and even counts', () => {
  assert.equal(median([30, 10, 20]), 20);
  assert.equal(median([20, 10]), 15);
  assert.equal(median([7]), 7);
});

test('durationSeconds: valid, missing and invalid timestamps', () => {
  assert.equal(durationSeconds('2026-01-01T00:00:00Z', '2026-01-01T00:01:30Z'), 90);
  assert.equal(durationSeconds(undefined, '2026-01-01T00:01:30Z'), null);
  assert.equal(durationSeconds('not-a-date', '2026-01-01T00:01:30Z'), null);
});

test('cargoTestStepSeconds: sums multiple matching steps, null when none match', () => {
  const job = {
    steps: [
      { name: 'Setup', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:00:10Z' },
      { name: 'Run cargo test (lib)', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:01:00Z' },
      { name: 'Run cargo test (doc)', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:02:00Z' },
    ],
  };
  const r = cargoTestStepSeconds(job, /cargo test/);
  assert.equal(r.seconds, 180);
  assert.deepEqual(r.raws, [60, 120]);
  assert.equal(cargoTestStepSeconds({ steps: [{ name: 'Build', started_at: '2026-01-01T00:00:00Z', completed_at: '2026-01-01T00:01:00Z' }] }, /cargo test/), null);
});

test('verdict thresholds at exactly -5, between -5 and 0, and > 0', () => {
  assert.equal(verdictOf(-5), 'fell');
  assert.equal(verdictOf(-4.99), 'flat');
  assert.equal(verdictOf(0), 'flat'); // grew requires delta > 0
  assert.equal(verdictOf(12.5), 'grew');
  assert.equal(deltaPct(100, 95), -5);
  assert.equal(deltaPct(100, 99), -1);
  assert.equal(deltaPct(100, 101), 1);
});

// ------------------------------------------------------------- fixture-driven checks

test('step-pattern fallback: job without matching steps uses whole job duration (metric=job)', () => {
  const ws = makeWorkspace();
  try {
    runFile(ws.before, 'r1.json', 'Build', [['Build', 100]]);
    runFile(ws.after, 'r1.json', 'Build', [['Build', 110]], { jobDur: 110 });
    const pattern = /cargo test/;
    const b = collectRuns(ws.before, pattern);
    const a = collectRuns(ws.after, pattern);
    assert.deepEqual(b.perJob.get('Build').map((r) => [r.seconds, r.metric]), [[100, 'job']]);
    const rows = computeRows({ before: b, after: a, jobs: ['Build'], minRuns: 1 });
    assert.equal(rows[0].metric, 'job');
    assert.equal(rows[0].delta, 10);
    assert.equal(rows[0].verdict, 'grew');
    assertRows(rows, { before: b, after: a });
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});

test('min-runs=3: two runs per side => verdict insufficient-runs', () => {
  const ws = makeWorkspace();
  try {
    for (const [side, secs] of [[ws.before, 100], [ws.after, 50]]) {
      for (const f of ['r1.json', 'r2.json']) runFile(side, f, 'J', [['cargo test', secs]]);
    }
    const pattern = /cargo test/;
    const b = collectRuns(ws.before, pattern);
    const a = collectRuns(ws.after, pattern);
    const rows = computeRows({ before: b, after: a, jobs: ['J'], minRuns: 3 });
    assert.equal(rows[0].verdict, 'insufficient-runs');
    assert.equal(rows[0].delta, null);
    // same data, min-runs=2 => real verdict
    const rows2 = computeRows({ before: b, after: a, jobs: ['J'], minRuns: 2 });
    assert.equal(rows2[0].verdict, 'fell');
    assert.equal(rows2[0].delta, -50);
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});

test('CLI: corrupted JSON file => nonzero exit naming the file', () => {
  const ws = makeWorkspace();
  try {
    runFile(ws.before, 'good.json', 'J', [['cargo test', 10]]);
    writeFileSync(join(ws.before, 'broken.json'), '{ not json');
    runFile(ws.after, 'good.json', 'J', [['cargo test', 12]]);
    const r = spawnSync(process.execPath, [SCRIPT, '--before', ws.before, '--after', ws.after], { encoding: 'utf8' });
    assert.notEqual(r.status, 0);
    assert.match(r.stderr, /broken\.json/);
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});

test('CLI: CSV row count equals table row count and values match', () => {
  const ws = makeWorkspace();
  try {
    // 3 before runs of 10/20/30 s and 3 after runs of 40/40/40 s (median 40) for job J;
    // one extra job present only on the before side must NOT appear.
    const t = (sec) => new Date(1730000000000 + sec * 1000).toISOString();
    const mk = (side, name, durs) => durs.forEach((d, i) => writeFileSync(join(side, `r${i}.json`), JSON.stringify({
      jobs: [{
        name, started_at: t(0), completed_at: t(d), conclusion: 'success',
        steps: [{ name: 'cargo test', number: 1, started_at: t(0), completed_at: t(d), conclusion: 'success' }],
      }],
    })));
    mk(ws.before, 'J', [10, 20, 30]);
    mk(ws.after, 'J', [40, 40, 40]);
    runFile(ws.before, 'only_before.json', 'Orphan', [['cargo test', 5]]);
    const csvPath = join(ws.dir, 'out.csv');
    const r = spawnSync(process.execPath, [SCRIPT, '--before', ws.before, '--after', ws.after, '--jobs', 'J', '--csv', csvPath], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    const mdRows = r.stdout.split('\n').filter((l) => l.startsWith('| J '));
    assert.equal(mdRows.length, 1);
    assert.match(mdRows[0], /\| steps \| 3 \| 20\.00 \| 3 \| 40\.00 \| 100\.00 \| grew \|/);
    const csv = readFileSync(csvPath, 'utf8').trim().split('\n');
    assert.equal(csv[0], 'job,metric,n_before,median_before_s,n_after,median_after_s,delta_pct,verdict');
    assert.equal(csv.length, 2, 'CSV rows (incl. header) == table rows + header');
    const [job, metric, nb, mb, na, ma, d, v] = csv[1].split(',');
    assert.equal(job.replace(/^"|"$/g, ''), 'J'); // CSV-quoted
    assert.equal(metric, 'steps');
    assert.equal(Number(mb), 20);
    assert.equal(Number(ma), 40);
    assert.equal(Number(d), 100);
    assert.equal(v, 'grew');
    assert.equal(Number(nb), 3);
    assert.equal(Number(na), 3);
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});

test('RED counterfactual: corrupting one input duration changes median/delta', () => {
  const ws = makeWorkspace();
  try {
    const t = (sec) => new Date(1730000000000 + sec * 1000).toISOString();
    const write = (side, name, durs) => durs.forEach((d, i) => writeFileSync(join(side, `r${i}.json`), JSON.stringify({
      jobs: [{
        name, started_at: t(0), completed_at: t(d), conclusion: 'success',
        steps: [{ name: 'cargo test', number: 1, started_at: t(0), completed_at: t(d), conclusion: 'success' }],
      }],
    })));
    write(ws.before, 'J', [10, 20, 30]);
    write(ws.after, 'J', [40, 40, 50]);
    const pattern = /cargo test/;
    const b0 = collectRuns(ws.before, pattern);
    const a0 = collectRuns(ws.after, pattern);
    const rows0 = computeRows({ before: b0, after: a0, jobs: ['J'], minRuns: 1 });
    assert.equal(rows0[0].b.med, 20);
    assert.equal(rows0[0].delta, 100);
    // corrupt one input: after median 40 -> 50, delta 100 -> 150
    const mut = JSON.parse(readFileSync(join(ws.after, 'r0.json'), 'utf8'));
    mut.jobs[0].completed_at = t(80);
    mut.jobs[0].steps[0].completed_at = t(80);
    writeFileSync(join(ws.after, 'r0.json'), JSON.stringify(mut));
    const b1 = collectRuns(ws.before, pattern);
    const a1 = collectRuns(ws.after, pattern);
    const rows1 = computeRows({ before: b1, after: a1, jobs: ['J'], minRuns: 1 });
    assertRows(rows1, { before: b1, after: a1 });
    assert.notEqual(rows1[0].a.med, rows0[0].a.med);
    assert.equal(rows1[0].a.med, 50);
    assert.notEqual(rows1[0].delta, rows0[0].delta);
    assert.equal(rows1[0].delta, 150);
  } finally {
    rmSync(ws.dir, { recursive: true, force: true });
  }
});
