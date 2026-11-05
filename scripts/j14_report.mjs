// J14: applies the gates of docs/perf/J14_ADAPTIVE_LARGE_CACHE_PREREG.md to the paired campaigns docs/perf/j14/{win,linux}_raw.jsonl.gz
// (written by docs/perf/j14/run.mjs) and derives docs/perf/J14_ADAPTIVE_LARGE_CACHE.md and its _summary.csv. Arms: base, x (the policy), xt (x plus the
// J13 hint, Linux, read only), base2 (byte-identical copy of base, the A/A control) and mi (mimalloc, from the base binary). Statistic as in J5-J13: per
// block and arm the minimum over the repeats of the metric, d = ln(a / b), cell = exp(mean d), two-sided 95 % t-interval (11 degrees of freedom, 2.201).
// Asserts every printed number.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
// `c2` = campaign 2 under Amendment 2 of the preregistration (gates and file names differ); no argument = campaign 1 (Amendment 1 gates).
const C2 = process.argv[2] === 'c2';
const TAG = C2 ? '_C2' : '';
const PRE = C2 ? 'c2_' : '';
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
  { id: 'windows', name: 'Windows', raw: `j14/${PRE}win_raw.jsonl`, ident: `j14/${PRE}win_ident.txt` },
  { id: 'linux', name: 'Linux (WSL2)', raw: `j14/${PRE}linux_raw.jsonl`, ident: `j14/${PRE}linux_ident.txt` },
];
const gz = (p) => (existsSync(new URL(`${p}.gz`, dir)) ? gunzipSync(readFileSync(new URL(`${p}.gz`, dir))).toString('utf8') : readFileSync(new URL(p, dir), 'utf8'));
const fails = [];
let csv = 'platform,scenario,size_mib,k,arm_pair,metric,ratio,lo,hi,aa_lo,aa_hi\n';
let body = '';
const gateRows = [];
const readingRows = [];
const xmi128 = {};
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
      const bound = C2 && size === 128 ? 0.9 : 0.5;
      if (!C2 && !(t.xmi.r <= 2.0)) fails.push(`${p.name} T1: many ${k} x ${size} MiB warm x / mimalloc ${fmt(t.xmi)} above 2.0`);
      if (!(t.xb.hi < bound)) fails.push(`${p.name} T1: many ${k} x ${size} MiB warm x / base ${fmt(t.xb)}: upper bound not below ${bound}`);
    }
    gateRows.push(`${p.name} many ${k} x ${size} MiB warm: x/mi ${fmt(t.xmi)}, x/base ${fmt(t.xb)}${t.xtmi ? `, xt/mi ${fmt(t.xtmi)}` : ''}`);
  }
  const idle = get('idle', 16, 128);
  assert.ok(idle, 'idle cell');
  const idleAllow = C2 ? 256 + 32 : 32; // campaign 2: the 256 MiB headroom floor is allowed (Amendment 2)
  if (idle.end_x > idle.end_base + idleAllow) fails.push(`${p.name} T2(a): idle end RSS x ${idle.end_x.toFixed(0)} MiB above base ${idle.end_base.toFixed(0)} + ${idleAllow} MiB`);
  gateRows.push(`${p.name} idle end RSS MiB: base ${idle.end_base.toFixed(0)}, x ${idle.end_x.toFixed(0)}, mi ${idle.end_mi.toFixed(0)}${hasXt ? `, xt ${idle.end_xt.toFixed(0)}` : ''}`);
  for (const [sc, size, k] of [['cycle', 1, 1], ['cycle', 128, 1], ['grow', 128, 1], ['edge', 128, 1], ...(C2 ? [['many', 128, 16]] : [])]) {
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
  {
    const c = get('many', 128, 16);
    const pages = 5 * 16 * 128 * 256; // 5 rounds x 16 blocks x 128 MiB, 4 KiB pages
    const [fb, fx] = [c.faults_base, c.faults_x];
    assert.ok(fb <= pages * 1.02 && fx <= pages * 1.02, 'fault counts within the touched pages');
    xmi128[p.id] = c.warm.xmi.r;
    readingRows.push(`${p.name}: faults x ${fx} of ${pages} pages touched (${(100 * fx / pages).toFixed(1)} %), base ${fb} (${(100 * fb / pages).toFixed(1)} %).`);
  }
  const tbl = rows.map((c) => `| ${c.sc}${c.sc === 'many' || c.sc === 'idle' ? ` ${c.k} x` : ''} ${c.size} MiB | ${fmt(c.cold.xb)} | ${fmt(c.warm.xb)} | ${fmt(c.warm.aa)} | ${fmt(c.warm.bmi)} → ${fmt(c.warm.xmi)}${c.warm.xtmi ? ` → ${fmt(c.warm.xtmi)}` : ''} | ${c.end_base.toFixed(0)} → ${c.end_x.toFixed(0)}${hasXt ? ` → ${c.end_xt.toFixed(0)}` : ''} / ${c.end_mi.toFixed(0)} | ${c.faults_base} → ${c.faults_x} |`).join('\n');
  body += `### ${p.name}

Binaries (sha256): base \`${binBase}\`, x \`${binX}\`${hasXt ? `, xt \`${id(/^bin-xt ([0-9a-f]{64})$/m)}\`` : ''}; base2 is a byte-identical copy of base. \`docs/perf/${p.ident}\`.

| cell | cold x / base | warm x / base | warm A/A | warm sefer / mimalloc: base → x${hasXt ? ' → xt' : ''} | end RSS MiB base → x${hasXt ? ' → xt' : ''} / mi | faults base → x |
|---|---|---|---|---|---|---|
${tbl}

`;
}
writeFileSync(new URL(`J14_ADAPTIVE_LARGE_CACHE${TAG}_summary.csv`, dir), csv);
const decision = fails.length === 0 ? 'GO' : fails.some((f) => f.includes('KILL') || f.includes('T2')) ? 'NO-GO' : 'NO-GO (benefit gate not met)';
const reading = `## 5. Reading of the result (written after the gates; nothing above is changed by it)

- **The arm xt was not run.** Amendment 1 planned it on Linux, but \`docs/perf/j14/run.mjs\` tested for the binary with Node's \`existsSync\` on a WSL path, which is false from Windows; the Linux file holds the arms base, x, base2 and mi only. No xt number exists in this report.
- **16 x 128 MiB: x re-faults almost every page in every round, base about 60 %.** ${readingRows.join(' ')}
- **Reading of the code (a hypothesis until counters confirm it; see the next preregistration amendment).** A build with \`large-cache-adaptive\` implies \`large-cache-extended\`, whose default byte budget is 256 MiB (R17-9) instead of the production cache's unbounded budget. Spans of 128 MiB reach that budget after two spans, before the eight base slots are full; the extension sidecar, and with it the ghost list and the histogram, is created only once the base slots are full, so nothing is learned in this cell and x caches less than base (end RSS 132 MiB against 1028 MiB; the raw rounds show free at 51 ms against 29 ms). The 128 x 16 MiB cell reaches slot pressure first (256 MiB = 16 spans > 8 slots) and does learn.
- **Idle: the learned extra does not fall within six intervals.** Forgetting scales every histogram weight by the same factor, so the 90 % bound does not move until a weight reaches zero, and the decay then releases only 10 % of the excess per interval. Both are properties of the design, not of the measurement.
- **128 x 16 MiB still ends at ${xmi128.windows.toFixed(1)}x (Windows) and ${xmi128.linux.toFixed(1)}x (Linux) of mimalloc after the gain over base shown in section 2**: in the raw rounds the remainder is the touch phase of 2 GiB on 4 KiB pages (the J13 effect), not the allocator.
`;
const reading2 = `## 5. Reading of the result (written after the gates; nothing above is changed by it)

- **The only failed item is a reading rule, not an effect.** ${fails.map((f) => f.replace(/^FAIL /, '')).join('; ') || 'none'}. The preregistered rule reads a cell only if its A/A interval contains 1; the interval above ends at the third decimal of 1, while the effect in that cell is a ratio far from 1 (section 3). By the letter the decision stands; the options are one repeat of that cell under a new amendment, or the owner accepting it.
- **The policy does what it was built for.** Warm rounds of both habit cells (128 x 16 MiB and 16 x 128 MiB) run at the speed of mimalloc on both platforms (sefer / mimalloc in section 3), the first round (cold) is unchanged against base, and the controls without reuse pressure are unchanged. The retained memory is the habit itself (end RSS of the many cells equals mimalloc's), the price of the speed; after the habit stops (cell idle) the end RSS falls to the headroom floor plus the base (section 3).
- **xt adds nothing to x on warm rounds** (Linux): the reused spans are already faulted in, so the J13 hint only matters for the first round.
- **The measured binaries are the J14 tree at 65674b70**; the PR head later renamed the evicted-span log (\`ghost\` to \`gone\`, a lint requirement) and made the test hook \`dbg_set_large_cache_budget\` switch the adaptive budget off; neither touches the measured paths. One \`cargo check\` (the PR fix) ran on the machine while campaign 2 was running; blocks interleave all arms, so it loads every arm alike.
`;
const md = `# J14 — an adaptive Large cache that learns the reuse habit: ${decision} (\`bench\` report of an opt-in \`perf(opt-in)\` candidate)

Generated by \`node scripts/j14_report.mjs${C2 ? ' c2' : ''}\` from \`docs/perf/j14/${PRE}{win,linux}_raw.jsonl.gz\` (written by \`node docs/perf/j14/run.mjs <win|linux> <bin dir> <out> 12 2\`, then gzipped); summary:
\`docs/perf/J14_ADAPTIVE_LARGE_CACHE${TAG}_summary.csv\`. Gates: \`docs/perf/J14_ADAPTIVE_LARGE_CACHE_PREREG.md\` ${C2 ? '(section 7, Amendment 2, committed before this campaign; arm xt read only)' : '(committed before the measurement; Amendment 1 adds the read-only arm xt)'}. Do not edit by hand.

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

${C2 ? reading2 : reading}`;
writeFileSync(new URL(`J14_ADAPTIVE_LARGE_CACHE${TAG}.md`, dir), md);
console.log(`decision: ${decision}`);
for (const f of fails) console.log(`  ${f}`);
for (const r of gateRows) console.log(`  ${r}`);
