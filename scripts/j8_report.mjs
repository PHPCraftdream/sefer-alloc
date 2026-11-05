// J8: derives docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP.md and docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP_summary.csv from docs/perf/j8/raw (docs/perf/j8/j8.sh)
// and docs/perf/j8/time_raw.jsonl (docs/perf/j8/run.mjs), judged by the criteria fixed in docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP_PREREG.md.
// Asserts every printed number and computes the verdict from the rule; nothing in the verdict is typed by hand.
import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
const rd = (p) => readFileSync(new URL(p, dir), 'utf8');
const SC = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase'];
const T975 = { 3: 3.182, 5: 2.571, 7: 2.365, 9: 2.262, 11: 2.201, 15: 2.131 };
const MIN_ACTIVATION_IR = 200, GROW64_GO_PCT = -2, WORSE_LIMIT_PCT = 0.5, TIME_IR_MIN_PCT = 0.5;
const f1 = (x) => x.toFixed(1), f2 = (x) => x.toFixed(2), f3 = (x) => x.toFixed(3);
const num = (s) => Number(s.replace(/,/g, ''));
const ident = rd('j8/raw/identity.txt');
const tar = /^tar ([0-9a-f]{64})$/m.exec(ident), patchSha = /^patch j8\.patch sha256 ([0-9a-f]{64})$/m.exec(ident);
assert.ok(tar && patchSha, 'identity');
const arms = ['base', 'patch'];
// path activation: self Ir of the discovery function per call, grow64 at scale 0.5
const fnIr = {}, calls = {};
for (const arm of arms) {
  const t = rd(`j8/raw/fn.${arm}.txt`);
  const m = /^([\d,]+) \(\s*[\d.]+%\)  \?\?\?:.*find_segment_with_free_impl/m.exec(t);
  assert.ok(m, `discovery function row ${arm}`);
  fnIr[arm] = num(m[1]);
  const c = /^(\d+) cfn=.*find_segment_with_free_impl/m.exec(rd(`j8/raw/calls.${arm}.txt`));
  assert.ok(c, `call count ${arm}`);
  calls[arm] = Number(c[1]);
}
assert.equal(calls.base, calls.patch, 'same number of discovery calls');
const perCall = { base: fnIr.base / calls.base, patch: fnIr.patch / calls.patch };
const activation = perCall.base - perCall.patch;
const activated = activation >= MIN_ACTIVATION_IR;
// Ir per op
const ir = {};
let csv = 'kind,cell,base,patch,delta_pct\n';
csv += `activation,find_segment_with_free_impl Ir per call (grow64 0.5),${f1(perCall.base)},${f1(perCall.patch)},${f2((perCall.patch / perCall.base - 1) * 100)}\n`;
const irRows = [];
let worst = -Infinity;
for (const sc of SC) {
  const o = {};
  for (const arm of arms) {
    const ops = JSON.parse(rd(`j8/raw/run.${sc}.${arm}.json`)).ops;
    const tot = num(/^([\d,]+) \(100\.0%\)  PROGRAM TOTALS/m.exec(rd(`j8/raw/ir.${sc}.${arm}.txt`))[1]);
    o[arm] = { ops, tot, per: tot / ops };
  }
  assert.equal(o.base.ops, o.patch.ops, `same ops ${sc}`);
  const d = (o.patch.tot / o.base.tot - 1) * 100;
  ir[sc] = { ...o, d };
  if (sc !== 'grow64') worst = Math.max(worst, d);
  irRows.push(`| ${sc} | ${f1(o.base.per)} | ${f1(o.patch.per)} | ${f2(d)} % (${o.patch.tot - o.base.tot} of ${o.base.tot}) |`);
  csv += `ir_per_op,${sc},${f1(o.base.per)},${f1(o.patch.per)},${f2(d)}\n`;
}
const bulk = {};
const bulkRows = [];
for (const sz of [16, 64]) {
  const v = {};
  for (const arm of arms) v[arm] = num(/^([\d,]+)/.exec(rd(`j8/raw/bulk.${arm}.${sz}.txt`))[1]);
  const d = (v.patch / v.base - 1) * 100;
  bulk[sz] = d;
  bulkRows.push(`| bulk 1024 x ${sz} | ${v.base} | ${v.patch} | ${f2(d)} % (${v.patch - v.base} of ${v.base}) |`);
  csv += `bulk_ir,${sz},${v.base},${v.patch},${f2(d)}\n`;
}
// time: d = ln(patch / base) per block on the min over repeats
const recs = rd('j8/time_raw.jsonl').trim().split(/\r?\n/).map((l) => JSON.parse(l));
const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
const t = T975[blocks.length - 1];
assert.ok(t, 'quantile');
const timeRows = [];
const claims = [];
for (const sc of [...SC, 'large']) {
  const d = blocks.map((b) => {
    const mn = (arm) => Math.min(...recs.filter((r) => r.block === b && r.arm === arm && r.r.scenario === sc).map((r) => r.r.ns_per_op));
    return Math.log(mn('patch') / mn('base'));
  });
  const n = d.length, m = d.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(d.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1)), half = t * sd / Math.sqrt(n);
  const [r, lo, hi] = [Math.exp(m), Math.exp(m - half), Math.exp(m + half)];
  const irDelta = ir[sc]?.d;
  const claim = irDelta !== undefined && Math.abs(irDelta) >= TIME_IR_MIN_PCT && (hi < 1 || lo > 1);
  if (claim) claims.push(sc);
  timeRows.push(`| ${sc} | ${f3(r)} | ${f3(lo)}–${f3(hi)} | ${irDelta === undefined ? 'n/a' : f2(irDelta) + ' %'} | ${claim ? (hi < 1 ? 'patch faster' : 'patch slower') : 'no time claim'} |`);
  csv += `time,${sc},1,${f3(r)},${f2((r - 1) * 100)}\n`;
}
const g64 = ir.grow64.d;
const go = activated && g64 <= GROW64_GO_PCT && worst <= WORSE_LIMIT_PCT && bulk[16] <= WORSE_LIMIT_PCT && bulk[64] <= WORSE_LIMIT_PCT;
assert.equal(go, activated && g64 <= -2 && Math.max(worst, bulk[16], bulk[64]) <= 0.5, 'verdict arithmetic');
writeFileSync(new URL('J8_DIRECTORY_ZERO_WORD_SKIP_summary.csv', dir), csv);
const md = `# J8 — skipping all-zero directory words in blocks of 8 (\`bench\`: ceiling of a \`perf(runtime)\` candidate; verdict computed from the pre-registered rule)

Generated by \`node scripts/j8_report.mjs\` from \`docs/perf/j8/raw\` (\`docs/perf/j8/j8.sh\`) and \`docs/perf/j8/time_raw.jsonl\` (\`docs/perf/j8/run.mjs\`); summary
\`docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP_summary.csv\`. Criteria: \`docs/perf/J8_DIRECTORY_ZERO_WORD_SKIP_PREREG.md\`. Do not edit by hand.

## 1. Method and identity

- Entry point: the real \`SeferAlloc\` through the judge's \`GlobalAlloc\` shims (\`examples/g1_wild_judge.rs\`, arm \`sefer\` for both builds), Linux (WSL2), \`--release --features production\`, alignment RUSTFLAGS.
- Source identity: one tree, git-archive tar sha256 \`${tar[1]}\`; the \`patch\` arm adds \`docs/perf/j8/patches/j8.patch\` (sha256 \`${patchSha[1]}\`); binary hashes in \`docs/perf/j8/raw/identity.txt\`.
- Ir: callgrind, scale 0.25 (judge scenarios) and scale 0.5 (discovery function on grow64), \`b4_pmu_probe bulk sefer 1024 {16, 64} 400\`. Time: ${blocks.length} blocks x 3 repeats of the J6 campaign, arm order alternating by block.

## 2. Path activation (hard assert)

\`find_segment_with_free_impl\` on grow64 at scale 0.5: ${calls.base} calls; self Ir per call ${f1(perCall.base)} (base) -> ${f1(perCall.patch)} (patch), a fall of ${f1(activation)} Ir per call
(requirement: at least ${MIN_ACTIVATION_IR}). Activation ${activated ? 'confirmed' : 'NOT confirmed: the run is void'}.

## 3. Whole-process Ir per operation (scale 0.25)

| scenario | base Ir/op | patch Ir/op | patch vs base |
|---|---:|---:|---|
${irRows.join('\n')}

| bulk (b4_pmu_probe) | base Ir | patch Ir | patch vs base |
|---|---:|---:|---|
${bulkRows.join('\n')}

## 4. Time (J6 statistic: per block ln(patch / base), exp(mean) with a 95 % t-interval, ${blocks.length - 1} degrees of freedom)

| scenario | patch / base | 95 % CI | Ir delta in this scenario | claim |
|---|---:|---|---|---|
${timeRows.join('\n')}

A time claim needs an Ir delta of at least ${TIME_IR_MIN_PCT} % in the same scenario and an interval that excludes 1. Time claims: ${claims.length ? claims.join(', ') : 'none'}.

## 5. Verdict (rule of the pre-registration)

grow64 Ir ${f2(g64)} % (GO needs at most ${GROW64_GO_PCT} %), worst other scenario ${f2(worst)} % and bulk ${f2(bulk[16])} % / ${f2(bulk[64])} % (limit +${WORSE_LIMIT_PCT} %), activation ${activated ? 'yes' : 'no'}:
**${go ? 'GO' : 'NO-GO'}**.
`;
writeFileSync(new URL('J8_DIRECTORY_ZERO_WORD_SKIP.md', dir), md);
console.log(md.split('\n').filter((l) => l.startsWith('| ') || l.startsWith('**') || l.startsWith('`find_segment') || l.startsWith('(requirement')).join('\n'));
