// Diagnostic (not a gate input): dispersion of the R-live first-call p50 metric from a gate2b run's samples_scalar.csv.
// Usage: node scripts/r19_prf04_live_dispersion.mjs <run dir>
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const dir = resolve(process.argv[2] ?? '');
const lines = readFileSync(resolve(dir, 'samples_scalar.csv'), 'utf8').trim().split('\n');
const head = lines[0].split(',');
const rows = lines.slice(1).map(l => Object.fromEntries(l.split(',').map((v, i) => [head[i], v])));
const live = rows.filter(r => r.mode === 'live' && r.diagnostic === 'false' && r.observer === 'true');
const num = r => Number(r.first_call_p50);
const q = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s[Math.ceil(p * s.length) - 1]; };
const mean = xs => xs.reduce((a, b) => a + b, 0) / xs.length;
const series = name => ({ A: live.filter(r => r.series === name && r.arm === 'A').map(num), B: live.filter(r => r.series === name && r.arm === 'B').map(num) });
const ab = series('ab'), aa = series('aa');
assert.equal(ab.A.length, 20 * 2 === 40 ? 40 : -1); assert.equal(ab.B.length, 40);
// Deterministic bootstrap (fixed LCG) of the difference of medians, unpaired.
let state = 0x9e3779b9;
const rnd = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 2 ** 32;
const draw = xs => xs.map(() => xs[Math.floor(rnd() * xs.length)]);
const med0 = xs => { const t = [...xs].sort((a, b) => a - b), m = t.length >> 1; return t.length % 2 ? t[m] : (t[m - 1] + t[m]) / 2; };
const diffs = Array.from({ length: 4000 }, () => med0(draw(ab.B)) - med0(draw(ab.A))).sort((a, b) => a - b);
const lo = diffs[Math.floor(0.025 * diffs.length)], hi = diffs[Math.ceil(0.975 * diffs.length) - 1];
// Same median as the gate (mean of the two middle values for an even count).
const med = xs => { const t = [...xs].sort((a, b) => a - b), m = t.length >> 1; return t.length % 2 ? t[m] : (t[m - 1] + t[m]) / 2; };
const out = {
  metric: 'first_call_p50 of the 64-thread live wave, ns, per process (observer builds)',
  ab: { n: ab.A.length, medianA: med(ab.A), medianB: med(ab.B), q1A: q(ab.A, .25), q3A: q(ab.A, .75), q1B: q(ab.B, .25), q3B: q(ab.B, .75), meanA: Math.round(mean(ab.A)), meanB: Math.round(mean(ab.B)), maxA: Math.max(...ab.A), maxB: Math.max(...ab.B) },
  aa: { n: aa.A.length, median: med(aa.A), q1: q(aa.A, .25), q3: q(aa.A, .75) },
  medianDifference: { observed: med(ab.B) - med(ab.A), bootstrap95: [lo, hi], resamples: diffs.length, seed: '0x9e3779b9 LCG' },
  note: 'Diagnostic only. The frozen KILL-1 rule uses |median(B)-median(A)|/median(A) > 5 % when the A/A band is <= 5 %.'
};
console.log(JSON.stringify(out, null, 2));
