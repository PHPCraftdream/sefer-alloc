#!/usr/bin/env node
// RP-2 report generator (measurement only): every number of docs/perf/RP2_REMOTE_FREE_RECLAIM.md comes from
// docs/perf/rp2/ through this script; ratios are asserted against the printed values.
// Input: per-process latency lines of the g1_rp1_probe, refill-path counters of a scratch-patched build,
// callgrind totals and call-tree extracts, three experiment series (pool 8, no release after drain, discovery skip).
// Output: docs/perf/rp2/tables.md, docs/perf/RP2_REMOTE_FREE_RECLAIM_summary.csv and, when doc_head.md / doc_tail.md
// exist, docs/perf/RP2_REMOTE_FREE_RECLAIM.md.
import fs from 'node:fs';
import assert from 'node:assert/strict';

const dir = 'docs/perf/rp2/';
const nl = new RegExp('\\r?\\n');
const rd = (n) => fs.readFileSync(dir + n, 'utf8');
const num = (s) => Number(String(s).replace(/,/g, ''));
const f1 = (v) => v.toFixed(1);
const f2 = (v) => v.toFixed(2);
const f3 = (v) => v.toFixed(3);
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const md = [], csv = ['table,case,metric,a,b,ratio_a_over_b'];
const add = (t, c, m, a, b) => csv.push([t, c, m, a, b, f3(a / b)].join(','));

const rows = (file) => rd(file).split(nl).filter((l) => l.startsWith('rp1 ')).map((l) => {
  const o = { who: l.split(' ')[1] };
  for (const m of l.matchAll(/(\w+)=([\d.]+)/g)) o[m[1]] = Number(m[2]);
  return o;
});
const sel = (rs, who, segs, size) => rs.filter((r) => r.who === who && r.segs === segs && r.size === size);

// ---- A. latency of the owner after a wave of foreign frees ----
const lat = rows('latency_raw.txt');
md.push('### A. Owner re-allocation after a wave of foreign frees (Linux, 3 freeing threads, medians of 7 processes per arm)', '',
  '| shape | metric | sefer | mimalloc | sefer / mi |', '|---|---|---:|---:|---:|');
const CASES = [[4, 64], [16, 64], [64, 64], [16, 1024]];
const METRICS = [['wave_ms', 'wave of foreign frees (ms)'], ['sum_ms', 'owner phase, sum of all alloc calls (ms)'], ['mean_ns', 'mean per alloc (ns)'], ['p99_ns', 'p99 (ns)'], ['p999_ns', 'p99.9 (ns)'], ['max_ns', 'longest single alloc (ns)'], ['over10us_ms', 'time in calls over 10 us (ms)']];
for (const [segs, size] of CASES) {
  const S = sel(lat, 'sefer', segs, size), M = sel(lat, 'mi', segs, size);
  assert.equal(S.length, 7, `${segs}x${size}: 7 sefer runs`); assert.equal(M.length, 7, `${segs}x${size}: 7 mi runs`);
  for (const [k, label] of METRICS) {
    const a = med(S.map((r) => r[k])), b = med(M.map((r) => r[k]));
    md.push(`| ${segs} seg x ${size} B (${S[0].total} blocks) | ${label} | ${k.endsWith('_ns') ? Math.round(a) : f2(a)} | ${k.endsWith('_ns') ? Math.round(b) : f2(b)} | ${f2(a / b)} |`);
    add('latency', `${segs}x${size}`, k, a, b);
  }
  const wa = med(S.map((r) => r.wave_ms)) + med(S.map((r) => r.sum_ms)), wb = med(M.map((r) => r.wave_ms)) + med(M.map((r) => r.sum_ms));
  md.push(`| ${segs} seg x ${size} B | wave + owner phase, sum of medians (ms) | ${f2(wa)} | ${f2(wb)} | ${f2(wa / wb)} |`);
  add('latency', `${segs}x${size}`, 'wave_plus_owner_ms', wa, wb);
}
md.push('');

// ---- B. refill-path counters (scratch-patched build) ----
const kv = (file) => { const l = rd(file).split(nl).find((x) => x.startsWith('rpc ')); assert.ok(l, file + ': rpc line'); const o = {}; for (const m of l.matchAll(/(\w+)=(\d+)/g)) o[m[1]] = Number(m[2]); return o; };
md.push('### B. Where the owner gets its blocks from (refill-path counters of a scratch-patched build, owner phase only)', '',
  '| shape | refills | blocks wanted | from current segment freelist | from class cursor | from discovery hit | from bump-carve | other | discovery calls / hits | segments reserved | drained: pooled / released / kept | ingress calls / slots |', '|---|---:|---:|---:|---:|---:|---:|---:|---|---:|---|---|');
const CF = [['counters_4x64.txt', '4 seg x 64 B'], ['counters_16x64.txt', '16 seg x 64 B'], ['counters_pool8_16x64.txt', '16 seg x 64 B, pool 8'], ['counters_16x1024.txt', '16 seg x 1024 B']];
const cnt = {};
for (const [f, label] of CF) {
  const c = kv(f); cnt[label] = c;
  const other = c.want_sum - c.from_cur_freelist - c.from_cursor - c.from_found_seg - c.from_carve;
  const pct = (x) => `${x} / ${c.want_sum} = ${f3(x / c.want_sum)}`;
  assert.equal(c.want_sum, c.refills * 16, label + ': 16 slots per refill');
  md.push(`| ${label} | ${c.refills} | ${c.want_sum} | ${pct(c.from_cur_freelist)} | ${pct(c.from_cursor)} | ${pct(c.from_found_seg)} | ${pct(c.from_carve)} | ${other} | ${c.find_calls} / ${c.find_some} | ${c.reserve} | ${c.drain_pooled} / ${c.drain_released} / ${c.drain_live_changed} | ${c.ingress_calls} / ${c.ingress_slots} |`);
  csv.push(['refill_counters', label, 'from_carve_over_wanted', c.from_carve, c.want_sum, f3(c.from_carve / c.want_sum)].join(','));
}
md.push('');
const snaps = rd('counters_16x64.txt').split(nl).filter((l) => l.startsWith('snap ')).map((l) => Object.fromEntries([...l.matchAll(/(\w+)=(\d+)/g)].map((m) => [m[1], Number(m[2])])));
md.push('### B2. Timeline of the 16 seg x 64 B case (cumulative, every 4th snapshot of 2048 refills)', '', '| refills | blocks from freelists | blocks from bump-carve | segments reserved | discovery calls |', '|---:|---:|---:|---:|---:|');
snaps.filter((_, i) => i % 4 === 0).forEach((s) => md.push(`| ${s.refills} | ${s.freelist_total} | ${s.carve} | ${s.reserve} | ${s.find_calls} |`));
md.push('');

// ---- C. callgrind of the owner phase ----
const cg = {};
for (const l of rd('cg_totals.txt').split(nl)) { const m = l.match(/^(\w+) allocs=(\d+) ([\d,]+)$/); if (m) (cg[m[1]] ??= {})[m[2]] = num(m[3]); }
const N = 943718;
md.push('### C. callgrind, owner phase = run with 943718 allocations minus run with 0 (16 seg x 64 B, harness statistics off)', '', '| arm | Ir, 0 allocs | Ir, 943718 allocs | owner phase Ir | Ir per block |', '|---|---:|---:|---:|---:|');
const ph = {};
for (const w of ['sefer', 'mi']) { assert.ok(cg[w] && cg[w]['0'] && cg[w][N]); ph[w] = cg[w][N] - cg[w]['0']; md.push(`| ${w} | ${cg[w]['0']} | ${cg[w][N]} | ${ph[w]} | ${f1(ph[w] / N)} |`); }
add('callgrind_owner_phase', '16x64', 'ir_per_block', ph.sefer / N, ph.mi / N);
md.push('', `sefer / mi = ${ph.sefer} / ${ph.mi} = ${f2(ph.sefer / ph.mi)}.`, '');
const tree = (f1n, f2n) => {
  const tc = rd(f1n), tc2 = rd(f2n);
  const one = (txt, re) => txt.split(nl).map((l) => l.match(re)).find(Boolean);
  const biggest = (txt, re) => txt.split(nl).map((l) => l.match(re)).filter(Boolean).reduce((x, y) => (!x || num(y[1]) > num(x[1]) ? y : x), null);
  const rm = one(tc, /^\s*([\d,]+) \(.*?\)\s+\*\s+\S*hot\.rs:<.*>::refill_magazine_miss\s*$/);
  const rmc = one(tc, /<\s+\S*hot\.rs:<.*>::refill_magazine_miss \(([\d,]+)x\)/);
  assert.ok(rm && rmc, 'refill_magazine_miss rows');
  const row = (name) => { const m = biggest(tc2, new RegExp('^\\s*([\\d,]+) \\(.*?\\)\\s+=>.*' + name + ' \\(([\\d,]+)x\\)')); return m ? { ir: num(m[1]), calls: num(m[2]) } : { ir: 0, calls: 0 }; };
  return { refill_magazine_miss: { ir: num(rm[1]), calls: num(rmc[1]) }, find_segment_with_free_impl: row('find_segment_with_free_impl'), process_ingress: row('process_ingress'), drain_segment_sidecar: row('drain_segment_sidecar'), try_carve_batch: row('try_carve_batch') };
};
const tN = tree('cg_tree_calls.txt', 'cg_tree_calls2.txt'), t0 = tree('cg0_tree_calls.txt', 'cg0_tree_calls2.txt');
md.push('### C2. Owner-phase share by function (sefer; inclusive Ir of the call tree of the run with 943718 allocations minus the run with 0, i.e. set-up and wave removed)', '', '| function | owner-phase inclusive Ir | owner-phase calls | Ir per call | Ir per owner block (/ 943718) |', '|---|---:|---:|---:|---:|');
const ow = {};
for (const [k, label] of [['refill_magazine_miss', 'refill_magazine_miss'], ['find_segment_with_free_impl', 'find_segment_with_free_impl (inside the previous)'], ['process_ingress', 'process_ingress (inside find_segment_with_free_impl)'], ['drain_segment_sidecar', 'drain_segment_sidecar (inside process_ingress)'], ['try_carve_batch', 'try_carve_batch']]) {
  const ir = tN[k].ir - t0[k].ir, calls = tN[k].calls - t0[k].calls;
  assert.ok(ir > 0 && calls > 0, k + ': positive owner-phase share');
  ow[k] = { ir, calls };
  md.push(`| ${label} | ${ir} | ${calls} | ${f1(ir / calls)} | ${f1(ir / N)} |`);
  csv.push(['callgrind_tree_owner_phase', '16x64', k, ir, calls, f3(ir / calls)].join(','));
}
md.push('');
md.push(`Cross-check against the counters of section B: owner-phase refills ${ow.refill_magazine_miss.calls} against ${cnt['16 seg x 64 B'].refills}, discovery calls ${ow.find_segment_with_free_impl.calls} against ${cnt['16 seg x 64 B'].find_calls}. Bump-carve costs ${f1(ow.try_carve_batch.ir / cnt['16 seg x 64 B'].from_carve)} Ir per carved block (${ow.try_carve_batch.ir} / ${cnt['16 seg x 64 B'].from_carve}); the sidecar drain costs ${f1(ow.drain_segment_sidecar.ir / N)} Ir per block of the ${N} blocks drained (${ow.drain_segment_sidecar.ir} / ${N}); discovery without its drains costs ${f1((ow.find_segment_with_free_impl.ir - ow.drain_segment_sidecar.ir) / ow.find_segment_with_free_impl.calls)} Ir per call.`, '');
assert.equal(ow.refill_magazine_miss.calls, cnt['16 seg x 64 B'].refills, 'refill counts agree');
assert.equal(ow.find_segment_with_free_impl.calls, cnt['16 seg x 64 B'].find_calls, 'discovery counts agree');

// ---- D. experiments ----
const e0 = rows('e0_pool8_raw.txt'), e1 = rows('e1_nodrainrelease_raw.txt'), e2 = rows('e2_raw.txt');
md.push('### D. Experiments (each a scratch change measured against the same base, interleaved, medians of 7 processes)', '',
  '| experiment | shape | metric | base | variant | mimalloc | variant / base |', '|---|---|---|---:|---:|---:|---:|');
const exp = (name, rs, baseWho, varWho, cases, keys, miWho = 'mi') => {
  for (const [segs, size] of cases) {
    for (const k of keys) {
      const b = sel(rs, baseWho, segs, size), v = sel(rs, varWho, segs, size), m = sel(rs, miWho, segs, size);
      assert.equal(b.length, 7, `${name} ${segs}x${size} base runs`); assert.equal(v.length, 7, `${name} ${segs}x${size} variant runs`);
      const a = med(b.map((r) => r[k])), c = med(v.map((r) => r[k])), x = m.length ? med(m.map((r) => r[k])) : NaN;
      md.push(`| ${name} | ${segs} seg x ${size} B | ${k} | ${f2(a)} | ${f2(c)} | ${Number.isNaN(x) ? '-' : f2(x)} | ${f3(c / a)} |`);
      add('experiment', `${name} ${segs}x${size}`, k, c, a);
    }
  }
};
exp('E0 pool 8 (Profile Throughput)', e0, 'sefer', 'sefer8', [[4, 64], [8, 64], [16, 64]], ['sum_ms', 'max_ns']);
exp('E1 no release/pool after a drain empties a segment', e1, 'base', 'e1', [[4, 64], [16, 64], [16, 1024]], ['sum_ms', 'max_ns']);
exp('E2 skip discovery for 63 refills after a miss (upper bound, unsafe)', e2, 'base', 'e2', [[16, 64]], ['sum_ms']);
md.push('');
const e2t = {};
for (const l of rd('e2_totals.txt').split(nl)) { const m = l.match(/^(\w+) (\w+)(?: allocs=(\d+))? ([\d,]+)$/); if (m) (e2t[m[1]] ??= {})[m[3] ? `${m[2]}:${m[3]}` : m[2]] = num(m[4]); }
md.push('### D2. Experiment E2 in callgrind Ir (upper bound; same binaries as the wall-clock row above)', '', '| scenario | base Ir | E2 Ir | E2 / base |', '|---|---:|---:|---:|');
const e2row = (label, a, b) => { md.push(`| ${label} | ${a} | ${b} | ${f3(b / a)} |`); add('experiment_ir', 'E2', label, b, a); };
e2row('rp1 owner phase (943718 allocs minus 0)', e2t.base['rp1:943718'] - e2t.base['rp1:0'], e2t.e2['rp1:943718'] - e2t.e2['rp1:0']);
e2row('g1_wild_judge grow64 (scale 0.3)', e2t.base.grow64, e2t.e2.grow64);
e2row('g1_wild_judge grow1k (scale 0.3)', e2t.base.grow1k, e2t.e2.grow1k);
md.push('');

fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/RP2_REMOTE_FREE_RECLAIM_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  const c16 = cnt['16 seg x 64 B'], c4 = cnt['4 seg x 64 B'];
  const mm = (rs, who, segs, size, k) => med(sel(rs, who, segs, size).map((r) => r[k]));
  const woRatio = (segs, size) => { const a = mm(lat, 'sefer', segs, size, 'wave_ms') + mm(lat, 'sefer', segs, size, 'sum_ms'), b = mm(lat, 'mi', segs, size, 'wave_ms') + mm(lat, 'mi', segs, size, 'sum_ms'); return `${f2(a)} / ${f2(b)} ms = ${f2(a / b)}`; };
  const e2ir = { a: e2t.base['rp1:943718'] - e2t.base['rp1:0'], b: e2t.e2['rp1:943718'] - e2t.e2['rp1:0'] };
  const S = sel(lat, 'sefer', 16, 64), M = sel(lat, 'mi', 16, 64);
  const subs = {
    CARVE_PCT: `${c16.from_carve} / ${c16.want_sum} = ${f3(c16.from_carve / c16.want_sum)}`,
    OWNER_RATIO: `${f1(med(S.map((r) => r.sum_ms)))} / ${f1(med(M.map((r) => r.sum_ms)))} ms = ${f2(med(S.map((r) => r.sum_ms)) / med(M.map((r) => r.sum_ms)))}`,
    MAX_PAUSE: `${f2(med(S.map((r) => r.max_ns)) / 1e6)} / ${f2(med(M.map((r) => r.max_ns)) / 1e6)} ms`,
    IR_BLOCK: `${f1(ph.sefer / N)} against ${f1(ph.mi / N)} Ir per block (${f2(ph.sefer / ph.mi)})`,
    DRAIN_OUT: `${c16.drain_pooled} pooled, ${c16.drain_released} released, ${c16.drain_live_changed} kept`,
    INGRESS: `${c16.ingress_slots} slots in ${c16.ingress_calls} call`,
    PAUSE_SEG: [[4, 64], [16, 64], [64, 64]].map(([sg, sz]) => { const x = med(sel(lat, 'sefer', sg, sz).map((r) => r.max_ns)); return `${f2(x / 1e6)} ms / ${sg} = ${f2(x / 1e6 / sg)} ms`; }).join('; '),
    WO_16_64: woRatio(16, 64),
    WO_16_1024: woRatio(16, 1024),
    DRAIN_IRB: f1(ow.drain_segment_sidecar.ir / N),
    DISC_IR: f1((ow.find_segment_with_free_impl.ir - ow.drain_segment_sidecar.ir) / ow.find_segment_with_free_impl.calls),
    CARVE_IRB: f1(ow.try_carve_batch.ir / c16.from_carve),
    POOL8_CARVE: `${f3(cnt['16 seg x 64 B, pool 8'].from_carve / cnt['16 seg x 64 B, pool 8'].want_sum)} against ${f3(c16.from_carve / c16.want_sum)}`,
    POOL8_SUM: `${f2(mm(e0, 'sefer8', 16, 64, 'sum_ms'))} / ${f2(mm(e0, 'sefer', 16, 64, 'sum_ms'))} ms = ${f3(mm(e0, 'sefer8', 16, 64, 'sum_ms') / mm(e0, 'sefer', 16, 64, 'sum_ms'))}`,
    E1_16_64: `${f2(mm(e1, 'e1', 16, 64, 'sum_ms'))} / ${f2(mm(e1, 'base', 16, 64, 'sum_ms'))} = ${f3(mm(e1, 'e1', 16, 64, 'sum_ms') / mm(e1, 'base', 16, 64, 'sum_ms'))}`,
    E1_16_1024: `${f2(mm(e1, 'e1', 16, 1024, 'sum_ms'))} / ${f2(mm(e1, 'base', 16, 1024, 'sum_ms'))} = ${f3(mm(e1, 'e1', 16, 1024, 'sum_ms') / mm(e1, 'base', 16, 1024, 'sum_ms'))}`,
    E1_4_MAX: `${Math.round(mm(e1, 'e1', 4, 64, 'max_ns'))} / ${Math.round(mm(e1, 'base', 4, 64, 'max_ns'))} ns = ${f3(mm(e1, 'e1', 4, 64, 'max_ns') / mm(e1, 'base', 4, 64, 'max_ns'))}`,
    E2_IR: `${e2ir.b} / ${e2ir.a} = ${f3(e2ir.b / e2ir.a)}`,
    E2_WALL: `${f2(mm(e2, 'e2', 16, 64, 'sum_ms'))} / ${f2(mm(e2, 'base', 16, 64, 'sum_ms'))} = ${f3(mm(e2, 'e2', 16, 64, 'sum_ms') / mm(e2, 'base', 16, 64, 'sum_ms'))}`,
    E2_GROW: `${f3(e2t.e2.grow64 / e2t.base.grow64)} (${e2t.e2.grow64} / ${e2t.base.grow64}) and ${f3(e2t.e2.grow1k / e2t.base.grow1k)} (${e2t.e2.grow1k} / ${e2t.base.grow1k})`,
    C4_CARVE: String(c4.from_carve),
    RESERVE: String(c16.reserve),
  };
  let head = rd('doc_head.md'), tail = rd('doc_tail.md');
  for (const [k, v] of Object.entries(subs)) { head = head.split(`{{${k}}}`).join(v); tail = tail.split(`{{${k}}}`).join(v); }
  assert.ok(!/\{\{[A-Z_0-9]+\}\}/.test(head + tail), 'unfilled placeholder');
  fs.writeFileSync('docs/perf/RP2_REMOTE_FREE_RECLAIM.md', head + md.join('\n') + '\n' + tail);
}
console.log(md.join('\n'));
