// J14: applies the gates of docs/perf/J14_ADAPTIVE_LARGE_CACHE_PREREG.md to the paired campaigns docs/perf/j14/{win,linux}_raw.jsonl.gz
// (written by docs/perf/j14/run.mjs) and derives docs/perf/J14_ADAPTIVE_LARGE_CACHE.md and its _summary.csv. Arms: base, x (the policy), xt (x plus the
// J13 hint, Linux, read only), base2 (byte-identical copy of base, the A/A control) and mi (mimalloc, from the base binary). Statistic as in J5-J13: per
// block and arm the minimum over the repeats of the metric, d = ln(a / b), cell = exp(mean d), two-sided 95 % t-interval (11 degrees of freedom, 2.201).
// Asserts every printed number.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
const T975_11 = 2.201;
const MIB = 1 << 20;
const f3 = (x) => x.toFixed(3);
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
const sdev = (v) => { const m = mean(v); return Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (v.length - 1)); };
const median = (v) => { const s = [...v].sort((a, b) => a - b); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
function cell(logs) {
  assert.equal(logs.length - 1, 11, 'twelve blocks');
  const m = mean(logs), half = T975_11 * sdev(logs) / Math.sqrt(logs.length);
  const [r, lo, hi] = [Math.exp(m), Math.exp(m - half), Math.exp(m + half)];
  assert.ok(lo <= r && r <= hi);
  return { r, lo, hi };
}
const contains1 = (c) => c.lo <= 1 && 1 <= c.hi;
const fmt = (c) => (c ? `${f3(c.r)} (${f3(c.lo)}–${f3(c.hi)})` : '–');

const platforms = [
  { id: 'windows', name: 'Windows', raw: 'j14/win_raw.jsonl', ident: 'j14/win_ident.txt' },
  { id: 'linux', name: 'Linux (WSL2)', raw: 'j14/linux_raw.jsonl', ident: 'j14/linux_ident.txt' },
];
const gz = (p) => (existsSync(new URL(`${p}.gz`, dir)) ? gunzipSync(readFileSync(new URL(`${p}.gz`, dir))).toString('utf8') : readFileSync(new URL(p, dir), 'utf8'));
const fails = [];
let csv = 'platform,scenario,size_mib,k,arm_pair,metric,ratio,lo,hi,aa_lo,aa_hi\n';
let body = '';
const gateRows = [];
for (const p of platforms) {
  const recs = gz(p.raw).trim().split(/\r?\n/).map((l) => JSON.parse(l));
  const ident = readFileSync(new URL(p.ident, dir), 'utf8');
  const id = (re) => { const m = re.exec(ident); assert.ok(m, `${p.id} identity ${re}`); return m[1]; };
  const [binBase, binX, binBase2] = [id(/^bin-base ([0-9a-f]{64})$/m), id(/^bin-x ([0-9a-f]{64})$/m), id(/^bin-base2 ([0-9a-f]{64})$/m)];
  assert.equal(binBase, binBase2, 'base2 is a byte-identical copy of base');
  const hasXt = recs.some((r) => r.arm === 'xt');
  const arms = ['base', 'x', 'base2', 'mi', ...(hasXt ? ['xt'] : [])];
  const key = (r) => `${r.r.scenario}|${r.r.size_mib}|${r.r.k}`;
  const keys = [...new Set(recs.map(key))];
  const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
  const rows = [];
  for (const k of keys) {
    const [sc, size, kk] = k.split('|');
    const cr = recs.filter((r) => key(r) === k);
    const per = (arm, f) => blocks.map((b) => {
      const v = cr.filter((r) => r.block === b && r.arm === arm).map((r) => f(r.r));
      assert.ok(v.length >= 1 && v.every((x) => x >= 0), `samples ${k} ${arm} ${b}`);
      return Math.min(...v);
    });
    const ratio = (a, b, f) => {
      const [pa, pb] = [per(a, f), per(b, f)];
      return [...pa, ...pb].some((x) => x === 0) ? null : cell(pa.map((x, i) => Math.log(x / pb[i])));
    };
    const med = (arm, f) => median(cr.filter((r) => r.arm === arm).map((r) => f(r.r)));
    const row = { sc, size: Number(size), k: Number(kk) };
    for (const m of ['cold', 'warm']) {
      const f = (r) => r[`${m}_ns`];
      row[m] = { xb: ratio('x', 'base', f), aa: ratio('base2', 'base', f), xmi: ratio('x', 'mi', f), bmi: ratio('base', 'mi', f), xtmi: hasXt ? ratio('xt', 'mi', f) : null, xtb: hasXt ? ratio('xt', 'base', f) : null };
    }
    for (const a of arms) { row[`end_${a}`] = med(a, (r) => r.end_rss) / MIB; row[`peak_${a}`] = med(a, (r) => r.peak_rss) / MIB; row[`faults_${a}`] = med(a, (r) => r.faults); }
    rows.push(row);
    for (const m of ['cold', 'warm']) {
      for (const [pair, c] of [['x/base', row[m].xb], ['x/mi', row[m].xmi], ['base/mi', row[m].bmi], ['xt/mi', row[m].xtmi], ['xt/base', row[m].xtb]]) {
        if (c) csv += [p.id, sc, size, kk, pair, m, f3(c.r), f3(c.lo), f3(c.hi), f3(row[m].aa?.lo ?? NaN), f3(row[m].aa?.hi ?? NaN)].join(',') + '\n';
      }
    }
  }
  const order = ['many', 'idle', 'cycle', 'grow', 'edge'];
  rows.sort((a, b) => order.indexOf(a.sc) - order.indexOf(b.sc) || a.size - b.size);
  const get = (sc, size, k) => rows.find((r) => r.sc === sc && r.size === size && (!k || r.k === k));
  // --- gates ---
  for (const [size, k] of [[16, 128], [128, 16]]) {
    const c = get('many', size, k);
    const t = c.warm;
    if (!contains1(t.aa)) fails.push(`${p.name} T1: many ${k} x ${size} MiB unreadable (A/A ${fmt(t.aa)} excludes 1)`);
    else {
      if (!(t.xmi.r <= 2.0)) fails.push(`${p.name} T1: many ${k} x ${size} MiB warm x / mimalloc ${fmt(t.xmi)} above 2.0`);
      if (!(t.xb.hi < 0.5)) fails.push(`${p.name} T1: many ${k} x ${size} MiB warm x / base ${fmt(t.xb)}: upper bound not below 0.5`);
    }
    gateRows.push(`${p.name} many ${k} x ${size} MiB warm: x/mi ${fmt(t.xmi)}, x/base ${fmt(t.xb)}${t.xtmi ? `, xt/mi ${fmt(t.xtmi)}` : ''}`);
  }
  const idle = get('idle', 16, 128);
  assert.ok(idle, 'idle cell');
  if (idle.end_x > idle.end_base + 32) fails.push(`${p.name} T2(a): idle end RSS x ${idle.end_x.toFixed(0)} MiB above base ${idle.end_base.toFixed(0)} + 32 MiB`);
  gateRows.push(`${p.name} idle end RSS MiB: base ${idle.end_base.toFixed(0)}, x ${idle.end_x.toFixed(0)}, mi ${idle.end_mi.toFixed(0)}${hasXt ? `, xt ${idle.end_xt.toFixed(0)}` : ''}`);
  for (const [sc, size, k] of [['cycle', 1, 1], ['cycle', 128, 1], ['grow', 128, 1], ['edge', 128, 1]]) {
    const c = get(sc, size, k);
    for (const m of ['cold', 'warm']) {
      const t = c[m];
      if (!t.xb || !t.aa || !contains1(t.aa)) continue; // below timer resolution or unreadable: not a gate
      if (t.xb.lo > 1.05) fails.push(`${p.name} T2(c): ${sc} ${size} MiB ${m} x/base ${fmt(t.xb)} wholly above 1.05`);
    }
  }
  for (const c of rows) {
    for (const m of ['cold', 'warm']) {
      const t = c[m];
      if (t.xb && t.aa && contains1(t.aa) && t.xb.lo > 1.10) fails.push(`${p.name} KILL: ${c.sc} ${c.size} MiB ${m} x/base ${fmt(t.xb)} wholly above 1.10`);
    }
  }
  const tbl = rows.map((c) => `| ${c.sc}${c.sc === 'many' || c.sc === 'idle' ? ` ${c.k} x` : ''} ${c.size} MiB | ${fmt(c.cold.xb)} | ${fmt(c.warm.xb)} | ${fmt(c.warm.aa)} | ${fmt(c.warm.bmi)} → ${fmt(c.warm.xmi)}${c.warm.xtmi ? ` → ${fmt(c.warm.xtmi)}` : ''} | ${c.end_base.toFixed(0)} → ${c.end_x.toFixed(0)}${hasXt ? ` → ${c.end_xt.toFixed(0)}` : ''} / ${c.end_mi.toFixed(0)} | ${c.faults_base} → ${c.faults_x} |`).join('\n');
  body += `### ${p.name}

Binaries (sha256): base \`${binBase}\`, x \`${binX}\`${hasXt ? `, xt \`${id(/^bin-xt ([0-9a-f]{64})$/m)}\`` : ''}; base2 is a byte-identical copy of base. \`docs/perf/${p.ident}\`.

| cell | cold x / base | warm x / base | warm A/A | warm sefer / mimalloc: base → x${hasXt ? ' → xt' : ''} | end RSS MiB base → x${hasXt ? ' → xt' : ''} / mi | faults base → x |
|---|---|---|---|---|---|---|
${tbl}

`;
}
writeFileSync(new URL('J14_ADAPTIVE_LARGE_CACHE_summary.csv', dir), csv);
const decision = fails.length === 0 ? 'GO' : fails.some((f) => f.includes('KILL') || f.includes('T2')) ? 'NO-GO' : 'NO-GO (benefit gate not met)';
const md = `# J14 — an adaptive Large cache that learns the reuse habit: ${decision} (\`bench\` report of an opt-in \`perf(opt-in)\` candidate)

Generated by \`node scripts/j14_report.mjs\` from \`docs/perf/j14/{win,linux}_raw.jsonl.gz\` (written by \`node docs/perf/j14/run.mjs <win|linux> <bin dir> <out> 12 2\`, then gzipped); summary:
\`docs/perf/J14_ADAPTIVE_LARGE_CACHE_summary.csv\`. Gates: \`docs/perf/J14_ADAPTIVE_LARGE_CACHE_PREREG.md\` (committed before the measurement; Amendment 1 adds the read-only arm xt). Do not edit by hand.

## 1. Method

- Entry point: real \`SeferAlloc\` and \`MiMalloc\` through the \`#[inline(never)]\` shims of \`examples/g1_big_judge.rs\` (the shipped layer). base = main plus the judge's \`idle\` scenario, built \`--features production\`; x = the J14 tree built \`--features "production large-cache-adaptive"\`;
  xt = x plus the J13 hint (tag \`archive/g1-j14-xt\`, Linux only, read only); base2 = byte-identical copy of base (A/A control); the mimalloc arm runs from the base binary. 12 blocks x 2 repeats per arm and cell (\`idle\`: 1), cells shuffled per block, arm order rotated by block.
- Cells: \`many\` 128 x 16 MiB and 16 x 128 MiB (the reuse habit: 5 rounds of allocate, touch every page, free in reverse; **cold** = round 1, **warm** = the best of rounds 2-5), \`idle\` (the 128 x 16 MiB habit for 3 rounds, then 6 s of one 4 MiB block allocated and freed every 100 ms,
  then end RSS), controls \`cycle\` 1 MiB and 128 MiB, \`grow\` 128 MiB, \`edge\` 128 MiB.
- Statistic: per block and arm the minimum over repeats, d = ln(a / b), cell = exp(mean d), 95 % t-interval (11 degrees of freedom, 2.201); a cell is read only if its A/A interval (base2 / base) contains 1.
- T0 (path activation) is carried by \`tests/j14_large_cache_adaptive.rs\` (mistakes counted, a whole 80-span round served from the cache after learning, extra under the heap's peak, habit forgotten; green on CI) and by the fault counts below (x re-faults far less than base in the same cells);
  the timed builds carry no allocator counters, so section 3 of the preregistration (a counter activation run) was not taken from them.

## 2. Result

${body}## 3. Gates

${gateRows.map((r) => `- ${r}`).join('\n')}

${fails.length === 0 ? 'T1, T2 and the kill rule pass on both platforms.' : fails.map((f) => `- FAIL ${f}`).join('\n')}

## 4. Decision

**${decision}.** ${decision === 'GO' ? 'The feature stays opt-in until the owner decides on promotion into \`production\`; the numbers above are the evidence.' : 'No promotion; the feature stays opt-in (or is removed) and this report records why.'}
Not claimed: a process that frees a habit-sized heap and then makes no further allocator call keeps it (decay runs from allocator calls); several heaps learn their own habit and are bounded by their own peak.
`;
writeFileSync(new URL('J14_ADAPTIVE_LARGE_CACHE.md', dir), md);
console.log(`decision: ${decision}`);
for (const f of fails) console.log(`  ${f}`);
for (const r of gateRows) console.log(`  ${r}`);
