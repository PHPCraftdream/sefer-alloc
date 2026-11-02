#!/usr/bin/env node
// MT1 T=1 decomposition report generator (measurement only): every number of
// docs/perf/MT1_T1_DECOMPOSITION.md comes from docs/perf/mt1c/ through this script; ratios and
// differences are asserted against the printed values.
// Input: callgrind totals, callgrind call-tree extracts, perf stat runs (7 / 5 interleaved rounds),
// perf record symbol shares. Output: docs/perf/mt1c/tables.md, docs/perf/MT1_T1_DECOMPOSITION_summary.csv
// and, when doc_head.md / doc_tail.md exist, docs/perf/MT1_T1_DECOMPOSITION.md.
import fs from 'node:fs';
import assert from 'node:assert/strict';

const dir = 'docs/perf/mt1c/';
const nl = new RegExp('\\r?\\n');
const rd = (n) => fs.readFileSync(dir + n, 'utf8');
const num = (s) => Number(String(s).replace(/,/g, ''));
const f3 = (v) => v.toFixed(3);
const f1 = (v) => v.toFixed(1);
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const md = [], csv = ['table,workload,metric,sefer,mi,ratio'];
const row = (t, w, m, s, i) => { const r = s / i; csv.push([t, w, m, s, i, f3(r)].join(',')); return r; };

// ---- callgrind totals (300 000 operations, T=1) ----
const tot = {};
for (const l of rd('totals.txt').split(nl)) { const m = l.match(/^(\w+) (\w+) ([\d,]+)$/); if (m) (tot[m[1]] ??= {})[m[2]] = num(m[3]); }
md.push('### callgrind Ir, T=1, 300 000 steps (Linux)', '', '| workload | sefer (base) | sefer (WILD-2) | mimalloc | sefer / mi | WILD-2 - base (Ir) |', '|---|---:|---:|---:|---:|---:|');
for (const w of ['mstress', 'larson']) {
  const t = tot[w]; assert.ok(t && t.base && t.x && t.mi, w + ': totals');
  const r = row('callgrind_ir', w, 'program_total', t.base, t.mi);
  assert.equal(f3(t.base / t.mi), f3(r));
  md.push(`| ${w} | ${t.base} | ${t.x} | ${t.mi} | ${f3(r)} (${t.base} / ${t.mi}) | ${t.x - t.base} |`);
}
md.push('');

// ---- per-call inclusive Ir of the allocator entry points (call-tree extract) ----
const shim = (w, arm) => {
  const out = {};
  for (const l of rd(`calls_shim.${w}.${arm}.txt`).split(nl)) {
    const m = l.match(/^\s*([\d,]+) \(.*?\)\s+=>.*?(alloc_shim|dealloc_shim)::<.*\(([\d,]+)x\)/);
    if (m) out[m[2]] = { ir: num(m[1]), calls: num(m[3]) };
  }
  return out;
};
md.push('### Inclusive Ir per allocator call (callgrind call tree, shim frame included)', '', '| workload | entry | sefer Ir | calls | Ir / call | mi Ir | calls | Ir / call | sefer - mi per call |', '|---|---|---:|---:|---:|---:|---:|---:|---:|');
const perCall = {};
for (const w of ['mstress', 'larson']) {
  const s = shim(w, 'sefer'), m = shim(w, 'mi');
  for (const e of ['alloc_shim', 'dealloc_shim']) {
    assert.ok(s[e] && m[e], `${w} ${e}: call-tree rows`);
    assert.equal(s[e].calls, m[e].calls, `${w} ${e}: equal call counts`);
    const a = s[e].ir / s[e].calls, b = m[e].ir / m[e].calls;
    perCall[w + e] = { a, b, s: s[e], m: m[e] };
    csv.push(['inclusive_ir_per_call', w, e, f1(a), f1(b), f3(a / b)].join(','));
    md.push(`| ${w} | ${e} | ${s[e].ir} | ${s[e].calls} | ${f1(a)} | ${m[e].ir} | ${m[e].calls} | ${f1(b)} | ${f1(a - b)} |`);
  }
}
md.push('');

// ---- cold magazine events (sefer park / refill against mimalloc generic / retire) ----
const first = (txt, re, what) => { const m = txt.split(nl).map((l) => l.match(re)).find(Boolean); assert.ok(m, 'no row for ' + what); return m; };
md.push('### Cold events (callgrind call tree, mstress, 300 000 steps)', '', '| event | Ir (inclusive) | calls |', '|---|---:|---:|');
const cs = rd('calls.mstress.sefer.txt'), cm = rd('calls.mstress.mi.txt');
const full = first(cs, /^\s*([\d,]+) \(.*?\)\s+\*\s+\S*dealloc_own_base\.rs:<.*>::free_magazine_full_fast /, 'full_fast');
const park = first(cs, /^\s*([\d,]+) \(.*?\)\s+<\s+\S*dealloc_own_base\.rs:<.*>::free_magazine_full_fast \(([\d,]+)x\)/, 'park');
const refill = first(cs, /^\s*([\d,]+) \(.*?\)\s+=>\s+\S*hot\.rs:<.*>::refill_magazine_miss \(([\d,]+)x\)/, 'refill');
const gen = first(cm, /^\s*([\d,]+) \(.*?\)\s+=>\s+\S*page\.c:_mi_malloc_generic \(([\d,]+)x\)/, 'generic');
const ret = first(cm, /^\s*([\d,]+) \(.*?\)\s+\*\s+\S*page\.c:_mi_page_retire /, 'retire');
md.push(`| sefer free_magazine_full_fast (includes park) | ${num(full[1])} | ${num(park[2])} (park calls) |`, `| sefer refill_magazine_miss | ${num(refill[1])} | ${num(refill[2])} |`, `| mimalloc _mi_malloc_generic | ${num(gen[1])} | ${num(gen[2])} |`, `| mimalloc _mi_page_retire | ${num(ret[1])} | not used |`);
const evS = num(full[1]) + num(refill[1]), evM = num(gen[1]) + num(ret[1]);
const gap = tot.mstress.base - tot.mstress.mi;
md.push('', `Cold-event Ir: sefer ${evS} (${evS} / ${tot.mstress.base} = ${f3(evS / tot.mstress.base)} of its run), mimalloc ${evM} (${evM} / ${tot.mstress.mi} = ${f3(evM / tot.mstress.mi)}); difference ${evS - evM} of the whole-run gap ${gap} (${evS - evM} / ${gap} = ${f3((evS - evM) / gap)}).`, '');
csv.push(['cold_events_ir', 'mstress', 'sefer_vs_mi', evS, evM, f3(evS / evM)].join(','));

// ---- perf stat (user mode), 3 000 000 steps ----
const stat = (file, re, keys) => {
  const cells = {};
  for (const l of rd(file).split(nl)) {
    const m = l.match(re); if (!m) continue;
    const c = (cells[m[2] + ' ' + m[3]] ??= keys.map(() => []));
    keys.forEach((_, i) => c[i].push(Number(m[4 + i])));
  }
  return cells;
};
const ps = stat('perf_stat_raw.txt', /^run(\d+) (\w+) (\w+) (\d+),,cycles:u.*? (\d+),,instructions:u.*? (\d+),,L1-dcache-load-misses/, ['cyc', 'ins', 'l1']);
md.push('### perf stat, user mode, 3 000 000 steps, medians of 7 interleaved runs (Linux)', '', '| workload | cycles sefer (M) | cycles mi (M) | sefer / mi | instructions sefer (M) | instructions mi (M) | sefer / mi | IPC sefer | IPC mi | L1D misses sefer / mi | min-to-max cycles sefer (M) |', '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|');
const cyc = {};
for (const w of ['mstress', 'larson']) {
  const s = ps[w + ' sefer'], m = ps[w + ' mi']; assert.ok(s && m && s[0].length === 7 && m[0].length === 7, w + ': 7 runs');
  const [cs0, ci0, ls] = s.map(med), [cm0, cm1, lm] = m.map(med);
  cyc[w] = { s: cs0, m: cm0 };
  const rc = row('perf_stat_cycles', w, 'median_cycles', cs0, cm0), ri = row('perf_stat_instructions', w, 'median_instructions', ci0, cm1), rl = row('perf_stat_l1d_misses', w, 'median_misses', ls, lm);
  md.push(`| ${w} | ${f1(cs0 / 1e6)} | ${f1(cm0 / 1e6)} | ${f3(rc)} | ${f1(ci0 / 1e6)} | ${f1(cm1 / 1e6)} | ${f3(ri)} | ${f3(ci0 / cs0)} | ${f3(cm1 / cm0)} | ${f3(rl)} | ${f1(Math.min(...s[0]) / 1e6)}-${f1(Math.max(...s[0]) / 1e6)} |`);
}
md.push('');

// ---- branch / icache / dTLB / loads (5 runs) ----
const br = stat('perf_br_raw.txt', /^run(\d+) (\w+) (\w+) (\d+),,branches:u.*? (\d+),,branch-misses:u.*? (\d+),,L1-icache-load-misses/, ['b', 'bm', 'ic']);
const tl = stat('perf_tlb_raw.txt', /^run(\d+) (\w+) (\w+) (\d+),,dTLB-load-misses.*? (\d+),,dTLB-loads.*? (\d+),,L1-dcache-loads/, ['tm', 'tl', 'l1']);
md.push('### Other counters, medians of 5 interleaved runs (3 000 000 steps)', '', '| workload | branches sefer / mi (M) | branch misses sefer / mi (M) | L1I misses sefer / mi (k) | dTLB load misses sefer / mi | L1D loads sefer / mi (M) |', '|---|---|---|---|---|---|');
for (const w of ['mstress', 'larson']) {
  const bs = br[w + ' sefer'].map(med), bm = br[w + ' mi'].map(med), ts = tl[w + ' sefer'].map(med), tm = tl[w + ' mi'].map(med);
  assert.ok(br[w + ' sefer'][0].length === 5 && tl[w + ' mi'][0].length === 5, w + ': 5 runs');
  row('branch_misses', w, 'median', bs[1], bm[1]); row('l1d_loads', w, 'median', ts[2], tm[2]); row('dtlb_load_misses', w, 'median', ts[0], tm[0]);
  md.push(`| ${w} | ${f1(bs[0] / 1e6)} / ${f1(bm[0] / 1e6)} | ${f3(bs[1] / 1e6)} / ${f3(bm[1] / 1e6)} | ${f1(bs[2] / 1e3)} / ${f1(bm[2] / 1e3)} | ${ts[0]} / ${tm[0]} | ${f1(ts[2] / 1e6)} / ${f1(tm[2] / 1e6)} |`);
}
md.push('');

// ---- larson: the share of std mpmc try_recv in cycles (perf record) ----
const share = (w, arm) => {
  const m = first(rd(`perf_sym2.${w}.${arm}.txt`), /^\s*([\d.]+)%\s+\[\.\]\s+\S*4mpmc\S*8Receiver/, `${w} ${arm} mpmc Receiver method`);
  return Number(m[1]);
};
const ts = share('larson', 'sefer'), tm = share('larson', 'mi');
const cS = cyc.larson.s * ts / 100, cM = cyc.larson.m * tm / 100, gapC = cyc.larson.s - cyc.larson.m;
md.push('### larson: cycles inside std mpmc try_recv (perf record cycles:u share x perf stat cycles)', '', `sefer ${ts} % of ${f1(cyc.larson.s / 1e6)} M = ${f1(cS / 1e6)} M cycles; mimalloc ${tm} % of ${f1(cyc.larson.m / 1e6)} M = ${f1(cM / 1e6)} M cycles; difference ${f1((cS - cM) / 1e6)} M of the cycle gap ${f1(gapC / 1e6)} M (${f1((cS - cM) / 1e6)} / ${f1(gapC / 1e6)} = ${f3((cS - cM) / gapC)}). Sampling shares carry skid; this row is an attribution hint, not a gate.`, '');
csv.push(['larson_try_recv_cycles', 'larson', 'perf_record_share_x_stat', Math.round(cS), Math.round(cM), f3(cS / cM)].join(','));

fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/MT1_T1_DECOMPOSITION_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  const pa = perCall.mstressalloc_shim, pd = perCall.mstressdealloc_shim;
  const pairS = pa.s.ir + pd.s.ir, pairM = pa.m.ir + pd.m.ir, pairGap = pairS - pairM;
  assert.equal(pa.s.calls, pd.s.calls, 'alloc and free calls pair up');
  const sub = (t) => t.split('{{FREE_S}}').join(f1(pd.a)).split('{{FREE_M}}').join(f1(pd.b))
    .split('{{ALLOC_S}}').join(f1(pa.a)).split('{{ALLOC_M}}').join(f1(pa.b))
    .split('{{PAIR_S}}').join(f1(pairS / pa.s.calls)).split('{{PAIR_M}}').join(f1(pairM / pa.s.calls))
    .split('{{PAIR_GAP}}').join(`${pairGap} Ir (${pairGap} / ${pa.s.calls} = ${f1(pairGap / pa.s.calls)} per pair)`)
    .split('{{PAIR_FRAC}}').join(`${pairGap} / ${gap} = ${f3(pairGap / gap)}`);
  fs.writeFileSync('docs/perf/MT1_T1_DECOMPOSITION.md', sub(rd('doc_head.md')) + md.join('\n') + '\n' + sub(rd('doc_tail.md')));
}
console.log(md.join('\n'));
