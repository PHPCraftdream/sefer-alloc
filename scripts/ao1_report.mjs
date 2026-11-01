#!/usr/bin/env node
// AO-1 confirmation campaign report (measurement only). Reads
// docs/perf/ao1/{win,linux}<C>_raw.jsonl (docs/perf/ao1/run.mjs; campaign C:
// '' for 1, '2' for 2), takes per block and arm the minimum over repeats,
// computes per block d = ln(x/base) and the A/A control a = ln(base2/base), the mean over
// blocks with a two-sided 95 % t-interval, and applies the gate of
// docs/perf/AO1_ASCENDING_ISSUE_PREREG.md. Usage: node scripts/ao1_report.mjs [--campaign 1|2] [--csv FILE]
import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const T975 = { 3: 3.182, 5: 2.571, 7: 2.365, 9: 2.262, 11: 2.201, 15: 2.131 };
const dir = join(REPO_ROOT, 'docs', 'perf', 'ao1');
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const t = T975[n - 1]; assert.ok(t, `no t quantile for n=${n}`);
  return { m, lo: m - t * sd / Math.sqrt(n), hi: m + t * sd / Math.sqrt(n), n };
};
const ci_ = process.argv.indexOf('--campaign');
const camp = ci_ >= 0 && process.argv[ci_ + 1] !== '1' ? process.argv[ci_ + 1] : '';
const rows = [];
for (const plat of ['win', 'linux']) {
  const f = join(dir, `${plat}${camp}_raw.jsonl`);
  if (!existsSync(f)) continue;
  const recs = readFileSync(f, 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l));
  for (const sc of [...new Set(recs.map((x) => x.r.scenario))]) {
    const by = (b, arm) => recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
    const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
    const d = [], a = [];
    for (const b of blocks) {
      const [B, B2, X] = ['base', 'base2', 'x'].map((arm) => by(b, arm));
      assert.ok(B.length >= 1 && B.length === B2.length && B.length === X.length, `${plat}/${sc}/block ${b}: equal repeats per arm`);
      assert.equal(new Set([...B, ...B2, ...X].map((x) => x.r.checksum)).size, 1, `${plat}/${sc}: same op sequence`);
      const mn = (runs) => Math.min(...runs.map((x) => x.r.ns_per_op));
      d.push(Math.log(mn(X) / mn(B)));
      a.push(Math.log(mn(B2) / mn(B)));
    }
    rows.push({ plat, sc, d: ci(d), a: ci(a) });
  }
}
const e = (v) => Math.exp(v).toFixed(3);
console.log('| Platform | Scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | cell verdict |');
console.log('|---|---|---:|---:|---|---:|---|---|');
let aaOk = true, target = null, kill = false, hold = false;
for (const r of rows) {
  const aa = r.a.lo <= 0 && r.a.hi >= 0; aaOk &&= aa;
  let v;
  // Campaign 1: target Windows grow64. Amendment 1 (campaign 2): grow64 on
  // both platforms and Windows grow1k.
  const isTarget = (r.plat === 'win' && r.sc === 'grow64') || (camp === '2' && ((r.plat === 'linux' && r.sc === 'grow64') || (r.plat === 'win' && r.sc === 'grow1k')));
  if (isTarget) {
    const met = Math.exp(r.d.m) <= 0.90 && Math.exp(r.d.hi) < 1.0;
    target = (target ?? true) && met; v = met ? 'target met' : 'target missed';
  } else if (Math.exp(r.d.lo) > 1.03) { kill = true; v = 'KILL (> +3 %)'; }
  else if (Math.exp(r.d.hi) > 1.03) { hold = true; v = 'HOLD (CI crosses +3 %)'; }
  else v = 'non-inferior';
  if (!aa) v += ', A/A FAIL';
  console.log(`| ${r.plat} | ${r.sc} | ${r.d.n} | ${e(r.d.m)} | ${e(r.d.lo)}–${e(r.d.hi)} | ${e(r.a.m)} | ${e(r.a.lo)}–${e(r.a.hi)} | ${v} |`);
}
const verdict = !aaOk ? 'HOLD (A/A control failed: campaign void)' : kill ? 'NO-GO' : target !== true ? 'NO-GO (target missed)' : hold ? 'HOLD' : 'GO (judge part; bench:table and MT controls separate)';
console.log(`\nJudge gate: ${verdict}`);
const i = process.argv.indexOf('--csv');
if (i >= 0) writeFileSync(process.argv[i + 1], ['platform,scenario,blocks,x_over_base,ci_lo,ci_hi,aa,aa_lo,aa_hi',
  ...rows.map((r) => [r.plat, r.sc, r.d.n, e(r.d.m), e(r.d.lo), e(r.d.hi), e(r.a.m), e(r.a.lo), e(r.a.hi)].join(','))].join('\n') + '\n');
