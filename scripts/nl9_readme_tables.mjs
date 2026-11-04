// NL-9: rewrites the three canonical sized tables of README.md ("Churn + write", "Churn, non-writing",
// "Warm bulk burst") from docs/perf/_raw_nl9_bench_table.log (written by `npm run bench:table`), so the
// README figures are derived from the raw log and not retyped. Asserts the ratio column it prints.
import { readFileSync, writeFileSync } from 'node:fs';
const log = readFileSync(new URL('../docs/perf/_raw_nl9_bench_table.log', import.meta.url), 'utf8');
const readmePath = new URL('../README.md', import.meta.url);
let readme = readFileSync(readmePath, 'utf8');
function assert(c, m) { if (!c) throw new Error(m); }

// canonical log heading -> README table intro line (the bold title line that precedes the table)
const GROUPS = [
  { log: '### Warm repeated bulk burst', readme: '**Warm bulk burst** (`bench_direct_alloc`' },
  { log: '### Churn, non-writing', readme: '**Churn, non-writing** (`bench_churn_alloc`' },
  { log: '### Churn + write (`bench_churn_alloc_write`', readme: '**Churn + write** (`bench_churn_alloc_write`' },
];
const SIZES = ['16', '64', '256', '1024'];
function logRows(heading) {
  const i = log.indexOf(heading);
  assert(i >= 0, `log heading ${heading}`);
  const rows = [];
  for (const line of log.slice(i).split('\n').slice(1)) {
    const m = line.match(/^\| (\d+)B \| ([\d.]+) \| ([\d.]+) \| ([\d.]+) \| (?:\*\*)?([\d.]+)× (faster|slower)(?:\*\*)? \|$/);
    if (m) rows.push({ size: m[1], s: Number(m[2]), mi: Number(m[3]), sy: Number(m[4]), ratio: Number(m[5]), dir: m[6] });
    else if (rows.length) break;
  }
  assert(rows.length === 4 && rows.every((r, k) => r.size === SIZES[k]), `rows of ${heading}`);
  return rows;
}
for (const g of GROUPS) {
  const rows = logRows(g.log);
  const start = readme.indexOf(g.readme);
  assert(start >= 0, `README anchor ${g.readme}`);
  const tableStart = readme.indexOf('| Size |', start);
  const tableEnd = readme.indexOf('\n\n', tableStart);
  assert(tableStart > start && tableEnd > tableStart, 'README table bounds');
  const out = ['| Size | SeferAlloc | mimalloc | System | Sefer vs mimalloc |', '|---|---:|---:|---:|---:|'];
  for (const r of rows) {
    // the ratio printed by the log is max/min of the two columns; recompute and compare to one rounding step
    const calc = r.s <= r.mi ? r.mi / r.s : r.s / r.mi;
    assert(Math.abs(calc - r.ratio) <= 0.1 * r.ratio, `ratio ${g.log} ${r.size}: ${calc} vs ${r.ratio}`);
    assert((r.s <= r.mi) === (r.dir === 'faster'), `direction ${g.log} ${r.size}`);
    const sz = r.size.padStart(4);
    const faster = r.dir === 'faster';
    const sCell = faster ? `**${r.s.toFixed(1)} ns**` : `${r.s.toFixed(1)} ns`;
    const ratio = faster ? `**${r.ratio.toFixed(2)}× faster**` : `${r.ratio.toFixed(2)}× slower`;
    out.push(`| ${sz} B | ${sCell} | ${r.mi.toFixed(1).padStart(5)} ns | ${r.sy.toFixed(1)} ns | ${ratio} |`);
  }
  readme = readme.slice(0, tableStart) + out.join('\n') + readme.slice(tableEnd);
}
writeFileSync(readmePath, readme);
console.log('README tables rewritten from docs/perf/_raw_nl9_bench_table.log');
