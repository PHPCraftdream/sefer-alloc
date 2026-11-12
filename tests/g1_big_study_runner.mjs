// CI ONLY: exercises the runner with temporary synthetic producers. Never run locally.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync, chmodSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const runner = join(root, 'scripts/paired-ab-runner.mjs');
const linux = process.platform === 'linux';

function fixture(t, mode = '', resultFormat = 'json') {
  const dir = mkdtempSync(join(tmpdir(), 'g1-runner-fixture-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const producer = join(dir, 'producer.mjs');
  writeFileSync(producer, `
    import { renameSync, writeFileSync, writeSync, existsSync } from 'node:fs';
    if (process.env.MODE === 'timeout') { writeSync(1, 'timeout-start\\n'); Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5000); }
    if (process.env.MODE === 'resistant' && !existsSync(process.env.MARKER_PATH)) {
      writeFileSync(process.env.MARKER_PATH, 'started');
      writeSync(1, 'term-resistant-start\\n');
      process.on('SIGTERM', () => writeSync(1, 'term-ignored\\n'));
      setTimeout(() => process.exit(0), 4000);
    }
    if (process.env.MODE === 'large') process.stdout.write('x'.repeat(140000));
    else if (process.env.MODE === 'malformed') process.stdout.write('{bad}\\n');
    else if (process.env.MODE === 'legacy') process.stdout.write('RESULT elapsed_ns=11\\n');
    else process.stdout.write(JSON.stringify({ cold_ns: 11, warm_ns: null, payload: 'fixture' }) + '\\n');
    if (process.env.MODE === 'nonzero') { process.stderr.write('failure-evidence\\n'); process.exitCode = 9; }
    if (process.env.MUTATE_TARGET) { writeFileSync(process.env.MUTATE_TARGET + '.new', 'fixture replacement'); renameSync(process.env.MUTATE_TARGET + '.new', process.env.MUTATE_TARGET); }
  `);
  const command = process.execPath;
  const args = [producer];
  const configFile = join(dir, 'config.json');
  const out = join(dir, 'out'); mkdirSync(out);
  const config = { result_format: resultFormat, metric: resultFormat === 'json' ? 'cold_ns' : 'elapsed_ns', timeout_ms: 1000, arms: {
    A: { command, args, env: { MODE: '', MUTATE_TARGET: '', MARKER_PATH: '' } },
    B: { command, args, env: { MODE: mode, MUTATE_TARGET: '', MARKER_PATH: join(dir, 'resistant-marker') } },
  } };
  writeFileSync(configFile, JSON.stringify(config));
  const invoke = (options = {}) => spawnSync(process.execPath,
    [runner, '--config', configFile, '--pairs', '1', ...(options.arms ? ['--arms', options.arms] : [])],
    { cwd: root, encoding: 'utf8', timeout: options.outerTimeout ?? 8000, killSignal: 'SIGKILL', env: { ...process.env, PAIRED_AB_OUT_DIR: out, ...options.env } });
  const evidence = () => JSON.parse(readFileSync(join(out, readdirSync(out).find((name) => name.startsWith('failed-'))), 'utf8'));
  return { dir, producer, config, configFile, out, invoke, evidence };
}

test('CI runner retains prior success on a later nonzero child and same-arm control works', (t) => {
  const f = fixture(t, 'nonzero');
  const result = f.invoke();
  assert.notEqual(result.status, 0);
  const failed = f.evidence();
  assert.equal(failed.raw_log.length, 1);
  assert.equal(failed.raw_log[0].parsed_row.payload, 'fixture');
  assert.equal(failed.failed.child_exit_code, 9);
  assert.match(failed.failed.raw_stderr, /failure-evidence/);

  const same = fixture(t);
  const success = same.invoke({ arms: 'A,A' });
  assert.equal(success.status, 0, success.stderr);
  const receipt = JSON.parse(readFileSync(join(same.out, readdirSync(same.out)[0]), 'utf8'));
  assert.equal(receipt.comparisons[0].arm_a, 'A');
  assert.equal(receipt.comparisons[0].raw_process_launches.length, 4);
  assert.match(success.stdout, /Descriptive statistics only; use the preregistered reducer/);
  assert.doesNotMatch(success.stdout, /means a REAL|non-noise wall-clock/);

  const legacy = fixture(t, 'legacy', 'result');
  const env = {};
  if (linux) {
    // Synthetic RESULT parsing must not initialize a real rustup toolchain.
    const bin = join(legacy.dir, 'bin'); mkdirSync(bin);
    const rustc = join(bin, 'rustc');
    writeFileSync(rustc, '#!/bin/sh\n[ "$#" -eq 2 ] && [ "$1" = --version ] && [ "$2" = --verbose ] || exit 2\nprintf "rustc synthetic-fixture-only\\n"\n');
    chmodSync(rustc, 0o755);
    env.PATH = `${bin}${delimiter}${process.env.PATH}`;
  }
  const legacyResult = legacy.invoke({ arms: 'B,B', env });
  assert.ifError(legacyResult.error);
  assert.equal(legacyResult.status, 0, legacyResult.stderr);
  if (linux) {
    const saved = JSON.parse(readFileSync(join(legacy.out, readdirSync(legacy.out)[0]), 'utf8'));
    assert.equal(saved.rustc_version_verbose, 'rustc synthetic-fixture-only');
  }
  assert.match(legacyResult.stdout, /means a REAL/);
  assert.match(legacyResult.stdout, /non-noise wall-clock difference/);
  assert.doesNotMatch(legacyResult.stdout, /Descriptive statistics only; use the preregistered reducer/);
});

test('CI runner receipts retain malformed output, timeout partial evidence and maxBuffer failure', (t) => {
  for (const mode of ['malformed', 'timeout', 'large']) {
    const f = fixture(t, mode);
    const result = f.invoke();
    assert.notEqual(result.status, 0, `${mode} unexpectedly succeeded`);
    const receipt = f.evidence();
    assert.equal(receipt.raw_log.length, 1);
    assert.equal(receipt.failed.raw_stdout.length > 0, true);
    if (mode === 'malformed') assert.match(receipt.failed.raw_stdout, /bad/);
    if (mode === 'timeout') { assert.equal(receipt.failed.timed_out, true); assert.match(receipt.failed.raw_stdout, /timeout-start/); }
    if (mode === 'large') assert.match(receipt.failed.error, /ENOBUFS|buffer|stdout/i);
  }
});

test('CI Linux resistant producer is killed at timeout and preserves prior raw evidence', { skip: !linux }, (t) => {
  const resistant = fixture(t, 'resistant');
  const resistantResult = resistant.invoke({ arms: 'A,B', outerTimeout: 8000 });
  assert.notEqual(resistantResult.status, 0);
  const receipt = resistant.evidence();
  assert.equal(receipt.raw_log.length, 1);
  assert.equal(receipt.failed.timed_out, true);
  assert.equal(receipt.failed.signal, 'SIGKILL');
  assert.equal(existsSync(join(resistant.dir, 'resistant-marker')), true);
  assert.match(receipt.failed.raw_stdout, /term-resistant-start/);
  assert.doesNotMatch(receipt.failed.raw_stdout, /term-ignored/);
});

test('CI Linux hash-mismatch oracle only mutates a temporary Node copy', { skip: !linux }, (t) => {
  const f = fixture(t);
  const copiedNode = join(f.dir, 'node-copy');
  copyFileSync(process.execPath, copiedNode); chmodSync(copiedNode, 0o755);
  f.config.arms.A.command = copiedNode; f.config.arms.B.command = copiedNode;
  f.config.arms.A.args = [f.producer]; f.config.arms.B.args = [f.producer];
  f.config.arms.B.env.MUTATE_TARGET = copiedNode;
  writeFileSync(f.configFile, JSON.stringify(f.config));
  const result = f.invoke();
  assert.notEqual(result.status, 0);
  assert.match(f.evidence().failed.failure, /changed during launch/);
});
