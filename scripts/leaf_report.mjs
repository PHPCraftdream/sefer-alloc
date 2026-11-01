#!/usr/bin/env node
// WG-LEAF report generator (measurement only): every number of
// docs/perf/G1_LEAF_CHUNKS.md comes from docs/perf/leaf/ through this script.
// Judge: per block and arm the minimum of ns_per_op over the repeats, per block
// d = ln(x / base) and the A/A control a = ln(base2 / base), the mean over blocks
// with a two-sided 95 % t-interval (df = blocks - 1). bench:table: the output of
// scripts/g1_magnitude_verdict.mjs on each paired set. MT: median of the
// samples per arm. Writes docs/perf/G1_LEAF_CHUNKS_summary.csv and
// docs/perf/leaf/tables.md.
import fs from 'node:fs';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const dir = 'docs/perf/leaf/';
const T975 = { 11: 2.201 };
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const t = T975[n - 1]; assert.ok(t, `no t quantile for n=${n}`);
  return { m, lo: m - t * sd / Math.sqrt(n), hi: m + t * sd / Math.sqrt(n) };
};
const e = (v) => Math.exp(v).toFixed(3);
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const md = [], csv = ['platform,scenario,blocks,x_over_base,ci_lo,ci_hi,aa_base2_over_base,aa_lo,aa_hi,min_base_ms,min_x_ms'];

for (const [plat, file] of [['windows', 'win_raw.jsonl.gz'], ['linux (WSL), scale 0.5', 'linux_raw.jsonl.gz']]) {
  const recs = zlib.gunzipSync(fs.readFileSync(dir + file)).toString().trim().split(/\r?\n/).map((l) => JSON.parse(l));
  md.push(`### Judge, ${plat}`, '', '| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | min base ms | min x ms |', '|---|---:|---:|---|---:|---|---:|---:|');
  for (const sc of ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large']) {
    const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
    const by = (b, arm) => recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
    const d = [], a = [];
    for (const b of blocks) {
      const [B, B2, X] = ['base', 'base2', 'x'].map((arm) => by(b, arm));
      assert.ok(B.length === 3 && B2.length === 3 && X.length === 3, `${plat}/${sc}/block ${b}: 3 repeats per arm`);
      assert.equal(new Set([...B, ...B2, ...X].map((x) => x.r.checksum)).size, 1, `${plat}/${sc}: same op sequence`);
      const mn = (runs) => Math.min(...runs.map((x) => x.r.ns_per_op));
      d.push(Math.log(mn(X) / mn(B))); a.push(Math.log(mn(B2) / mn(B)));
    }
    const D = ci(d), A = ci(a);
    const ms = (arm) => Math.min(...recs.filter((x) => x.arm === arm && x.r.scenario === sc).map((x) => x.r.ns_per_op * x.r.ops / 1e6));
    md.push(`| ${sc} | ${d.length} | ${e(D.m)} | ${e(D.lo)}–${e(D.hi)} | ${e(A.m)} | ${e(A.lo)}–${e(A.hi)} | ${ms('base').toFixed(0)} | ${ms('x').toFixed(0)} |`);
    csv.push([plat.split(' ')[0], sc, d.length, e(D.m), e(D.lo), e(D.hi), e(A.m), e(A.lo), e(A.hi), ms('base').toFixed(0), ms('x').toFixed(0)].join(','));
  }
  md.push('');
}

for (const s of ['s1', 's2']) {
  const r = spawnSync('node', ['scripts/g1_magnitude_verdict.mjs', dir + 'bt', `${s}base`, `${s}leaf`, 'leaf'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  md.push(`### bench:table paired set ${s} (order base, leaf, leaf, base; aligned code)`, '', r.stdout.trim(), '');
}

{
  const cells = {}; let cur = null, wl = null;
  for (const l of fs.readFileSync(dir + 'mt_raw.txt', 'utf8').split(/\r?\n/)) {
    let m;
    if ((m = l.match(/^== (mainb|leaf) \d/))) { cur = m[1]; continue; }
    if ((m = l.match(/workload: (\w+)/))) { wl = m[1]; continue; }
    if (cur && (m = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M/))) ((cells[`${wl} T=${m[1]}`] ??= { mainb: [], leaf: [] })[cur]).push(+m[2]);
  }
  md.push('### MT control (malloc_macro, aggregate Mops/s, 6 samples per arm, ABBA x 3)', '', '| cell | base median | leaf median | leaf / base (medians) | base samples | leaf samples |', '|---|---:|---:|---:|---|---|');
  for (const [k, v] of Object.entries(cells)) {
    assert.equal(v.mainb.length, 6); assert.equal(v.leaf.length, 6);
    md.push(`| ${k} | ${med(v.mainb).toFixed(1)} | ${med(v.leaf).toFixed(1)} | ${(med(v.leaf) / med(v.mainb)).toFixed(3)} | ${v.mainb.join(' ')} | ${v.leaf.join(' ')} |`);
  }
  md.push('');
}
{
  const recs = zlib.gunzipSync(fs.readFileSync(dir + 'confirm_raw.jsonl.gz')).toString().trim().split(/\r?\n/).map((l) => JSON.parse(l));
  md.push('### Confirmation on the final commit (windows; arms: base, base2 = the binary measured above, x = the final-commit binary)', '',
    '| scenario | measured leaf / base | 95 % CI | final leaf / base | 95 % CI | final / measured | 95 % CI |', '|---|---:|---|---:|---|---:|---|');
  for (const sc of ['grow64', 'grow1k', 'regrow', 'growmix']) {
    const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
    assert.equal(blocks.length, 12);
    const mn = (b, arm) => { const v = recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc); assert.equal(v.length, 3); return Math.min(...v.map((x) => x.r.ns_per_op)); };
    const A = ci(blocks.map((b) => Math.log(mn(b, 'base2') / mn(b, 'base')))), D = ci(blocks.map((b) => Math.log(mn(b, 'x') / mn(b, 'base')))), F = ci(blocks.map((b) => Math.log(mn(b, 'x') / mn(b, 'base2'))));
    md.push(`| ${sc} | ${e(A.m)} | ${e(A.lo)}–${e(A.hi)} | ${e(D.m)} | ${e(D.lo)}–${e(D.hi)} | ${e(F.m)} | ${e(F.lo)}–${e(F.hi)} |`);
    csv.push(['windows-confirm', sc, 12, e(D.m), e(D.lo), e(D.hi), e(A.m), e(A.lo), e(A.hi), '', ''].join(','));
  }
  md.push('');
}
{
  const rows = fs.readFileSync(dir + 'share_raw.jsonl', 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l).r);
  md.push('### Motivation: time inside the per-leaf System requests (uncommitted timers, docs/perf/leaf/share_ident.txt), windows, 5 runs per cell, medians', '',
    '| scenario | arm | total ms | leaf allocs | leaf alloc ms (raw) | timer pair ns | leaf alloc ms net = raw - allocs x timer | net / total | leaf frees | leaf free ms | free / total |', '|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const sc of ['grow64', 'grow1k', 'regrow', 'growmix']) for (const arm of ['sefer', 'sefer_pool']) {
    const v = rows.filter((r) => r.scenario === sc && r.arm === arm); assert.equal(v.length, 5);
    const tot = med(v.map((r) => r.ns_per_op * r.ops / 1e6)), n = med(v.map((r) => r.leaf_alloc_n)), raw = med(v.map((r) => r.leaf_alloc_ms));
    const tm = med(v.map((r) => r.timer_ns)), net = med(v.map((r) => r.leaf_alloc_ms - r.leaf_alloc_n * r.timer_ns / 1e6));
    const fn = med(v.map((r) => r.leaf_free_n)), fms = med(v.map((r) => r.leaf_free_ms));
    md.push(`| ${sc} | ${arm} | ${tot.toFixed(0)} | ${n} | ${raw} | ${tm} | ${net.toFixed(1)} | ${(net / tot * 100).toFixed(1)} % (${net.toFixed(1)} / ${tot.toFixed(0)} ms) | ${fn} | ${fms} | ${(fms / tot * 100).toFixed(1)} % (${fms} / ${tot.toFixed(0)} ms) |`);
  }
  md.push('');
}
fs.writeFileSync('docs/perf/G1_LEAF_CHUNKS_summary.csv', csv.join('\n') + '\n');
fs.writeFileSync(dir + 'tables.md', md.join('\n'));
console.log(md.join('\n'));
