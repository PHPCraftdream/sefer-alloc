import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collect } from '../scripts/collect-test-selection-criterion.mjs';
import test from 'node:test';
import { CORE_ROWS, evaluateWindow } from '../scripts/test-selection-criterion.mjs';
import { parseCoreLog } from '../scripts/test-selection-criterion/cargo-log.mjs';
import { sealSelection } from '../scripts/test-selection-criterion/selection-integrity.mjs';

const retained = 'tests/global_alloc.rs';
const omitted = 'tests/heap_invariants.rs';
const integration = { kind: 'integration', name: 'heap_invariants', path: omitted };
const lib = { kind: 'lib', name: 'sefer-alloc' };
const doctest = { kind: 'doctest', name: 'doc' };
const rawSubset = { mode: 'subset', reason: 'every changed file is attributed', total: 2, tests: [retained] };

function selectionBytes(raw) {
  return Buffer.from(JSON.stringify(raw));
}

function seal(raw) {
  return sealSelection(raw, selectionBytes(raw));
}

function run(n) {
  const headSha = n.toString(16).padStart(40, '0');
  return {
    id: String(1000 + n),
    createdAt: new Date(Date.UTC(2026, 10, 18, 0, n)).toISOString(),
    event: 'pull_request',
    headBranch: 'ordinary-change',
    headSha,
    status: 'completed',
    attempt: 1,
    conclusion: 'success',
    selection: seal(rawSubset),
    core: {
      id: String(2000 + n),
      conclusion: 'success',
      shadow: true,
      rows: CORE_ROWS.map((name) => ({ name, conclusion: 'success' })),
    },
  };
}

function greenRuns(count = 30) {
  return Array.from({ length: count }, (_, i) => run(i + 1));
}

function redRun(evidence, rowCount = 1) {
  const red = run(31);
  red.conclusion = red.core.conclusion = 'failure';
  for (const row of red.core.rows.slice(0, rowCount)) {
    row.conclusion = 'failure';
    row.evidence = evidence;
  }
  return red;
}

function failed(...failures) {
  return { status: 'test-failures', failures };
}

test('criterion passes with 30 observed ordinary runs and nonzero subset share', () => {
  assert.equal(evaluateWindow(greenRuns()).verdict, 'PASS');
});

test('criterion rejects compact selection edits against the original receipt', () => {
  const record = run(1);
  for (const [tests, message] of [
    [[], /selection integrity test count mismatch/],
    [[omitted], /selection integrity digest mismatch/],
  ]) {
    const mutant = { ...record, selection: { ...record.selection, tests } };
    assert.throws(() => evaluateWindow([mutant, ...greenRuns().slice(1)]), message);
  }
});

test('criterion fails for a red omitted target even though success-only filtering passes', () => {
  const runs = [...greenRuns(), redRun(failed(integration))];
  const successOnly = runs.filter(({ conclusion }) => conclusion === 'success');
  assert.equal(successOnly.length, 30);
  assert.equal(evaluateWindow(successOnly).verdict, 'PASS');
  assert.equal(evaluateWindow(runs).verdict, 'FAIL');
  assert.equal(evaluateWindow([...successOnly, redRun(failed(integration), 2)]).counts.misses, 1);
});

test('criterion has insufficient window at 29 runs', () => {
  assert.equal(evaluateWindow(greenRuns(29)).verdict, 'INSUFFICIENT_WINDOW');
});

test('criterion retains lib failures in subset mode', () => {
  assert.equal(evaluateWindow([...greenRuns(), redRun(failed(lib))]).verdict, 'PASS');
});

test('criterion counts no miss in all mode', () => {
  const red = redRun(failed(integration, doctest));
  red.selection = seal({ mode: 'all', reason: 'full', total: 2 });
  assert.equal(evaluateWindow([...greenRuns(), red]).verdict, 'PASS');
});

test('parsed Cargo doctest failure is a subset miss', () => {
  const red = redRun(undefined);
  red.core.rows = parseCoreLog(ghLog(cargoStep(0, [
    'Doc-tests sefer_alloc',
    'running 1 test',
    'test src/lib.rs - example (line 12) ... FAILED',
    'failures:',
    '    src/lib.rs - example (line 12)',
    'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s',
    "error: doctest failed, to rerun pass '--doc'",
    'error: 1 target failed:',
    '    `--doc`',
    exit,
  ])), red.core.rows);
  assert.deepEqual(red.core.rows[0].evidence, failed(doctest));
  assert.equal(evaluateWindow([...greenRuns(), red]).verdict, 'FAIL');
});

test('criterion does not count red pre-test failures as a complete observation', () => {
  const red = redRun({ status: 'pre-test', reason: 'compile failed' });
  assert.notEqual(evaluateWindow([...greenRuns(29), red]).verdict, 'PASS');
});

test('criterion blocks a complete green window only for unresolved core test failures', () => {
  const greens = greenRuns();
  assert.equal(evaluateWindow(greens).verdict, 'PASS');

  const missingArtifact = redRun(failed(integration));
  delete missingArtifact.selection;
  const missingLog = redRun(undefined);
  const missingPath = redRun(failed({ kind: integration.kind, name: integration.name }));
  const missingCore = { ...redRun(undefined), core: null };
  const unavailableCore = { ...missingCore, coreLookup: 'unavailable' };
  for (const [label, red] of Object.entries({ missingArtifact, missingLog, missingPath, missingCore, unavailableCore })) {
    const { verdict, counts, shortfall } = evaluateWindow([...greens, red]);
    assert.equal(shortfall, 0, label);
    assert.equal(counts.unresolvedRed, 1, label);
    assert.equal(verdict, 'INSUFFICIENT_WINDOW', label);
  }

  const preTest = redRun({ status: 'pre-test', reason: 'compile failed' });
  const skipped = { ...missingCore, core: { id: preTest.core.id, conclusion: 'skipped', shadow: false, rows: [] } };
  const cancelledBefore = { ...missingCore, core: {
    id: preTest.core.id, conclusion: 'cancelled', shadow: true,
    rows: CORE_ROWS.map(name => ({ name, conclusion: 'skipped' })),
  } };
  const cancelledDuring = { ...cancelledBefore, core: { ...cancelledBefore.core,
    rows: cancelledBefore.core.rows.map((row, i) => i === 0 ? { ...row, conclusion: 'cancelled' } : row),
  } };
  for (const [label, red] of Object.entries({ preTest, skipped, cancelledBefore })) {
    const { verdict, counts } = evaluateWindow([...greens, red]);
    assert.equal(counts.informative, 30, label);
    assert.equal(counts.unresolvedRed, 0, label);
    assert.equal(verdict, 'PASS', label);
  }
  const during = evaluateWindow([...greens, cancelledDuring]);
  assert.equal(during.counts.unresolvedRed, 1);
  assert.equal(during.verdict, 'INSUFFICIENT_WINDOW');
});

test('criterion does not treat cancelled, malformed, truncated, or unresolved evidence as zero misses', () => {
  const cancelled = run(31);
  cancelled.conclusion = cancelled.core.conclusion = 'cancelled';
  cancelled.core.rows[0].conclusion = 'cancelled';
  const malformed = redRun({ status: 'test-failures', failures: [] });
  const truncated = redRun({ status: 'unresolved', reason: 'missing Cargo failure exit' });
  truncated.issues = ['# TRUNCATED — fixture omitted incomplete evidence'];
  const unknown = redRun(undefined);
  for (const [label, record] of Object.entries({ cancelled, malformed, truncated, unknown })) {
    assert.notEqual(evaluateWindow([...greenRuns(29), record]).verdict, 'PASS', label);
  }
});

const commands = [
  'cargo test  --no-fail-fast',
  'cargo test --features experimental  --no-fail-fast',
  'cargo test --features "alloc-core internals"  --no-fail-fast',
];
const reset = '\x1b[0m';
const redBold = '\x1b[1m\x1b[31m';
const greenBold = '\x1b[1m\x1b[32m';
const exit = '##[error]Process completed with exit code 101.';
const integrationOutput = [
  `\u0020    ${greenBold}Running${reset} tests/heap_invariants.rs (target/debug/deps/heap_invariants-4027bc6f7e907f5a)`,
  'running 1 test',
  'test smoke ... FAILED',
  'failures:',
  '---- smoke stdout ----',
  "thread 'smoke' panicked at tests/heap_invariants.rs:17:5:",
  'assertion failed: result.is_ok()',
  'failures:',
  '    smoke',
  'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s',
  `${redBold}error${reset}: test failed, to rerun pass \x60--test heap_invariants\x60`,
];
const aggregate = [
  `${redBold}error${reset}: 2 targets failed:`,
  '    `--test heap_invariants`',
  '    `--test global_alloc`',
];
const secondIntegration = integrationOutput.map(line => line.replaceAll('heap_invariants', 'global_alloc'));

function cargoStep(index, lines) {
  const command = commands[index];
  return [`##[group]Run ${command}`, `\x1b[36;1m${command}${reset}`, 'shell: /usr/bin/bash -e {0}', '##[endgroup]', ...lines];
}

function ghLog(...lines) {
  return lines.flat().map((line, i) =>
    `2026-11-18T00:00:${String(i).padStart(2, '0')}.1234567Z ${line}\n`,
  ).join('');
}

const integrationAggregate = [
  `${redBold}error${reset}: 1 target failed:`,
  '    `--test heap_invariants`',
];

test('Cargo parser separates the three ANSI GitHub Actions step groups', () => {
  const red = redRun(undefined, 3);
  const rows = parseCoreLog(ghLog(
    cargoStep(0, [...integrationOutput, ...integrationAggregate, exit]),
    cargoStep(1, [...integrationOutput, ...secondIntegration, ...aggregate, exit]),
    cargoStep(2, [`${redBold}error${reset}: could not compile \x60sefer-alloc\x60 (lib) due to 1 previous error`, exit]),
  ), red.core.rows);
  assert.deepEqual(rows.map(({ name }) => name), CORE_ROWS);
  assert.deepEqual(rows[0].evidence, failed(integration));
  assert.deepEqual(rows[1].evidence, failed(integration, { kind: 'integration', name: 'global_alloc', path: retained }));
  assert.equal(rows[2].evidence.status, 'pre-test');
});

test('Cargo parser does not use lookalike test-body output or another step as the footer', () => {
  const { rows } = redRun(undefined).core;
  const body = [
    ...integrationOutput.slice(0, 6),
    'fixture printed: error: test failed, to rerun pass `--test global_alloc`',
    'fixture printed: error: 1 target failed:',
    ...integrationOutput.slice(6),
  ];
  const complete = parseCoreLog(ghLog(cargoStep(0, [...body, ...integrationAggregate, exit])), rows);
  assert.deepEqual(complete[0].evidence, failed(integration));
  const truncated = parseCoreLog(ghLog(cargoStep(0, [...body, exit]),
    ['##[group]Run echo unrelated', '##[endgroup]', ...integrationAggregate, exit]), rows);
  assert.equal(truncated[0].evidence.status, 'unresolved');
});

test('Cargo parser rejects mismatched aggregate counts and missing process exit', () => {
  const { rows } = redRun(undefined).core;
  for (const lines of [
    [...integrationOutput, ...aggregate, exit],
    [...integrationOutput, ...integrationAggregate],
  ]) {
    const parsed = parseCoreLog(ghLog(cargoStep(0, lines)), rows);
    assert.equal(parsed[0].evidence.status, 'unresolved');
  }
});

test('Cargo parser reconciles authentic receipts from CI run 44231928052 job 134559519559', () => {
  const log = [
    '2026-11-18T03:47:08.2405137Z ##[group]Run cargo test  --no-fail-fast',
    '2026-11-18T03:47:42.2449484Z \x1b[1m\x1b[92m     Running\x1b[0m tests/no_panic_doc_accuracy.rs (target/debug/build/sefer-alloc/d67b74618e0dcfe9/out/no_panic_doc_accuracy-d67b74618e0dcfe9)',
    '2026-11-18T03:47:42.2514515Z test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s',
    '2026-11-18T03:47:42.2515037Z \x1b[1m\x1b[91merror\x1b[0m: test failed, to rerun pass `--test no_panic_doc_accuracy`',
    '2026-11-18T03:47:42.2522806Z \x1b[1m\x1b[92m     Running\x1b[0m tests/no_stale_doc_references.rs (target/debug/build/sefer-alloc/0ac562fe1b70eeaf/out/no_stale_doc_references-0ac562fe1b70eeaf)',
    '2026-11-18T03:47:42.2991599Z \x1b[1m\x1b[91merror\x1b[0m: test failed, to rerun pass `--test no_stale_doc_references`',
    '2026-11-18T03:47:42.2992125Z \x1b[1m\x1b[92m     Running\x1b[0m tests/numa_alloc.rs (target/debug/build/sefer-alloc/5807f37f8b84b98c/out/numa_alloc-5807f37f8b84b98c)',
    '2026-11-18T03:47:42.2992787Z test result: FAILED. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s',
    '2026-11-18T03:47:48.6618082Z \x1b[1m\x1b[92m   Doc-tests\x1b[0m sefer_alloc',
    '2026-11-18T03:47:48.6738356Z \x1b[1m\x1b[91merror\x1b[0m: 2 targets failed:',
    '2026-11-18T03:47:48.6738655Z     `--test no_panic_doc_accuracy`',
    '2026-11-18T03:47:48.6820767Z     `--test no_stale_doc_references`',
    '2026-11-18T03:47:48.6830375Z ##[error]Process completed with exit code 101.',
  ].join('\n');
  const red = redRun(undefined);
  red.core.rows = parseCoreLog(log, red.core.rows);
  assert.deepEqual(red.core.rows[0].evidence, failed(
    { kind: 'integration', name: 'no_panic_doc_accuracy', path: 'tests/no_panic_doc_accuracy.rs' },
    { kind: 'integration', name: 'no_stale_doc_references', path: 'tests/no_stale_doc_references.rs' },
  ));
  assert.equal(evaluateWindow([...greenRuns(), red]).verdict, 'FAIL');
});

function tempDir(t) {
  const dir = mkdtempSync(join(tmpdir(), 'selection-criterion-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function apiRun(record, { attempt = record.attempt, status = record.status, conclusion = record.conclusion } = {}) {
  // A listed in-progress run's explicit null conclusion is preserved.
  return {
    id: Number(record.id), created_at: record.createdAt, event: record.event, head_branch: record.headBranch,
    head_sha: record.headSha, status, conclusion, run_attempt: attempt,
  };
}

function assertStableRecord(record, expected) {
  const { collectionCheck, ...evidence } = record;
  if (collectionCheck !== undefined) assert.equal(collectionCheck, 'stable');
  assert.deepEqual(evidence, expected);
}

test('offline collector reuses selection, jobs, and reduced log on its second pass', (t) => {
  const inputsDir = tempDir(t);
  const repo = 'fixture/sefer-alloc';
  const root = `repos/${repo}/actions`;
  const red = redRun(failed(integration));
  const listing = ['api', `${root}/workflows/ci.yml/runs?created=%3E%3D2026-11-17T13:04:56Z&per_page=100&page=1`];
  const jobs = ['api', `${root}/runs/${red.id}/attempts/1/jobs?per_page=100&page=1`];
  const logs = ['api', `${root}/jobs/${red.core.id}/logs`];
  const details = ['api', `${root}/runs/${red.id}`];
  const calls = [];
  const page = (key, entries) => JSON.stringify({ total_count: entries.length, [key]: entries });
  const call = (args) => {
    calls.push([...args]);
    if (args[1] === listing[1]) return page('workflow_runs', [apiRun(red)]);
    if (args[1] === details[1]) return JSON.stringify(apiRun(red));
    if (args[1] === jobs[1]) return page('jobs', [
      { id: 4001, name: 'select tests (diff + test-impact map)', conclusion: 'success' },
      {
        id: Number(red.core.id), name: 'test (core tiers)', conclusion: 'failure',
        steps: [{ name: 'Selected tests', conclusion: 'skipped' },
          ...red.core.rows.map(({ name, conclusion }) => ({ name, conclusion }))],
      },
    ]);
    if (args[1] === logs[1]) return ghLog(cargoStep(0, [...integrationOutput, ...integrationAggregate, exit]));
    if (args[0] === 'run' && args[1] === 'download') {
      assert.deepEqual(args, ['run', 'download', red.id, '--name', 'test-selection', '--repo', repo, '--dir', args[8]]);
      writeFileSync(join(args[8], 'selection.json'), selectionBytes(rawSubset));
      return '';
    }
    throw new Error(`Unexpected fake gh call: ${args.join(' ')}`);
  };
  const options = { inputsDir, repo, call, maxCalls: 5 };
  assert.equal(collect(options).listingComplete, true);
  assert.deepEqual(calls.map(args => args[0] === 'run' ? args.slice(0, 5) : args), [
    listing, jobs, ['run', 'download', red.id, '--name', 'test-selection'], logs, details,
  ]);
  const path = join(inputsDir, `${red.id}.json`);
  const first = readFileSync(path);
  assertStableRecord(JSON.parse(first), { ...red, coreLookup: 'available', evidenceAttempt: 1, issues: [] });
  calls.length = 0;
  assert.equal(collect(options).listingComplete, true);
  assert.deepEqual(calls, [listing]);
  assert.deepEqual(readFileSync(path), first);
});

function sequentialCollector(t) {
  const inputsDir = tempDir(t);
  const repo = 'fixture/sefer-alloc';
  const root = `repos/${repo}/actions`;
  const source = run(31);
  const recordPath = join(inputsDir, `${source.id}.json`);
  const listing = ['api', `${root}/workflows/ci.yml/runs?created=%3E%3D2026-11-17T13:04:56Z&per_page=100&page=1`];
  const jobs = attempt => ['api', `${root}/runs/${source.id}/attempts/${attempt}/jobs?per_page=100&page=1`];
  const logs = id => ['api', `${root}/jobs/${id}/logs`];
  const details = ['api', `${root}/runs/${source.id}`];
  const download = ['run', 'download', source.id, '--name', 'test-selection', '--repo', repo, '--dir', '<dir>'];
  const page = (key, entries) => JSON.stringify({ total_count: entries.length, [key]: entries });

  return {
    jobs, logs, download,
    runDetails: metadata => JSON.stringify(apiRun(source, metadata)),
    collectPass(metadata, requests = [], {
      verify = requests.length > 0,
      verificationResponse = JSON.stringify(apiRun(source, metadata)),
      maxCalls,
    } = {}) {
      const expected = [[listing, page('workflow_runs', [apiRun(source, metadata)])], ...requests];
      if (verify) expected.push([details, verificationResponse]);
      const transcript = [];
      const { listingComplete } = collect({ inputsDir, repo, maxCalls, call(args) {
        const isDownload = args[0] === 'run' && args[1] === 'download' && args.length === 9;
        const normalized = isDownload ? [...args.slice(0, 8), '<dir>'] : [...args];
        const next = expected[transcript.length];
        transcript.push(normalized);
        assert.deepEqual(normalized, next?.[0]);
        if (isDownload) {
          writeFileSync(join(args[8], 'selection.json'), selectionBytes(rawSubset));
          return '';
        }
        return Array.isArray(next[1]) ? page('jobs', next[1]) : next[1];
      } });
      return { record: JSON.parse(readFileSync(recordPath)), listingComplete,
        assertCalls: () => assert.deepEqual(transcript, expected.map(([args]) => args)) };
    },
  };
}

const selectorJob = { id: 4001, name: 'select tests (diff + test-impact map)', status: 'completed', conclusion: 'success' };
const transportUnavailable = { status: 1, stderr: 'Service Unavailable (HTTP 503)' };

function coreJob(id, conclusion) {
  return {
    id, name: 'test (core tiers)', status: 'completed', conclusion,
    steps: [{ name: 'Selected tests', conclusion: 'skipped' },
      ...CORE_ROWS.map((name, i) => ({ name, conclusion: i === 0 ? conclusion : 'success' }))],
  };
}

function assertUnresolvedWindow({ record, listingComplete }) {
  assert.equal(listingComplete, true);
  const { verdict, counts, shortfall } = evaluateWindow([...greenRuns(), record], { listingComplete });
  assert.equal(shortfall, 0);
  assert.equal(counts.unresolvedRed, 1);
  assert.equal(verdict, 'INSUFFICIENT_WINDOW');
}

function assertUnstableWindow({ record, assertCalls }) {
  assertCalls();
  assert.equal(record.collectionCheck, 'unstable');
  assert.ok(record.selection == null);
  assert.ok(record.core == null);
  assert.ok(record.evidenceAttempt == null);
  // An unstable record must block the verdict even when the run listing was complete.
  const { verdict, counts, shortfall } = evaluateWindow([...greenRuns(), record], { listingComplete: true });
  assert.equal(shortfall, 0);
  assert.equal(counts.informative, 30);
  assert.equal(counts.subset, 30);
  assert.equal(counts.misses, 0);
  assert.equal(verdict, 'INSUFFICIENT_WINDOW');
}

test('collector discards fresh green evidence when verification sees a red rerun', (t) => {
  const fixture = sequentialCollector(t);
  assertUnstableWindow(fixture.collectPass({ conclusion: 'success' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'success')]], [fixture.download],
  ], { verificationResponse: fixture.runDetails({ attempt: 2, conclusion: 'failure' }) }));
});

test('collector fails closed for unavailable or malformed verification of a green run', async (t) => {
  for (const [name, verificationResponse] of [
    ['HTTP 503', transportUnavailable],
    ['malformed metadata', JSON.stringify({ run_attempt: 1, status: 'completed', conclusion: 'success' })],
  ]) {
    await t.test(name, (t) => {
      const fixture = sequentialCollector(t);
      assertUnstableWindow(fixture.collectPass({ conclusion: 'success' }, [
        [fixture.jobs(1), [selectorJob, coreJob(5031, 'success')]], [fixture.download],
      ], { verificationResponse }));
    });
  }
});

test('collector fails closed when the call budget is exhausted before verification', (t) => {
  const fixture = sequentialCollector(t);
  assertUnstableWindow(fixture.collectPass({ conclusion: 'failure' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'failure')]], [fixture.download],
    [fixture.logs(5031), ghLog(cargoStep(0, [...integrationOutput, ...integrationAggregate, exit]))],
  ], { maxCalls: 4, verify: false }));
});

test('collector keeps latest red rerun metadata without borrowing attempt-one evidence', (t) => {
  const fixture = sequentialCollector(t);
  const first = fixture.collectPass({ conclusion: 'success' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'success')]], [fixture.download],
  ]);
  first.assertCalls();
  assert.deepEqual(first.record.selection, seal(rawSubset));
  assert.equal(first.record.core.id, '5031');
  assert.equal(first.record.evidenceAttempt, 1);
  if (first.record.collectionCheck !== undefined) assert.equal(first.record.collectionCheck, 'stable');
  const verified = evaluateWindow([...greenRuns(), first.record], { listingComplete: first.listingComplete });
  assert.equal(verified.verdict, 'PASS');
  assert.equal(verified.counts.subset, 31);

  const latest = fixture.collectPass({ attempt: 2, conclusion: 'failure' }, [
    [fixture.jobs(2), [{ ...selectorJob, id: 4002 }, coreJob(5032, 'failure')]],
    [fixture.logs(5032), ghLog(cargoStep(0, [...integrationOutput, ...integrationAggregate, exit]))],
  ]);
  assert.equal(latest.record.attempt, 2);
  assert.equal(latest.record.conclusion, 'failure');
  latest.assertCalls(); // Attempt-2 jobs and log are scoped; the artifact download is not.
  assert.equal(latest.record.selection, null);
  assert.equal(latest.record.core.id, '5032');
  assert.equal(latest.record.evidenceAttempt, 2);
  assertUnresolvedWindow(latest);
});

test('collector tags evidence with attempt 1 when an in-progress run finishes red', (t) => {
  const fixture = sequentialCollector(t);
  const active = fixture.collectPass({ status: 'in_progress', conclusion: null });
  active.assertCalls();
  assert.equal(active.record.status, 'in_progress');
  assert.equal(active.record.core, null);
  assert.equal(active.record.selection, null);

  const latest = fixture.collectPass({ conclusion: 'failure' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'failure')]], [fixture.download],
    [fixture.logs(5031), transportUnavailable],
  ]);
  latest.assertCalls();
  assert.equal(latest.record.status, 'completed');
  assert.equal(latest.record.attempt, 1);
  assert.equal(latest.record.conclusion, 'failure');
  assert.equal(latest.record.evidenceAttempt, 1);
  assert.equal(latest.record.core.id, '5031');
  assert.equal(latest.record.core.rows[0].evidence, undefined);
  assert.match(latest.record.issues.join('\n'), /core log unavailable:.*HTTP 503/);
  assertUnresolvedWindow(latest);
});

test('collector preserves an observed red attempt when the latest attempt turns green', (t) => {
  const fixture = sequentialCollector(t);
  const red = fixture.collectPass({ conclusion: 'failure' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'failure')]], [fixture.download],
    [fixture.logs(5031), ghLog(cargoStep(0, [...integrationOutput, ...integrationAggregate, exit]))],
  ]);
  red.assertCalls();
  assert.deepEqual(red.record.core.rows[0].evidence, failed(integration));

  const { record, listingComplete, assertCalls } = fixture.collectPass({ attempt: 2, conclusion: 'success' }, [
    [fixture.jobs(2), [{ ...selectorJob, id: 4002 }, coreJob(5032, 'success')]],
  ]);
  assert.equal(record.attempt, 2);
  assert.equal(record.conclusion, 'success');
  assertCalls();
  assert.equal(record.selection, null);
  assert.equal(record.core.id, '5032');
  assert.equal(record.evidenceAttempt, 2);
  assert.ok(Array.isArray(record.priorAttempts));
  assert.equal(record.priorAttempts.length, 1);
  const [prior] = record.priorAttempts;
  assert.equal(prior.attempt, 1);
  assert.equal(prior.conclusion, 'failure');
  assert.equal(prior.evidenceAttempt, 1);
  assert.equal(prior.priorAttempts, undefined);
  assert.deepEqual(prior.selection, red.record.selection);
  assert.deepEqual(prior.core, red.record.core);
  assert.equal(listingComplete, true);
  const { verdict, counts } = evaluateWindow([...greenRuns(), record], { listingComplete });
  assert.equal(counts.misses, 1);
  assert.equal(verdict, 'FAIL');
});

test('collector invalidates old jobs and selection when the same attempt changes from success to failure', (t) => {
  const fixture = sequentialCollector(t);
  const first = fixture.collectPass({ conclusion: 'success' }, [
    [fixture.jobs(1), [selectorJob, coreJob(5031, 'success')]], [fixture.download],
  ]);
  first.assertCalls();
  assert.deepEqual(first.record.selection, seal(rawSubset));
  assert.equal(first.record.core.id, '5031');
  assert.equal(first.record.evidenceAttempt, 1);

  const latest = fixture.collectPass({ conclusion: 'failure' }, [
    [fixture.jobs(1), transportUnavailable],
  ]);
  latest.assertCalls();
  assert.equal(latest.record.attempt, 1);
  assert.equal(latest.record.conclusion, 'failure');
  assert.equal(latest.record.selection, null);
  assert.equal(latest.record.core, null);
  assert.equal(latest.record.coreLookup, 'unavailable');
  assertUnresolvedWindow(latest);
});

test('collector preserves a jobs API failure as unavailable rather than skipped', (t) => {
  const inputsDir = tempDir(t);
  const repo = 'fixture/sefer-alloc';
  const red = redRun(undefined);
  const listing = ['api', `repos/${repo}/actions/workflows/ci.yml/runs?created=%3E%3D2026-11-17T13:04:56Z&per_page=100&page=1`];
  const jobs = ['api', `repos/${repo}/actions/runs/${red.id}/attempts/1/jobs?per_page=100&page=1`];
  const details = ['api', `repos/${repo}/actions/runs/${red.id}`];
  const calls = [];
  const { listingComplete } = collect({ inputsDir, repo, call(args) {
    calls.push(args);
    if (args[1] === listing[1]) return { status: 0, stdout: JSON.stringify({ total_count: 1, workflow_runs: [apiRun(red)] }) };
    if (args[1] === jobs[1]) return { status: 1, stderr: 'gh: Service Unavailable (HTTP 503)' };
    if (args[1] === details[1]) return JSON.stringify(apiRun(red));
    throw new Error(`Unexpected fake gh call: ${args.join(' ')}`);
  } });
  assert.equal(listingComplete, true);
  assert.deepEqual(calls, [listing, jobs, details]);
  const record = JSON.parse(readFileSync(join(inputsDir, `${red.id}.json`)));
  assert.equal(record.coreLookup, 'unavailable');
  assert.equal(record.core, null);
  assert.match(record.issues.join('\n'), /HTTP 503.*exit 1/);
  const greens = greenRuns();
  const result = evaluateWindow([...greens, record], { listingComplete });
  assert.equal(result.shortfall, 0);
  assert.equal(result.counts.unresolvedRed, 1);
  assert.equal(result.verdict, 'INSUFFICIENT_WINDOW');
  const skipped = { ...record, coreLookup: 'available', issues: [],
    core: { id: red.core.id, conclusion: 'skipped', shadow: false, rows: [] },
  };
  assert.equal(evaluateWindow([...greens, skipped], { listingComplete }).verdict, 'PASS');
});

test('offline report is byte-for-byte deterministic and passes --check', (t) => {
  const dir = tempDir(t);
  const inputsDir = join(dir, 'inputs');
  const reportPath = join(dir, 'report.md');
  mkdirSync(inputsDir);
  writeJson(join(inputsDir, 'collection.json'), {
    repo: 'fixture/sefer-alloc', since: '2026-11-17T13:04:56Z', listingComplete: true, issues: [],
  });
  for (const record of greenRuns()) writeJson(join(inputsDir, `${record.id}.json`), { ...record, issues: [] });
  const script = fileURLToPath(new URL('../scripts/test-selection-criterion.mjs', import.meta.url));
  const invoke = (mode, expectedStatus = 0) => {
    const result = spawnSync(process.execPath, [script, mode, '--inputs', inputsDir, '--report-path', reportPath], {
      cwd: dir, env: { ...process.env, PATH: '', Path: '' }, timeout: 10_000,
    });
    assert.ifError(result.error);
    assert.equal(result.status, expectedStatus, `${result.stdout}\n${result.stderr}`);
    return result;
  };
  const { stdout: firstStdout } = invoke('--report');
  const first = readFileSync(reportPath);
  assert.deepEqual(firstStdout, first);
  assert.match(first.toString(), /\bPASS\b/);
  const { stdout: secondStdout } = invoke('--report');
  const second = readFileSync(reportPath);
  assert.deepEqual(secondStdout, second);
  assert.deepEqual(second, first);
  invoke('--check');
  const corrupt = run(1);
  corrupt.selection.tests = [omitted];
  writeJson(join(inputsDir, `${corrupt.id}.json`), { ...corrupt, issues: [] });
  const { stderr } = invoke('--check', 1);
  assert.match(stderr.toString(), /selection integrity digest mismatch/);
  assert.deepEqual(readFileSync(reportPath), first);
});

