#!/usr/bin/env node
// WILD-2 report generator (measurement only): every number of docs/perf/WILD2_LANE_SECOND_CHANCE.md comes
// from docs/perf/wild2/ through this script, with the rules of docs/perf/wild2/PREREG.md applied mechanically.
// Judge: per block and arm the minimum of ns_per_op over the repeats, per block d = ln(x / base) and the A/A
// control a = ln(base2 / base), mean over blocks with a two-sided 95 % t-interval (df = blocks - 1).
// bench:table: scripts/g1_magnitude_verdict.mjs on each paired set. MT: median of the samples per arm.
// Writes docs/perf/WILD2_LANE_SECOND_CHANCE_summary.csv, docs/perf/wild2/tables.md and, when doc_head.md and
// doc_tail.md exist, docs/perf/WILD2_LANE_SECOND_CHANCE.md.
import fs from 'node:fs';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const dir = 'docs/perf/wild2/';
const nl = new RegExp('\\r?\\n');
const T975 = { 11: 2.201, 15: 2.131, 19: 2.093, 23: 2.069 };
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const t = T975[n - 1]; assert.ok(t, `no t quantile for n=${n}`);
  return { m, lo: m - t * sd / Math.sqrt(n), hi: m + t * sd / Math.sqrt(n) };
};
const e = (v) => Math.exp(v).toFixed(3);
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const gz = (f) => zlib.gunzipSync(fs.readFileSync(dir + f)).toString().trim().split(nl).map((l) => JSON.parse(l));
const ident = fs.readFileSync(dir + 'ident.txt', 'utf8').split(nl);
const xCommit = ident.find((l) => l.startsWith('x '));
assert.ok(xCommit, 'ident.txt names the x commit');
const md = [], csv = ['campaign,platform,scenario,blocks,x_over_base,ci_lo,ci_hi,aa_base2_over_base,aa_lo,aa_hi,readable,rule,verdict'];

// Rules of PREREG.md section 3. `readable`: the A/A interval includes 1.
const judgeVerdict = (platform, sc, D, A) => {
  const readable = A.lo <= 0 && A.hi >= 0;
  const mean = Math.exp(D.m), hi = Math.exp(D.hi);
  let rule, verdict;
  if (sc === 'wild') {
    const goMean = platform === 'windows' ? 0.97 : 0.98;
    rule = platform === 'windows' ? 'T1' : 'T2';
    verdict = !readable ? 'UNREADABLE (A/A excludes 1)' : (mean <= goMean && hi < 1 ? 'PASS' : 'FAIL');
  } else {
    rule = platform === 'windows' ? 'C1' : 'C2';
    verdict = !readable ? 'UNREADABLE (A/A excludes 1)' : mean > 1.03 ? 'KILL' : (hi > 1.03 && mean <= 1.0 ? 'HOLD' : 'ok');
  }
  return { readable, rule, verdict };
};

const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large', 'burst'];
const judge = (title, campaign, platform, file) => {
  const recs = gz(file);
  const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
  md.push(`### ${title}`, '', `| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | rule | verdict | min base ms | min x ms |`, '|---|---:|---:|---:|---:|---:|---|---|---:|---:|');
  for (const sc of SCEN) {
    const by = (b, arm) => recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
    const d = [], a = [];
    let reps = 0;
    for (const b of blocks) {
      const [B, B2, X] = ['base', 'base2', 'x'].map((arm) => by(b, arm));
      assert.ok(B.length === B2.length && B.length === X.length && B.length >= 3, `${title}/${sc}/block ${b}: equal repeats per arm`);
      reps = B.length;
      assert.equal(new Set([...B, ...B2, ...X].map((x) => x.r.checksum)).size, 1, `${title}/${sc}: same op sequence in all arms`);
      const mn = (runs) => Math.min(...runs.map((x) => x.r.ns_per_op));
      d.push(Math.log(mn(X) / mn(B))); a.push(Math.log(mn(B2) / mn(B)));
    }
    const D = ci(d), A = ci(a), v = judgeVerdict(platform, sc, D, A);
    const ms = (arm) => Math.min(...recs.filter((x) => x.arm === arm && x.r.scenario === sc).map((x) => x.r.ns_per_op * x.r.ops / 1e6));
    md.push(`| ${sc} | ${d.length} (x${reps}) | ${e(D.m)} | ${e(D.lo)}–${e(D.hi)} | ${e(A.m)} | ${e(A.lo)}–${e(A.hi)} | ${v.rule} | ${v.verdict} | ${ms('base').toFixed(0)} | ${ms('x').toFixed(0)} |`);
    csv.push([campaign, platform, sc, d.length, e(D.m), e(D.lo), e(D.hi), e(A.m), e(A.lo), e(A.hi), v.readable, v.rule, v.verdict.split(' ')[0]].join(','));
  }
  md.push('');
};
judge('Judge, Windows, campaign 1 (preregistered design: 12 blocks, minimum of 3 repeats)', 'c1', 'windows', 'win_raw.jsonl.gz');
if (fs.existsSync(dir + 'linux_raw.jsonl.gz')) judge('Judge, Linux (WSL2), campaign 1 (wild/phase/large/burst at scale 1, GiB scenarios at scale 0.5)', 'c1', 'linux', 'linux_raw.jsonl.gz');
if (fs.existsSync(dir + 'win2_raw.jsonl.gz')) judge('Judge, Windows, confirmation campaign (Amendment 1: more blocks and repeats, machine otherwise idle)', 'c2', 'windows', 'win2_raw.jsonl.gz');

// ---- bench:table paired sets ----
for (const s of ['s1', 's2']) {
  if (!fs.existsSync(dir + `bt/${s}base_r1.csv`)) continue;
  const r = spawnSync('node', ['scripts/g1_magnitude_verdict.mjs', dir + 'bt', `${s}base`, `${s}x`, 'x'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  md.push(`### bench:table paired set ${s} (order base, x, x, base; aligned code)`, '', r.stdout.trim(), '');
}

// ---- MT control ----
if (fs.existsSync(dir + 'mt_raw.txt')) {
  const cells = {}; let cur = null, wl = null;
  for (const l of fs.readFileSync(dir + 'mt_raw.txt', 'utf8').split(nl)) {
    let m;
    if ((m = l.match(/^== (base|x) \d/))) { cur = m[1]; continue; }
    if ((m = l.match(/workload: (\w+)/))) { wl = m[1]; continue; }
    if (cur && (m = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M/))) ((cells[`${wl} T=${m[1]}`] ??= { base: [], x: [] })[cur]).push(+m[2]);
  }
  md.push('### MT control (malloc_macro, SeferAlloc aggregate Mops/s, 6 samples per arm, ABBA x 3), rule C4: x worse than base by more than 3 % fails', '', '| cell | base median | x median | x / base (medians) | C4 | base samples | x samples |', '|---|---:|---:|---:|---|---|---|');
  for (const [k, v] of Object.entries(cells)) {
    assert.equal(v.base.length, 6); assert.equal(v.x.length, 6);
    const ratio = med(v.x) / med(v.base);
    md.push(`| ${k} | ${med(v.base).toFixed(1)} | ${med(v.x).toFixed(1)} | ${ratio.toFixed(3)} | ${ratio < 0.97 ? 'FAIL' : 'ok'} | ${v.base.join(' ')} | ${v.x.join(' ')} |`);
    csv.push(['mt', 'windows', k, 6, ratio.toFixed(3), '', '', '', '', '', '', 'C4', ratio < 0.97 ? 'FAIL' : 'ok'].join(','));
  }
  md.push('');
}

// ---- activation oracle (T3) ----
if (fs.existsSync(dir + 'activation.txt')) {
  const wild1 = fs.readFileSync('docs/perf/wild1/probe_counts.txt', 'utf8').split(nl).filter((l) => l.startsWith('wild run') && l.includes('LANE seed') && !l.includes('TIER1'));
  const memoExits = med(wild1.map((l) => +l.match(/memo_key=(\d+)/)[1]));
  const rows = {};
  for (const l of fs.readFileSync(dir + 'activation.txt', 'utf8').split(nl)) {
    const m = l.match(/^(\w+) run\d+ "lane_second_chance":(\d+),"tier1_hits":(\d+),"tier1_misses":(\d+)/);
    if (m) (rows[m[1]] ??= []).push([+m[2], +m[3], +m[4]]);
  }
  md.push('### Path activation (x + bench-internals build, Windows, sefer, scale 1, 5 runs, medians)', '',
    `T3 compares the frees accepted by the second chance in \`wild\` with the memo exits of the base probe (WILD-1, median ${memoExits} over 5 runs, \`docs/perf/wild1/probe_counts.txt\`); the threshold is 40 %.`, '',
    '| scenario | second-chance accepts | Tier-1 hits | Tier-1 misses (hash probes) | share |', '|---|---:|---:|---:|---|');
  for (const sc of ['wild', 'phase', 'burst', 'growmix', 'regrow']) {
    const r = rows[sc]; assert.ok(r && r.length === 5, `${sc}: 5 activation runs`);
    const a = med(r.map((x) => x[0]));
    const share = sc === 'wild' ? `${(a / memoExits * 100).toFixed(1)} % (${a} / ${memoExits}) ${a / memoExits >= 0.4 ? 'T3 PASS' : 'T3 FAIL'}` : '';
    md.push(`| ${sc} | ${a} | ${med(r.map((x) => x[1]))} | ${med(r.map((x) => x[2]))} | ${share} |`);
  }
  md.push('');
}

// ---- callgrind Ir (C5) ----
if (fs.existsSync(dir + 'callgrind_totals.txt')) {
  const t = {};
  for (const l of fs.readFileSync(dir + 'callgrind_totals.txt', 'utf8').split(nl)) {
    const m = l.match(/^(\w+) (base|x) (\d+)$/);
    if (m) (t[m[1]] ??= {})[m[2]] = +m[3];
  }
  md.push('### callgrind Ir (Linux, sefer arm), rule C5', '', '| scenario (scale) | base Ir | x Ir | x / base - 1 | limit | C5 |', '|---|---:|---:|---:|---|---|');
  const limit = { burst: [-0.003, 0.003], phase: [-Infinity, 0.005], wild: [-Infinity, 0] };
  for (const [sc, v] of Object.entries(t)) {
    assert.ok(v.base && v.x, `${sc}: both arms`);
    const d = v.x / v.base - 1, [lo, hi] = limit[sc.split(' ')[0]] ?? [-Infinity, Infinity];
    md.push(`| ${sc} | ${v.base} | ${v.x} | ${(d * 100).toFixed(2)} % (${v.x - v.base} / ${v.base}) | ${Number.isFinite(lo) ? `${lo * 100}..` : '..'}${hi * 100} % | ${d >= lo && d <= hi ? 'ok' : 'FAIL'} |`);
  }
  md.push('');
}

fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/WILD2_LANE_SECOND_CHANCE_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  fs.writeFileSync('docs/perf/WILD2_LANE_SECOND_CHANCE.md', fs.readFileSync(dir + 'doc_head.md', 'utf8') + md.join('\n') + '\n' + fs.readFileSync(dir + 'doc_tail.md', 'utf8'));
}
console.log(md.join('\n'));
