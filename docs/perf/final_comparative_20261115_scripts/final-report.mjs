// Builds docs/perf/FINAL_COMPARATIVE_BENCH_20261115.md from the two derived summary CSVs (tables are never typed by hand).
import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const read = (p) => readFileSync(p, 'utf8').trim().split('\n').map(l => l.split(','));
const [bh, ...b] = read('docs/perf/FINAL_COMPARATIVE_BENCH_20261115_summary.csv');
const [jh, ...j] = read('docs/perf/J13_FINAL_COMPARATIVE_20261115_summary.csv');
const bi = Object.fromEntries(bh.map((h, i) => [h, i]));
const ji = Object.fromEntries(jh.map((h, i) => [h, i]));
const fx = (r) => { const v = Number(r[bi.sefer_vs_mimalloc_x]); return v >= 1 ? `${v.toFixed(2)}x faster` : `${(1 / v).toFixed(2)}x slower`; };

const sized = (g) => b.filter(r => r[bi.group] === g).map(r => `| ${r[bi.size]} | ${r[bi.sefer]} | ${r[bi.mimalloc]} | ${r[bi.system]} | ${fx(r)} |`).join('\n');
const one = (g) => b.filter(r => r[bi.group] === g).map(r => `| ${r[bi.sefer]} | ${r[bi.mimalloc]} | ${r[bi.system]} | ${fx(r)} |`).join('\n');
const head = '| Size | SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |\n|---|---:|---:|---:|---:|';

const jrows = j.map(r => {
  const ratio = Number(r[ji.always_over_off]);
  assert(Number(r[ji.ci95_lo]) <= ratio && ratio <= Number(r[ji.ci95_hi]), `${r[ji.cell]}: ratio outside its CI`);
  assert.equal(r[ji.aa_interval_contains_1], 'true', `${r[ji.cell]}: A/A not readable`);
  const note = r[ji.ci95_lo] <= 1 && Number(r[ji.ci95_hi]) >= 1 ? 'no difference detected' : (ratio < 1 ? `Always ${(1 / ratio).toFixed(2)}x faster` : `Always ${ratio.toFixed(2)}x slower`);
  return `| ${r[ji.cell]} | ${r[ji.regime]} | ${ratio.toFixed(4)} (${r[ji.ci95_lo]}-${r[ji.ci95_hi]}) | ${r[ji.aa_ratio]} (${r[ji.aa_lo]}-${r[ji.aa_hi]}) | ${r[ji.off_median_ns]} -> ${r[ji.always_median_ns]} | ${note} |`;
}).join('\n');

const sha = b[0][bi.source_sha];
const wn = b[0][bi.wnice_exe_sha256];
const md = `# Final comparative measurements under hard limits (2026-11-15)

Status: measurement-only. No \`perf(runtime)\` / \`perf(opt-in)\` change; no product GO/veto is derived here; relative numbers, not canonical ones.

## Identity

- Source: \`${sha}\` (main after PR #234); the measured tree equals it (the worktree differs only by documents).
- Limiter: \`wnice\` built by GitHub CI from win-nice master \`0acb5f7\` (sha256 prefix \`${wn}\`), installed system-wide on 2026-11-15.
- Host: Windows 10 19045, 11th Gen Intel Core i7-11800H (16 logical CPUs), WSL2 Ubuntu-24.04 (kernel 6.18, THP \`madvise\`, 4096-byte pages, 2 MiB huge pages, 24.2 GB available).
- Toolchains: Windows bench table rustc 1.97.0; WSL J13 study rustc 1.98.1 (recorded in the build receipt).

## Constraints (owner request)

Everything ran under \`wnice -p high -c 10\` — a hard cap of 10% of all logical CPUs (1.6 CPUs) with HIGH priority.
- Windows: \`wnice -p high -c 10 -- node scripts\\bench-table.mjs\`. The cap is a Job Object hard cap (flags 0x5, rate 1000); HIGH reaches every process of the tree through wnice's unelevated booster (job completion port + SetPriorityClass).
- WSL2: \`wnice --wsl -c 10 -p high -- bash j13-run.sh <phase>\` — the limits are applied inside the guest (systemd scope: CPUQuota=160%, Nice=-20, CPUWeight=10000), because a Windows Job Object does not reach the VM.
- Job-free launch: the agent shell is itself inside a Job Object (nested CPU rates multiply: 10% became 0.9% in the W1 spike), so every measurement was started through WMI \`Win32_Process.Create\` (\`scripts/run-job-free.ps1\`; probe \`in_job=False\` before wnice, hard cap exactly 10% under it).
- Limits of the method: a single-threaded benchmark uses at most 1 of the 1.6 allowed CPUs, so it is not throttled; multi-threaded parts and compilation are. The WSL guest cap does not stop Windows-side processes from displacing the VM threads (the host-side vmmem process cannot be opened without elevation). The numbers are suitable for relative comparison only.

## 1. bench:table — SeferAlloc vs mimalloc vs System (ns per pair unless stated)

\`bench-table\` self-check: PASS, 159 bench ids parsed, all 51 expected ids present. Raw log: \`docs/perf/_raw_final_comparative_bench_20261115.log\`; summary: \`docs/perf/FINAL_COMPARATIVE_BENCH_20261115_summary.csv\` (derived by \`bench-table-to-csv.mjs\`).

### Warm repeated bulk burst (1 alloc + 1 free per pair)

${head}
${sized('global_alloc')}

### Churn, non-writing

${head}
${sized('global_alloc_churn')}

### Churn + write

${head}
${sized('global_alloc_churn_write')}

### Churn + teardown (diagnostic composite, not a clean per-pair figure)

${head}
${sized('global_alloc_churn_with_teardown')}

### manual_realloc_sim (ns per closure: manual grow chain, not GlobalAlloc::realloc)

| SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---:|---:|---:|---:|
${one('manual_realloc_sim')}

### segment_decommit_cycle (ns per batch of 34 alloc + 34 free)

| SeferAlloc | mimalloc | System | SeferAlloc vs mimalloc |
|---:|---:|---:|---:|
${one('segment_decommit_cycle')}

Reading: SeferAlloc is faster than mimalloc in every sized cell except churn+teardown at 1024B, and slower in the manual realloc chain. Each ratio is mimalloc/SeferAlloc of the printed point estimates (the CSV script asserts it within rounding).

## 2. J13 registered study — Always vs Off (WSL2, tail trim included)

Registered protocol unchanged: 10 cells, 12 ABBA blocks plus 12 A/A blocks each (960 children), min of 2 launches per block, exp(mean of 12 paired log ratios), two-sided 95% paired t interval (df=11, t*=2.201). The CI blocks \`G1 limited-study single CI build and provenance receipt\` and \`Manual J13 measurement\` were extracted verbatim from the repository (\`extract-j13-steps.mjs\`) and run unmodified in the guest. A discarded 16-child smoke came first (all host checks passed, reducer refused the one-block input as intended). The whole study took 2 min 40 s (children are milliseconds long).

Always/Off is the ratio of the selected metric (cold_ns, or cache_ns for the CACHE cell); below 1 means Always is faster. Every A/A interval contains 1 (payload readability only, never speed evidence). Evidence: \`docs/perf/j13_final_20261115/\` (files above 200 KiB gzip-compressed); summary: \`docs/perf/J13_FINAL_COMPARATIVE_20261115_summary.csv\` (derived by \`j13-summary.mjs\` from the per-cell reducer reports).

| Cell | Regime | Always/Off (95% CI) | A/A payload ratio (95% CI) | Median Off -> Always (ns) | Reading |
|---|---|---:|---:|---:|---|
${jrows}

Reading, with numerators and denominators: for cold allocation+touch+free at 8 MiB and above, Always/Off is 0.37-0.46 (geometric mean of 12 per-block ratios of Always over Off), i.e. Always is 2.2-2.7x faster; at 1 MiB and 8 MiB minus 1 byte (below the 8 MiB threshold, advice not applied) no difference is detected. The CACHE cell (second life of a cached 16 MiB block) shows no detectable difference (interval contains 1), unlike the earlier GitHub-runner study (1.0231, 1.0093-1.0371).

**Regression to report to the owner (not new, not a veto):** edge-128MiB (128 MiB request, 1 repetition, untouched payload, cache off) is ${Number(j.find(r => r[ji.cell] === 'edge-128MiB')[ji.always_over_off]).toFixed(1)}x slower with Always: median ${j.find(r => r[ji.cell] === 'edge-128MiB')[ji.off_median_ns]} ns (Off) -> ${j.find(r => r[ji.cell] === 'edge-128MiB')[ji.always_median_ns]} ns (Always), roughly 145 microseconds of madvise cost over 128 MiB when no page is touched. The earlier registered study measured the same cell at 10.04 (8.71-11.58). Advice has a fixed per-MiB cost that only pays off when the memory is touched; no default change is inferred.

Comparability caveats: the earlier study ran on a GitHub runner (AMD EPYC, bare-metal-like) at rustc 1.99.1 before tail trim; this one runs in a WSL2 VM on a laptop CPU under a CPU cap, so absolute magnitudes differ (for example cycle-8MiB 0.4602 here vs 0.1336 there) and only the direction and the per-cell readability carry over.

## Reproduction

Scripts are kept in \`docs/perf/final_comparative_20261115_scripts/\`: \`run-job-free.ps1\`, \`jobprobe.ps1\`, \`bench-table-to-csv.mjs\`, \`extract-j13-steps.mjs\`, \`wsl-setup-j13.sh\`, \`j13-run.sh\`, \`j13-export.sh\`, \`j13-summary.mjs\`, \`final-report.mjs\`. Order: setup guest (\`wsl-setup-j13.sh <sha>\`), extract the two CI blocks, then \`run-job-free.ps1 ... wnice --wsl -c 10 -p high -- bash j13-run.sh build|smoke|full\`; export, derive the CSVs, build this report.
`;
writeFileSync('docs/perf/FINAL_COMPARATIVE_BENCH_20261115.md', md);
console.log(`wrote report: ${md.split('\n').length} lines`);
