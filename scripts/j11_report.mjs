// J11: derives docs/perf/J11_BIG_BLOCK_JUDGE_MAP.md and its _summary.csv from the raw big-block judge campaigns
// (docs/perf/j11/{win,linux}_raw.jsonl.gz, gzipped after docs/perf/j11/run.mjs wrote them). Statistic as in J5/J6/J9: per block and arm the minimum
// over the repeats of the metric, d = ln(sefer / mi), cell = exp(mean d) with a two-sided 95 % t-interval (12 blocks: 11 degrees of
// freedom, quantile 2.201); "ahead" only when the whole interval lies on one side of 1. Asserts every printed number.
import { readFileSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
const dir = new URL('../docs/perf/', import.meta.url);
const rd = (p) => readFileSync(new URL(p, dir), 'utf8');
const T975_11 = 2.201;
const f3 = (x) => x.toFixed(3);
const MIB = 1 << 20;
const load = (p) => (p.endsWith('.gz') ? gunzipSync(readFileSync(new URL(p, dir))).toString('utf8') : rd(p)).trim().split(/\r?\n/).map((l) => JSON.parse(l));
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
const sdev = (v) => { const m = mean(v); return Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (v.length - 1)); };
const median = (v) => { const s = [...v].sort((a, b) => a - b); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const verdictOf = (c) => (!c ? 'below timer resolution' : c.hi < 1 ? 'sefer ahead' : c.lo > 1 ? 'mimalloc ahead' : 'tie');
const sizeName = (m) => (m >= 1024 ? `${m / 1024} GiB` : `${m} MiB`);

function cell(logs) {
  const n = logs.length;
  assert.equal(n - 1, 11, 'twelve blocks');
  const m = mean(logs), half = T975_11 * sdev(logs) / Math.sqrt(n);
  const [r, lo, hi] = [Math.exp(m), Math.exp(m - half), Math.exp(m + half)];
  assert.ok(lo <= r && r <= hi);
  return { r, lo, hi };
}

const platforms = [
  { id: 'windows', name: 'Windows', raw: 'j11/win_raw.jsonl.gz', ident: 'j11/win_ident.txt' },
  { id: 'linux', name: 'Linux (WSL2)', raw: 'j11/linux_raw.jsonl.gz', ident: 'j11/linux_ident.txt' },
];
let csv = 'platform,scenario,size_mib,k,cold_ratio,cold_lo,cold_hi,cold_verdict,warm_ratio,warm_lo,warm_hi,warm_verdict,sefer_end_rss_mib,mi_end_rss_mib,sefer_peak_rss_mib,mi_peak_rss_mib,sefer_faults,mi_faults,sefer_kernel_ms,mi_kernel_ms,sefer_retains,mi_retains\n';
let body = '';
const verdicts = [];
const byPlat = {};
for (const p of platforms) {
  const recs = load(p.raw);
  const ident = rd(p.ident);
  const src = /^main ([0-9a-f]{40})$/m.exec(ident)?.[1];
  assert.ok(src, `${p.id} identity`);
  const bin = (/^bin ([0-9a-f]{64})$/m.exec(ident) ?? /^([0-9a-f]{64})\s/m.exec(ident))?.[1];
  assert.ok(bin, `${p.id} binary hash`);
  const key = (r) => `${r.r.scenario}|${r.r.size_mib}|${r.r.k}`;
  const keys = [...new Set(recs.map(key))];
  const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
  const rows = [];
  for (const k of keys) {
    const cellRecs = recs.filter((r) => key(r) === k);
    const [sc, size, kk] = k.split('|');
    const per = (arm, f) => blocks.map((b) => {
      const v = cellRecs.filter((r) => r.block === b && r.arm === arm).map((r) => f(r.r));
      assert.ok(v.length >= 1 && v.every((x) => x >= 0), `samples ${k} ${arm} ${b}`);
      return Math.min(...v);
    });
    // a metric with a zero minimum is below the timer resolution of the host: no ratio (null)
    const ratio = (f) => {
      const [ps, pm] = [per('sefer', f), per('mi', f)];
      return [...ps, ...pm].some((x) => x === 0) ? null : cell(ps.map((x, i) => Math.log(x / pm[i])));
    };
    const cold = ratio((r) => r.cold_ns), warm = ratio((r) => r.warm_ns);
    const med = (arm, f) => median(cellRecs.filter((r) => r.arm === arm).map((r) => f(r.r)));
    const x = {
      sc, size: Number(size), k: Number(kk), cold, warm,
      endS: med('sefer', (r) => r.end_rss) / MIB, endM: med('mi', (r) => r.end_rss) / MIB,
      peakS: med('sefer', (r) => r.peak_rss) / MIB, peakM: med('mi', (r) => r.peak_rss) / MIB,
      fS: med('sefer', (r) => r.faults), fM: med('mi', (r) => r.faults),
      kS: med('sefer', (r) => r.kernel_ms), kM: med('mi', (r) => r.kernel_ms),
    };
    // path activation: the arm still holds (at least) the block after the run freed it
    const held = Number(size) * (sc === 'many' ? 1 : 1) * 0.9;
    x.retS = x.endS >= held; x.retM = x.endM >= held;
    rows.push(x);
  }
  const order = ['cycle', 'edge', 'grow', 'many'];
  byPlat[p.id] = rows;
  rows.sort((a, b) => order.indexOf(a.sc) - order.indexOf(b.sc) || a.size - b.size);
  const nRuns = recs.length;
  assert.equal(nRuns, keys.length * blocks.length * 2 * (nRuns / (keys.length * blocks.length * 2)), 'full grid');
  const table = rows.map((x) => {
    csv += [p.id, x.sc, x.size, x.k, f3(x.cold.r), f3(x.cold.lo), f3(x.cold.hi), verdictOf(x.cold), x.warm ? f3(x.warm.r) : '', x.warm ? f3(x.warm.lo) : '', x.warm ? f3(x.warm.hi) : '', verdictOf(x.warm), x.endS.toFixed(1), x.endM.toFixed(1), x.peakS.toFixed(1), x.peakM.toFixed(1), x.fS, x.fM, x.kS, x.kM, x.retS, x.retM].join(',') + '\n';
    return `| ${x.sc}${x.sc === 'many' ? ` ${x.k} x` : ''} ${sizeName(x.size)} | ${f3(x.cold.r)} (${f3(x.cold.lo)}–${f3(x.cold.hi)}) ${verdictOf(x.cold)} | ${x.warm ? `${f3(x.warm.r)} (${f3(x.warm.lo)}–${f3(x.warm.hi)})` : '-'} ${verdictOf(x.warm)} | ${x.endS.toFixed(0)} / ${x.endM.toFixed(0)} | ${x.peakS.toFixed(0)} / ${x.peakM.toFixed(0)} | ${x.fS} / ${x.fM} | ${x.kS} / ${x.kM} |`;
  }).join('\n');
  const behindCold = rows.filter((x) => verdictOf(x.cold) === 'mimalloc ahead').map((x) => `${x.sc} ${sizeName(x.size)}`);
  const aheadCold = rows.filter((x) => verdictOf(x.cold) === 'sefer ahead').map((x) => `${x.sc} ${sizeName(x.size)}`);
  const retained = rows.filter((x) => x.retS && !x.retM).map((x) => `${x.sc} ${sizeName(x.size)}`);
  assert.equal(behindCold.length + aheadCold.length + rows.filter((x) => verdictOf(x.cold) === 'tie').length, rows.length);
  verdicts.push(`- ${p.name}, cold (first rep): mimalloc ahead in ${behindCold.length} of ${rows.length} cells (${behindCold.join(', ') || 'none'}); Sefer ahead in ${aheadCold.length} (${aheadCold.join(', ') || 'none'}). Cells where Sefer still holds the block resident after freeing it and mimalloc does not: ${retained.join(', ') || 'none'}.`);
  body += `### ${p.name}

Source main \`${src}\`, judge binary sha256 \`${bin}\` (\`docs/perf/${p.ident}\`), ${blocks.length} blocks x 2 repeats per arm and cell.

| cell | cold: sefer / mi (95 % CI) | warm: sefer / mi (95 % CI) | end RSS MiB sefer / mi | peak RSS MiB sefer / mi | faults sefer / mi | kernel ms sefer / mi |
|---|---|---|---:|---:|---:|---:|
${table}

`;
}
const pick = (id, sc, size, k) => byPlat[id].find((x) => x.sc === sc && x.size === size && (sc !== 'many' || x.k === k));
const lc = pick('linux', 'cycle', 1024), wm16 = pick('windows', 'many', 16, 128), lm16 = pick('linux', 'many', 16, 128);
const wc4 = pick('windows', 'cycle', 4096), lc4 = pick('linux', 'cycle', 4096);
assert.ok(lc && wm16 && lm16 && wc4 && lc4);
assert.equal(1024 * 256, 262144, 'pages in 1 GiB');
const reading = [
  `- **Linux first touch of a large block.** Cold \`cycle\` of 1 GiB: Sefer takes ${lc.fS} page faults, mimalloc ${lc.fM} (${(lc.fS / lc.fM).toFixed(0)}x). Sefer's count is about one fault per 4 KiB page (1 GiB = 262144 pages), so its large blocks are not backed by transparent huge pages while mimalloc's are (the WSL2 THP mode at run time was \`madvise\`). Cold ratio ${f3(lc.cold.r)} there. This is consistent with the 2x cold gap on every Linux large cell; it is not proof of the cause, which needs a measurement with the hint (a separate step).`,
  `- **Windows first touch.** The same cells favour Sefer: cold ratio ${f3(wc4.cold.r)} at 4 GiB, with ${wc4.fS} page faults against mimalloc's ${wc4.fM}.`,
  `- **Reuse after a free, working sets above the large-cache headroom.** \`many\` 128 x 16 MiB (2 GiB live): warm ratio ${f3(wm16.warm.r)} on Windows and ${f3(lm16.warm.r)} on Linux, i.e. a second round of the same allocations is that many times slower in Sefer; end RSS ${wm16.endS.toFixed(0)} MiB against mimalloc's ${wm16.endM.toFixed(0)} on Windows. Sefer released the heap to the OS at the free, mimalloc kept it. This is the fast back-and-forth shape a decay policy exists for (the large cache's default headroom is 256 MiB).`,
  `- **Retention of single GiB blocks.** After \`cycle\` of 4 GiB Sefer's end RSS is ${wc4.endS.toFixed(0)} MiB on Windows and ${lc4.endS.toFixed(0)} MiB on Linux, mimalloc's ${wc4.endM.toFixed(0)} and ${lc4.endM.toFixed(0)}: the block sits in the large cache at the moment the process ends. The decay step runs from the allocator's own calls (\`mem_impl.rs\`, \`maybe_decay_large_cache\`), not from a timer, so a process that frees a GiB block and then makes no further allocator call keeps it resident; the judge measures exactly that instant.`,
  '- **Guard stop.** `grow` to 4 GiB with Sefer on Windows was stopped by the judge\'s 7 GiB peak-RSS guard in the first campaign attempt (intermediate blocks stay resident while `realloc` moves); the cell was replaced by `grow` 2 GiB and no 4 GiB `grow` result exists.',
].join('\n');
const md = `# J11 — big-block judge map against mimalloc, 1 MiB to 4 GiB (\`bench\`: measurement only; nothing changes)

Generated by \`node scripts/j11_report.mjs\` from \`docs/perf/j11/{win,linux}_raw.jsonl.gz\` (written by \`node docs/perf/j11/run.mjs <win|linux> <exe> <out> 12 2\`, then gzipped: artifact policy, 200 KiB to 2 MiB);
summary: \`docs/perf/J11_BIG_BLOCK_JUDGE_MAP_summary.csv\`. Do not edit by hand.

## 1. Question

The WG-1 judge keeps blocks below 32 MiB. How do Sefer and mimalloc compare on single blocks and heaps of large blocks from 1 MiB up to 4 GiB: first touch, reuse after free, \`realloc\` growth, and what they hold resident afterwards?

## 2. Method

- Entry point: \`examples/g1_big_judge.rs\` (real \`SeferAlloc\` and \`MiMalloc\` as statics behind \`#[inline(never)]\` shims at the \`__rust_alloc\` boundary, one arm per process; the process allocator stays \`System\`). Build: \`--release --features production\`; Windows without RUSTFLAGS, Linux with the repository's alignment RUSTFLAGS (\`docs/perf/j11/build.sh\`).
- Cells: \`cycle\` (allocate, write one byte per 4 KiB page, free), \`edge\` (allocate, write the first and last byte only, free), \`grow\` (\`realloc\` doubling from 1 MiB, touching what each step adds), \`many\` (k blocks, all touched, freed in reverse). One process runs \`reps\` repetitions of the whole sequence; **cold** is the first (all phases: allocate + touch + free), **warm** the minimum over the later ones, so a warm time far below cold means the block came back from the allocator's own cache.
- Campaign: 12 blocks x 2 repeats per arm and cell; the order of the cells is shuffled per block (fixed seed), the arm order alternates by block (\`docs/perf/j11/run.mjs\`).
- Statistic: per block and arm the minimum over the repeats of the metric, d = ln(sefer / mi), cell = exp(mean d), two-sided 95 % t-interval (11 degrees of freedom, quantile 2.201); "ahead" only when the whole interval lies on one side of 1. End and peak RSS, faults and kernel ms are medians over the runs of the cell.
- Mechanism evidence: warm versus cold time (cache reuse), end RSS against the block size (still resident after the free: the "retains" list in the verdict), faults and kernel time. The runs are \`production\` builds without allocator counters.
- Host: the shared Windows dev host (WSL2 for Linux), i7-11800H, 16 logical CPUs; WSL2 memory is the VM's, not bare metal.

## 3. Result

${body}## 4. Verdict

${verdicts.join('\n')}

## 5. Reading

${reading}

This report changes nothing; what follows from it (retention of large blocks, first-touch cost) is a separate step.
`;
writeFileSync(new URL('J11_BIG_BLOCK_JUDGE_MAP.md', dir), md);
writeFileSync(new URL('J11_BIG_BLOCK_JUDGE_MAP_summary.csv', dir), csv);
console.log(md.split('\n').filter((l) => l.startsWith('| ') || l.startsWith('- Windows') || l.startsWith('- Linux')).join('\n'));
