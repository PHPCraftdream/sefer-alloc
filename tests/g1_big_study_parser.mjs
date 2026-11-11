import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { parseJsonResult, parseResult } from '../scripts/paired-ab-parse-result.mjs';

const runnerSource = readFileSync(new URL('../scripts/paired-ab-runner.mjs', import.meta.url), 'utf8');
const configStart = runnerSource.indexOf('function loadConfig() {');
const configEnd = runnerSource.indexOf('\nconst CONFIG = loadConfig();', configStart);
const configFunction = runnerSource.slice(configStart, configEnd);

function loadConfigFixture(config, exists = true, text = JSON.stringify(config)) {
  return runInNewContext(`${configFunction}\nconst CONFIG = loadConfig();\nCONFIG`, {
    configArg: 'fixture.json', existsSync: () => exists, readFileSync: () => text,
    process: { env: {} },
  });
}

test('JSON parser preserves judge evidence and requires one positive-metric object', () => {
  const row = { cold_ns: 17, warm_ns: null, rep: [[1, 2, 3]], oracle: { x: null } };
  assert.deepEqual(parseJsonResult(`${JSON.stringify(row)}\n`, 'cold_ns'), row);
  for (const text of ['', '{bad}', '{}', 'null', '7', '[]', '{}\n{}']) assert.throws(() => parseJsonResult(text, 'cold_ns'));
  for (const cold_ns of [0, -1, '1', null]) assert.throws(() => parseJsonResult(JSON.stringify({ cold_ns }), 'cold_ns'));
  assert.throws(() => parseJsonResult('{"cold_ns":1e309}', 'cold_ns'));
  assert.throws(() => parseJsonResult('{"cold_ns":17,"warm_ns":19}', 'cold_ns'), /warm_ns must be null/);
});

test('legacy RESULT parsing and custom env config remain compatible', () => {
  assert.deepEqual(parseResult('noise\nRESULT cold_ns=17\nRESULT label=ok\n'), { cold_ns: 17, label: 'ok' });
  assert.equal(loadConfigFixture({ arms: { one: { command: 'bridge', env: { CUSTOM_BRIDGE: 'yes' } } } }).arms.one.env.CUSTOM_BRIDGE, 'yes');
});

test('JSON config needs same-launch executables, string argv and explicit string env', () => {
  const arm = { command: '/trusted/judge', args: ['cycle'], env: { G1_BIG_THP: 'off' } };
  const config = { result_format: 'json', arms: { A: arm, B: { ...arm, env: { G1_BIG_THP: 'always' } } } };
  assert.equal(loadConfigFixture(config).result_format, 'json');
  assert.throws(() => loadConfigFixture(config, false), /file not found/);
  assert.throws(() => loadConfigFixture(config, true, '{bad}'), /not valid JSON/);
  assert.throws(() => loadConfigFixture({ ...config, arms: { A: arm, B: { ...config.arms.B, args: ['different'], env: { G1_BIG_THP: 'off' } } } }), /same executable and arguments/);
  assert.throws(() => loadConfigFixture({ ...config, metric: 'elapsed_ns' }), /metric must be cold_ns/);
  assert.throws(() => loadConfigFixture({ ...config, build: { command: 'cargo', args: [] } }), /prebuilt executable/);
  assert.throws(() => loadConfigFixture({ ...config, timeout_ms: 120001 }), /timeout_ms must be 1..120000/);
  assert.throws(() => loadConfigFixture({ ...config, sanity: { key: 'x', nonzero_arms: [] } }), /RESULT sanity gates/);
  assert.throws(() => loadConfigFixture({ ...config, arms: { A: { ...arm, env: { BAD: 1 } }, B: config.arms.B } }), /env must contain string values/);
});

test('runner records source identity before any JSON launch', () => {
  const main = runnerSource.slice(runnerSource.indexOf('async function main() {'), runnerSource.indexOf('\nmain().catch('));
  const identity = main.indexOf('sourceIdentity = {');
  const build = main.indexOf('await buildArms(arms);');
  assert.ok(identity > build && identity < main.indexOf('if (verifyOnly) {'));
  assert.ok(runnerSource.includes('if (armA === armB)') || runnerSource.includes('const sameArm = armA === armB'));
});
