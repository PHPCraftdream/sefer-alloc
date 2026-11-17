import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test } from 'node:test';
import { generateReport } from '../scripts/ci-long-jobs.mjs';
import { parseJobLog } from '../scripts/ci-long-jobs-log.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = join(ROOT, 'scripts', 'ci-long-jobs.mjs');
const DEFAULT_REPORT = join(ROOT, 'docs', 'perf', 'CI_LONG_JOBS.md');
const BASE = Date.parse('2026-10-01T00:00:00Z');
const time = seconds => new Date(BASE + seconds * 1000).toISOString();
const logTime = seconds => time(seconds).slice(0, -1);

function workspace() {
  const dir = mkdtempSync(join(tmpdir(), 'ci-long-jobs-test-'));
  const inputs = join(dir, 'inputs');
  mkdirSync(inputs);
  return { dir, inputs };
}

function run(id, index, { event = 'schedule', camelJobs = index % 2 === 1, nested = false } = {}) {
  const start = time(index * 86400);
  const durations = [1200, 900, 800, 700, 600, 500, 400, 300, 200, null].map(value => value === null ? null : value + index * 10);
  const jobs = durations.map((duration, j) => {
    const job = { id: `${id}-${j}`, name: `Job ${j + 1}`, os: 'ubuntu-latest', conclusion: 'success', steps: [] };
    if (duration !== null) Object.assign(job, camelJobs
      ? { startedAt: start, completedAt: time(index * 86400 + duration) }
      : { started_at: start, completed_at: time(index * 86400 + duration) });
    if (j === 0) {
      let cursor = 0;
      const step = (name, seconds, uses = '') => {
        const fields = camelJobs
          ? { startedAt: time(index * 86400 + cursor), completedAt: time(index * 86400 + cursor + seconds) }
          : { started_at: time(index * 86400 + cursor), completed_at: time(index * 86400 + cursor + seconds) };
        job.steps.push({ name, conclusion: 'success', ...fields, ...(uses ? { uses } : {}) });
        cursor += seconds;
      };
      step('Install dependencies', 60);
      step('Rust toolchain', 30);
      step('Restore cache', 30, 'actions/cache/restore@v4');
      step('cargo check #1', 25);
      step('cargo check #2', 25);
      step('cargo check #3', 25);
      step('cargo check #4', 25);
      step('cargo test', 200);
      step('Run Miri tests', 20);
      step('Diagnostic', 30);
      step('Run ./.github/actions/ci-composite', 5);
      job.steps.push({ name: 'Unknown interval', conclusion: 'success' });
    }
    return job;
  });
  const runData = {
    id, event, branch: 'main', created_at: start, conclusion: 'success',
    ...(event === 'pull_request' ? {} : { sample: event === 'push' ? 'ordinary-main-push' : 'regular-full' }),
  };
  const record = { run: runData, jobs, total_count: jobs.length };
  return nested ? record : { ...runData, jobs, total_count: jobs.length };
}

function writeRecords(dir, records) {
  records.forEach((record, index) => writeFileSync(join(dir, `${index}.json`), JSON.stringify(record)));
}

function fixture(dir, { count = 6, push = true, includeFullPr = true } = {}) {
  const records = [];
  for (let i = 0; i < count; i++) {
    const isPr = includeFullPr && i === count - 1;
    records.push(run(isPr ? '43781371392' : `full-${i + 1}`, i, {
      event: isPr ? 'pull_request' : 'schedule', camelJobs: i % 2 === 1, nested: i === 2,
    }));
  }
  if (push) records.push(run('push-1', count, { event: 'push', camelJobs: true }));
  writeRecords(dir, records);
  if (includeFullPr && count) writeFileSync(join(dir, 'README.md'), '- `43781371392.json.gz` — full PR by standalone CI: full trailer.\n');
}

function cli(args) {
  return spawnSync(process.execPath, [SCRIPT, ...args], { cwd: ROOT, encoding: 'utf8' });
}

test('log parser partitions nested Run/command phases without treating shell echoes as cargo execution', () => {
  const start = time(0), end = time(50);
  const log = [
    `${start} ##[group]Run composite`,
    `${logTime(1)}Z ##[group]Run cargo miri test --test alpha --test beta --features "one two"`,
    `${logTime(1)}Z \u001b[36;1mcargo miri test --test alpha --test beta --features "one two"\u001b[0m`,
    `${logTime(2)}Z ##[endgroup]`,
    `${logTime(3)}Z Compiling crate v0.1.0`,
    `${logTime(7)}Z Finished [1m	test[0m profile [unoptimized] target(s) in 4.00s`,
    `${logTime(8)}Z Running tests/alpha.rs`,
    `${logTime(12)}Z running 1 test`,
    `${logTime(18)}Z test result: ok. 1 passed; 0 failed; finished in 6.0s`,
    `${logTime(20)}Z ##[group]Run cargo install kani-verifier`,
    `${logTime(21)}Z ##[endgroup]`,
    `${logTime(22)}Z Compiling installer crate v1.0.0`,
    `${logTime(26)}Z ##[group]Restore cache`,
    `${logTime(30)}Z Cache restored`,
    `${logTime(31)}Z ##[endgroup]`,
    `${logTime(32)}Z ##[group]Save cache`,
    `${logTime(36)}Z Cache saved`,
    `${logTime(37)}Z ##[endgroup]`,
    `${logTime(40)}Z ##[group]Run cargo test --test next`,
    `${logTime(41)}Z ##[endgroup]`,
    `${logTime(42)}Z Running tests/next.rs`,
    `${logTime(48)}Z test result: ok`,
  ].join('\n');
  const parsed = parseJobLog(log, { runId: 'r', jobId: 'j', jobName: 'miri (core)', startedAt: start, completedAt: end });
  assert.equal(parsed.durationSeconds, 50);
  assert.equal(Object.values(parsed.phases).reduce((a, b) => a + b, 0), 50);
  assert.equal(parsed.phases.build, 4);
  assert.equal(parsed.phases.execution, 17);
  assert.equal(parsed.phases.cacheRestore, 4);
  assert.equal(parsed.phases.cacheSave, 4);
  assert.equal(parsed.phases.setup, 11);
  assert.equal(parsed.phases.other, 8);
  assert.equal(parsed.phases.unknown, 2);
  assert.equal(parsed.commands.length, 3);
  assert.deepEqual(parsed.commands[0].targets, ['alpha', 'beta']);
  assert.equal(parsed.cargoSpans.length, 3);
  assert.equal(parsed.cargoSpans[1].kind, 'install');
  assert.equal(parsed.cargoSpans[1].compile.length, 0);
  assert.deepEqual(parsed.cargoSpans[0].compile[0], { start: 3, end: 7, sourceLines: [5, 6], reportedSeconds: 4, bounded: true });
  assert.equal(parsed.cargoSpans[0].execution[0].start, 7);
  assert.equal(parsed.cargoSpans[0].execution[0].end, 18);
  assert.equal(parsed.groups[0].scopeEnd, 1);
});

test('Kani harness checks remain execution after the actual Cargo Finished boundary', () => {
  const start = time(0), end = time(103);
  const parsed = parseJobLog([
    `${start} ##[group]Run cargo kani --harness kani_proofs`,
    `${logTime(0.5)}Z \u001b[36;1mcargo kani --harness kani_proofs\u001b[0m`,
    `${logTime(0.75)}Z ##[endgroup]`,
    `${logTime(1)}Z Compiling sefer-alloc v0.3.0 (/workspace/sefer-alloc)`,
    `${logTime(3)}Z Finished \u001b[1m\tdev\u001b[0m profile [unoptimized + debuginfo] target(s) in 3.55s`,
    `${logTime(3)}Z Checking harness kani_proofs::depot_overflow_proofs::reload_simple_is_reload_then_tick...`,
    `${logTime(103)}Z VERIFICATION: SUCCESSFUL`,
  ].join('\n'), { startedAt: start, completedAt: end });
  assert.equal(parsed.phases.build, 2);
  assert.equal(parsed.phases.execution, 100);
  assert.equal(parsed.cargoSpans.length, 1);
  assert.equal(parsed.cargoSpans[0].compile[0].start, 1);
  assert.equal(parsed.cargoSpans[0].compile[0].end, 3);
  assert.equal(parsed.cargoSpans[0].compile[0].reportedSeconds, 3.55);
  assert.equal(parsed.cargoSpans[0].execution[0].start, 3);
  assert.equal(parsed.cargoSpans[0].execution[0].end, 103);
  assert.equal(parsed.cargoSpans[0].commandAttribution, 'known');
});

test('target attribution is suppressed for multi-target and multi-command Run groups', () => {
  const start = time(0), end = time(12);
  const parsed = parseJobLog([
    `${start} ##[group]Run cargo miri test --test alpha --test beta`,
    `${logTime(0.5)}Z ##[endgroup]`,
    `${logTime(1)}Z Running tests/alpha.rs`,
    `${logTime(2)}Z test result: ok`,
    `${logTime(3)}Z ##[group]Run composite shell`,
    `${logTime(4)}Z \u001b[36;1mcargo test --test first\u001b[0m`,
    `${logTime(5)}Z \u001b[36;1mcargo test --test second\u001b[0m`,
    `${logTime(6)}Z ##[endgroup]`,
    `${logTime(7)}Z Running tests/second.rs`,
    `${logTime(9)}Z test result: ok`,
  ].join('\n'), { startedAt: start, completedAt: end });
  assert.equal(parsed.cargoSpans.length, 2);
  assert.equal(parsed.cargoSpans[0].targetAttribution, 'ambiguous');
  assert.equal(parsed.cargoSpans[0].targetAttributionReason, 'multiple-targets');
  assert.equal(parsed.cargoSpans[1].commandAttribution, 'ambiguous');
  assert.equal(parsed.cargoSpans[1].commandAttributionReason, 'multiple-distinct-commands-in-group');
  assert.equal(parsed.cargoSpans[1].targetAttribution, 'ambiguous');
});

test('log parser keeps invalid/out-of-order times conservative and rejects no-log endpoint invention', () => {
  const start = time(0), end = time(8);
  const parsed = parseJobLog([
    `${start} ##[group]Run cargo miri test --test sample`,
    `${logTime(3)}Z Compiling crate v1`,
    `${logTime(2)}Z Finished test profile in 1.0s`,
    'not a timestamp ##[group]Run cargo test',
    `${logTime(9)}Z outside job`,
  ].join('\n'), { startedAt: start, completedAt: end });
  assert.equal(parsed.diagnostics.outOfOrderTimestamps, 1);
  assert.equal(parsed.diagnostics.outsideJobTimestamps, 1);
  assert.equal(Object.values(parsed.phases).reduce((a, b) => a + b, 0), 8);
  const empty = parseJobLog('', { startedAt: start, completedAt: end });
  assert.equal(empty.phases.unknown, 8);
  assert.equal(Object.values(empty.phases).reduce((sum, value) => sum + value, 0), 8);
  const invalid = parseJobLog('no timestamps', { startedAt: 'not-a-time', completedAt: end });
  assert.equal(invalid.durationSeconds, null);
  assert.ok(Object.values(invalid.phases).every(value => value === null));
});

test('log parser bounds unassociated compile output and includes active phase through API EOF', () => {
  const start = time(0), end = time(10);
  const parsed = parseJobLog([
    `${start} ##[group]Run composite`,
    `${logTime(1)}Z ##[endgroup]`,
    `${logTime(2)}Z Compiling unassociated-crate v1.0.0`,
    `${logTime(4)}Z Finished test profile in 40m0s`,
    `${logTime(6)}Z Running tests/unassociated.rs`,
  ].join('\n'), { startedAt: start, completedAt: end });
  assert.equal(parsed.phases.build, 4);
  assert.equal(parsed.phases.execution, 4);
  assert.equal(parsed.phases.other, 2);
  assert.equal(Object.values(parsed.phases).reduce((sum, value) => sum + value, 0), 10);
  assert.equal(parsed.cargoSpans.length, 1);
  assert.equal(parsed.cargoSpans[0].command, 'unknown cargo command');
  assert.equal(parsed.cargoSpans[0].compile[0].reportedSeconds, 2400);
  assert.deepEqual(parsed.cargoSpans[0].execution[0].sourceLines, [5, 5]);
});

test('log parser preserves nested/missing groups and treats cargo echoes as metadata only', () => {
  const start = time(0), end = time(8);
  const parsed = parseJobLog([
    `${start} ##[group]Run outer composite`,
    `${logTime(1)}Z ##[group]Run cargo test --test echoed`,
    `${logTime(2)}Z \u001b[36;1mcargo test --test echoed\u001b[0m`,
    `${logTime(3)}Z ##[group]Run peer without previous endgroup`,
    `${logTime(4)}Z Running tests/actual.rs`,
    `${logTime(6)}Z test result: ok`,
  ].join('\n'), { startedAt: start, completedAt: end });
  assert.equal(parsed.groups[0].autoClosedAt, 1);
  assert.equal(parsed.groups[0].scopeEnd, 1);
  assert.equal(parsed.groups[1].autoClosedAt, 3);
  assert.equal(parsed.diagnostics.unclosedGroups, 1);
  assert.equal(parsed.cargoSpans.length, 1);
  assert.equal(parsed.cargoSpans[0].command, 'unknown cargo command');
  assert.equal(parsed.cargoSpans[0].execution.length, 1);
  assert.equal(parsed.phases.unknown, 0);
});

 test('module import from unrelated cwd is side-effect free', () => {
  const dir = mkdtempSync(join(tmpdir(), 'ci-long-jobs-import-'));
  try {
    const result = spawnSync(process.execPath, ['-e', `import(${JSON.stringify(pathToFileURL(SCRIPT).href)})`], { cwd: dir, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(readdirSync(dir), []);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('snake/camel job timestamps and wrapped run records yield exact totals, medians, top-eight and phases', () => {
  const ws = workspace();
  try {
    fixture(ws.inputs);
    const report = generateReport({ inputs: ws.inputs });
    assert.match(report, /Full-selected completed attempts: 6\/6; successful: 6; failed\/censored attempts: 0/);
    assert.match(report, /Attempts: full-1 schedule\/success/);
    assert.match(report, /43781371392 pull_request\/success/);
    assert.match(report, /Total 582\.5 job-min; per-run job-min median\/range 97\.1 \(93\.3–100\.8; n=6\); pooled top-8 560\.0 \/ 582\.5 min \(96\.1%\)/);
    assert.match(report, /per-run job-min median\/range 97\.1 \(93\.3–100\.8; n=6\)/);
    assert.match(report, /\| full-1 ·[^\n]*93\.3 \| 90\.0 \/ 93\.3 \| 96\.4% \| 9\/10 \| 0 \| 0 \|/);
    assert.match(report, /\| Install dependencies \| 60\.0 \(60\.0–60\.0; n=6\) \| none \|/);
    assert.match(report, /\| cargo check #1 \| 25\.0 \(25\.0–25\.0; n=6\) \| none \|/);
    assert.match(report, /\| Unknown interval \| н\/д \| none \|/);
    assert.match(report, /## Miri target attribution and bounded sharding/);
    assert.match(report, /Log-derived phase measurements/);
    assert.match(report, /Log-derived phase measurements/);
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('committed raw snapshot recognizes both successful schedules and README-selected full PR', () => {
  const report = generateReport();
    assert.match(report, /Full-selected completed attempts: 6\/6; successful: 3; failed\/censored attempts: 3/);
  assert.match(report, /Attempts: 43410623227 schedule\/failure[^\n]*43582569269 schedule\/failure[^\n]*43704208363 schedule\/success[^\n]*43781371392 pull_request\/success[^\n]*43851422289 schedule\/success[^\n]*43886332211 schedule\/failure/);
  assert.match(report, /per-run job-min median\/range 349\.5 \(318\.7–402\.2; n=6\)/);
  assert.match(report, /Schema ci-long-jobs-log-phases-v1, parserVersion 3; 26\/26 stored log records/);
  assert.match(report, /COVERAGE GAP: 24\/26 requested job logs succeeded\/available/);
  assert.match(report, /No-runner cancellation excluded from runner-time: 43886332211\/133365637356/);
});

test('full PR selection evidence in adjacent README counts as full; missing sample remains explicit', () => {
  const ws = workspace();
  try {
    fixture(ws.inputs, { count: 3 });
    const output = join(ws.dir, 'partial.md');
    const result = cli(['--inputs', ws.inputs, '--report', output]);
    assert.equal(result.status, 0, result.stderr);
    const report = readFileSync(output, 'utf8');
    assert.match(report, /Full-selected completed attempts: 3\/6; successful: 3; failed\/censored attempts: 0/);
    assert.match(report, /Attempts: full-1 schedule\/success[^\n]*full-2 schedule\/success[^\n]*43781371392 pull_request\/success/);
    assert.match(report, /Main push comparison: 1/);
    assert.doesNotMatch(report, /Full-selected completed attempts: 6\/6/);
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('malformed run document and mismatched snapshot count are rejected', () => {
  const ws = workspace();
  try {
    writeFileSync(join(ws.inputs, 'bad.json'), JSON.stringify({ event: 'schedule' }));
    assert.throws(() => generateReport({ inputs: ws.inputs }), /expected run document \{run,jobs\}/);
    rmSync(join(ws.inputs, 'bad.json'));
    const bad = run('bad-count', 0);
    bad.total_count++;
    writeFileSync(join(ws.inputs, 'bad-count.json'), JSON.stringify(bad));
    assert.throws(() => generateReport({ inputs: ws.inputs }), /saved job count mismatch/);
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('duplicate run IDs are rejected rather than counted twice', () => {
  const ws = workspace();
  try {
    const value = run('repeated', 0);
    writeRecords(ws.inputs, [value, value]);
    assert.throws(() => generateReport({ inputs: ws.inputs }), /duplicate|repeated/i);
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('an unexpected skipped job is rejected in full-coverage fixtures', () => {
  const ws = workspace();
  try {
    const value = run('incomplete-full', 0);
    value.jobs[1].conclusion = 'skipped';
    writeFileSync(join(ws.inputs, 'incomplete.json'), JSON.stringify(value));
    assert.throws(() => generateReport({ inputs: ws.inputs }), /unexpected skipped group\/job/);
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('reversed job or step intervals are rejected, not silently rendered as missing', () => {
  for (const level of ['job', 'step']) {
    const ws = workspace();
    try {
      const value = run(`reversed-${level}`, 0);
      if (level === 'job') {
        value.jobs[0].started_at = time(100);
        value.jobs[0].completed_at = time(50);
      } else {
        value.jobs[0].steps[0].started_at = time(100);
        value.jobs[0].steps[0].completed_at = time(50);
      }
      writeFileSync(join(ws.inputs, 'reversed.json'), JSON.stringify(value));
      assert.throws(() => generateReport({ inputs: ws.inputs }), /negative|reversed|invalid|interval/i, `${level} interval`);
    } finally { rmSync(ws.dir, { recursive: true, force: true }); }
  }
});

test('report generation and positive/corrupt check use only a temporary destination', () => {
  const ws = workspace();
  try {
    fixture(ws.inputs);
    const output = join(ws.dir, 'report.md');
    const args = ['--inputs', ws.inputs, '--report', output];
    assert.equal(cli(args).status, 0);
    const first = readFileSync(output, 'utf8');
    assert.equal(cli(args).status, 0);
    assert.equal(readFileSync(output, 'utf8'), first);
    assert.equal(cli(['--inputs', ws.inputs, '--check', output]).status, 0);
    writeFileSync(output, `${first}\ncorrupt\n`);
    const corrupt = cli(['--inputs', ws.inputs, '--check', output]);
    assert.equal(corrupt.status, 1);
    assert.match(corrupt.stderr, /report missing or stale/);
    assert.ok(!existsSync(join(ROOT, 'docs', 'perf', 'ci_long_jobs_inputs', 'report.md')));
  } finally { rmSync(ws.dir, { recursive: true, force: true }); }
});

test('committed generated report check is read-only', { skip: !existsSync(DEFAULT_REPORT) }, () => {
  const before = readFileSync(DEFAULT_REPORT, 'utf8');
  const result = cli(['--check']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(DEFAULT_REPORT, 'utf8'), before);
});
