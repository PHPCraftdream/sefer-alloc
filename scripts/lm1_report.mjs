#!/usr/bin/env node
// LM-1 report generator (measurement only): every number of docs/perf/LM1_LOSS_MAP.md comes from
// docs/perf/lm1/ through this script.
// Judge (Windows): per block and arm the minimum of ns_per_op over the repeats; per block
//   d = ln(sefer / mi); mean over blocks with a two-sided 95 % t-interval (df = blocks - 1).
// bench:table: the tables printed by scripts/bench-table-paired.mjs --from <prefix>; a cell's
//   sefer / mi time ratio is taken from the printed "N.NN x slower|faster" and checked against the
//   printed ns/pair columns.
// MT: median over the runs of each (workload, T) cell; time ratio sefer / mi = mi Mops / sefer Mops.
// Writes docs/perf/LM1_LOSS_MAP_summary.csv and docs/perf/lm1/tables.md.
import fs from 'node:fs';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const dir = 'docs/perf/lm1/';
const T975 = { 11: 2.201 };
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const t = T975[n - 1]; assert.ok(t, `no t quantile for n=${n}`);
  return { m, lo: m - t * sd / Math.sqrt(n), hi: m + t * sd / Math.sqrt(n) };
};
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const f3 = (v) => v.toFixed(3);
const LOSS = 1.03; // a cell counts as a loss from +3 % (the repo's loss threshold)
const klass = (r, lo, hi) => (lo !== undefined
  ? (lo > 1 && r >= LOSS ? 'LOSS' : hi < 1 ? 'WIN' : 'PARITY')
  : (r >= LOSS ? 'LOSS' : r <= 1 / LOSS ? 'WIN' : 'PARITY'));
const rows = []; // {src, cell, ratio, lo, hi, cls}
const attr = []; // attribution rows: CSV only, not part of the class counts
const md = [];
const nl = new RegExp('\\r?\\n');
const ident = fs.readFileSync(dir + 'ident.txt', 'utf8').split(nl)[0].match(/^commit ([0-9a-f]{40}) tree ([0-9a-f]{40})$/);
assert.ok(ident, 'ident.txt first line: commit and tree');
const manifest = JSON.parse(fs.readFileSync(dir + 'bt_s1.manifest.json', 'utf8'));
const platform = `${manifest.cpu}; ${manifest.os}`.replace(/,/g, ' ');
const csv = ['source,cell,ratio,ci_lo,ci_hi,class,samples,commit,tree,features,platform'];

// ---- judge ----
{
  const file = fs.existsSync(dir + 'judge_raw.jsonl.gz') ? zlib.gunzipSync(fs.readFileSync(dir + 'judge_raw.jsonl.gz')).toString() : fs.readFileSync(dir + 'judge_raw.jsonl', 'utf8');
  const recs = file.trim().split(nl).map((l) => JSON.parse(l));
  md.push('### Judge (Windows, g1_wild_judge, scale 1), sefer / mimalloc', '', '| scenario | blocks | sefer / mi (exp mean d) | 95 % CI | class | min sefer ms | min mi ms | peak RSS sefer MiB | peak RSS mi MiB | end RSS sefer MiB | end RSS mi MiB |', '|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|');
  for (const sc of ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large']) {
    const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
    const d = [];
    for (const b of blocks) {
      const by = (arm) => recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
      const S = by('sefer'), M = by('mi');
      assert.ok(S.length === 3 && M.length === 3, `${sc}/block ${b}: 3 repeats per arm`);
      assert.equal(new Set([...S, ...M].map((x) => x.r.checksum)).size, 1, `${sc}: same op sequence in both arms`);
      const mn = (runs) => Math.min(...runs.map((x) => x.r.ns_per_op));
      d.push(Math.log(mn(S) / mn(M)));
    }
    const D = ci(d), r = Math.exp(D.m), lo = Math.exp(D.lo), hi = Math.exp(D.hi), cls = klass(r, lo, hi);
    const ms = (arm) => Math.min(...recs.filter((x) => x.arm === arm && x.r.scenario === sc).map((x) => x.r.ns_per_op * x.r.ops / 1e6));
    const rss = (arm, k) => med(recs.filter((x) => x.arm === arm && x.r.scenario === sc).map((x) => x.r[k])) / 1048576;
    md.push(`| ${sc} | ${d.length} | ${f3(r)} | ${f3(lo)}–${f3(hi)} | ${cls} | ${ms('sefer').toFixed(0)} | ${ms('mi').toFixed(0)} | ${rss('sefer', 'peak_rss').toFixed(0)} | ${rss('mi', 'peak_rss').toFixed(0)} | ${rss('sefer', 'end_rss').toFixed(0)} | ${rss('mi', 'end_rss').toFixed(0)} |`);
    rows.push({ src: 'judge-win', cell: sc, ratio: r, lo, hi, cls, n: `${d.length} blocks x 3 repeats`, feat: 'production internals' });
  }
  md.push('', 'RSS columns are medians over all runs of the cell (peak_rss / end_rss as printed by the judge).', '');
}

// ---- release attribution: sefer_pool arm (pool of 1024 segments / 4 GiB, no release in free) ----
{
  const file = fs.existsSync(dir + 'pool_raw.jsonl.gz') ? zlib.gunzipSync(fs.readFileSync(dir + 'pool_raw.jsonl.gz')).toString() : fs.readFileSync(dir + 'pool_raw.jsonl', 'utf8');
  const recs = file.trim().split(nl).map((l) => JSON.parse(l));
  md.push('### Release attribution, sefer_pool arm (Windows)', '', '| scenario | blocks | sefer / mi | sefer_pool / mi | sefer_pool / sefer | share of the sefer-mi gap that disappears without release | seg_released sefer (median) | seg_released sefer_pool (max over runs) | config_conflicts (max over runs) |', '|---|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const sc of ['wild', 'phase', 'large']) {
    const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
    const ds = [], dp = [], dq = [];
    for (const b of blocks) {
      const by = (arm) => recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
      const S = by('sefer'), P = by('sefer_pool'), M = by('mi');
      assert.ok(S.length === 3 && P.length === 3 && M.length === 3, `${sc}/block ${b}: 3 repeats per arm`);
      assert.equal(new Set([...S, ...P, ...M].map((x) => x.r.checksum)).size, 1, `${sc}: same op sequence in all arms`);
      const mn = (runs) => Math.min(...runs.map((x) => x.r.ns_per_op));
      ds.push(Math.log(mn(S) / mn(M))); dp.push(Math.log(mn(P) / mn(M))); dq.push(Math.log(mn(P) / mn(S)));
    }
    const DS = ci(ds), DP = ci(dp), DQ = ci(dq);
    const share = (DS.m - DP.m) / DS.m;
    assert.ok(Math.abs(Math.exp(DP.m) / Math.exp(DS.m) - Math.exp(DQ.m)) < 1e-9, `${sc}: ratios compose`);
    const runsOf = (arm) => recs.filter((x) => x.arm === arm && x.r.scenario === sc);
    attr.push({ src: 'pool-attribution', cell: `${sc} sefer/mi`, ratio: Math.exp(DS.m), lo: Math.exp(DS.lo), hi: Math.exp(DS.hi), cls: 'n/a', n: `${ds.length} blocks x 3 repeats`, feat: 'production internals' }, { src: 'pool-attribution', cell: `${sc} sefer_pool/mi`, ratio: Math.exp(DP.m), lo: Math.exp(DP.lo), hi: Math.exp(DP.hi), cls: 'n/a', n: `${ds.length} blocks x 3 repeats`, feat: 'production internals' }, { src: 'pool-attribution', cell: `${sc} sefer_pool/sefer`, ratio: Math.exp(DQ.m), lo: Math.exp(DQ.lo), hi: Math.exp(DQ.hi), cls: 'n/a', n: `${ds.length} blocks x 3 repeats`, feat: 'production internals' });
    md.push(`| ${sc} | ${ds.length} | ${f3(Math.exp(DS.m))} (${f3(Math.exp(DS.lo))}–${f3(Math.exp(DS.hi))}) | ${f3(Math.exp(DP.m))} (${f3(Math.exp(DP.lo))}–${f3(Math.exp(DP.hi))}) | ${f3(Math.exp(DQ.m))} (${f3(Math.exp(DQ.lo))}–${f3(Math.exp(DQ.hi))}) | ${(share * 100).toFixed(0)} % (= (ln ${f3(Math.exp(DS.m))} - ln ${f3(Math.exp(DP.m))}) / ln ${f3(Math.exp(DS.m))}) | ${med(runsOf('sefer').map((x) => x.r.seg_released))} | ${Math.max(...runsOf('sefer_pool').map((x) => x.r.seg_released))} | ${Math.max(...recs.filter((x) => x.r.scenario === sc).map((x) => x.r.config_conflicts))} |`);
  }
  md.push('', 'The share is computed from the means of the per-block log ratios; it is meaningful only where sefer / mi is above 1. seg_released of the sefer_pool arm is the path-activation oracle (0 = no segment was released in free).', '');
}

// ---- bench:table paired sets ----
{
  const sets = ['s1', 's2'], cells = new Map();
  for (const s of sets) {
    const r = spawnSync('node', ['scripts/bench-table-paired.mjs', '--from', dir + 'bt_' + s], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    md.push(`### bench:table paired set ${s} (sefer vs mimalloc, one build)`, '', r.stdout.trim(), '');
    let grp = null;
    for (const l of r.stdout.split(nl)) {
      let m;
      if ((m = l.match(/^### (.+)$/))) { grp = m[1]; continue; }
      if (!(m = l.match(/^\| (\d+)B \| ([\d.]+) \([^)]*\) \| ([\d.]+) \([^)]*\) \| ([\d.]+) \([^)]*\) \| \**([\d.]+)× (slower|faster)\** \|$/))) continue;
      const sefer = +m[2], mi = +m[3], printed = +m[5];
      const ratio = m[6] === 'slower' ? printed : 1 / printed;
      assert.ok(Math.abs(sefer / mi - ratio) / ratio < 0.04, `${s} ${grp} ${m[1]}B: columns ${sefer}/${mi} vs printed ${printed}x ${m[6]}`);
      const key = `${grp.split(' (')[0]} ${m[1]} B`;
      if (!cells.has(key)) cells.set(key, {});
      cells.get(key)[s] = ratio;
    }
  }
  md.push('### bench:table cells, sefer / mi time ratio in both sets', '', '| cell | set s1 | set s2 | class |', '|---|---:|---:|---|');
  for (const [key, v] of cells) {
    assert.ok(v.s1 && v.s2, `${key}: both sets`);
    const lo = Math.min(v.s1, v.s2), hi = Math.max(v.s1, v.s2);
    const cls = lo >= LOSS ? 'LOSS' : hi <= 1 / LOSS ? 'WIN' : (hi >= LOSS ? 'MIXED' : 'PARITY');
    md.push(`| ${key} | ${f3(v.s1)} | ${f3(v.s2)} | ${cls} |`);
    rows.push({ src: 'bench:table', cell: key, ratio: (v.s1 + v.s2) / 2, cls, lo, hi, n: '2 sets x 3 processes x 200 rounds', feat: 'production' });
  }
  md.push('');
}

// ---- MT ----
{
  const cells = {}; let wl = null, runs = 0;
  for (const l of fs.readFileSync(dir + 'mt_raw.txt', 'utf8').split(nl)) {
    let m;
    if (/^== run \d/.test(l)) { runs++; continue; }
    if ((m = l.match(/workload: (\w+)/))) { wl = m[1]; continue; }
    if ((m = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M\s*$/))) {
      const c = (cells[`${wl} T=${m[1]}`] ??= { sefer: [], mi: [], sys: [] });
      c.sefer.push(+m[2]); c.mi.push(+m[3]); c.sys.push(+m[4]);
    }
  }
  md.push(`### MT (malloc_macro, ${runs} runs, unpinned), aggregate Mops/s, median over runs`, '', '| cell | sefer | mimalloc | System | sefer / mi time | class |', '|---|---:|---:|---:|---:|---|');
  for (const [key, c] of Object.entries(cells)) {
    assert.equal(c.sefer.length, runs, `${key}: one sample per run`);
    const ratio = med(c.mi) / med(c.sefer), cls = klass(ratio);
    md.push(`| ${key} | ${med(c.sefer).toFixed(2)} | ${med(c.mi).toFixed(2)} | ${med(c.sys).toFixed(2)} | ${f3(ratio)} | ${cls} |`);
    rows.push({ src: 'mt', cell: key, ratio, cls, n: `${runs} runs`, feat: 'production' });
  }
  md.push('');
}

// ---- loss list ----
const losses = rows.filter((r) => r.cls === 'LOSS' || r.cls === 'MIXED').sort((a, b) => b.ratio - a.ratio);
md.push('### Loss list (class LOSS or MIXED), sorted by sefer / mi time ratio', '', '| source | cell | sefer / mi | class |', '|---|---|---:|---|');
for (const r of losses) md.push(`| ${r.src} | ${r.cell} | ${f3(r.ratio)} | ${r.cls} |`);
md.push('', `Counts: ${rows.length} cells, ${rows.filter((r) => r.cls === 'LOSS').length} LOSS, ${rows.filter((r) => r.cls === 'MIXED').length} MIXED, ${rows.filter((r) => r.cls === 'PARITY').length} PARITY, ${rows.filter((r) => r.cls === 'WIN').length} WIN.`, '');
for (const r of [...rows, ...attr]) csv.push([r.src, r.cell, f3(r.ratio), r.lo === undefined ? '' : f3(r.lo), r.hi === undefined ? '' : f3(r.hi), r.cls, r.n, ident[1], ident[2], r.feat, platform].join(','));
fs.writeFileSync(dir + 'tables.md', md.join('\n'));
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  fs.writeFileSync('docs/perf/LM1_LOSS_MAP.md', fs.readFileSync(dir + 'doc_head.md', 'utf8') + md.join('\n') + '\n' + fs.readFileSync(dir + 'doc_tail.md', 'utf8'));
}
fs.writeFileSync('docs/perf/LM1_LOSS_MAP_summary.csv', csv.join('\n') + '\n');
console.log(md.join('\n'));
