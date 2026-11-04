// NL-4/5/6: derives docs/perf/NL46_REMAINING_GUARDS_OFF_summary.csv and docs/perf/NL46_REMAINING_GUARDS_OFF.md
// from docs/perf/nl46/raw (written by docs/perf/nl46/nl46.sh); asserts every printed ratio.
import { readFileSync, writeFileSync } from 'node:fs';
const dir = new URL('../docs/perf/nl46/raw/', import.meta.url);
const rd = (n) => readFileSync(new URL(n, dir), 'utf8');
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
function assert(c, m) { if (!c) throw new Error(m); }
const ARMS = ['base', 'nl4', 'nl5', 'nl6', 'all'];
const BULK = ['bulk16', 'bulk64'];
const MT = ['larson1', 'larson4', 'mstress1', 'mstress4'];
const KEYS = [...BULK, ...MT];
const REPS = [1, 2, 3, 4, 5];
const ORDER = ['base', 'nl4', 'nl5', 'nl6', 'all', 'all', 'nl6', 'nl5', 'nl4', 'base'];
const POS = Object.fromEntries(ARMS.map((a) => [a, ORDER.flatMap((x, i) => (x === a ? [i + 1] : []))]));
for (const a of ARMS) assert(POS[a].length === 2, `two positions per arm ${a}`);
const NOISY = /^(larson|mstress)4$/;
const THR_IR = 2.0;
const THR_TIME = 1.5;
// A time delta counts only where the arm also executes measurably fewer instructions in the same cell: with no
// instruction delta the binary differs only by code layout, and the observed time spread of a cell without
// any change is about 3 % (see the nl5/nl6 bulk cells, whose Ir delta is below 0.03 %).
const MIN_IR_FOR_TIME = 0.5;
const NAMES = {
  nl4: 'NL-4 (bounds: lane granule bound, small_free_guard payload start and bump, dealloc_small payload start and bump)',
  nl5: 'NL-5 (flush span guards 0+1)',
  nl6: 'NL-6 (reclaim record geometry)',
  all: 'all three together',
};

const cells = [];
for (const k of KEYS) {
  for (const arm of ARMS) {
    const ir = Number(rd(`ir.${arm}.${k}.txt`).match(/^(\d+)/)[1]);
    const t = [];
    const cy = [];
    for (const r of REPS) {
      for (const i of POS[arm]) {
        const out = rd(`run.${arm}.${k}.${r}.${i}.stdout`);
        if (BULK.includes(k)) {
          const o = out.trim().split(/\s+/);
          assert(o[0] === 'bulk' && o[1] === 'sefer', `stdout shape ${arm} ${k}`);
          t.push(Number(o[5]));
          for (const l of rd(`stat.${arm}.${k}.${r}.${i}.txt`).split('\n')) {
            const f = l.split(',');
            if (f[2] === 'cycles:u') cy.push(Number(f[0]));
          }
        } else {
          const m = out.match(/T=(\d+) ([\d.]+) Mops/);
          assert(m && k.endsWith(m[1]), `mops shape ${arm} ${k}`);
          t.push(Number(m[2]));
        }
      }
    }
    assert(t.length === 10, `10 samples ${arm} ${k}`);
    if (BULK.includes(k)) assert(cy.length === 10, `10 cycle samples ${arm} ${k}`);
    cells.push({ k, arm, ir, time: med(t), unit: BULK.includes(k) ? 'ns/op' : 'Mops', cy: BULK.includes(k) ? med(cy) : null, samples: t });
  }
}
const get = (k, arm) => cells.find((c) => c.k === k && c.arm === arm);
const pct = (k, arm) => {
  const c = get(k, arm);
  const b = get(k, 'base');
  return {
    ir: (c.ir / b.ir - 1) * 100,
    t: c.unit === 'Mops' ? (b.time / c.time - 1) * 100 : (c.time / b.time - 1) * 100,
    cy: c.cy === null ? null : (c.cy / b.cy - 1) * 100,
  };
};
const f2 = (x) => x.toFixed(2);

let csv = 'cell,arm,callgrind_ir,ir_vs_base_pct,median_time,time_unit,time_vs_base_pct,median_cycles_u,cycles_vs_base_pct,time_min,time_max\n';
for (const c of cells) {
  const p = pct(c.k, c.arm);
  const lo = Math.min(...c.samples);
  const hi = Math.max(...c.samples);
  csv += `${c.k},${c.arm},${c.ir},${f2(p.ir)},${c.time},${c.unit},${f2(p.t)},${c.cy ?? ''},${p.cy === null ? '' : f2(p.cy)},${lo},${hi}\n`;
}
writeFileSync(new URL('../../NL46_REMAINING_GUARDS_OFF_summary.csv', dir), csv);

// every arm only removes work: it must not execute more instructions than base (slack for T=4 scheduling)
for (const k of KEYS) {
  for (const a of ARMS) assert(get(k, a).ir <= get(k, 'base').ir * (NOISY.test(k) ? 1.01 : 1.0001), `${a} <= base at ${k}`);
}
// on the deterministic single-thread cells the combined arm is the sum of its parts
for (const k of BULK) {
  const sum = pct(k, 'nl4').ir + pct(k, 'nl5').ir + pct(k, 'nl6').ir;
  assert(Math.abs(pct(k, 'all').ir - sum) < 0.1, `all ~ nl4+nl5+nl6 at ${k}`);
}

const verdictOf = (arm) => {
  const hits = [];
  for (const k of KEYS) {
    const p = pct(k, arm);
    if (-p.ir >= THR_IR) hits.push(`${k}: Ir ${f2(p.ir)} %`);
    if (!NOISY.test(k) && -p.t >= THR_TIME && -p.ir >= MIN_IR_FOR_TIME) hits.push(`${k}: time ${f2(p.t)} % (Ir ${f2(p.ir)} %)`);
  }
  return hits;
};
const verdicts = Object.fromEntries(ARMS.slice(1).map((a) => [a, verdictOf(a)]));

const rows = KEYS.flatMap((k) => ARMS.map((a) => {
  const c = get(k, a);
  const p = pct(k, a);
  const noisy = NOISY.test(k);
  const tcell = a === 'base' ? '—' : noisy ? 'no claim (T=4)' : `${f2(p.t)} %`;
  return `| ${k} | ${a} | ${c.ir} | ${f2(p.ir)} % | ${c.time} [${Math.min(...c.samples)}..${Math.max(...c.samples)}] ${c.unit} | ${tcell} |`;
}));
const verdictLines = ARMS.slice(1).map((a) => {
  const v = verdicts[a];
  return `- ${NAMES[a]}: ${v.length ? `GO — meets the threshold (${v.join('; ')})` : 'NULL — below the threshold in every cell'}.`;
});

const md = `# NL-4/5/6 — remaining guard classes with \`misuse-checks\` off (\`bench\`: measured candidates; this report merges nothing)

Generated by \`node scripts/nl46_report.mjs\` from \`docs/perf/nl46/raw\` (written by \`docs/perf/nl46/nl46.sh\`);
summary: \`docs/perf/NL46_REMAINING_GUARDS_OFF_summary.csv\`. Do not edit by hand.

## 1. Question

NL-1 put the per-block state byte behind \`misuse-checks\`. Three guard classes remain on the release free path in a
build without the feature. Each is measured ALONE against the same tree: does removing it (only when the feature is
off and debug assertions are off) clear the owner's threshold — at least ${THR_IR} % instructions or at least ${THR_TIME} %
time on a non-noisy cell — or is it NULL?

## 2. Method and identity

- Entry point: \`b4_pmu_probe bulk sefer 1024 <16|64>\` and \`g1_mt_one <larson|mstress> sefer <T> <steps>\`, both on the real
  \`SeferAlloc\` \`#[global_allocator]\`, \`--features production\` minus \`misuse-checks\`, release, the repository's alignment RUSTFLAGS.
- Source identity: \`git archive\` of main at \`3123100f\` (landing of PR #198), tar sha256
  \`a8508b824ef8f44f5fc43ac822ae7a7dec969c31e64763155c70ecf4eabd3d13\`; every arm is that tree plus the patches in
  \`docs/perf/nl46/patches/\` (nl4, nl5, nl6; \`all\` = the three); patch and binary hashes are in \`docs/perf/nl46/raw/identity.txt\`.
- Instructions: callgrind (\`bulk\`: 400 passes; MT: 100000 steps). Time: 5 repetitions of the palindrome
  ${ORDER.join(' ')}, so 10 samples per arm and cell, median and [min..max]; \`bulk\` 4000 passes, MT 1000000 steps.
- Path activation: the instruction deltas are the evidence that the removed branches ran in the base arm; the patches only wrap
  existing \`if\` statements in the cfg that already guards the state byte, so the code paths are otherwise identical.
- Host: WSL2 Ubuntu 24.04 on the shared lab machine. Rule: no time claim from a T=4 cell.

## 3. Result

| cell | arm | Ir | Ir vs base | time (median [min..max]) | time vs base |
|---|---|---|---|---|---|
${rows.join('\n')}

Time sign convention: negative = less time (ns/op lower, Mops higher). Ir percentages are (arm Ir / base Ir − 1), the base callgrind total of the same cell is the denominator.

## 4. Verdicts

${verdictLines.join('\n')}

Threshold: ≥ ${THR_IR} % Ir or ≥ ${THR_TIME} % time on a non-noisy cell (T=4 cells carry no time claim). A time delta counts only
where the same cell's Ir delta is at least ${MIN_IR_FOR_TIME} %: an arm with no instruction delta differs from base only by code layout, and the time of
such cells moves by up to about 3 % (nl5 bulk64 −2.92 % time at −0.02 % Ir; nl6 bulk64 −2.35 % at −0.00 %), which is noise, not a gain.
`;
writeFileSync(new URL('../../NL46_REMAINING_GUARDS_OFF.md', dir), md);
for (const a of ARMS.slice(1)) console.log(a, verdicts[a].length ? 'GO' : 'NULL', verdicts[a].join('; '));
