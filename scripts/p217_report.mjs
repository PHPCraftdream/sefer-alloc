// P217: derives every table of docs/perf/P217_ADAPTIVE_PROMOTION.md and its summary CSV from the raw
// files in docs/perf/p217/. Usage: node scripts/p217_report.mjs [--csv <path>]
// Estimator (LM-1 / WILD-1 method): per block the minimum over repeats of ns_per_op per arm, the
// per-block ln ratio, exp of the mean with a 95 % t interval over blocks. Buckets: per block and arm the
// repeat with the smallest wall time, mean difference sefer - mi in ms with a 95 % t interval.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const DIR = 'docs/perf/p217';
const csvPath = (() => { const i = process.argv.indexOf('--csv'); return i > 0 ? process.argv[i + 1] : null; })();
const T975 = { 11: 2.201 };
const read = (f) => fs.readFileSync(path.join(DIR, f), 'utf8').trim().split(/\r?\n/);
const jsonl = (f) => read(f).map((l) => JSON.parse(l));
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
const tci = (v) => {
  const n = v.length, m = mean(v), t = T975[n - 1];
  assert.ok(t, `no t quantile for n=${n}`);
  const h = t * Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1)) / Math.sqrt(n);
  return { m, lo: m - h, hi: m + h };
};
const ratio = (logs) => { const c = tci(logs); return { r: Math.exp(c.m), lo: Math.exp(c.lo), hi: Math.exp(c.hi), n: logs.length }; };
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const f3 = (v) => v.toFixed(3);
const fr = (x) => `${f3(x.r)} (${f3(x.lo)}–${f3(x.hi)})`;
const klass = (x) => (x.lo > 1 ? 'LOSS' : x.hi < 1 ? 'WIN' : 'PARITY');
const mib = (b) => (b / 1048576).toFixed(0);
const csv = [['section', 'scenario_or_cell', 'metric', 'value', 'ci_lo', 'ci_hi', 'n']];
const row = (...a) => csv.push(a.map(String));

// Per-scenario paired ln ratios of arm a over arm b; asserts repeats and equal op sequences.
function paired(recs, sc, a, b, repeats) {
  const blocks = [...new Set(recs.filter((x) => x.r.scenario === sc).map((x) => x.block))].sort((p, q) => p - q);
  return blocks.map((blk) => {
    const by = (arm) => recs.filter((x) => x.block === blk && x.arm === arm && x.r.scenario === sc);
    const A = by(a), B = by(b);
    assert.equal(A.length, repeats, `${sc}/${blk}/${a}: repeats`);
    assert.equal(B.length, repeats, `${sc}/${blk}/${b}: repeats`);
    assert.equal(new Set([...A, ...B].map((x) => x.r.checksum)).size, 1, `${sc}/${blk}: same op sequence`);
    const mn = (R) => Math.min(...R.map((x) => x.r.ns_per_op));
    return Math.log(mn(A) / mn(B));
  });
}

// 1. LM-1 loss map on main (sefer vs mi, same binary).
const lm = jsonl('lm1_judge_raw.jsonl');
const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large'];
console.log('### 1. Loss map on main (`g1_wild_judge`, sefer/mi)\n');
console.log('| scenario | blocks | sefer/mi (95 % CI) | class | peak RSS MiB sefer/mi | end RSS MiB sefer/mi |');
console.log('|---|---:|---|---|---|---|');
const lmRes = {};
for (const sc of SCEN) {
  const x = ratio(paired(lm, sc, 'sefer', 'mi', 3));
  lmRes[sc] = x;
  const rss = (arm, k) => mib(med(lm.filter((y) => y.arm === arm && y.r.scenario === sc).map((y) => y.r[k])));
  console.log(`| ${sc} | ${x.n} | ${fr(x)} | ${klass(x)} | ${rss('sefer', 'peak_rss')}/${rss('mi', 'peak_rss')} | ${rss('sefer', 'end_rss')}/${rss('mi', 'end_rss')} |`);
  row('lm1', sc, 'sefer_over_mi', f3(x.r), f3(x.lo), f3(x.hi), x.n);
}

// 2. MT (malloc_macro, 6 runs, median throughput).
const cells = {}; let wl = null, runs = 0;
for (const l of read('mt_raw.txt')) {
  let m;
  if (/^== run \d/.test(l)) { runs++; continue; }
  if ((m = l.match(/workload: (\w+)/))) { wl = m[1]; continue; }
  if ((m = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M\s*$/))) {
    const c = (cells[`${wl} T=${m[1]}`] ??= { sefer: [], mi: [] });
    c.sefer.push(+m[2]); c.mi.push(+m[3]);
  }
}
assert.ok(runs > 0, 'MT runs present');
console.log(`\n### 2. Multi-thread (\`malloc_macro\`, ${runs} runs, medians)\n`);
console.log('| cell | sefer Mops | mi Mops | sefer/mi time |');
console.log('|---|---:|---:|---:|');
for (const [key, c] of Object.entries(cells)) {
  assert.equal(c.sefer.length, runs, `${key}: one sample per run`);
  const t = med(c.mi) / med(c.sefer);
  console.log(`| ${key} | ${med(c.sefer).toFixed(2)} | ${med(c.mi).toFixed(2)} | ${f3(t)} |`);
  row('mt', key, 'sefer_over_mi_time', f3(t), '', '', runs);
}

// 3. Bucket profile (WJ_PROF=1).
const pr = jsonl('prof_raw.jsonl');
const BUCKETS = ['burst_alloc', 'burst_free', 'large_alloc', 'large_free', 'large_realloc', 'small_realloc', 'teardown'];
console.log('\n### 3. Where the time goes (`WJ_PROF=1`, sefer − mi, ms)\n');
for (const sc of [...new Set(pr.map((x) => x.r.scenario))]) {
  const blocks = [...new Set(pr.filter((x) => x.r.scenario === sc).map((x) => x.block))];
  const pick = (b, arm) => pr.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc).sort((p, q) => p.r.wall_ns - q.r.wall_ns)[0].r;
  const d = {}; const add = (k, v) => (d[k] ??= []).push(v);
  for (const b of blocks) {
    const S = pick(b, 'sefer'), M = pick(b, 'mi');
    assert.equal(S.checksum, M.checksum, `${sc}/${b}: same op sequence`);
    let rs = S.wall_ns, rm = M.wall_ns;
    for (const k of BUCKETS) {
      const s = S.prof[k] ? S.prof[k][0] : 0, m = M.prof[k] ? M.prof[k][0] : 0;
      rs -= s; rm -= m;
      add(k, (s - m) / 1e6); add(`${k}:s`, s / 1e6); add(`${k}:m`, m / 1e6);
    }
    add('rest', (rs - rm) / 1e6); add('rest:s', rs / 1e6); add('rest:m', rm / 1e6);
    add('wall', (S.wall_ns - M.wall_ns) / 1e6); add('wall:s', S.wall_ns / 1e6); add('wall:m', M.wall_ns / 1e6);
    add('kernel', S.kernel_ms - M.kernel_ms);
  }
  // The buckets and the rest partition the wall time.
  const sumDiff = [...BUCKETS, 'rest'].reduce((a, k) => a + mean(d[k]), 0);
  assert.ok(Math.abs(sumDiff - mean(d.wall)) < 1e-6, `${sc}: buckets + rest = wall`);
  console.log(`**${sc}** (${blocks.length} blocks; kernel-time difference ${mean(d.kernel).toFixed(1)} ms)\n`);
  console.log('| bucket | sefer ms | mi ms | diff ms (95 % CI) |');
  console.log('|---|---:|---:|---|');
  for (const k of [...BUCKETS, 'rest', 'wall']) {
    const c = tci(d[k]);
    if (k !== 'wall' && k !== 'rest' && Math.abs(c.m) < 0.05 && med(d[`${k}:s`]) < 0.05) continue;
    console.log(`| ${k} | ${med(d[`${k}:s`]).toFixed(1)} | ${med(d[`${k}:m`]).toFixed(1)} | ${c.m.toFixed(1)} (${c.lo.toFixed(1)}..${c.hi.toFixed(1)}) |`);
    row('buckets', sc, `${k}_diff_ms`, c.m.toFixed(1), c.lo.toFixed(1), c.hi.toFixed(1), blocks.length);
  }
  row('buckets', sc, 'kernel_diff_ms', mean(d.kernel).toFixed(1), '', '', blocks.length);
  console.log('');
}

// 4. A/B: base (production internals) vs x (+ large-cache-adaptive), mi arm from the base binary.
const ab = jsonl('ab_raw.jsonl');
console.log('### 4. A/B: `large-cache-adaptive` (x) against `production` (base) and mimalloc\n');
console.log('| scenario | x/base | base/mi | x/mi | seg_released base/x (median) | peak RSS MiB base/x/mi | end RSS MiB base/x/mi |');
console.log('|---|---|---|---|---|---|---|');
for (const sc of [...new Set(ab.map((x) => x.r.scenario))]) {
  const xb = ratio(paired(ab, sc, 'x', 'base', 3)), bm = ratio(paired(ab, sc, 'base', 'mi', 3)), xm = ratio(paired(ab, sc, 'x', 'mi', 3));
  // ln(x/mi) = ln(x/base) + ln(base/mi) per block, so the means add up.
  assert.ok(Math.abs(Math.log(xm.r) - Math.log(xb.r) - Math.log(bm.r)) < 1e-9, `${sc}: ratio chain`);
  const m = (arm, k) => med(ab.filter((y) => y.arm === arm && y.r.scenario === sc).map((y) => y.r[k]));
  for (const arm of ['base', 'x']) assert.equal(m(arm, 'config_conflicts'), 0, `${sc}/${arm}: config conflicts`);
  console.log(`| ${sc} | ${fr(xb)} | ${fr(bm)} | ${fr(xm)} | ${m('base', 'seg_released')}/${m('x', 'seg_released')} | ${mib(m('base', 'peak_rss'))}/${mib(m('x', 'peak_rss'))}/${mib(m('mi', 'peak_rss'))} | ${mib(m('base', 'end_rss'))}/${mib(m('x', 'end_rss'))}/${mib(m('mi', 'end_rss'))} |`);
  row('ab', sc, 'x_over_base', f3(xb.r), f3(xb.lo), f3(xb.hi), xb.n);
  row('ab', sc, 'base_over_mi', f3(bm.r), f3(bm.lo), f3(bm.hi), bm.n);
  row('ab', sc, 'x_over_mi', f3(xm.r), f3(xm.lo), f3(xm.hi), xm.n);
  row('ab', sc, 'seg_released_base', m('base', 'seg_released'), '', '', '');
  row('ab', sc, 'seg_released_x', m('x', 'seg_released'), '', '', '');
  row('ab', sc, 'peak_rss_mib_base', mib(m('base', 'peak_rss')), '', '', '');
  row('ab', sc, 'peak_rss_mib_x', mib(m('x', 'peak_rss')), '', '', '');
}

const id = JSON.parse(fs.readFileSync(path.join(DIR, 'identity.json'), 'utf8'));
assert.match(id.commit_sha, /^[0-9a-f]{40}$/, 'commit sha');
for (const k of ['base_sha256', 'x_sha256']) assert.match(id[k], /^[0-9a-f]{64}$/, k);
const header = Object.entries(id).map(([k, v]) => `# ${k}=${v}`);
if (csvPath) fs.writeFileSync(csvPath, [...header, ...csv.map((r) => r.join(','))].join('\n') + '\n');
