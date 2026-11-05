// J9: derives docs/perf/J9_POST_J8_JUDGE_MAP.md and its _summary.csv from the raw judge campaigns of main 9c5f5494 (after J8)
// (docs/perf/j9/{win,linux}_raw.jsonl) and the pre-J8 campaigns of main 875ddcd6 (docs/perf/j5/win_raw.jsonl, docs/perf/j6/linux_raw.jsonl).
// Statistic as in J5/J6: per block and arm the minimum ns/op over the repeats, d = ln(sefer / mi), cell = exp(mean d), two-sided 95 % t-interval.
// Before-vs-after change of one quantity (sefer ns/op, mimalloc ns/op, the ratio): exp(difference of the means of the per-block logs), unpaired
// (the blocks of two campaigns are independent), pooled variance, t quantile of n1 + n2 - 2 degrees of freedom. Asserts every printed number.
import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
const rd = (p) => readFileSync(new URL(p, dir), 'utf8');
const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large'];
const T975 = { 11: 2.201, 22: 2.074 };
const f3 = (x) => x.toFixed(3);
const pct = (x) => `${x >= 1 ? '+' : '-'}${Math.abs((x - 1) * 100).toFixed(1)} %`;
const load = (p) => rd(p).trim().split(/\r?\n/).map((l) => JSON.parse(l));
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
const sdev = (v) => { const m = mean(v); return Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (v.length - 1)); };

/** Per block: minimum ns/op over the repeats of one arm in one scenario. */
function mins(recs, sc, arm) {
  const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
  return blocks.map((b) => {
    const v = recs.filter((r) => r.block === b && r.arm === arm && r.r.scenario === sc).map((r) => r.r.ns_per_op);
    assert.ok(v.length >= 1 && v.every((x) => x > 0), `samples ${b} ${arm} ${sc}`);
    return Math.min(...v);
  });
}
/** exp(mean ln) with the one-sample 95 % interval of n = 12 blocks. */
function cell(logs) {
  const n = logs.length, t = T975[n - 1];
  assert.ok(t, `quantile for n=${n}`);
  const m = mean(logs), half = t * sdev(logs) / Math.sqrt(n);
  const [r, lo, hi] = [Math.exp(m), Math.exp(m - half), Math.exp(m + half)];
  assert.ok(lo <= r && r <= hi);
  return { r, lo, hi };
}
/** exp(mean(a) - mean(b)) with a pooled-variance two-sample 95 % interval. */
function change(a, b) {
  const [n1, n2] = [a.length, b.length], t = T975[n1 + n2 - 2];
  assert.ok(t, `quantile for ${n1 + n2 - 2} df`);
  const sp2 = ((n1 - 1) * sdev(a) ** 2 + (n2 - 1) * sdev(b) ** 2) / (n1 + n2 - 2);
  const se = Math.sqrt(sp2 * (1 / n1 + 1 / n2)), d = mean(a) - mean(b);
  const [r, lo, hi] = [Math.exp(d), Math.exp(d - t * se), Math.exp(d + t * se)];
  assert.ok(lo <= r && r <= hi);
  return { r, lo, hi };
}
const verdictOf = (c) => (c.hi < 1 ? 'sefer ahead' : c.lo > 1 ? 'mimalloc ahead' : 'tie');
const moved = (c) => (c.hi < 1 ? 'lower' : c.lo > 1 ? 'higher' : 'no change');

const platforms = [
  { id: 'windows', name: 'Windows', now: 'j9/win_raw.jsonl', before: 'j5/win_raw.jsonl', identNow: 'j9/win_ident.txt', summary: 'J5_WINDOWS_JUDGE_MAP_summary.csv' },
  { id: 'linux', name: 'Linux (WSL2)', now: 'j9/linux_raw.jsonl', before: 'j6/linux_raw.jsonl', identNow: 'j9/linux_ident.txt', summary: 'J6_LINUX_JUDGE_MAP_summary.csv' },
];
let csv = 'platform,scenario,ratio_now,lo_now,hi_now,verdict_now,ratio_before,lo_before,hi_before,verdict_before,sefer_ns_change,sefer_lo,sefer_hi,sefer_moved,mi_ns_change,mi_lo,mi_hi,mi_moved,ratio_change,ratio_change_lo,ratio_change_hi,ratio_moved\n';
let body = '';
const verdictLines = [];
for (const p of platforms) {
  const now = load(p.now), before = load(p.before);
  assert.deepEqual([...new Set(now.map((r) => r.r.scenario))].sort(), [...SCEN].sort());
  const ident = rd(p.identNow);
  const src = /^main ([0-9a-f]{40})$/m.exec(ident)?.[1];
  assert.ok(src, `${p.id} identity`);
  const bin = (/^bin ([0-9a-f]{64})$/m.exec(ident) ?? /^([0-9a-f]{64})\s/m.exec(ident))?.[1];
  assert.ok(bin, `${p.id} binary hash`);
  const baseCsv = {};
  for (const l of rd(p.summary).trim().split(/\r?\n/).slice(1)) { const c = l.split(','); baseCsv[c[0]] = +c[1]; }
  const reps = now.filter((r) => r.block === now[0].block && r.arm === 'sefer' && r.r.scenario === SCEN[0]).length;
  const rows = [];
  for (const sc of SCEN) {
    const [sn, mn, sb, mb] = [mins(now, sc, 'sefer'), mins(now, sc, 'mi'), mins(before, sc, 'sefer'), mins(before, sc, 'mi')];
    assert.ok(sn.length === 12 && mn.length === 12 && sb.length === 12 && mb.length === 12, 'twelve blocks per arm');
    const [dNow, dBefore] = [sn.map((x, i) => Math.log(x / mn[i])), sb.map((x, i) => Math.log(x / mb[i]))];
    const [cn, cb] = [cell(dNow), cell(dBefore)];
    assert.equal(f3(cb.r), f3(baseCsv[sc]), `${p.id} ${sc}: recomputed baseline equals the published summary`);
    const sefer = change(sn.map(Math.log), sb.map(Math.log));
    const mi = change(mn.map(Math.log), mb.map(Math.log));
    const ratio = change(dNow, dBefore);
    rows.push({ sc, cn, cb, sefer, mi, ratio });
    csv += [p.id, sc, f3(cn.r), f3(cn.lo), f3(cn.hi), verdictOf(cn), f3(cb.r), f3(cb.lo), f3(cb.hi), verdictOf(cb), f3(sefer.r), f3(sefer.lo), f3(sefer.hi), moved(sefer), f3(mi.r), f3(mi.lo), f3(mi.hi), moved(mi), f3(ratio.r), f3(ratio.lo), f3(ratio.hi), moved(ratio)].join(',') + '\n';
  }
  const table = rows.map((x) => `| ${x.sc} | ${f3(x.cn.r)} (${f3(x.cn.lo)}–${f3(x.cn.hi)}) | ${verdictOf(x.cn)} | ${f3(x.cb.r)} (${f3(x.cb.lo)}–${f3(x.cb.hi)}) | ${pct(x.sefer.r)} (${f3(x.sefer.lo)}–${f3(x.sefer.hi)}) ${moved(x.sefer)} | ${pct(x.mi.r)} (${f3(x.mi.lo)}–${f3(x.mi.hi)}) ${moved(x.mi)} | ${f3(x.ratio.r)} (${f3(x.ratio.lo)}–${f3(x.ratio.hi)}) ${moved(x.ratio)} |`).join('\n');
  const behind = rows.filter((x) => verdictOf(x.cn) === 'mimalloc ahead').map((x) => x.sc);
  const ahead = rows.filter((x) => verdictOf(x.cn) === 'sefer ahead').map((x) => x.sc);
  const tied = rows.filter((x) => verdictOf(x.cn) === 'tie').map((x) => x.sc);
  assert.equal(behind.length + ahead.length + tied.length, SCEN.length);
  const better = rows.filter((x) => moved(x.ratio) === 'lower').map((x) => x.sc);
  const worse = rows.filter((x) => moved(x.ratio) === 'higher').map((x) => x.sc);
  const drift = rows.filter((x) => moved(x.mi) !== 'no change').map((x) => `${x.sc} ${pct(x.mi.r)}`);
  verdictLines.push(`- ${p.name}: mimalloc ahead in ${behind.length} of ${SCEN.length} (${behind.join(', ') || 'none'}); ties ${tied.join(', ') || 'none'}; Sefer ahead ${ahead.join(', ') || 'none'}. Ratio lower than before (whole interval below 1): ${better.join(', ') || 'none'}; higher: ${worse.join(', ') || 'none'}. Mimalloc arm moved between the campaigns in: ${drift.join(', ') || 'none'}.`);
  body += `### ${p.name}

Source main \`${src}\`, judge binary sha256 \`${bin}\` (\`docs/perf/${p.identNow}\`), ${reps} repeats x 12 blocks. Before: \`docs/perf/${p.before}\` (main 875ddcd6).

| scenario | now: sefer / mi (95 % CI) | verdict now | before (95 % CI) | sefer ns/op now / before | mimalloc ns/op now / before (control) | ratio now / before |
|---|---|---|---|---|---|---|
${table}

`;
}
const md = `# J9 — judge map against mimalloc after J8 (\`bench\`: measurement only; the J8 change itself landed in PR #215)

Generated by \`node scripts/j9_report.mjs\` from \`docs/perf/j9/{win,linux}_raw.jsonl\` (written by \`node docs/perf/wg1/run.mjs <exe> docs/perf/j9/win_raw.jsonl 12 3\`
and \`node docs/perf/j6/run.mjs <linux exe> docs/perf/j9/linux_raw.jsonl 12 3\`); summary: \`docs/perf/J9_POST_J8_JUDGE_MAP_summary.csv\`. Do not edit by hand.

## 1. Question

After J8 (the directory walk skips all-zero words in blocks of eight, the only runtime change on main since the J5/J6 campaigns), where do Sefer and mimalloc stand in the seven WG-1 judge scenarios on Windows and Linux (WSL2)?

## 2. Method

- Entry point: \`examples/g1_wild_judge.rs\` (real \`SeferAlloc\` as \`#[global_allocator]\` arm vs mimalloc arm in one binary), scale 1, seed 1, shuffle seed and arm alternation of \`docs/perf/wg1/run.mjs\`; builds as in J5 (Windows, no RUSTFLAGS) and J6 (Linux, alignment RUSTFLAGS), from one source tree of the measured commit.
- Cell: per block d = ln(sefer / mi) of the per-block minimum ns/op; cell = exp(mean d) with a two-sided 95 % t-interval (11 degrees of freedom, quantile 2.201); "ahead" only when the whole interval lies on one side of 1.
- Before-vs-after of one quantity (the ratio, Sefer's ns/op, mimalloc's ns/op): exp(difference of the means of the per-block logs), pooled variance, quantile 2.074 (22 degrees of freedom); "lower"/"higher" only when the whole interval lies on one side of 1. The mimalloc column is the control: a mimalloc arm that moved means the host moved between the campaigns, and the Sefer and ratio columns then carry that drift.
- The J5/J6 baselines are recomputed from their raw files and asserted equal to their published summaries.

## 3. Result

${body}## 4. Verdict

${verdictLines.join('\n')}

This report changes nothing; the attribution of what remains is a separate step.
`;
writeFileSync(new URL('J9_POST_J8_JUDGE_MAP.md', dir), md);
writeFileSync(new URL('J9_POST_J8_JUDGE_MAP_summary.csv', dir), csv);
console.log(md.split('\n').filter((l) => l.startsWith('| ') || l.startsWith('- ')).join('\n'));
