#!/usr/bin/env node
// WILD-1 report generator (measurement only): every number of docs/perf/WILD1_GAP_DECOMPOSITION.md comes
// from docs/perf/wild1/ through this script.
// Input: one JSON line per run (WJ_PROF=1 judge, sefer and mi arms, 3 repeats per block and arm).
// Per block and arm the repeat with the smallest wall time is taken whole (so the buckets add up to the
// wall time of one real run); rest = wall - sum(buckets). Per bucket and block d = sefer_ms - mi_ms; the
// cell is the mean over blocks with a two-sided 95 % t-interval (df = blocks - 1). The identity
// sum(bucket differences) = wall difference is asserted per block.
// Writes docs/perf/WILD1_GAP_DECOMPOSITION_summary.csv, docs/perf/wild1/tables.md and, when
// doc_head.md / doc_tail.md exist, docs/perf/WILD1_GAP_DECOMPOSITION.md.
import fs from 'node:fs';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';

const dir = 'docs/perf/wild1/';
const nl = new RegExp('\\r?\\n');
const T975 = { 11: 2.201 };
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const t = T975[n - 1]; assert.ok(t, `no t quantile for n=${n}`);
  return { m, lo: m - t * sd / Math.sqrt(n), hi: m + t * sd / Math.sqrt(n) };
};
const f1 = (v) => v.toFixed(1);
const f3 = (v) => v.toFixed(3);
const PLATFORMS = [['windows', 'prof_raw.jsonl.gz'], ['linux (WSL2)', 'linux_prof_raw.jsonl.gz']];
const load = (name) => zlib.gunzipSync(fs.readFileSync(dir + name)).toString().trim().split(nl).map((l) => JSON.parse(l));
let recs = [];
const ident = fs.readFileSync(dir + 'ident.txt', 'utf8').split(nl)[0].match(/^commit ([0-9a-f]{40}) tree ([0-9a-f]{40})$/);
assert.ok(ident, 'ident.txt first line: commit and tree');
const BUCKETS = ['burst_alloc', 'burst_free', 'large_alloc', 'large_free', 'large_realloc', 'small_realloc', 'teardown'];
const md = [], csv = ['platform,scenario,bucket,sefer_ms,mi_ms,diff_ms,diff_lo,diff_hi,share_of_wall_diff,calls_sefer,calls_mi,commit,tree,samples'];

const pick = (b, arm, sc) => {
  const runs = recs.filter((x) => x.block === b && x.arm === arm && x.r.scenario === sc);
  assert.equal(runs.length, 3, `${sc}/${arm}/block ${b}: 3 repeats`);
  const best = runs.reduce((a, c) => (c.r.wall_ns < a.r.wall_ns ? c : a));
  const p = best.r.prof;
  const ms = {};
  let sum = 0;
  for (const k of BUCKETS) { ms[k] = p[k][0] / 1e6; sum += p[k][0]; }
  ms.rest = (best.r.wall_ns - sum) / 1e6;
  assert.ok(ms.rest > -1e-6, `${sc}/${arm}/block ${b}: buckets exceed the wall time`);
  return { ms, wall: best.r.wall_ns / 1e6, calls: Object.fromEntries(BUCKETS.map((k) => [k, p[k][1]])), moved: { large_realloc: p.large_realloc[2], small_realloc: p.small_realloc[2] }, checksum: best.r.checksum };
};

for (const [plat, fname] of PLATFORMS) {
recs = load(fname);
for (const sc of ['wild', 'phase', 'large']) {
  const blocks = [...new Set(recs.filter((x) => x.r.scenario === sc).map((x) => x.block))].sort((a, b) => a - b);
  const per = Object.fromEntries([...BUCKETS, 'rest', 'wall'].map((k) => [k, { s: [], m: [], d: [] }]));
  const calls = { sefer: {}, mi: {} }, moved = { sefer: { large_realloc: 0, small_realloc: 0 }, mi: { large_realloc: 0, small_realloc: 0 } };
  for (const b of blocks) {
    const S = pick(b, 'sefer', sc), M = pick(b, 'mi', sc);
    assert.equal(S.checksum, M.checksum, `${sc}/block ${b}: same op sequence`);
    let dsum = 0;
    for (const k of [...BUCKETS, 'rest']) { per[k].s.push(S.ms[k]); per[k].m.push(M.ms[k]); per[k].d.push(S.ms[k] - M.ms[k]); dsum += S.ms[k] - M.ms[k]; }
    per.wall.s.push(S.wall); per.wall.m.push(M.wall); per.wall.d.push(S.wall - M.wall);
    assert.ok(Math.abs(dsum - (S.wall - M.wall)) < 1e-6, `${sc}/block ${b}: bucket differences add up to the wall difference`);
    for (const k of BUCKETS) { calls.sefer[k] = S.calls[k]; calls.mi[k] = M.calls[k]; }
    moved.sefer = S.moved; moved.mi = M.moved;
  }
  const mean = (v) => v.reduce((a, c) => a + c, 0) / v.length;
  const W = ci(per.wall.d);
  md.push(`### ${sc} (${plat}, ${blocks.length} blocks, one whole run per block and arm: the repeat with the smallest wall time)`, '',
    '| bucket | sefer ms | mi ms | sefer - mi ms | 95 % CI | share of the wall-time difference (bucket / wall) | calls sefer | calls mi |', '|---|---:|---:|---:|---:|---:|---:|---:|');
  for (const k of [...BUCKETS, 'rest', 'wall']) {
    const D = ci(per[k].d), share = D.m / W.m;
    const cs = k === 'rest' || k === 'wall' ? '' : calls.sefer[k], cm = k === 'rest' || k === 'wall' ? '' : calls.mi[k];
    md.push(`| ${k} | ${f1(mean(per[k].s))} | ${f1(mean(per[k].m))} | ${f1(D.m)} | ${f1(D.lo)}..${f1(D.hi)} | ${(share * 100).toFixed(0)} % (${f1(D.m)} / ${f1(W.m)}) | ${cs} | ${cm} |`);
    csv.push([plat, sc, k, f3(mean(per[k].s)), f3(mean(per[k].m)), f3(D.m), f3(D.lo), f3(D.hi), f3(share), cs, cm, ident[1], ident[2], `${blocks.length} blocks x 3 repeats`].join(','));
  }
  md.push('', `Realloc moved (new pointer) in the last block: sefer large ${moved.sefer.large_realloc} of ${calls.sefer.large_realloc}, small ${moved.sefer.small_realloc} of ${calls.sefer.small_realloc}; mi large ${moved.mi.large_realloc} of ${calls.mi.large_realloc}, small ${moved.mi.small_realloc} of ${calls.mi.small_realloc}.`, '');
}
}

// ---- the same small bursts on a clean heap (scenario burst) and inside wild / phase ----
md.push('### Small LIFO bursts: clean heap (scenario burst) against the same bursts inside wild and phase', '',
  'ns per block = bucket ms / calls; ratio = exp(mean over blocks of ln(sefer / mi)) with a 95 % t-interval; one whole run per block and arm (the repeat with the smallest wall time).', '',
  '| platform | context | sefer alloc ns/block | mi alloc ns/block | alloc sefer / mi (95 % CI) | sefer free ns/block | mi free ns/block | free sefer / mi (95 % CI) |', '|---|---|---:|---:|---:|---:|---:|---:|');
const csv2 = ['platform,context,phase,sefer_ns_per_block,mi_ns_per_block,sefer_over_mi,ci_lo,ci_hi,commit,tree,samples'];
const SRC = [['windows', [['burst', 'burst_raw.jsonl.gz'], ['wild', 'prof_raw.jsonl.gz'], ['phase', 'prof_raw.jsonl.gz']]],
  ['linux (WSL2)', [['burst', 'linux_burst_raw.jsonl.gz'], ['wild', 'linux_prof_raw.jsonl.gz'], ['phase', 'linux_prof_raw.jsonl.gz']]]];
for (const [plat, items] of SRC) {
  for (const [sc, fname] of items) {
    recs = load(fname);
    const blocks = [...new Set(recs.filter((x) => x.r.scenario === sc).map((x) => x.block))].sort((a, b) => a - b);
    const na = { sefer: [], mi: [] }, nf = { sefer: [], mi: [] }, ra = [], rf = [];
    for (const b of blocks) {
      const S = pick(b, 'sefer', sc), M = pick(b, 'mi', sc);
      const a = (X) => X.ms.burst_alloc * 1e6 / X.calls.burst_alloc, fr = (X) => X.ms.burst_free * 1e6 / X.calls.burst_free;
      na.sefer.push(a(S)); na.mi.push(a(M)); nf.sefer.push(fr(S)); nf.mi.push(fr(M));
      ra.push(Math.log(a(S) / a(M))); rf.push(Math.log(fr(S) / fr(M)));
    }
    const mean = (v) => v.reduce((x, y) => x + y, 0) / v.length;
    const A = ci(ra), F = ci(rf);
    md.push(`| ${plat} | ${sc} | ${f1(mean(na.sefer))} | ${f1(mean(na.mi))} | ${f3(Math.exp(A.m))} (${f3(Math.exp(A.lo))}-${f3(Math.exp(A.hi))}) | ${f1(mean(nf.sefer))} | ${f1(mean(nf.mi))} | ${f3(Math.exp(F.m))} (${f3(Math.exp(F.lo))}-${f3(Math.exp(F.hi))}) |`);
    csv2.push([plat, sc, 'alloc', f3(mean(na.sefer)), f3(mean(na.mi)), f3(Math.exp(A.m)), f3(Math.exp(A.lo)), f3(Math.exp(A.hi)), ident[1], ident[2], `${blocks.length} blocks x 3 repeats`].join(','));
    csv2.push([plat, sc, 'free', f3(mean(nf.sefer)), f3(mean(nf.mi)), f3(Math.exp(F.m)), f3(Math.exp(F.lo)), f3(Math.exp(F.hi)), ident[1], ident[2], `${blocks.length} blocks x 3 repeats`].join(','));
  }
}
md.push('');

// ---- Linux hardware counters (perf stat, user mode), medians of 5 runs ----
{
  const med = (v) => { const s = [...v].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
  const cells = {};
  for (const l of fs.readFileSync(dir + 'linux_tools/perf_stat_raw.txt', 'utf8').split(nl)) {
    const m = l.match(/^run(\d+) (\w+) (\w+) (\d+),,cycles:u.*? (\d+),,instructions:u.*? (\d+),,L1-dcache-load-misses/);
    if (!m) continue;
    const c = (cells[m[2] + ' ' + m[3]] ??= { cyc: [], ins: [], l1: [] });
    c.cyc.push(+m[4]); c.ins.push(+m[5]); c.l1.push(+m[6]);
  }
  md.push('### Linux (WSL2) hardware counters, perf stat, user mode, median of 5 runs', '', '| scenario | arm | cycles (M) | instructions (M) | IPC | L1D load misses (M) |', '|---|---|---:|---:|---:|---:|');
  for (const sc of ['burst', 'wild', 'phase']) {
    for (const arm of ['sefer', 'mi']) {
      const c = cells[sc + ' ' + arm];
      assert.ok(c && c.cyc.length === 5, sc + ' ' + arm + ': 5 runs');
      md.push('| ' + sc + ' | ' + arm + ' | ' + f1(med(c.cyc) / 1e6) + ' | ' + f1(med(c.ins) / 1e6) + ' | ' + f3(med(c.ins) / med(c.cyc)) + ' | ' + f1(med(c.l1) / 1e6) + ' |');
    }
    const s = cells[sc + ' sefer'], m = cells[sc + ' mi'];
    md.push('| ' + sc + ' | sefer / mi | ' + f3(med(s.cyc) / med(m.cyc)) + ' | ' + f3(med(s.ins) / med(m.ins)) + ' | | ' + f3(med(s.l1) / med(m.l1)) + ' |');
  }
  md.push('');
}

// ---- probe counters: why the free lane misses (throwaway probe build, Windows) ----
{
  const med = (v) => { const s = [...v].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
  const rows = {};
  for (const l of fs.readFileSync(dir + 'probe_counts.txt', 'utf8').split(nl)) {
    const m = l.match(/^(\w+) run\d+ (.*)$/);
    if (!m) continue;
    const lane = m[2].match(/LANE seed=(\d+) memo_key=(\d+) kind=(\d+) bump=(\d+) state=(\d+) accepted=(\d+) mag_full=(\d+)/);
    const t1 = m[2].match(/TIER1 hits=(\d+) misses=(\d+)/);
    const r = (rows[m[1]] ??= { lane: [], t1: [] });
    if (lane && !t1) r.lane.push(lane.slice(1).map(Number));
    if (lane && t1) r.t1.push([...t1.slice(1).map(Number), +lane[6]]);
  }
  md.push('### Why the free lane misses (probe build, Windows, sefer, median of 5 runs)', '',
    'lane frees = accepted + memo_key exits + seed exits (kind, bump and state exits were 0 in every run). Chain lookups after a memo miss: own_cache hits = TIER1 hits - accepted, own_cache misses (hash probe) = TIER1 misses.', '',
    '| scenario | lane frees | accepted by the memo | memo_key exits | memo exits / lane frees | own_cache hits after a memo miss | own_cache misses (hash probe) | hash probes / lane frees |', '|---|---:|---:|---:|---:|---:|---:|---:|');
  for (const sc of ['burst', 'wild', 'phase']) {
    const r = rows[sc];
    assert.ok(r && r.lane.length === 5 && r.t1.length === 5, sc + ': 5 + 5 runs');
    const acc = med(r.lane.map((x) => x[5])), mk = med(r.lane.map((x) => x[1])), sd = med(r.lane.map((x) => x[0]));
    const hits = med(r.t1.map((x) => x[0] - x[2])), miss = med(r.t1.map((x) => x[1]));
    const tot = acc + mk + sd;
    md.push('| ' + sc + ' | ' + tot + ' | ' + acc + ' | ' + mk + ' | ' + (mk / tot * 100).toFixed(1) + ' % (' + mk + ' / ' + tot + ') | ' + hits + ' | ' + miss + ' | ' + (miss / tot * 100).toFixed(1) + ' % (' + miss + ' / ' + tot + ') |');
  }
  md.push('');
}

// ---- own_cache 16 (committed) against 64 (throwaway edit), wild, Windows ----
{
  const med = (v) => { const s = [...v].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
  const by = { 'probe_bi.exe': [], 'probe_bi64.exe': [] };
  for (const l of fs.readFileSync(dir + 'probe_counts_size64.txt', 'utf8').split(nl)) {
    const m = l.match(/^(probe_bi(?:64)?\.exe) run\d+ TIER1 hits=(\d+) misses=(\d+) LANE seed=(\d+) memo_key=(\d+) .* accepted=(\d+)/);
    if (m) by[m[1]].push({ hits: +m[2], misses: +m[3], memo: +m[5], acc: +m[6], seed: +m[4] });
  }
  assert.equal(by['probe_bi.exe'].length, 5); assert.equal(by['probe_bi64.exe'].length, 5);
  md.push('### own_cache size, wild, Windows, sefer, median of 5 runs (throwaway probe builds)', '', '| OWN_CACHE_SIZE | memo_key exits | hash probes (own_cache misses) | hash probes / lane frees |', '|---:|---:|---:|---:|');
  for (const [name, size] of [['probe_bi.exe', 16], ['probe_bi64.exe', 64]]) {
    const r = by[name];
    const memo = med(r.map((x) => x.memo)), miss = med(r.map((x) => x.misses)), tot = med(r.map((x) => x.memo + x.acc + x.seed));
    md.push('| ' + size + ' | ' + memo + ' | ' + miss + ' | ' + (miss / tot * 100).toFixed(1) + ' % (' + miss + ' / ' + tot + ') |');
  }
  md.push('');
}
fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/WILD1_GAP_DECOMPOSITION_summary.csv', csv.join('\n') + '\n');
fs.writeFileSync('docs/perf/WILD1_GAP_DECOMPOSITION_bursts_summary.csv', csv2.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  fs.writeFileSync('docs/perf/WILD1_GAP_DECOMPOSITION.md', fs.readFileSync(dir + 'doc_head.md', 'utf8') + md.join('\n') + '\n' + fs.readFileSync(dir + 'doc_tail.md', 'utf8'));
}
console.log(md.join('\n'));
