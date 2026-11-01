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
  if (platform.endsWith('-aligned') || platform.endsWith('-pilot')) rule = `control(${rule})`;
  return { readable, rule, verdict };
};

const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large', 'burst'];
const judge = (title, campaign, platform, file, scen = SCEN) => {
  const recs = gz(file);
  const blocks = [...new Set(recs.map((x) => x.block))].sort((a, b) => a - b);
  md.push(`### ${title}`, '', `| scenario | blocks | x / base (exp mean d) | 95 % CI | A/A base2 / base | A/A 95 % CI | rule | verdict | min base ms | min x ms |`, '|---|---:|---:|---:|---:|---:|---|---|---:|---:|');
  for (const sc of scen) {
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
if (fs.existsSync(dir + 'linux2_raw.jsonl.gz')) judge('Judge, Linux (WSL2), confirmation campaign (Amendment 2: 24 blocks, minimum of 5 repeats; wild/phase/burst/large at scale 1, regrow/growmix at 0.5)', 'c2', 'linux', 'linux2_raw.jsonl.gz', ['wild', 'phase', 'burst', 'regrow', 'growmix', 'large']);
if (fs.existsSync(dir + 'linux3_raw.jsonl.gz')) judge('Judge, Linux (WSL2), layout control (Amendment 3): aligned-code builds, same design as the confirmation campaign (a control, not a gate)', 'c3', 'linux-aligned', 'linux3_raw.jsonl.gz', ['wild', 'phase', 'burst', 'regrow', 'growmix', 'large']);
if (fs.existsSync(dir + 'win_pilot_high_raw.jsonl.gz')) judge('Judge, Windows, pilot at HIGH priority (Amendment 2: wild and phase only, 12 blocks, minimum of 3; a noise check, not a gate)', 'pilot', 'windows-pilot', 'win_pilot_high_raw.jsonl.gz', ['wild', 'phase']);
if (fs.existsSync(dir + 'win2_raw.jsonl.gz')) judge('Judge, Windows, confirmation campaign (Amendment 1: more blocks and repeats, machine otherwise idle)', 'c2', 'windows', 'win2_raw.jsonl.gz');

// ---- bench:table paired sets ----
for (const s of ['s1', 's2']) {
  if (!fs.existsSync(dir + `bt/${s}base_r1.csv`)) continue;
  const r = spawnSync('node', ['scripts/g1_magnitude_verdict.mjs', dir + 'bt', `${s}base`, `${s}x`, 'x'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  md.push(`### bench:table paired set ${s} (order base, x, x, base; aligned code)`, '', r.stdout.trim(), '');
  {
    const load = (name) => { const m = {}; for (const l of fs.readFileSync(dir + `bt/${name}.csv`, 'utf8').trim().split(nl).slice(1)) { const c = l.split(','); m[`${c[0]} ${c[1]} ${c[2]}`] = Number(c[8]); } return m; };
    const b1 = load(`${s}base_r1`), b2 = load(`${s}base_r2`), x1 = load(`${s}x_r1`), x2 = load(`${s}x_r2`);
    const cells = Object.keys(b1).filter((k) => k.endsWith(' sefer')).map((k) => k.slice(0, -6));
    const rr = (m, k) => m[`${k} sefer`] / m[`${k} mimalloc`];
    const bad = cells.filter((k) => Math.abs(rr(b1, k) / rr(b2, k) - 1) > 0.10 || Math.abs(rr(x1, k) / rr(x2, k) - 1) > 0.10);
    md.push(`Noise floor of set ${s} (Amendment 1 point 4, post hoc): cells whose two runs of the SAME build differ in their sefer / mi ratio by more than 10 %: ${bad.length} of ${cells.length} (${bad.join('; ') || 'none'}). Such cells are UNREADABLE for rule C3 and are neither passed nor failed.`, '');
    csv.push(['bench-table-noise', 'windows', s, cells.length, bad.length, '', '', '', '', '', '', 'C3-noise', bad.length > 0 ? 'UNREADABLE-CELLS' : 'ok'].join(','));
  }
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
    const spread = Math.max(...v.base) / Math.min(...v.base), c4 = spread > 1.25 ? `UNREADABLE (base spread ${spread.toFixed(2)})` : ratio < 0.97 ? 'FAIL' : 'ok';
    md.push(`| ${k} | ${med(v.base).toFixed(1)} | ${med(v.x).toFixed(1)} | ${ratio.toFixed(3)} | ${c4} | ${v.base.join(' ')} | ${v.x.join(' ')} |`);
    csv.push(['mt', 'windows', k, 6, ratio.toFixed(3), '', '', '', '', '', '', 'C4', c4.split(' ')[0]].join(','));
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


const perfStat = (file, title) => {
  if (!fs.existsSync(dir + file)) return;
  const cells = {};
  for (const l of fs.readFileSync(dir + file, 'utf8').split(nl)) {
    const m = l.match(/^run(\d+) (\w+) (\w+) (\d+),,cycles:u.*? (\d+),,instructions:u.*? (\d+),,L1-dcache-load-misses/);
    if (!m) continue;
    const c = (cells[m[2] + ' ' + m[3]] ??= { cyc: [], ins: [], l1: [] });
    c.cyc.push(+m[4]); c.ins.push(+m[5]); c.l1.push(+m[6]);
  }
  md.push(title, '', '| scenario | cycles base (M) | cycles x (M) | x / base | instructions base (M) | instructions x (M) | x / base | L1D misses base (M) | L1D misses x (M) | x / base |', '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const sc of ['wild', 'phase', 'burst']) {
    const b = cells[sc + ' base'], x = cells[sc + ' x'];
    assert.ok(b && x && b.cyc.length === 5 && x.cyc.length === 5, sc + ': 5 runs per arm');
    const r = (k) => [med(b[k]) / 1e6, med(x[k]) / 1e6, med(x[k]) / med(b[k])];
    const [c0, c1, cr] = r('cyc'), [i0, i1, ir] = r('ins'), [l0, l1, lr] = r('l1');
    md.push('| ' + sc + ' | ' + c0.toFixed(1) + ' | ' + c1.toFixed(1) + ' | ' + cr.toFixed(3) + ' | ' + i0.toFixed(1) + ' | ' + i1.toFixed(1) + ' | ' + ir.toFixed(3) + ' | ' + l0.toFixed(1) + ' | ' + l1.toFixed(1) + ' | ' + lr.toFixed(3) + ' |');
  }
  md.push('');
};
perfStat('linux_tools/perf_stat_raw.txt', '### Linux (WSL2) hardware counters, perf stat, user mode, median of 5 runs, default-layout builds (supporting)');
perfStat('linux_tools/perf_stat_aligned_raw.txt', '### Linux (WSL2) hardware counters, perf stat, user mode, median of 5 runs, aligned-code builds (Amendment 3 layout control)');

// ---- callgrind per function: where the saved instructions come from (wild, scale 0.3, Linux) ----
if (fs.existsSync(dir + 'linux_tools/cgfn_base.txt')) {
  const pick = (name, fn) => {
    const txt = fs.readFileSync(dir + 'linux_tools/' + name, 'utf8');
    const line = txt.split(nl).find((l) => l.includes('dealloc_fast::' + fn + ' '));
    const ex = txt.split(nl).find((l) => l.includes(' ' + fn + ' entry='));
    return { ir: line ? +line.trim().split(' ')[0].replace(/,/g, '') : 0, n: ex ? +ex.match(/executions=(\d+)/)[1] : 0 };
  };
  const bm = pick('cgfn_base.txt', 'lane_miss'), xm = pick('cgfn_x.txt', 'lane_miss'), xs = pick('cgfn_x.txt', 'lane_second_chance');
  assert.ok(bm.n > 0 && xs.n > 0, 'entry counts parsed');
  md.push('### Where the instructions go (callgrind, Linux, wild at scale 0.3, sefer arm)', '',
    '| function | Ir (self) | entries | Ir per entry |', '|---|---:|---:|---:|',
    '| base lane_miss | ' + bm.ir + ' | ' + bm.n + ' | ' + (bm.ir / bm.n).toFixed(1) + ' |',
    '| x lane_second_chance | ' + xs.ir + ' | ' + xs.n + ' | ' + (xs.ir / xs.n).toFixed(1) + ' |',
    '| x lane_miss (frees that fell through) | ' + xm.ir + ' | ' + xm.n + ' | ' + (xm.ir / xm.n).toFixed(1) + ' |',
    '', 'Accepted by the second chance: ' + (xs.n - xm.n) + ' of ' + xs.n + ' entries (' + ((xs.n - xm.n) / xs.n * 100).toFixed(1) + ' %). Instructions of the memo-miss frees: base ' + bm.ir + ' against x ' + (xs.ir + xm.ir) + ' (' + (((xs.ir + xm.ir) / bm.ir - 1) * 100).toFixed(1) + ' %, ' + (xs.ir + xm.ir - bm.ir) + ' / ' + bm.ir + ').', '');
}

// ---- C6: the memo-hit path of the inlined dealloc (judge shim a_dealloc), disassembly of base against x ----
if (fs.existsSync(dir + 'linux_tools/text_diff.txt')) {
  const diff = fs.readFileSync(dir + 'linux_tools/text_diff.txt', 'utf8').trim();
  md.push('### C6: disassembly diff of the judge shim a_dealloc (inlined dealloc hit path), base against x, addresses and call targets masked', '', '~~~text', diff, '~~~', '');
}
fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/WILD2_LANE_SECOND_CHANCE_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  fs.writeFileSync('docs/perf/WILD2_LANE_SECOND_CHANCE.md', fs.readFileSync(dir + 'doc_head.md', 'utf8') + md.join('\n') + '\n' + fs.readFileSync(dir + 'doc_tail.md', 'utf8'));
}
console.log(md.join('\n'));
