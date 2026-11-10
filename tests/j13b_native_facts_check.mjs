// CI-only observer oracle: independently hash the reported executable with Node crypto.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { hashExecutable } from '../scripts/j13b_artifacts.mjs';

const [path, ...extra] = process.argv.slice(2);
assert(path && extra.length === 0, 'usage: node tests/j13b_native_facts_check.mjs <native-test-log>');
const prefix = '[j13b-native-facts-json] ';
const records = readFileSync(resolve(path), 'utf8').split('\n')
  .filter(line => line.includes(prefix)).map(line => JSON.parse(line.slice(line.indexOf(prefix) + prefix.length)));
assert.equal(records.length, 1, 'native observation must actually execute exactly once');
const { executable_path: executable, observed } = records[0];
assert.equal(observed.kind, 'observed-linux-process-facts');
assert.equal(hashExecutable(resolve(executable)), observed.binary_sha256, 'opened process image must match actual test executable bytes');
for (const field of ['arch', 'kernel', 'cpu', 'boot_id', 'host_id', 'thp_top_level_mode']) {
  assert.equal(typeof observed.host[field], 'string');
  assert(observed.host[field].length > 0);
}
assert.equal(process.platform, 'linux', 'native oracle requires Linux');
assert.equal(observed.host.arch, { x64: 'x86_64', arm64: 'aarch64' }[process.arch]);
assert.equal(observed.host.kernel, readFileSync('/proc/sys/kernel/osrelease', 'utf8').trim());
assert.equal(observed.host.boot_id, readFileSync('/proc/sys/kernel/random/boot_id', 'utf8').trim());
const cpu = readFileSync('/proc/cpuinfo', 'utf8').split('\n')
  .find(line => /^(model name|Hardware)\s*:/.test(line));
assert(cpu, 'independent CPU model must exist');
assert.equal(observed.host.cpu, cpu.slice(cpu.indexOf(':') + 1).trim());
assert.equal(observed.host.host_id, JSON.stringify([
  observed.host.arch, observed.host.kernel, observed.host.cpu, observed.host.boot_id,
]));
const thp = readFileSync('/sys/kernel/mm/transparent_hugepage/enabled', 'utf8');
const active = [...thp.matchAll(/\[(always|madvise|never)\]/g)];
assert.equal(active.length, 1, 'independent top-level THP mode must be unambiguous');
assert.equal(observed.host.thp_top_level_mode, active[0][1]);
assert(['always', 'madvise', 'never'].includes(observed.host.thp_top_level_mode));
assert(observed.host.thp_policy_scope.includes('per-size/defrag policy not authenticated'));
assert(observed.build_provenance.includes('require reviewed builder'));
assert.equal(observed.source_sha256, undefined, 'process facts must not invent source provenance');
assert.equal(observed.rustc, undefined, 'process facts must not invent actual compiler provenance');
console.log('[j13b-native-facts-check] actual image matches independent Node digest; host observations scoped, no payload/source claim');
