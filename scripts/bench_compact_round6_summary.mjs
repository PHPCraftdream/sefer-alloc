#!/usr/bin/env node
// Derives the tables of docs/perf/BENCH_COMPACT_ROUND6_BASE_VS_HEAD_2026-10-18.md
// from the raw logs, so no number in the report is typed by hand (CLAUDE.md:
// gate tables are derived by one checked script from raw data).
//
//   node scripts/bench_compact_round6_summary.mjs --ingest <dir>   # sanitise + copy raw logs, write manifest
//   node scripts/bench_compact_round6_summary.mjs --write          # write CSVs + generated report block
//
// <dir> holds base/ and head/ with A1f_global_alloc.log, A1f_global_alloc_run2.log,
// A3_paired_ab.log, A4_malloc_macro.log. Host paths are replaced by placeholders
// on ingest; the manifest records the sha256 of both the original and the
// committed file.
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(import.meta.url), '..', '..');
const PERF = join(ROOT, 'docs', 'perf');
const BASE = 'BENCH_COMPACT_ROUND6_BASE_VS_HEAD_2026-10-18';
const REPORT = join(PERF, `${BASE}.md`);
const SIDES = ['base', 'head'];
const FILES = [
  ['A1_run1', 'A1f_global_alloc.log'],
  ['A1_run2', 'A1f_global_alloc_run2.log'],
  ['A3', 'A3_paired_ab.log'],
  ['A4', 'A4_malloc_macro.log'],
];
const OPS = 1024; // benches/global_alloc.rs: pairs per criterion iteration
const NOISE_FLOOR = 0.05; // runbook §5: below ±5% is not interpreted
const rawPath = (side, tag) => join(PERF, `_raw_bench_compact_round6_${side}_${tag}.log`);
const sha = (buf) => createHash('sha256').update(buf).digest('hex');

// Host paths are derived at run time (never hard-coded): the two bench worktrees,
// the repo root and CARGO_HOME, each in both slash styles, longest first.
function sanitise(text) {
  const forms = (p) => [p, p.replaceAll('\\', '/'), p.replaceAll('/', '\\')];
  const subs = [];
  for (const side of SIDES) for (const p of forms(join(ROOT, 'worktrees', `bench-${side}`))) subs.push([p, '<repo>']);
  for (const p of forms(ROOT)) subs.push([p, '<repo>']);
  if (process.env.CARGO_HOME) for (const p of forms(process.env.CARGO_HOME)) subs.push([p, '<cargo-home>']);
  subs.sort((a, b) => b[0].length - a[0].length);
  let out = text;
  for (const [p, r] of subs) out = out.split(p).join(r);
  return out;
}

function ingest(dir) {
  const rows = ['side,tag,source_sha256,committed_sha256,committed_file'];
  for (const side of SIDES) {
    for (const [tag, file] of FILES) {
      const src = readFileSync(join(resolve(dir), side, file));
      const out = sanitise(src.toString('utf8'));
      if (/[A-Za-z]:[\\/]|\/Users\//.test(out.replace(/<repo>|<cargo-home>/g, ''))) {
        throw new Error(`private path left in ${side}/${file}`);
      }
      writeFileSync(rawPath(side, tag), out);
      rows.push([side, tag, sha(src), sha(Buffer.from(out)), `_raw_bench_compact_round6_${side}_${tag}.log`].join(','));
    }
    copyFileSync(join(resolve(dir), side, '_sha.txt'), join(PERF, `_raw_bench_compact_round6_${side}_sha.txt`));
  }
  writeFileSync(join(PERF, `${BASE}_manifest.csv`), rows.join('\n') + '\n');
}

const UNIT = { ns: 1, 'µs': 1e3, us: 1e3, ms: 1e6, s: 1e9 };
function parseA1(text) {
  const re = /^(global_alloc\S*)\s+time:\s+\[[\d.]+ \S+ ([\d.]+) (ns|µs|us|ms|s) [\d.]+ \S+\]/gm;
  const out = new Map();
  for (const m of text.matchAll(re)) {
    const unscaled = m[1].includes('/manual_realloc_sim/');
    const ns = Number(m[2]) * UNIT[m[3]] / (unscaled ? 1 : OPS);
    if (out.has(m[1])) throw new Error(`duplicate bench ${m[1]}`);
    out.set(m[1], ns);
  }
  return out;
}

function splitName(name) {
  const p = name.split('/');
  if (p[1] === 'manual_realloc_sim') return { scenario: 'manual_realloc_sim', arm: p[2], unit: 'ns/chain' };
  return { scenario: `${p[0]}/${p[2]}`, arm: p[1], unit: 'ns/pair' };
}

function parseA4(text) {
  const rows = [];
  let workload = null;
  for (const line of text.split(/\r?\n/)) {
    const w = line.match(/^--- workload: (\S+)\s+\(mode: (\S+)\)/);
    if (w) { workload = `${w[1]}/${w[2]}`; continue; }
    const r = line.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M\s*$/);
    if (r && workload) {
      ['SeferAlloc', 'mimalloc', 'System'].forEach((arm, i) => rows.push({ workload, threads: Number(r[1]), arm, mops: Number(r[2 + i]) }));
    }
  }
  return rows;
}

function parseA3(text) {
  const samples = new Map(); // arm -> {elapsed:[], rss:[], commit:[]}
  let arm = null;
  for (const line of text.split(/\r?\n/)) {
    let m;
    if ((m = line.match(/RESULT arm=(\S+)/))) { arm = m[1]; if (!samples.has(arm)) samples.set(arm, { elapsed: [], rss: [], commit: [] }); }
    else if ((m = line.match(/RESULT elapsed_ns=(\d+)/)) && arm) samples.get(arm).elapsed.push(Number(m[1]));
    else if ((m = line.match(/RESULT rss_after_kib=(\d+)/)) && arm) samples.get(arm).rss.push(Number(m[1]));
    else if ((m = line.match(/RESULT commit_after_kib=(\d+)/)) && arm) samples.get(arm).commit.push(Number(m[1]));
  }
  const tests = [];
  const re = /=== (sefer vs \S+) \(A - B, ns\) ===\s*\n\s*(n=\d+ .*?=> \S+)[^\n]*\n\s*(sign test: [^\n]+)/g;
  for (const m of text.matchAll(re)) tests.push({ label: m[1], stat: m[2].replace(/\s+/g, ' '), sign: m[3].trim() });
  return { samples, tests };
}

const mean = (a) => a.reduce((s, x) => s + x, 0) / a.length;
const pct = (x) => `${(x * 100).toFixed(1)}%`;
const f = (x, d = 1) => x.toFixed(d);

function build() {
  const a1 = {};
  const a3 = {};
  const a4 = {};
  for (const side of SIDES) {
    a1[side] = ['A1_run1', 'A1_run2'].map((t) => parseA1(readFileSync(rawPath(side, t), 'utf8')));
    a3[side] = parseA3(readFileSync(rawPath(side, 'A3'), 'utf8'));
    a4[side] = parseA4(readFileSync(rawPath(side, 'A4'), 'utf8'));
  }
  const names = [...a1.base[0].keys()];
  for (const side of SIDES) for (const run of a1[side]) {
    if (run.size !== names.length || names.some((n) => !run.has(n))) throw new Error('bench name sets differ between runs');
  }
  if (names.length !== 51) throw new Error(`expected 51 A1 benches, got ${names.length}`);

  const rows = names.map((name) => {
    const { scenario, arm, unit } = splitName(name);
    const b = a1.base.map((r) => r.get(name));
    const h = a1.head.map((r) => r.get(name));
    const bm = mean(b), hm = mean(h);
    const ratio = hm / bm;
    const p1 = h[0] / b[0], p2 = h[1] / b[1];
    // exact arithmetic checks on every derived figure
    if (Math.abs(ratio - mean(h) / mean(b)) > 1e-12) throw new Error('ratio mismatch');
    return {
      name, scenario, arm, unit,
      b1: b[0], b2: b[1], h1: h[0], h2: h[1], bm, hm, ratio, p1, p2,
      noiseBase: Math.abs(b[0] - b[1]) / bm, noiseHead: Math.abs(h[0] - h[1]) / hm,
    };
  });
  const byScenario = new Map();
  for (const r of rows) {
    if (!byScenario.has(r.scenario)) byScenario.set(r.scenario, {});
    byScenario.get(r.scenario)[r.arm] = r;
  }
  const scen = [...byScenario.entries()].map(([scenario, arms]) => {
    for (const a of ['SeferAlloc', 'mimalloc', 'System']) if (!arms[a]) throw new Error(`missing ${a} in ${scenario}`);
    const s = arms.SeferAlloc;
    // mimalloc and System run identical code on both commits: their base->head drift is the host noise.
    const ctrl = Math.max(Math.abs(arms.mimalloc.ratio - 1), Math.abs(arms.System.ratio - 1));
    const bar = Math.max(NOISE_FLOOR, ctrl);
    const sameSide = (s.p1 - 1) * (s.p2 - 1) > 0;
    const signal = Math.abs(s.ratio - 1) > bar && sameSide && Math.abs(s.p1 - 1) > NOISE_FLOOR && Math.abs(s.p2 - 1) > NOISE_FLOOR;
    return { scenario, arms, s, ctrl, bar, signal, verdict: signal ? (s.ratio > 1 ? 'SLOWER' : 'FASTER') : 'within noise' };
  });

  // ---- CSV (A1) ----
  const csv = ['scenario,arm,unit,base_run1,base_run2,head_run1,head_run2,base_mean,head_mean,head_over_base,pair1_ratio,pair2_ratio,noise_base,noise_head'];
  for (const r of rows) csv.push([r.scenario, r.arm, r.unit, r.b1, r.b2, r.h1, r.h2, r.bm, r.hm, r.ratio, r.p1, r.p2, r.noiseBase, r.noiseHead].map((x) => (typeof x === 'number' ? x.toPrecision(6) : x)).join(','));
  writeFileSync(join(PERF, `${BASE}_summary.csv`), csv.join('\n') + '\n');

  // ---- Markdown ----
  const md = [];
  const sefer = scen.map((c) => c.s);
  const nSignal = scen.filter((c) => c.signal).length;
  md.push('<!-- BEGIN GENERATED (scripts/bench_compact_round6_summary.mjs --write) -->', '');
  md.push('### A1 — SeferAlloc, `base` -> `head` (ns per alloc+free pair, mean of 2 runs; manual_realloc_sim: ns per chain)', '');
  md.push('Control bar = max(5%, larger base->head drift of the mimalloc and System arms in the same scenario; both run identical code on both commits). A change is a signal only if SeferAlloc\'s mean ratio beats the bar and both run pairs (run1 vs run1, run2 vs run2) lie on the same side beyond 5%.', '');
  md.push('| scenario | base | head | head/base | pair1 | pair2 | control drift | verdict |', '|---|---:|---:|---:|---:|---:|---:|---|');
  for (const c of scen) md.push(`| ${c.scenario} | ${f(c.s.bm)} | ${f(c.s.hm)} | ${f(c.s.ratio, 3)} | ${f(c.s.p1, 3)} | ${f(c.s.p2, 3)} | ${pct(c.ctrl)} | ${c.verdict} |`);
  md.push('', `Signals: ${nSignal} of ${scen.length} scenarios (denominator: scenarios = group x size, plus manual_realloc_sim).`, '');

  const runNoise = (arm, side) => rows.filter((r) => r.arm === arm).map((r) => (side === 'base' ? r.noiseBase : r.noiseHead));
  md.push('### A1 — noise: run1 vs run2 on the same commit (|run1-run2| / mean, over all 17 scenarios per arm)', '');
  md.push('| arm | commit | median | max |', '|---|---|---:|---:|');
  for (const arm of ['SeferAlloc', 'mimalloc', 'System']) for (const side of SIDES) {
    const v = runNoise(arm, side).sort((x, y) => x - y);
    md.push(`| ${arm} | ${side} | ${pct(v[Math.floor(v.length / 2)])} | ${pct(v[v.length - 1])} |`);
  }
  md.push('');
  md.push('### A1 — arms against each other on `head` (mean of 2 runs, ns per pair; ratio = SeferAlloc / other)', '');
  md.push('| scenario | SeferAlloc | mimalloc | System | Sefer/mimalloc | Sefer/System |', '|---|---:|---:|---:|---:|---:|');
  for (const c of scen) md.push(`| ${c.scenario} | ${f(c.arms.SeferAlloc.hm)} | ${f(c.arms.mimalloc.hm)} | ${f(c.arms.System.hm)} | ${f(c.arms.SeferAlloc.hm / c.arms.mimalloc.hm, 2)} | ${f(c.arms.SeferAlloc.hm / c.arms.System.hm, 2)} |`);
  md.push('');

  md.push('### A4 — malloc_macro (single run per commit, Mops/s of alloc+free pairs, higher is better)', '');
  md.push('| workload | threads | arm | base | head | head/base |', '|---|---:|---|---:|---:|---:|');
  const key = (r) => `${r.workload}|${r.threads}|${r.arm}`;
  const hmap = new Map(a4.head.map((r) => [key(r), r.mops]));
  if (a4.base.length !== 18 || a4.head.length !== 18) throw new Error(`A4 expected 18 rows per commit, got ${a4.base.length}/${a4.head.length}`);
  for (const r of a4.base) {
    const hv = hmap.get(key(r));
    md.push(`| ${r.workload} | ${r.threads} | ${r.arm} | ${f(r.mops, 2)} | ${f(hv, 2)} | ${f(hv / r.mops, 2)} |`);
  }
  const ctrlA4 = a4.base.filter((r) => r.arm !== 'SeferAlloc').map((r) => Math.abs(hmap.get(key(r)) / r.mops - 1));
  md.push('', `A4 control drift (mimalloc and System arms, identical code on both commits): median ${pct(ctrlA4.sort((x, y) => x - y)[Math.floor(ctrlA4.length / 2)])}, max ${pct(Math.max(...ctrlA4))} over ${ctrlA4.length} cells.`, '');

  md.push('### A4 — SeferAlloc scaling and gap to mimalloc (per commit; scaling = SeferAlloc(T) / SeferAlloc(1 thread); gap = mimalloc / SeferAlloc at the same T)', '');
  md.push('| commit | workload | threads | SeferAlloc (M) | scaling vs 1 thread | mimalloc (M) | gap (mimalloc / SeferAlloc) |', '|---|---|---:|---:|---:|---:|---:|');
  const a4csv = ['commit,workload,threads,sefer_mops,sefer_scaling_vs_1t,mimalloc_mops,gap_mimalloc_over_sefer,system_mops,runs'];
  for (const side of SIDES) {
    const get = (w, t, arm) => a4[side].find((r) => r.workload === w && r.threads === t && r.arm === arm).mops;
    for (const w of [...new Set(a4[side].map((r) => r.workload))]) for (const t of [1, 2, 4]) {
      const s = get(w, t, 'SeferAlloc'), s1 = get(w, 1, 'SeferAlloc'), m = get(w, t, 'mimalloc'), y = get(w, t, 'System');
      md.push(`| ${side} | ${w} | ${t} | ${f(s, 2)} | ${f(s / s1, 3)} | ${f(m, 2)} | ${f(m / s, 1)} |`);
      a4csv.push([side, w, t, s, s / s1, m, m / s, y, 'single run'].join(','));
    }
  }
  writeFileSync(join(PERF, `${BASE}_A4_summary.csv`), a4csv.join('\n') + '\n');
  md.push('');

  md.push('### A3 — paired A/B/B/A, 20 pairs, within each commit (sefer vs mimalloc / system; NOT base vs head)', '');
  md.push('| commit | comparison | statistic | sign test |', '|---|---|---|---|');
  for (const side of SIDES) for (const t of a3[side].tests) md.push(`| ${side} | ${t.label} | ${t.stat} | ${t.sign} |`);
  md.push('', '| commit | arm | samples | mean elapsed (us) | mean RSS (KiB) | mean commit (KiB) |', '|---|---|---:|---:|---:|---:|');
  for (const side of SIDES) for (const [arm, s] of a3[side].samples) {
    if (s.elapsed.length !== s.rss.length || s.rss.length !== s.commit.length) throw new Error(`A3 sample length mismatch: ${side}/${arm}`);
    md.push(`| ${side} | ${arm} | ${s.elapsed.length} | ${f(mean(s.elapsed) / 1e3)} | ${f(mean(s.rss), 0)} | ${f(mean(s.commit), 0)} |`);
  }
  md.push('', '<!-- END GENERATED -->');
  return md.join('\n');
}

const args = process.argv.slice(2);
if (args[0] === '--ingest' && args[1]) {
  ingest(args[1]);
} else if (args[0] === '--write') {
  const block = build();
  if (!existsSync(REPORT)) {
    writeFileSync(REPORT, block + '\n');
  } else {
    const cur = readFileSync(REPORT, 'utf8');
    const re = /<!-- BEGIN GENERATED[\s\S]*?<!-- END GENERATED -->/;
    if (!re.test(cur)) throw new Error('report has no GENERATED block');
    writeFileSync(REPORT, cur.replace(re, () => block));
  }
} else {
  console.error('usage: --ingest <dir> | --write');
  process.exit(2);
}
