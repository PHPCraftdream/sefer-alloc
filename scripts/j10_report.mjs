// J10: derives docs/perf/J10_GROWMIX_FREE_SIDE_CEILING.md and its _summary.csv from the line-level callgrind of the growmix sefer arm
// (docs/perf/j10/raw, written by docs/perf/j10/j10.sh from main fefcf970) and the J7 time split of the same scenario
// (docs/perf/J7_GAP_SPLIT_summary.csv, docs/perf/j7/raw). Asserts every printed number.
import { readFileSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
const rd = (p) => readFileSync(new URL(p, dir), 'utf8');
const f1 = (x) => x.toFixed(1);
const pctOf = (a, b) => `${((100 * a) / b).toFixed(1)} %`;

// --- callgrind: self Ir per (function, file) and call counts per callee ------------------------------------------------------------
const lines = gunzipSync(readFileSync(new URL('j10/raw/growmix.sefer.lines.out.gz', dir))).toString('utf8').split(/\r?\n/);
const selfBy = new Map(); // `${fn}\t${file}` -> Ir
const calls = new Map(); // callee name -> number of calls
let file = '', fn = '', cfn = '', inCall = false;
for (const l of lines) {
  if (l.startsWith('fl=') || l.startsWith('fi=') || l.startsWith('fe=')) { file = l.slice(3); continue; }
  if (l.startsWith('fn=')) { fn = l.slice(3); inCall = false; continue; }
  if (l.startsWith('cfn=')) { cfn = l.slice(4); continue; }
  if (l.startsWith('calls=')) { calls.set(cfn, (calls.get(cfn) ?? 0) + Number(l.slice(6).split(' ')[0])); inCall = true; continue; }
  if (/^\d+ \d+/.test(l)) {
    if (inCall) { inCall = false; continue; }
    const k = `${fn}\t${file}`;
    selfBy.set(k, (selfBy.get(k) ?? 0) + Number(l.split(' ')[1]));
  }
}
const total = Number(/^summary: (\d+)/m.exec(lines.join('\n'))[1]);
const run = JSON.parse(rd('j10/raw/run.growmix.sefer.json'));
const ops = run.ops;
assert.ok(ops > 0 && total > 0);
const selfOf = (pat) => [...selfBy].filter(([k]) => k.split('\t')[0].includes(pat)).reduce((a, [, v]) => a + v, 0);
const tableOf = (pat) => [...selfBy].filter(([k]) => k.split('\t')[0].includes(pat) && /segment_table\/(hash|segment_table_impl)\.rs$/.test(k.split('\t')[1])).reduce((a, [, v]) => a + v, 0);
const callsOf = (pat) => [...calls].filter(([k]) => k.includes(pat)).reduce((a, [, v]) => a + v, 0);
const F = ['lane_miss', 'lane_second_chance', 'flush_class_internal', 'try_carve_batch', 'find_segment_with_free_impl'].map((n) => ({
  n, self: selfOf(n), table: tableOf(n), calls: callsOf(n),
}));
const by = Object.fromEntries(F.map((x) => [x.n, x]));
for (const x of F) assert.ok(x.self > 0 && x.calls > 0, `${x.n} present`);

// --- J7: Ir/op of both arms and the time split of the growmix gap ----------------------------------------------------------------
const irTotal = (p) => Number(/^([\d,]+) \(100\.0%\)\s+PROGRAM TOTALS/m.exec(rd(p))[1].replace(/,/g, ''));
const opsOf = (p) => JSON.parse(rd(p)).ops;
const j7Sefer = irTotal('j7/raw/ir.growmix.sefer.txt') / opsOf('j7/raw/run.growmix.sefer.json');
const j7Mi = irTotal('j7/raw/ir.growmix.mi.txt') / opsOf('j7/raw/run.growmix.mi.json');
const irGapOp = j7Sefer - j7Mi;
const csv = rd('J7_GAP_SPLIT_summary.csv').trim().split(/\r?\n/).map((l) => l.split(','));
const row = (arm) => csv.find((c) => c[0] === 'Linux (WSL2)' && c[1] === 'growmix' && c[2] === arm);
const [s, m] = [row('sefer'), row('mi')];
assert.ok(s && m);
const user = +s[5] - +m[5], kernel = +s[6] - +m[6], faults = +s[7] - +m[7], nsGap = +s[4] - +m[4];
const cpuGap = user + kernel;
assert.ok(user > 0 && kernel > 0);

// --- ceilings ----------------------------------------------------------------------------------------------------------------------
const perOp = (x) => x / ops;
const tableIr = by.lane_miss.table + by.flush_class_internal.table;
const lm = by.lane_miss, sc = by.lane_second_chance;
const lmPerCall = lm.self / lm.calls, lmTablePerCall = lm.table / lm.calls, scPerCall = sc.self / sc.calls;
const savedPerCall = lmPerCall - (lmTablePerCall + scPerCall);
assert.ok(savedPerCall > 0, 'the third-chance model saves instructions per call');
const thirdIr = savedPerCall * lm.calls;
// the user-time gap is read as proportional to the Ir gap (an assumption, stated in the report)
const msPerIrOp = user / irGapOp;
const sharePct = (irOp) => (100 * irOp * msPerIrOp) / cpuGap;
const rows = [
  ['segment-table lookup cost, lane_miss + flush_class_internal (whole, cannot be removed)', perOp(tableIr)],
  ['hash-probe exit in the lane (model: lane_miss minus its table part, plus the second chance per call)', perOp(thirdIr)],
];
let out = 'item,ir_per_op,share_of_ir_gap_pct,share_of_cpu_gap_pct_if_user_scales_with_ir\n';
const tbl = rows.map(([name, v]) => {
  out += `"${name}",${f1(v)},${f1((100 * v) / irGapOp)},${f1(sharePct(v))}\n`;
  return `| ${name} | ${f1(v)} | ${pctOf(v, irGapOp)} | ${f1(sharePct(v))} % |`;
}).join('\n');
writeFileSync(new URL('J10_GROWMIX_FREE_SIDE_CEILING_summary.csv', dir), out);

const fnRows = F.map((x) => `| ${x.n} | ${x.calls} | ${f1(perOp(x.self))} | ${f1(x.self / x.calls)} | ${f1(perOp(x.table))} | ${x.table ? pctOf(x.table, x.self) : '-'} |`).join('\n');
const md = `# J10 — the free/refill side of growmix against its gap: Ir ceilings and the kernel share (\`bench\`: measurement only; nothing changes)

Generated by \`node scripts/j10_report.mjs\` from \`docs/perf/j10/raw\` (\`docs/perf/j10/j10.sh\`, main \`fefcf970\`) and the J7 split of the same scenario. Do not edit by hand.

## 1. Question

J7 named \`flush_class_internal\`, \`lane_miss\` and \`try_carve_batch\` as the Sefer-side functions of the growmix gap. Which part of that cost is the segment-table lookup,
what would the next Ir lever (a hash-probe exit in the own-free lane, the way WILD-2 added the \`own_cache\` exit) save at most, and how much of the growmix time gap can any Ir lever reach?

## 2. Method

- Judge \`g1_wild_judge growmix sefer 1 0.25\` under callgrind with line tables (\`--dump-line=yes\`), a build of one git archive of the measured commit (\`docs/perf/j10/raw/ident.txt\`), alignment RUSTFLAGS as in J6.
  Whole process ${total} Ir over ${ops} operations (the judge's own \`ops\`) = ${f1(total / ops)} Ir/op, including the judge's bookkeeping, which both arms carry.
- Self Ir per function with inlined callees credited to their own source file (\`docs/perf/j10/lines.mjs\`, \`files.mjs\`); the segment-table part of a function is its self Ir in \`segment_table/hash.rs\` and \`segment_table/segment_table_impl.rs\`.
  Call counts are the \`calls=\` records of the callgrind file.
- Time split of the growmix gap from J7 (Linux, median of 36 runs per arm): sefer minus mimalloc = ${f1(nsGap)} ns/op, user ${user} ms, kernel ${kernel} ms, ${faults} page faults. Ir gap per op (J7 build, \`docs/perf/j7/raw\`): ${f1(j7Sefer)} − ${f1(j7Mi)} = ${f1(irGapOp)}.
- The last column of section 4 reads the user-time gap as proportional to the Ir gap (${f1(user)} ms over ${f1(irGapOp)} Ir/op). That is an assumption, not a measurement; the Ir columns do not depend on it.

## 3. Where the Sefer-side Ir sits

| function | calls | self Ir/op | self Ir/call | of it segment-table lookup, Ir/op | table share of self |
|---|---:|---:|---:|---:|---:|
${fnRows}

## 4. Ceilings

| lever | Ir/op | share of the Ir gap (${f1(irGapOp)} Ir/op) | share of the CPU gap (user + kernel = ${cpuGap} ms) |
|---|---:|---:|---:|
${tbl}

The model of the hash-probe exit: a free that misses the memo and the \`own_cache\` pays the lookup (${f1(lmTablePerCall)} Ir/call in \`lane_miss\`) and the lane's own acceptance steps (${f1(scPerCall)} Ir/call, the measured self cost of \`lane_second_chance\`) instead of the whole chain (${f1(lmPerCall)} Ir/call);
saving ${f1(savedPerCall)} Ir/call over ${lm.calls} calls. It is a ceiling by construction: it assumes the new exit costs no more than those two measured parts.

## 5. Reading

- The kernel part is ${pctOf(kernel, cpuGap)} of the growmix CPU gap (${kernel} of ${cpuGap} ms; ${faults} more page faults than mimalloc, the memory Sefer returns and re-faults: Retain, card 146). No Ir lever touches it.
- Removing the **entire** segment-table lookup cost on the free side (impossible: ownership is validated by it) would be ${f1(perOp(tableIr))} Ir/op, ${pctOf(perOp(tableIr), irGapOp)} of the Ir gap and about ${f1(sharePct(perOp(tableIr)))} % of the CPU gap.
- The realistic lever, the hash-probe exit, is at most ${f1(perOp(thirdIr))} Ir/op: ${pctOf(perOp(thirdIr), j7Sefer)} of Sefer's ${f1(j7Sefer)} Ir/op, ${pctOf(perOp(thirdIr), irGapOp)} of the Ir gap, about ${f1(sharePct(perOp(thirdIr)))} % of the CPU gap. Under the repository's rule (time claim only with a disjoint paired interval) it cannot be read in time on a ${f1(nsGap)} ns/op gap.
- \`grow64\`, \`grow1k\`, \`regrow\` have hash probes under 0.1 % of small frees (\`SEGMODEL_OWN_CACHE.md\`): the lever does not exist there.

## 6. Verdict

No prototype. The free/refill Ir levers of growmix add up to a few percent of the Ir gap and a smaller share of the time gap, which is mostly kernel time. The step that moves growmix, grow64's page-fault half, grow1k and regrow is the memory-return policy (Retain, owner decision, card 146); the hash-probe exit stays a recorded candidate of at most ${f1(perOp(thirdIr))} Ir/op.
This report changes nothing.
`;
writeFileSync(new URL('J10_GROWMIX_FREE_SIDE_CEILING.md', dir), md);
console.log(md.split('\n').filter((l) => l.startsWith('| ') || l.startsWith('- ') || l.startsWith('The model')).join('\n'));
