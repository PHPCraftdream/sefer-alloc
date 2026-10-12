// Offline re-parse of the retained raw Callgrind output of one consumed PRF01 A-only producer preflight.
// Launches nothing; reads retained files, checks them against the run receipts and prints/derives the numbers.
// Usage: node scripts/r19_prf01_preflight_reparse.mjs <preflight-dir> [--csv <file>]
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const { parseProfiles, publicText } = await import(pathToFileURL(resolve(here, 'r19_prf01_sidecar_summary_run.mjs')).href);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const PRODUCER = 'r19_prf01_joined_producer_same_word';
const [dirArg, flag, csvArg] = process.argv.slice(2);
assert.ok(dirArg && (flag === undefined || flag === '--csv' && csvArg), 'usage: <preflight-dir> [--csv <file>]');
const out = resolve(dirArg);

const plan = JSON.parse(readFileSync(`${out}/plan/preflight-plan.json`));
const cap = JSON.parse(readFileSync(`${out}/capture.json`));
const inv = JSON.parse(readFileSync(`${out}/${cap.inventory.path}`));
const cell = inv.cells.find(c => c.name === PRODUCER);
assert.equal(sha(readFileSync(`${out}/capture.json`)), plan.captureSha256, 'plan is not bound to this capture');
const rawRoot = resolve(out, '..', `prf01-preflight-${plan.captureSha256}`);
const parsed = {}, files = {};
for (const job of plan.argv) {
  const role = job.role, home = `${rawRoot}/${role}/${PRODUCER}`;
  const result = JSON.parse(readFileSync(`${out}/plan/preflight-result-${role}.json`));
  const known = new Set(result.raw.map(r => r.originalSha256));
  const names = readdirSync(home).sort();
  files[role] = names.map(n => ({ name: n, size: statSync(`${home}/${n}`).size, sha256: sha(readFileSync(`${home}/${n}`)) }));
  for (const f of files[role]) assert.ok(known.has(f.sha256), `retained file not in the run receipt: ${role}/${f.name}`);
  assert.equal(files[role].length, known.size, 'file set differs from the run receipt');
  const profiles = names.filter(n => /\.out(?:[.#-]+\d+)*$/.test(n) && statSync(`${home}/${n}`).size > 0).map(n => publicText(readFileSync(`${home}/${n}`, 'utf8')));
  const logs = names.filter(n => /\.log(?:[.#-]+\d+)*$/.test(n)).map(n => publicText(readFileSync(`${home}/${n}`, 'utf8'))).join('\n');
  const build = cap.builds[`A-${role}`];
  const expectedTools = job.argv.slice(1, job.argv.indexOf(`./${build.binaryPath}`));
  const debug = 'callgrind: Arguments: ' + expectedTools.map(x => publicText(x)).join(' ') + '\n';
  const p = parseProfiles(profiles, logs, { name: PRODUCER, cell, producer: true, control: role === 'control', expectedExecutable: publicText(`./${build.binaryPath}`), debug });
  assert.deepEqual(p.toolVector, expectedTools.map(x => publicText(x)), `tool argv drift ${role}`);
  parsed[role] = p;
}
const primary = parsed.iai, control = parsed.control;
const overhead = control.childIr;
assert.ok(overhead >= 1 && overhead <= 16, 'boundary overhead outside the frozen 1..16');
assert.equal(primary.boundaryExecutions, 1);
assert.equal(control.boundaryExecutions, 1);
const workloadIr = primary.childIr - overhead, deltaIr = primary.totalIr - control.totalIr;
assert.ok(workloadIr > 0, 'empty primary remainder');
assert.equal(workloadIr, deltaIr, 'both totals count only the toggled window, so the remainder equals the total delta');
const summary = {
  scope: 'diagnostic only; offline re-parse of the retained raw files of one consumed preflight; never gate evidence',
  captureSha256: plan.captureSha256,
  primary: { totalIr: primary.totalIr, childIr: primary.childIr, boundaryExecutions: primary.boundaryExecutions, threads: primary.threads, executedFree: primary.executedFree },
  control: { totalIr: control.totalIr, childIr: control.childIr, boundaryExecutions: control.boundaryExecutions, threads: control.threads },
  boundaryOverheadIr: overhead, workloadIr, controlWorkloadIr: 0, deltaIr, childFreeCountDeclared: 4096, files,
};
writeFileSync(`${out}/offline-reparse.json`, JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
if (csvArg) {
  const rows = [
    ['role', 'totalIr', 'childIr', 'boundaryExecutions', 'threadProfiles', 'boundaryOverheadIr', 'workloadIr'],
    ['primary', primary.totalIr, primary.childIr, primary.boundaryExecutions, primary.threads.length, overhead, workloadIr],
    ['control', control.totalIr, control.childIr, control.boundaryExecutions, control.threads.length, overhead, 0],
  ];
  writeFileSync(resolve(csvArg), rows.map(r => r.join(',')).join('\n') + '\n', { flag: 'wx' });
}
console.log(JSON.stringify({ ...summary, files: undefined }, null, 2));
