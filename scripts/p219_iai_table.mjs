// #219: derives the iai refresh (NL-9 reference vs origin/main d5e89119) and the bench:table repeatability check
// from raw logs in docs/perf/. Writes P219_IAI_summary.csv and P219_BENCH_TABLE_REPRO_summary.csv, prints the
// IAI_BASELINE.md section and the README note. Asserts every printed figure.
import { readFileSync, writeFileSync } from 'node:fs';
const perf = new URL('../docs/perf/', import.meta.url);
const read = (n) => readFileSync(new URL(n, perf), 'utf8');
function assert(c, m) { if (!c) throw new Error(m); }
const num = (s) => Number(s.replace(/,/g, ''));
const f2 = (x) => x.toFixed(2);

function iai(name) {
  const text = read(name);
  const start = text.indexOf('  bench                                                 Ir');
  assert(start >= 0, `${name}: summary table not found`);
  const rows = new Map();
  for (const line of text.slice(start).split('\n').slice(2)) {
    const m = line.match(/^ {2}([a-z0-9_]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+(\S+)\s*$/);
    if (!m) { if (rows.size) break; continue; }
    rows.set(m[1], { ir: num(m[2]), cyc: num(m[6]) });
  }
  const ratio = new Map();
  const r0 = text.indexOf('  workload                     Sefer Ir/op');
  assert(r0 >= 0, `${name}: ratio table not found`);
  for (const line of text.slice(r0).split('\n').slice(2)) {
    const m = line.match(/^ {2}([a-z0-9_]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*$/);
    if (!m) { if (ratio.size) break; continue; }
    const [s, mi, r] = [m[2], m[3], m[4]].map(Number);
    assert(Math.abs(s / mi - r) < 0.0015, `${name}: ratio ${m[1]}`);
    ratio.set(m[1], { s, mi, r });
  }
  assert(/\[iai\] PASS — \d+ bench\(es\) produced Ir/.test(text), `${name}: no PASS line`);
  return { rows, ratio };
}

const OLD = '_raw_nl9_iai_off.log', NEW = '_raw_p219_iai.log';
const a = iai(OLD), b = iai(NEW);
assert(a.rows.size >= 100 && a.rows.size === b.rows.size, `bench count old=${a.rows.size} new=${b.rows.size}`);
let csv = 'bench,ir_nl9,ir_p219,ir_delta,ir_delta_pct,estcycles_nl9,estcycles_p219,estcycles_delta_pct\n';
const all = [];
for (const [k, x] of a.rows) {
  const y = b.rows.get(k);
  assert(y, `bench ${k} missing in ${NEW}`);
  const d = y.ir - x.ir, p = (y.ir / x.ir - 1) * 100, cp = (y.cyc / x.cyc - 1) * 100;
  csv += `${k},${x.ir},${y.ir},${d},${f2(p)},${x.cyc},${y.cyc},${f2(cp)}\n`;
  all.push({ k, x, y, d, p, cp });
}
writeFileSync(new URL('P219_IAI_summary.csv', perf), csv);
const lower = all.filter((c) => c.d < 0).length, same = all.filter((c) => c.d === 0).length, higher = all.filter((c) => c.d > 0).length;
assert(lower + same + higher === all.length, 'partition');
const best = [...all].sort((p, q) => p.p - q.p)[0], worst = [...all].sort((p, q) => q.p - p.p)[0];
const KEY = ['small_churn_16b', 'churn_256b', 'cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b',
  'recycle_alloc_free_256x64b', 'dealloc_free_only_16b', 'dealloc_own_thread_body_only_16b', 'large_alloc_free_cycle'];
const keyRows = KEY.map((k) => {
  const c = all.find((z) => z.k === k);
  assert(c, `key bench ${k}`);
  return `| ${k} | ${c.x.ir.toLocaleString('en-US')} | ${c.y.ir.toLocaleString('en-US')} | ${f2(c.p)} % (${c.d} of ${c.x.ir}) | ${f2(c.cp)} % |`;
});
const ratioRows = [...b.ratio].map(([k, n]) => {
  const o = a.ratio.get(k);
  assert(o, `ratio ${k} missing in ${OLD}`);
  return `| ${k} | ${o.s.toFixed(1)} / ${o.mi.toFixed(1)} = ${o.r.toFixed(3)} | ${n.s.toFixed(1)} / ${n.mi.toFixed(1)} = ${n.r.toFixed(3)} |`;
});
// The Ir/op drop on the churn pair comes from the bootstrap constant B (large_alloc_free_cycle) growing more than the bench.
const sc = all.find((z) => z.k === 'small_churn_16b'), bs = all.find((z) => z.k === 'large_alloc_free_cycle');
const OPS = 64;
assert(Math.abs((sc.x.ir - bs.x.ir) / OPS - a.ratio.get('small_churn_16b').s) < 0.05, 'old Ir/op from B');
assert(Math.abs((sc.y.ir - bs.y.ir) / OPS - b.ratio.get('small_churn_16b').s) < 0.05, 'new Ir/op from B');
assert(bs.d > sc.d, 'B grew more than the churn bench');
const top = (arr, n) => arr.slice(0, n).map((c) => `${c.k} ${f2(c.p)} % (${c.x.ir} → ${c.y.ir})`).join('; ');
const rises = [...all].filter((c) => c.d > 0).sort((p, q) => q.p - p.p);
const drops = [...all].filter((c) => c.d < 0).sort((p, q) => p.p - q.p);

// bench:table repeatability: the four canonical sized groups, three allocators, two runs.
const GROUPS = ['### Warm repeated bulk burst', '### Churn, non-writing', '### Churn + write', '### Churn + teardown'];
const ALLOC = ['SeferAlloc', 'mimalloc', 'System'];
function table(name) {
  const text = read(name);
  assert(/\[bench-table\] PASS — \d+ bench id\(s\) parsed, all \d+ expected ids present/.test(text), `${name}: no PASS line`);
  const out = new Map();
  for (const g of GROUPS) {
    const i = text.indexOf(g);
    assert(i >= 0, `${name}: ${g}`);
    const rows = text.slice(i).split('\n').filter((l) => /^\| \d+B \|/.test(l)).slice(0, 4);
    assert(rows.length === 4, `${name}: ${g} rows`);
    for (const r of rows) {
      const c = r.split('|').map((s) => s.trim());
      ALLOC.forEach((al, j) => out.set(`${g.slice(4)}|${c[1]}|${al}`, Number(c[2 + j])));
    }
  }
  return out;
}
const R1 = '_raw_p219_bench_table_run1.log', R2 = '_raw_p219_bench_table_run2.log';
const t1 = table(R1), t2 = table(R2);
assert(t1.size === 48 && t2.size === 48, 'cell count');
let rcsv = 'group,size,allocator,ns_run1,ns_run2,run2_over_run1\n';
const spread = [];
for (const [k, v1] of t1) {
  const v2 = t2.get(k);
  assert(v1 > 0 && v2 > 0, `cell ${k}`);
  const [g, s, al] = k.split('|');
  rcsv += `"${g}",${s},${al},${v1},${v2},${(v2 / v1).toFixed(3)}\n`;
  spread.push({ k, v1, v2, f: Math.max(v1 / v2, v2 / v1) });
}
writeFileSync(new URL('P219_BENCH_TABLE_REPRO_summary.csv', perf), rcsv);
const over2 = spread.filter((c) => c.f > 2).length;
const maxc = [...spread].sort((p, q) => q.f - p.f)[0];
const ex = (k) => { const c = spread.find((z) => z.k === k); assert(c, k); return `${c.v1.toFixed(1)} → ${c.v2.toFixed(1)} ns`; };

const md = `
## #219 (2026-11-26) — refresh at \`origin/main\` \`d5e89119\` (\`large-cache-adaptive\` in \`production\`)

Generated by \`node scripts/p219_iai_table.mjs\` from \`docs/perf/${OLD}\` (the NL-9 reference) and \`docs/perf/${NEW}\`
(per-bench summary: \`docs/perf/P219_IAI_summary.csv\`). **This is the new reference for \`production\`.**

- **Source:** \`origin/main\` \`d5e89119d32148f016539a31f0e1ba189a36c235\`, clean worktree, \`npm run iai\` default features
  \`production bench-internals internals\` (\`production\` now also lists \`large-cache-adaptive\`, task #217), one run,
  \`iai-callgrind-runner 0.14.2\`, WSL, under a 10 % CPU cap (\`Ir\` does not depend on it).
- **Not a paired comparison:** the NL-9 reference was measured on the PR #201 tree; between it and \`d5e89119\` lie the
  workspace split (P1/P2), the \`sefer-alloc-api\` adapter and the \`large-cache-adaptive\` promotion. The deltas below are
  the net change over that whole window and are not attributed to any one commit.
- **Counts:** ${all.length} benches; Ir lower in ${lower}, equal in ${same}, higher in ${higher}. Largest drops: ${top(drops, 3)}.
  Largest rises: ${top(rises, 3)}.

| bench | Ir NL-9 | Ir d5e89119 | change | Estimated Cycles change |
|---|---:|---:|---:|---:|
${keyRows.join('\n')}

Sefer/mimalloc marginal Ir/op (Sefer Ir/op ÷ mimalloc Ir/op, bootstrap subtracted per arm; < 1 means fewer instructions than mimalloc):

| workload | NL-9 | d5e89119 |
|---|---:|---:|
${ratioRows.join('\n')}

**The churn Ir/op drop is not a faster hot path.** Raw Ir of \`small_churn_16b\` rose by ${sc.d}; its Ir/op fell because the
bootstrap constant it subtracts (\`large_alloc_free_cycle\`) rose by ${bs.d}: (${sc.x.ir} − ${bs.x.ir}) / ${OPS} =
${((sc.x.ir - bs.x.ir) / OPS).toFixed(1)} before, (${sc.y.ir} − ${bs.y.ir}) / ${OPS} = ${((sc.y.ir - bs.y.ir) / OPS).toFixed(1)} now. The
\`large_cache_eviction_*\` rises are in benches that run the Large cache, whose retention policy \`large-cache-adaptive\`
changes; this run does not isolate that feature.
`;

const note = `**Not refreshed at \`d5e89119\` (#219, 2026-11-26).** Two full \`npm run bench:table\` runs at \`origin/main\`
\`d5e89119\` (\`large-cache-adaptive\` in \`production\`) under a 10 % CPU cap
(\`docs/perf/${R1}\`, \`docs/perf/${R2}\`) do not reproduce each other: in the four canonical sized groups
${over2} of 48 cells differ by more than 2× between the runs (largest ${maxc.f.toFixed(1)}×, ${maxc.k.replace(/\|/g, ' ')}; e.g.
mimalloc warm bulk burst 16B ${ex('Warm repeated bulk burst|16B|mimalloc')}, SeferAlloc churn + write 16B
${ex('Churn + write|16B|SeferAlloc')}; per-cell list \`docs/perf/P219_BENCH_TABLE_REPRO_summary.csv\`). The tables above
therefore stay at their NL-9 values. The deterministic refresh for \`d5e89119\` is the \`#219\` section of
\`docs/perf/IAI_BASELINE.md\`.`;

console.log(md);
console.log('----- README note -----');
console.log(note);
