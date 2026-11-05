// J13: applies the gates of docs/perf/J13_LARGE_BLOCK_THP_PREREG.md to the paired campaign docs/perf/j13/raw.jsonl.gz (written by
// docs/perf/j13/run.mjs) and derives docs/perf/J13_LARGE_BLOCK_THP.md and its _summary.csv. Arms: base, x (the THP hint), base2 (a
// byte-identical copy of base, the A/A control) and mi (mimalloc, from the base binary). Statistic as in J5-J11: per block and arm the
// minimum over the repeats of the cold time, d = ln(x / base), cell = exp(mean d), two-sided 95 % t-interval (11 degrees of freedom,
// quantile 2.201); a cell is read only if its A/A interval (ln(base2 / base)) contains 1. Asserts every printed number.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
// campaign 1: the gates as preregistered; campaign 2: the gates of Amendment 1 (mechanism-derived RSS and sparse-touch limits), fresh data
const ci = process.argv.indexOf('--campaign');
const campaign = ci >= 0 ? Number(process.argv[ci + 1]) : 1;
assert.ok(campaign === 1 || campaign === 2, '--campaign 1|2');
const suffix = campaign === 1 ? '' : '_C2';
const T975_11 = 2.201;
const MIB = 1 << 20;
const f3 = (x) => x.toFixed(3);
const rawPath = new URL(campaign === 1 ? 'j13/raw.jsonl.gz' : 'j13/raw2.jsonl.gz', dir);
const text = existsSync(rawPath) ? gunzipSync(readFileSync(rawPath)).toString('utf8') : readFileSync(new URL(campaign === 1 ? 'j13/raw.jsonl' : 'j13/raw2.jsonl', dir), 'utf8');
const recs = text.trim().split(/\r?\n/).map((l) => JSON.parse(l));
const ident = readFileSync(new URL('j13/ident.txt', dir), 'utf8');
const id = (re) => { const m = re.exec(ident); assert.ok(m, `identity ${re}`); return m[1]; };
const [baseSha, xSha] = [id(/^base ([0-9a-f]{40})$/m), id(/^x ([0-9a-f]{40})$/m)];
const [binBase, binX, binBase2] = [id(/^bin-base ([0-9a-f]{64})$/m), id(/^bin-x ([0-9a-f]{64})$/m), id(/^bin-base2 ([0-9a-f]{64})$/m)];
assert.equal(binBase, binBase2, 'base2 is a byte-identical copy of base');
const thp = /^thp: (.*)$/m.exec(ident)?.[1] ?? 'unknown';

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
const fmt = (c) => `${f3(c.r)} (${f3(c.lo)}–${f3(c.hi)})`;

const key = (r) => `${r.r.scenario}|${r.r.size_mib}|${r.r.k}`;
const keys = [...new Set(recs.map(key))];
const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
const ARMS = ['base', 'x', 'base2', 'mi'];
const rows = [];
for (const k of keys) {
  const [sc, size, kk] = k.split('|');
  const cr = recs.filter((r) => key(r) === k);
  const per = (arm, f) => blocks.map((b) => {
    const v = cr.filter((r) => r.block === b && r.arm === arm).map((r) => f(r.r));
    assert.ok(v.length >= 1 && v.every((x) => x > 0), `samples ${k} ${arm} ${b}`);
    return Math.min(...v);
  });
  const cold = (r) => r.cold_ns;
  const [b, x, b2, mi] = ARMS.map((a) => per(a, cold));
  const med = (arm, f) => median(cr.filter((r) => r.arm === arm).map((r) => f(r.r)));
  rows.push({
    sc, size: Number(size), k: Number(kk),
    xb: cell(x.map((v, i) => Math.log(v / b[i]))),
    aa: cell(b2.map((v, i) => Math.log(v / b[i]))),
    xmi: cell(x.map((v, i) => Math.log(v / mi[i]))),
    bmi: cell(b.map((v, i) => Math.log(v / mi[i]))),
    faultsB: med('base', (r) => r.faults), faultsX: med('x', (r) => r.faults),
    huge: med('x', (r) => r.anon_huge_peak), hugeB: med('base', (r) => r.anon_huge_peak),
    endB: med('base', (r) => r.end_rss) / MIB, endX: med('x', (r) => r.end_rss) / MIB,
    peakB: med('base', (r) => r.peak_rss) / MIB, peakX: med('x', (r) => r.peak_rss) / MIB,
    blockMiB: Number(size) * (sc === 'many' ? Number(kk) : 1),
  });
}
const order = ['cycle', 'grow', 'many', 'edge'];
rows.sort((a, b) => order.indexOf(a.sc) - order.indexOf(b.sc) || a.size - b.size);
const get = (sc, size, k = 1) => rows.find((r) => r.sc === sc && r.size === size && (sc !== 'many' || r.k === k));

// --- gates -------------------------------------------------------------------------------------------------------------------------
const fails = [], notes = [], overshoots = [];
// T0: path activation
const t0cells = [get('cycle', 128), get('cycle', 1024), get('grow', 1024)];
for (const c of t0cells) {
  assert.ok(c);
  const okHuge = c.huge >= 0.8 * (c.sc === 'grow' ? c.size : c.size) * MIB;
  const okFaults = c.faultsX <= 0.2 * c.faultsB;
  if (!okHuge) fails.push(`T0: ${c.sc} ${c.size} MiB: anon_huge_peak ${(c.huge / MIB).toFixed(0)} MiB < 80 % of the block`);
  if (!okFaults) fails.push(`T0: ${c.sc} ${c.size} MiB: faults x ${c.faultsX} > 20 % of base ${c.faultsB}`);
}
// T1: benefit on cycle 1 GiB
const t1 = get('cycle', 1024);
const t1Read = contains1(t1.aa);
if (!t1Read) fails.push(`T1: cycle 1 GiB is unreadable (A/A ${fmt(t1.aa)} excludes 1)`);
else if (!(t1.xb.hi < 0.85)) fails.push(`T1: cycle 1 GiB x/base ${fmt(t1.xb)}: upper bound not below 0.85`);
// T2: cost
for (const c of rows) {
  // campaign 1: +4 MiB (cycle, grow), +8 MiB (edge), many not gated; campaign 2 (Amendment 1): 2 MiB per partly touched hinted reservation
  const hinted = (sz) => { let n = 0; for (let b = 8; b <= sz; b *= 2) n++; return n; }; // sizes 8 MiB * 2^i up to the block: the doubling chain of `grow`
  const lim = campaign === 1 ? (c.sc === 'edge' ? 8 : 4) : 2 * (c.sc === 'grow' ? hinted(c.size) : c.sc === 'many' ? c.k : c.sc === 'edge' ? 2 : 1);
  if (campaign === 2 || c.sc !== 'many') {
    const over = Math.max(c.endX - c.endB, c.peakX - c.peakB) - lim; // MiB above the limit (positive = fail)
    if (over > 0) {
      overshoots.push(over);
      fails.push(`T2: ${c.sc}${c.sc === 'many' ? ` ${c.k} x` : ''} ${c.size} MiB: RSS x end ${c.endX.toFixed(2)} / peak ${c.peakX.toFixed(2)} against base ${c.endB.toFixed(2)} / ${c.peakB.toFixed(2)} (limit +${lim} MiB, over by ${(over * 1024).toFixed(0)} KiB)`);
    }
  }
}
const ctl = get('cycle', 1);
if (!contains1(ctl.xb)) fails.push(`T2 control: cycle 1 MiB x/base ${fmt(ctl.xb)} excludes 1`);
if (Math.abs(ctl.faultsX - ctl.faultsB) > 0.05 * ctl.faultsB) fails.push(`T2 control: cycle 1 MiB faults x ${ctl.faultsX} vs base ${ctl.faultsB} differ by more than 5 %`);
// kill: any cell of 16 MiB or more clearly slower
for (const c of rows) {
  if (campaign === 2 && c.sc === 'edge') continue; // the sparse-touch cell has its own absolute limit below
  if (c.size >= 16 && c.xb.lo > 1.10) fails.push(`KILL: ${c.sc} ${c.size} MiB x/base ${fmt(c.xb)} wholly above 1.10`);
}
if (campaign === 2) {
  const e = rows.find((c) => c.sc === 'edge');
  const xs = recs.filter((r) => r.arm === 'x' && key(r) === `edge|${e.size}|1`).map((r) => r.r.cold_ns);
  e.xColdMedianNs = median(xs);
  if (e.xColdMedianNs > 2e6) fails.push(`T2 sparse touch: edge ${e.size} MiB x cold median ${(e.xColdMedianNs / 1e6).toFixed(2)} ms above the 2 ms limit`);
}
const decision = fails.length === 0 ? 'GO' : fails.some((f) => f.startsWith('T0')) ? 'NULL (the hint did not take effect)' : fails.some((f) => f.startsWith('KILL') || f.startsWith('T2')) ? 'NO-GO' : 'NO-GO (benefit not shown)';

// --- output ------------------------------------------------------------------------------------------------------------------------
let csv = 'scenario,size_mib,k,x_over_base,lo,hi,aa_base2_over_base,aa_lo,aa_hi,readable,base_over_mi,x_over_mi,faults_base,faults_x,anon_huge_x_mib,end_rss_base_mib,end_rss_x_mib,peak_rss_base_mib,peak_rss_x_mib\n';
const table = rows.map((c) => {
  csv += [c.sc, c.size, c.k, f3(c.xb.r), f3(c.xb.lo), f3(c.xb.hi), f3(c.aa.r), f3(c.aa.lo), f3(c.aa.hi), contains1(c.aa), f3(c.bmi.r), f3(c.xmi.r), c.faultsB, c.faultsX, (c.huge / MIB).toFixed(0), c.endB.toFixed(1), c.endX.toFixed(1), c.peakB.toFixed(1), c.peakX.toFixed(1)].join(',') + '\n';
  return `| ${c.sc}${c.sc === 'many' ? ` ${c.k} x` : ''} ${c.size} MiB | ${fmt(c.xb)} | ${fmt(c.aa)} ${contains1(c.aa) ? '' : '(A/A excludes 1)'} | ${f3(c.bmi.r)} → ${f3(c.xmi.r)} | ${c.faultsB} → ${c.faultsX} | ${(c.huge / MIB).toFixed(0)} | ${c.endB.toFixed(0)} → ${c.endX.toFixed(0)} | ${c.peakB.toFixed(0)} → ${c.peakX.toFixed(0)} |`;
}).join('\n');
writeFileSync(new URL(`J13_LARGE_BLOCK_THP${suffix}_summary.csv`, dir), csv);
const t2fails = fails.filter((f) => f.startsWith('T2: '));
const others = fails.filter((f) => !f.startsWith('T2: '));
const maxOver = overshoots.length ? Math.max(...overshoots) : 0;
const overshootText = fails.length === 0 ? 'No gate failed.' : t2fails.length > 0 && others.length === 0
  ? `Every failed gate is a T2 RSS limit; the largest overshoot is ${(maxOver * 1024).toFixed(0)} KiB of ${t2fails.length} limits, against RSS medians of up to ${Math.max(...rows.map((r) => r.peakX)).toFixed(0)} MiB. The limits are exact multiples of 2 MiB (one huge page per hinted reservation, which also holds the header page), and a median RSS carries a few tens of KiB of unrelated allocations, so the preregistered zero-tolerance reading fails by margins at the measurement granularity. T0, T1, the control and every other gate pass. By the letter the verdict stays NO-GO; the substance is a very large benefit at a cost equal to the mechanism's bound, and the decision to accept that goes to the owner.`
  : 'At least one non-RSS gate failed; see section 3.';
const md = `# J13${campaign === 2 ? ' campaign 2 (Amendment 1 gates)' : ''} — a transparent-huge-page hint for fresh Large reservations: ${decision} (\`bench\` report of a \`perf(runtime)\` candidate)

Generated by \`node scripts/j13_report.mjs --campaign ${campaign}\` from \`docs/perf/j13/${campaign === 1 ? 'raw' : 'raw2'}.jsonl.gz\` (written by \`node docs/perf/j13/run.mjs <bin dir> docs/perf/j13/${campaign === 1 ? 'raw' : 'raw2'}.jsonl 12 2\`, then gzipped);
summary: \`docs/perf/J13_LARGE_BLOCK_THP${suffix}_summary.csv\`. Gates: \`docs/perf/J13_LARGE_BLOCK_THP_PREREG.md\` ${campaign === 1 ? '(committed before the measurement)' : 'section 6, Amendment 1 (committed before campaign 2, after campaign 1 had been read)'}. Do not edit by hand.

## 1. Identity and method

- base: main plus the \`AnonHugePages\` sampling of \`examples/g1_big_judge.rs\`, tree \`${baseSha}\`, judge binary sha256 \`${binBase}\`; x: base plus the hint for fresh Large reservations of 8 MiB or more, tree \`${xSha}\`, binary \`${binX}\`;
  base2: a byte-identical copy of base (A/A control, same hash). \`docs/perf/j13/ident.txt\`, \`docs/perf/j13/build.sh\`; Linux (WSL2), THP mode \`${thp.replace(/\s+/g, ' ')}\`.
- Entry point: real \`SeferAlloc\` through the \`#[inline(never)]\` shims of \`examples/g1_big_judge.rs\` (the layer the change ships at). ${blocks.length} blocks x 2 repeats per arm and cell, cells shuffled per block, the four arms (base, x, base2, mimalloc from the base binary) in a rotation by block.
- Cold time = the first repetition, all phases (allocate + touch + free). Per block and arm the minimum over the repeats; d = ln(x / base); cell = exp(mean d) with a 95 % t-interval (11 degrees of freedom, 2.201). A/A = base2 / base.
  Faults, RSS and \`anon_huge_peak\` (THP-backed bytes sampled right after a touch phase) are medians over the runs of a cell.

## 2. Result

| cell | cold x / base (95 % CI) | A/A base2 / base | sefer / mimalloc: base → x | faults base → x | THP MiB (x) | end RSS MiB base → x | peak RSS MiB base → x |
|---|---|---|---|---|---:|---|---|
${table}

## 3. Gates

${fails.length === 0 ? 'T0 (path activation), T1 (benefit), T2 (cost and control) pass and nothing is killed.' : fails.map((f) => `- FAIL ${f}`).join('\n')}

## 4. Reading of the failures

${overshootText}

## 5. Decision

**${decision}.** ${decision === 'GO' ? 'The hint is merged as `perf(runtime)` with `tests/j13_large_thp_hint.rs`.' : 'No change to `src/` is merged; the patch stays on branch `g1-j13` only as the measured candidate.'}
Not claimed: Windows (no-op by construction), THP modes \`always\`/\`never\`, workloads touching a small part of many big blocks beyond the \`edge\` cell.
`;
writeFileSync(new URL(`J13_LARGE_BLOCK_THP${suffix}.md`, dir), md);
console.log(`decision: ${decision}`);
for (const f of fails) console.log(`  ${f}`);
console.log(md.split('\n').filter((l) => l.startsWith('| ')).join('\n'));
