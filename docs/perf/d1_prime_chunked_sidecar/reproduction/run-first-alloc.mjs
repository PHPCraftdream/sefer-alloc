import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const samplesN = Number(process.argv[2] ?? 30);
assert.ok(Number.isInteger(samplesN) && samplesN >= 10);
const runs = [
  { id: 'before', repo: path.join(root, 'before-repo'), target: path.join(root, 'target-first-before'), out: path.join(root, 'raw/first-alloc-before'), commit: 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9', tree: 'b5d22c5a770e503750bb549152245325d915f350' },
  { id: 'after', repo: path.resolve(root, '../..'), target: path.join(root, 'target-first-after'), out: path.join(root, 'raw/first-alloc-after'), commit: '734395b7bb12bdd21f605c07de5426b2e4e717ef', tree: '89bfd1c99ff3f528b1d9d1e93c468029c3e82496' },
];
const required = [
  'rss_before_kib', 'rss_after_1_heap_kib', 'rss_after_8_heaps_kib', 'rss_after_64_heaps_kib',
  'commit_before_kib', 'commit_after_1_heap_kib', 'commit_after_8_heaps_kib', 'commit_after_64_heaps_kib',
  'first_alloc_latency_ns', 'heaps_claimed_high_water',
];

function parse(out) {
  const result = {};
  for (const line of out.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(\d+)$/.exec(line.trim());
    if (match) result[match[1]] = Number(match[2]);
  }
  for (const key of required) assert.ok(Number.isFinite(result[key]), `missing RESULT ${key}`);
  return result;
}
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
function stats(values) {
  return { n: values.length, min: Math.min(...values), median: median(values), max: Math.max(...values) };
}

for (const run of runs) {
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.commit);
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.tree);
  fs.mkdirSync(run.out, { recursive: true });
  const env = { ...process.env, CARGO_TARGET_DIR: run.target };
  const build = spawnSync('cargo', ['build', '--release', '--example', 'first_alloc_process', '--features', 'production internals'], { cwd: run.repo, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  fs.writeFileSync(path.join(run.out, 'build.log'), `${build.stdout ?? ''}${build.stderr ?? ''}\nexit=${build.status}\n`);
  assert.equal(build.status, 0, `${run.id}: example build failed`);
  const exe = path.join(run.target, 'release/examples/first_alloc_process.exe');
  const rows = [];
  for (let i = 0; i < samplesN; i++) {
    const child = spawnSync(exe, [], { cwd: run.repo, env, encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, timeout: 120_000 });
    assert.equal(child.status, 0, `${run.id} sample ${i}: process failed`);
    const record = { sample: i + 1, ...parse(child.stdout), stdout: child.stdout };
    rows.push(record);
    fs.writeFileSync(path.join(run.out, 'samples.json'), JSON.stringify({ run: run.id, commit: run.commit, tree: run.tree, features: 'production internals', sample_count: rows.length, samples: rows }, null, 2) + '\n');
  }
  const deltas = {
    first_alloc_latency_ns: rows.map((row) => row.first_alloc_latency_ns),
    rss_delta_1_heap_kib: rows.map((row) => row.rss_after_1_heap_kib - row.rss_before_kib),
    rss_delta_8_heaps_kib: rows.map((row) => row.rss_after_8_heaps_kib - row.rss_before_kib),
    rss_delta_64_heaps_kib: rows.map((row) => row.rss_after_64_heaps_kib - row.rss_before_kib),
    commit_delta_1_heap_kib: rows.map((row) => row.commit_after_1_heap_kib - row.commit_before_kib),
    commit_delta_8_heaps_kib: rows.map((row) => row.commit_after_8_heaps_kib - row.commit_before_kib),
    commit_delta_64_heaps_kib: rows.map((row) => row.commit_after_64_heaps_kib - row.commit_before_kib),
  };
  const summary = Object.fromEntries(Object.entries(deltas).map(([key, values]) => [key, stats(values)]));
  fs.writeFileSync(path.join(run.out, 'summary.json'), JSON.stringify({ run: run.id, commit: run.commit, tree: run.tree, features: 'production internals', sample_count: samplesN, summary }, null, 2) + '\n');
  console.log(`${run.id}: ${samplesN} fresh-process samples recorded`);
}
