// Tier-3 excerpt for a gate2b run directory: samples.json is too large to commit, so derive a
// scalar per-slot CSV from it and prove the CSV re-derives every verdict headline.
// Usage: node scripts/r19_prf04_gate2b_excerpt.mjs <run dir>   (needs samples.json + verdict.json)
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';

const dir = resolve(process.argv[2] ?? '');
const raw = readFileSync(resolve(dir, 'samples.json'));
const rows = JSON.parse(raw.toString('utf8'));
const verdict = JSON.parse(readFileSync(resolve(dir, 'verdict.json'), 'utf8'));
assert.equal(rows.length, 492, 'exact schedule cardinality');
const median = xs => { const s = [...xs].sort((a, b) => a - b), m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const nearestRank = (xs, q) => [...xs].sort((a, b) => a - b)[Math.ceil(q * xs.length) - 1];
const latency = (r, q) => r.first_call_samples ? nearestRank(r.first_call_samples.map(v => v[2]), q) : null;

const columns = ['index', 'arm', 'mode', 'k', 'w', 'observer', 'diagnostic', 'series', 'repetition', 'position',
  'count_delta', 'commit_idle', 'rss_idle', 'wall_ns', 'first_call_p50', 'first_call_p99', 'live_valid'];
const table = rows.map(r => ({ ...r, first_call_p50: latency(r, 0.5), first_call_p99: latency(r, 0.99) }));
const csv = columns.join(',') + '\n' + table.map(r => columns.map(c => r[c] ?? '').join(',')).join('\n') + '\n';

// Re-derive each headline numerator/denominator from the scalar columns only.
const pick = f => table.filter(f);
const near = (a, b) => assert(Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b)), `${a} != ${b}`);
const axis = name => verdict.axes.find(a => a.axis === name);
const commit = arm => median(pick(r => r.arm === arm && r.mode === 'wave' && r.k === 32 && r.w === 128 && !r.observer && !r.diagnostic && r.series === 'wave').map(r => r.commit_idle));
near(commit('A') - commit('B'), axis('commit').numerator); near(commit('A'), axis('commit').denominator);
const waveP99 = arm => median(pick(r => r.arm === arm && r.mode === 'wave' && r.k === 32 && r.w === 128 && r.observer && !r.diagnostic && r.series === 'wave').map(r => r.first_call_p99));
near(waveP99('B') - waveP99('A'), axis('wave_p99').numerator); near(waveP99('A'), axis('wave_p99').denominator);
for (const [mode, metric, name] of [['live', 'first_call_p50', 'live'], ['churn', 'wall_ns', 'churn']]) {
  const ab = pick(r => r.mode === mode && r.series === 'ab' && !r.diagnostic);
  assert.equal(ab.length, 80);
  const a = [], b = [];
  for (let i = 0; i < 20; i++) { const g = ab.slice(i * 4, i * 4 + 4); a.push(g[0][metric], g[3][metric]); b.push(g[1][metric], g[2][metric]); }
  near(median(b) - median(a), axis(name).numerator); near(median(a), axis(name).denominator);
}
const out = resolve(dir, 'samples_scalar.csv');
assert(!existsSync(out), 'no overwrite');
writeFileSync(out, csv, { flag: 'wx' });
console.log(`samples.json sha256=${createHash('sha256').update(raw).digest('hex')} bytes=${raw.length}`);
console.log(`samples_scalar.csv bytes=${Buffer.byteLength(csv)} rows=${rows.length}; all four axis headlines re-derived from it`);
