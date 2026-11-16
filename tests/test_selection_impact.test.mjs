// Offline, hermetic node --test suite for scripts/test-selection-impact.mjs.
// Test fixture file IO is redirected to fresh mkdtemp dirs; the final production
// --check reads the committed inputs/report and writes nothing. No git history,
// network, cargo, or production writes.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { cargoTestStepSeconds } from '../scripts/ci-job-durations.mjs';
import {
  CLASSES, classifyReason, computeAggregates, computePerRun, loadInputs, renderReport,
} from '../scripts/test-selection-impact.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = join(repoRoot, 'scripts', 'test-selection-impact.mjs');

// ── fixtures ────────────────────────────────────────────────────────────────
// Deterministic constants only: no clock, no randomness, no machine paths.
const SHA = (c) => c.repeat(40); // 40-hex-looking constants (chars are hex digits)
const BASE = Date.parse('2026-10-01T00:00:00Z') / 1000;
const iso = (sec) => new Date(sec * 1000).toISOString();
const TEST_JOB = 'test (core tiers)';
const STEP_PATTERN = /^test \(/;

// Eight runs covering every classification path; the file order is deliberately
// NOT the created_at order (the script sorts). `steps` = the three cargo-test
// step durations in seconds; subset runs are SHORTER than all-mode runs so the
// sums stay distinguishable. r1's head_sha equals its artifact's mapShas[0]
// (the one map-current run); every other head_sha differs from its mapShas.
const RUN_SPECS = [
  { id: 'r5', event: 'pull_request', createdAt: '2026-10-07T22:05:00Z', cls: 'unmapped-changed-file', reason: 'tests/foo.test.mjs is executed by no mapped test', file: 'tests/foo.test.mjs', mapChar: '5', headChar: 'd', steps: [640, 520, 404] },
  { id: 'r2', event: 'push', createdAt: '2026-10-01T08:00:00Z', cls: 'selector-no-data', reason: 'no test-impact map available', mapChar: '2', headChar: 'a', steps: [610, 505, 401] },
  { id: 'r8', event: 'push', createdAt: '2026-10-08T03:10:00Z', cls: 'subset', total: 6, tests: ['sefer_alloc_core', 'sefer_alloc_heap'], mapChar: '8', headChar: '9', steps: [66, 55, 44] },
  { id: 'r1', event: 'pull_request', createdAt: '2026-10-03T12:00:00Z', cls: 'full-run', reason: 'full run requested', mapChar: '1', headChar: '1', steps: [600, 500, 400] },
  { id: 'r7', event: 'pull_request', createdAt: '2026-10-06T14:20:00Z', cls: 'subset', total: 10, tests: ['sefer_alloc_core', 'sefer_alloc_tcache', 'sefer_alloc_batch'], mapChar: '7', headChar: 'f', steps: [60, 50, 40] },
  { id: 'r3', event: 'pull_request', createdAt: '2026-10-05T09:30:00Z', cls: 'selector-no-data', reason: 'map download failed', mapChar: '3', headChar: 'b', steps: [620, 510, 402] },
  { id: 'r6', event: 'push', createdAt: '2026-10-04T06:45:00Z', cls: 'unattributable', reason: 'benches/x.rs is not attributable to a test', file: 'benches/x.rs', mapChar: '6', headChar: 'e', steps: [650, 525, 405] },
  { id: 'r4', event: 'push', createdAt: '2026-10-02T10:15:00Z', cls: 'manifest-or-config', reason: 'Cargo.toml changes every build', file: 'Cargo.toml', mapChar: '4', headChar: 'c', steps: [630, 515, 403] },
];
// Suffix-class reasons carry the changed file as their prefix; the manifest
// reason names a root-level ALWAYS_ALL file of scripts/select-tests.mjs (no slash).

const stepSeconds = (s) => s.steps[0] + s.steps[1] + s.steps[2];
const savedTargets = (s) => (s.cls === 'subset' ? s.total - s.tests.length : 0);
const savedSeconds = (s) => (s.cls === 'subset' ? (stepSeconds(s) * savedTargets(s)) / s.total : 0);
const sortedSpecs = [...RUN_SPECS].sort((a, b) => (a.createdAt < b.createdAt ? -1 : 1));
const DEN = sortedSpecs.reduce((acc, s) => acc + stepSeconds(s), 0); // independent Σ testSeconds
const NUM = sortedSpecs.reduce((acc, s) => acc + savedSeconds(s), 0); // independent Σ subset savedSeconds
const MAP_CURRENT = RUN_SPECS.filter((s) => s.headChar === s.mapChar).length;
const MAP_SHA_COUNT = new Set(RUN_SPECS.map((s) => s.mapChar)).size;

function buildDocs() {
  const selections = {};
  const runs = [];
  const jobsById = {};
  const attempts = [];
  for (const [i, s] of RUN_SPECS.entries()) {
    selections[s.id] = s.cls === 'subset'
      ? {
          mode: 'subset', total: s.total, tests: s.tests,
          why: Object.fromEntries(s.tests.map((t) => [t, 'changed file is mapped by ' + t])),
          mapShas: [SHA(s.mapChar)],
        }
      : { mode: 'all', reason: s.reason, total: 551 - i, mapShas: [SHA(s.mapChar)] };
    runs.push({ id: s.id, event: s.event, conclusion: 'success', created_at: s.createdAt, head_sha: SHA(s.headChar), head_branch: 'main' });
    const t0 = BASE + 36000 * (i + 1); // per-run deterministic step clock
    const [d1, d2, d3] = s.steps;
    const step = (n, name, a, b) => ({ number: n, name, started_at: iso(a), completed_at: iso(b) });
    jobsById[s.id] = {
      run_id: s.id,
      jobs: [{
        name: TEST_JOB, conclusion: 'success', status: 'completed',
        started_at: iso(t0), completed_at: iso(t0 + d1 + d2 + d3),
        steps: [
          { number: 1, name: 'Run actions/checkout@v5' }, // not matched; null-ish timestamps are fine
          step(2, 'test ()', t0, t0 + d1),
          step(3, 'test (--features experimental)', t0 + d1, t0 + d1 + d2),
          step(4, 'test (--features alloc-core)', t0 + d1 + d2, t0 + d1 + d2 + d3),
          { number: 5, name: 'Post Run actions/checkout@v5' }, // not matched
        ],
      }],
    };
    attempts.push({ id: s.id, downloaded: true, conclusion: 'success' });
  }
  attempts.push({ id: 'x1', downloaded: false, conclusion: 'success' }); // no artifact: absent from runs[]
  attempts.push({ id: 'x2', downloaded: false, conclusion: 'success' });
  const gh_calls = {
    run_list: 1,
    artifact_download_attempts: attempts.length,
    artifact_downloads_succeeded: RUN_SPECS.length,
    jobs_api: RUN_SPECS.length,
    total: 1 + attempts.length + RUN_SPECS.length,
  };
  return { selections, jobsById, runsDoc: { provenance: { gh_calls, download_attempt_order: attempts }, runs } };
}

function writeInputs(tmp, docs) {
  const dir = join(tmp, 'inputs');
  mkdirSync(join(dir, 'jobs'), { recursive: true });
  writeFileSync(join(dir, 'selections.json'), JSON.stringify(docs.selections, null, 1));
  writeFileSync(join(dir, 'runs.json'), JSON.stringify(docs.runsDoc, null, 1));
  for (const [id, doc] of Object.entries(docs.jobsById)) {
    writeFileSync(join(dir, 'jobs', id + '.json'), JSON.stringify(doc, null, 1));
  }
  return dir;
}

function fixture(mutate) {
  const tmp = mkdtempSync(join(tmpdir(), 'sel-impact-test-'));
  const docs = buildDocs();
  if (mutate) mutate(docs);
  writeInputs(tmp, docs);
  return { tmp, docs };
}

function loaded(tmp) {
  const inputs = loadInputs(join(tmp, 'inputs'));
  const rows = computePerRun(inputs);
  const agg = computeAggregates(inputs, rows);
  return { inputs, rows, agg };
}

// ── 1. classification ───────────────────────────────────────────────────────
test('classifyReason: fixture reasons land in their classes; suffix classes extract the file prefix; subset savedTargets = total - tests.length; unknown reasons are accepted as unknown and malformed shapes fail', () => {
  const { tmp, docs } = fixture();
  for (const s of RUN_SPECS) {
    const c = classifyReason(docs.selections[s.id]);
    assert.equal(c.cls, s.cls, 'class for ' + s.id);
    assert.equal(c.mode, s.cls === 'subset' ? 'subset' : 'all', 'mode for ' + s.id);
    if (s.file) assert.equal(c.file, s.file, 'reason prefix extracted as file for ' + s.id);
    assert.equal(c.savedTargets, savedTargets(s), 'savedTargets for ' + s.id);
  }
  const mystery = classifyReason({ mode: 'all', total: 5, reason: 'mysterious' });
  assert.equal(mystery.cls, 'unknown');
  assert.equal(mystery.reason, 'mysterious', 'unknown reason is preserved');
  assert.equal(classifyReason({ mode: 'all', total: 0 }).cls, 'unknown', 'missing reason is unknown');
  assert.equal(classifyReason({ mode: 'all', total: 5, reason: 'Cargo.lock changes every build' }).cls, 'manifest-or-config',
    'root Cargo.lock is accepted by the manifest/config suffix rule');
  const duplicate = { mode: 'subset', total: 2, tests: ['same', 'same'], why: { same: 'reason' } };
  assert.throws(() => classifyReason(duplicate), /tests are unique/, 'duplicate selected tests are invalid');
  assert.throws(() => classifyReason({ mode: 'all', total: -1, reason: 'full run requested' }), /nonnegative integer/);
  assert.throws(() => classifyReason({ mode: 'all', total: 1.5, reason: 'full run requested' }), /nonnegative integer/);
  const subsetTotal = classifyReason({ mode: 'subset', total: 2, tests: ['a', 'b'], why: { a: 'x', b: 'x' } });
  assert.equal(subsetTotal.savedTargets, 0, 'subset mode selecting all targets saves none');
  assert.equal(subsetTotal.cls, 'subset');
  const hole = structuredClone(docs.selections.r7);
  delete hole.why[hole.tests[1]];
  assert.throws(() => classifyReason(hole), /why entry for/, 'subset why must cover every selected test');
  assert.equal(classifyReason({ mode: 'all', total: 0, reason: 'selector failed' }).cls, 'selector-no-data',
    'total=0 may only classify selector-no-data');
  assert.throws(() => classifyReason({ mode: 'all', total: 0, reason: 'full run requested' }),
    /total=0/, 'total=0 with full-run reason throws');
  rmSync(tmp, { recursive: true, force: true });
});

// ── 2. ci-job-durations reuse ───────────────────────────────────────────────
test('ci-job-durations reuse: cargoTestStepSeconds equals the independent sum of exactly the three matched steps; the checkout steps are ignored', () => {
  const { tmp, docs } = fixture();
  for (const s of RUN_SPECS) {
    const job = docs.jobsById[s.id].jobs[0];
    const got = cargoTestStepSeconds(job, STEP_PATTERN);
    const independent = s.steps.reduce((a, d) => a + d, 0);
    assert.equal(got.seconds, independent, 'step sum for ' + s.id);
    assert.equal(got.raws.length, 3, 'exactly three matched steps contributed for ' + s.id);
    assert.equal(job.steps.filter((st) => STEP_PATTERN.test(st.name)).length, 3,
      'fixture has 3 matched steps of 5 for ' + s.id);
  }
  rmSync(tmp, { recursive: true, force: true });
});

// ── 3. per-run rows ─────────────────────────────────────────────────────────
test('computePerRun: one row per run sorted by created_at ascending; testSeconds and subset savedSeconds match the independent recomputation; all-mode savedSeconds is 0', () => {
  const { tmp } = fixture();
  const rows = computePerRun(loadInputs(join(tmp, 'inputs')));
  assert.equal(rows.length, RUN_SPECS.length, 'one row per run');
  assert.deepEqual(rows.map((r) => r.id), sortedSpecs.map((s) => s.id), 'rows sorted by created_at ascending');
  for (const [i, r] of rows.entries()) {
    const s = sortedSpecs[i];
    assert.equal(r.id, s.id);
    assert.equal(r.testSeconds, stepSeconds(s), 'testSeconds for ' + r.id);
    if (s.cls === 'subset') {
      assert.equal(r.savedSeconds, (stepSeconds(s) * savedTargets(s)) / s.total, 'savedSeconds for ' + r.id);
    } else {
      assert.equal(r.savedSeconds, 0, 'all-mode savedSeconds zero for ' + r.id);
    }
    assert.ok(r.savedSeconds >= 0 && r.savedSeconds <= r.testSeconds, '0 <= savedSeconds <= testSeconds for ' + r.id);
  }
  rmSync(tmp, { recursive: true, force: true });
});

// ── 4. aggregates ───────────────────────────────────────────────────────────
test('computeAggregates: subset+all == run count; per-class counts sum == run count; numerator/denominator equal independent sums; shares in [0,1]; map sha distinctness and currency', () => {
  const { tmp } = fixture();
  const { inputs, rows, agg } = loaded(tmp);
  const subset = RUN_SPECS.filter((s) => s.cls === 'subset');
  const allMode = RUN_SPECS.filter((s) => s.cls !== 'subset');
  assert.equal(agg.subsetCount, subset.length, 'subset count');
  assert.equal(agg.subsetCount + allMode.length, rows.length, 'subset + all == run count');
  let clsSum = 0;
  for (const c of CLASSES) clsSum += agg.perClass.get(c).count;
  assert.equal(clsSum, rows.length, 'per-class counts sum == run count');
  assert.equal(agg.perClass.get('subset').count, subset.length, 'subset per-class count == number of subset fixture runs');
  assert.ok(renderReport(inputs, rows, agg).includes('`subset` |'), 'rendered class table contains the subset row');
  assert.equal(agg.denominator, DEN, 'denominator == independent Σ testSeconds');
  assert.equal(agg.numerator, NUM, 'numerator == independent Σ subset savedSeconds');
  assert.ok(agg.numerator <= agg.denominator, 'numerator <= denominator');
  assert.ok(agg.savedShare >= 0 && agg.savedShare <= 1, 'savedShare in [0,1]');
  assert.ok(agg.reducedShare >= 0 && agg.reducedShare <= 1, 'reducedShare in [0,1]');
  assert.equal(agg.savedShare, NUM / DEN, 'savedShare == independent fraction');
  assert.equal(agg.reducedShare, subset.length / rows.length, 'reducedShare == independent fraction');
  assert.equal(agg.mapShas.length, MAP_SHA_COUNT, 'distinct map shas across the artifacts');
  assert.equal(agg.mapCurrent, MAP_CURRENT, 'map-current == runs whose head_sha is in their mapShas');
  rmSync(tmp, { recursive: true, force: true });
});

test('computeAggregates: subset selecting every target remains subset mode but contributes no reduced run or saved seconds', () => {
  const { tmp } = fixture((docs) => {
    const allSelected = docs.selections.r8;
    allSelected.total = allSelected.tests.length;
    docs.selections.r7 = { mode: 'all', reason: 'full run requested', total: 10, mapShas: [SHA('7')] };
  });
  const { rows, agg } = loaded(tmp);
  const row = rows.find((r) => r.id === 'r8');
  assert.equal(row.mode, 'subset');
  assert.equal(row.selCount, row.total, 'subset selected all total targets');
  assert.equal(row.savedTargets, 0);
  assert.equal(row.savedSeconds, 0);
  assert.equal(agg.subsetCount, 1, 'all-selected subset still counts as subset mode');
  assert.equal(agg.reducedCount, 0, 'all-selected subset does not count as reduced');
  assert.equal(agg.numerator, 0, 'all-selected subset contributes no saved seconds');
  rmSync(tmp, { recursive: true, force: true });
});

// ── 5. report rendering ─────────────────────────────────────────────────────
test('renderReport: subset fixture avoids false all/no-change/zero-numerator claims, is deterministic, and prints target fractions; all-only window reports no change correctly', () => {
  const { tmp, docs } = fixture();
  const { inputs, rows, agg } = loaded(tmp);
  const report = renderReport(inputs, rows, agg);
  assert.equal(renderReport(inputs, rows, agg), report, 'rendering the same fixture twice is byte-identical');
  assert.equal(report.split('This report makes NO GO/NO-GO decision.').length - 1, 1,
    'exact closing NO GO/NO-GO sentence appears once');
  assert.ok(report.includes(NUM.toFixed(1) + ' / ' + DEN.toFixed(1) + ' s = '),
    'independent numerator and denominator appear in the savings equation');
  assert.ok(!report.includes(tmp), 'report contains no temp-directory path');
  assert.ok(report.includes('Subset-mode runs: 2 of 8'), 'subset fixture count is data-dependent');
  assert.ok(report.includes('Strictly reduced runs (subset mode and savedTargets > 0): 2 of 8'), 'strict reductions are counted');
  assert.ok(!report.includes('## Why every run selected all'), 'subset fixture is not rendered as an all-selected window');
  assert.ok(!report.includes('all 8 runs selected `all`'), 'subset fixture is not described as selecting all targets on every run');
  assert.ok(!report.includes('In this window enforce would have changed nothing'), 'subset fixture is not described as unchanged');
  assert.ok(!report.includes('No run in this window has a strict target reduction'), 'subset fixture is not described as having no reductions');
  assert.ok(!report.includes('Estimated saved test-core cargo-test seconds: 0.0 / ' + DEN.toFixed(1) + ' s'),
    'nonzero subset savings are not rendered as a zero numerator');
  for (const spec of RUN_SPECS.filter((s) => s.cls === 'subset')) {
    const fraction = 'selected ' + spec.tests.length + '/' + spec.total + '; skipped ' + savedTargets(spec) + '/' + spec.total;
    assert.ok(report.includes(fraction), 'per-run selected and skipped fractions present for ' + spec.id);
    assert.ok(report.includes('| ' + spec.id + ' |'), 'per-run row present for ' + spec.id);
  }
  assert.ok(report.includes('| r1 |') && report.includes('`full run requested`'), 'all-mode row carries recorded reason');
  assert.ok(report.includes('estimated saved s |'), 'table includes estimated seconds');

  const allTmp = mkdtempSync(join(tmpdir(), 'sel-impact-all-'));
  const allDocs = buildDocs();
  for (const s of RUN_SPECS.filter((x) => x.cls === 'subset')) {
    allDocs.selections[s.id] = { mode: 'all', reason: 'full run requested', total: s.total, mapShas: [SHA(s.mapChar)] };
  }
  writeInputs(allTmp, allDocs);
  const all = loaded(allTmp);
  const allReport = renderReport(all.inputs, all.rows, all.agg);
  assert.ok(allReport.includes('In this window enforce would have changed nothing'));
  assert.ok(allReport.includes('Subset-mode runs: 0 of 8'));
  assert.ok(allReport.includes('estimated saved s |'));
  rmSync(allTmp, { recursive: true, force: true });
  rmSync(tmp, { recursive: true, force: true });
});

test('renderReport: zero-duration test steps print non-estimable denominator without NaN and preserve exact zero estimates', () => {
  const { tmp, docs } = fixture((d) => {
    for (const jobDoc of Object.values(d.jobsById)) {
      for (const step of jobDoc.jobs[0].steps) {
        if (STEP_PATTERN.test(step.name)) step.completed_at = step.started_at;
      }
    }
  });
  const { inputs, rows, agg } = loaded(tmp);
  const report = renderReport(inputs, rows, agg);
  assert.equal(agg.denominator, 0);
  assert.equal(agg.numerator, 0);
  assert.ok(report.includes('not estimable (0.0 / 0.0 s)'));
  assert.ok(!report.includes('NaN'));
  assert.ok(rows.every((r) => r.savedSeconds === 0));
  rmSync(tmp, { recursive: true, force: true });
});

// ── 6. CLI ──────────────────────────────────────────────────────────────────
test('CLI --report writes the file, second generation byte-identical, --check exits 0, one tampered character makes --check exit nonzero with --check FAILED', () => {
  const { tmp } = fixture();
  const reportPath = join(tmp, 'out', 'report.md');
  const args = ['--repo-root', tmp, '--inputs', join(tmp, 'inputs'), '--report-path', reportPath];
  execFileSync(process.execPath, [SCRIPT, ...args, '--report'], { encoding: 'utf8' });
  const first = readFileSync(reportPath, 'utf8', 'first --report wrote the file');
  assert.ok(first.includes('This report makes NO GO/NO-GO decision.'), 'report content written');
  execFileSync(process.execPath, [SCRIPT, ...args, '--report'], { encoding: 'utf8' });
  assert.equal(readFileSync(reportPath, 'utf8'), first, 'two generations byte-identical');
  const checkOut = execFileSync(process.execPath, [SCRIPT, ...args, '--check'], { encoding: 'utf8' });
  assert.ok(checkOut.includes('--check ok'), '--check exits 0');
  writeFileSync(reportPath, first.replace('This report makes NO GO/NO-GO decision.', 'This report makes NO GO/NO-GO decision?'));
  assert.throws(() => execFileSync(process.execPath, [SCRIPT, ...args, '--check'], { encoding: 'utf8' }),
    /--check FAILED/, 'tampered report must fail --check');
  rmSync(tmp, { recursive: true, force: true });
});

// ── 7. import side effects ──────────────────────────────────────────────────
test('importing the CLI module has no side effects: fresh empty cwd stays empty, exit 0', () => {
  const tmp = mkdtempSync(join(tmpdir(), 'sel-impact-import-'));
  execFileSync(
    process.execPath,
    ['-e', 'import(require("node:url").pathToFileURL(process.argv[1]).href)', SCRIPT],
    { cwd: tmp, encoding: 'utf8' },
  );
  assert.deepEqual(readdirSync(tmp), [], 'importing must write nothing');
  rmSync(tmp, { recursive: true, force: true });
});

// ── 8. loadInputs failure modes ─────────────────────────────────────────────
test('loadInputs hard-fails on each corrupted fixture variant with the expected message content', () => {
  const cases = [
    ['selections.json key mismatch vs runs.json', (d) => { delete d.selections.r5; },
      /selections\.json keys match runs\.json runs\[\] exactly/],
    ['missing jobs/<id>.json (file count kept equal by an unrelated extra file)',
      (d) => { delete d.jobsById.r3; d.jobsById.extra = { run_id: 'extra', jobs: [] }; },
      /ENOENT.*r3\.json/],
    ['cargo-test step with null timestamps',
      (d) => { const st = d.jobsById.r2.jobs[0].steps[1]; st.started_at = null; st.completed_at = null; },
      /non-null/],
    ['job with only 2 matching cargo test steps',
      (d) => { const j = d.jobsById.r4.jobs[0]; j.steps = j.steps.filter((st) => st.name !== 'test (--features alloc-core)'); },
      /exactly 3 cargo test steps/],
    ['consistent gh-call budget breach remains loadable and derives total/excess', (d) => {
      const prov = d.runsDoc.provenance;
      for (let i = 0; i < 36; i++) prov.download_attempt_order.push({ id: 'x' + (100 + i), downloaded: false, conclusion: 'success' });
      prov.gh_calls.artifact_download_attempts = prov.download_attempt_order.length;
      prov.gh_calls.total = prov.gh_calls.run_list + prov.gh_calls.artifact_download_attempts + prov.gh_calls.jobs_api;
    }, null],
    ['gh_calls.total inconsistent with the component sum',
      (d) => { d.runsDoc.provenance.gh_calls.total += 1; },
      /gh_calls\.total == run_list \+ artifact_download_attempts \+ jobs_api/],
  ];
  for (const [what, mutate, pattern] of cases) {
    const { tmp } = fixture(mutate);
    if (pattern === null) {
      const loadedInputs = loadInputs(join(tmp, 'inputs'));
      assert.equal(loadedInputs.budgetExceeded, 10);
      const report = renderReport(loadedInputs, computePerRun(loadedInputs), computeAggregates(loadedInputs, computePerRun(loadedInputs)));
      assert.ok(report.includes('55 total against a 45-total-call budget: EXCEEDED by 10 calls'), what);
    } else {
      assert.throws(() => loadInputs(join(tmp, 'inputs')), pattern, what);
    }
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('CLI --check validates committed production report read-only', () => {
  const output = execFileSync(process.execPath, [SCRIPT, '--check'], { cwd: repoRoot, encoding: 'utf8' });
  assert.match(output, /--check ok/);
});
