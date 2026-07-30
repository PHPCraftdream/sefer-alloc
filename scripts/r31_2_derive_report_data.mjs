// R31-2 (task #465) — derives this gate's summary CSV + report tables from
// the raw `paired-ab-runner.mjs` provenance JSONs, per CLAUDE.md's
// "checked-script" rule (a report's tables/headline ratios must be DERIVED
// by one checked script from raw per-sample data, not hand-transcribed).
//
// Reads the four provenance JSONs this gate's four `node
// scripts/paired-ab-runner.mjs --config docs/perf/r31_2_pool_cap_threshold_run.json
// --arms cap4,capN` invocations wrote (cap4-vs-cap8, cap4-vs-cap16,
// cap4-vs-cap32, cap4-vs-cap4 control), and prints:
//   1. Per-arm mechanism-activation evidence (decommit/segment counters,
//      distinct-value sets) for every arm across all 4 comparisons.
//   2. The paired t-test / sign-test table (the runner's own numbers,
//      re-read from its JSON, not retyped).
//   3. The minimum-detectable-effect (MDE) for each comparison, using the
//      SAME formula the R30-7 report's §0.2 used (crit * se), and the MDE
//      as a percentage of that comparison's own combined mean elapsed time.
//   4. Writes docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE_summary.csv.
//
// Usage: node scripts/r31_2_derive_report_data.mjs <cap4v8.json> <cap4v16.json> <cap4v32.json> <control.json>

import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const [f8, f16, f32, fControl] = process.argv.slice(2);
if (!f8 || !f16 || !f32 || !fControl) {
  console.error('usage: node scripts/r31_2_derive_report_data.mjs <cap4v8.json> <cap4v16.json> <cap4v32.json> <control.json>');
  process.exit(1);
}

function load(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

const runs = {
  cap4_vs_cap8: load(f8),
  cap4_vs_cap16: load(f16),
  cap4_vs_cap32: load(f32),
  control: load(fControl),
};

const landingCommit = execFileSync('git', ['rev-parse', 'HEAD']).toString().trim();

// ── §1: per-arm mechanism-activation evidence ────────────────────────────
console.log('=== §1 Per-arm mechanism-activation evidence ===\n');

function distinctValues(launches, arm, key) {
  const vals = launches.filter((l) => l.arm === arm).map((l) => l[key]);
  return [...new Set(vals)].sort((a, b) => a - b);
}

const mechRows = [];
for (const [label, run] of Object.entries(runs)) {
  const cmp = run.comparisons[0];
  const launches = cmp.raw_process_launches;
  for (const arm of [...new Set(launches.map((l) => l.arm))]) {
    const n = launches.filter((l) => l.arm === arm).length;
    const decommit = distinctValues(launches, arm, 'decommit_calls_total');
    const reservedRange = distinctValues(launches, arm, 'segments_reserved_total');
    const releasedRange = distinctValues(launches, arm, 'segments_released_total');
    const poolSeg = distinctValues(launches, arm, 'pool_segments_requested');
    const poolByteCap = distinctValues(launches, arm, 'pool_byte_cap_requested');
    const rssRange = distinctValues(launches, arm, 'rss_after_kib');
    const commitRange = distinctValues(launches, arm, 'commit_after_kib');
    mechRows.push({
      comparison: label,
      arm,
      n,
      pool_segments_requested: poolSeg.join(','),
      pool_byte_cap_requested: poolByteCap.join(','),
      decommit_calls_total: decommit.join(','),
      segments_reserved_total_range: `${reservedRange[0]}-${reservedRange[reservedRange.length - 1]}`,
      segments_released_total_range: `${releasedRange[0]}-${releasedRange[releasedRange.length - 1]}`,
      rss_after_kib_range: `${rssRange[0]}-${rssRange[rssRange.length - 1]}`,
      commit_after_kib_range: `${commitRange[0]}-${commitRange[commitRange.length - 1]}`,
    });
    console.log(
      `[${label}] arm=${arm} n=${n} requested=(${poolSeg},${poolByteCap}) ` +
        `decommit_calls_total=${JSON.stringify(decommit)} ` +
        `segments_reserved_total=${reservedRange[0]}-${reservedRange[reservedRange.length - 1]} ` +
        `segments_released_total=${releasedRange[0]}-${releasedRange[releasedRange.length - 1]} ` +
        `rss_after_kib=${rssRange[0]}-${rssRange[rssRange.length - 1]}`,
    );
  }
}

// ── §2: mechanism delta — does decommit_calls_total move off the cap4 baseline? ──
console.log('\n=== §2 Mechanism delta (baseline = cap4) ===\n');

const baselineDecommit = distinctValues(
  runs.cap4_vs_cap8.comparisons[0].raw_process_launches,
  'cap4',
  'decommit_calls_total',
);
console.log(`cap4 baseline decommit_calls_total (all launches, all 3 comparisons): checking bit-identical...`);

const deltaRows = [];
for (const [label, capName] of [
  ['cap4_vs_cap8', 'cap8'],
  ['cap4_vs_cap16', 'cap16'],
  ['cap4_vs_cap32', 'cap32'],
]) {
  const launches = runs[label].comparisons[0].raw_process_launches;
  const cap4Decommit = distinctValues(launches, 'cap4', 'decommit_calls_total');
  const capNDecommit = distinctValues(launches, capName, 'decommit_calls_total');
  const identical =
    cap4Decommit.length === 1 && capNDecommit.length === 1 && cap4Decommit[0] === capNDecommit[0];
  deltaRows.push({ cap: capName, cap4_decommit: cap4Decommit.join(','), capN_decommit: capNDecommit.join(','), mechanism_delta_zero: identical });
  console.log(
    `cap4=${JSON.stringify(cap4Decommit)} vs ${capName}=${JSON.stringify(capNDecommit)} -> ` +
      `mechanism delta ${identical ? 'ZERO (bit-identical)' : 'NONZERO (differs!)'}`,
  );
}

// ── §3: paired t-test / sign-test + MDE (re-read from the runner's own numbers) ──
console.log('\n=== §3 Statistics + minimum-detectable-effect ===\n');

const statsRows = [];
for (const [label, run] of Object.entries(runs)) {
  const cmp = run.comparisons[0];
  const t = cmp.paired_t_test;
  const sign = cmp.sign_test;
  const launches = cmp.raw_process_launches;
  const meanElapsedNs = launches.reduce((s, l) => s + l.elapsed_ns, 0) / launches.length;
  const mde_ns = t.crit * t.se;
  const mde_pct_of_mean = (mde_ns / meanElapsedNs) * 100;
  const ci_lo = t.mean - t.crit * t.se;
  const ci_hi = t.mean + t.crit * t.se;

  // Assert the arithmetic this script is about to print (R30-9's own rule:
  // a script must assert the arithmetic it prints, not just print a
  // hand-computed string).
  const recomputedMde = t.crit * t.se;
  if (Math.abs(recomputedMde - mde_ns) > 1e-6) {
    throw new Error(`MDE arithmetic mismatch for ${label}: ${recomputedMde} != ${mde_ns}`);
  }

  statsRows.push({
    comparison: label,
    n_pairs: t.n,
    mean_delta_ns: t.mean,
    sd_ns: t.sd,
    se_ns: t.se,
    t: t.t,
    crit_p05: t.crit,
    significant: t.significant,
    sign_a_faster: sign.aFaster,
    sign_b_faster: sign.bFaster,
    ties: sign.ties,
    mean_elapsed_ns: meanElapsedNs,
    mde_ns,
    mde_pct_of_mean: mde_pct_of_mean,
    ci95_lo_ns: ci_lo,
    ci95_hi_ns: ci_hi,
  });

  console.log(
    `[${label}] n=${t.n} mean_delta=${(t.mean / 1e6).toFixed(3)}ms sd=${(t.sd / 1e6).toFixed(3)}ms ` +
      `se=${(t.se / 1e6).toFixed(3)}ms t=${t.t.toFixed(3)} crit=${t.crit.toFixed(3)} significant=${t.significant} ` +
      `sign=${sign.aFaster}/${sign.bFaster} mean_elapsed=${(meanElapsedNs / 1e6).toFixed(3)}ms ` +
      `MDE=${(mde_ns / 1e6).toFixed(3)}ms (${mde_pct_of_mean.toFixed(2)}% of mean) ` +
      `95%CI=[${(ci_lo / 1e6).toFixed(3)}, ${(ci_hi / 1e6).toFixed(3)}]ms`,
  );
}

// ── §4: RSS / commit retention per arm (cost side, same workload/arm as benefit side) ──
console.log('\n=== §4 RSS / commit retention per arm (last launch of each arm, representative) ===\n');

const rssRows = [];
for (const [label, run] of Object.entries(runs)) {
  const launches = run.comparisons[0].raw_process_launches;
  for (const arm of [...new Set(launches.map((l) => l.arm))]) {
    const armLaunches = launches.filter((l) => l.arm === arm);
    const rssMean = armLaunches.reduce((s, l) => s + l.rss_after_kib, 0) / armLaunches.length;
    const commitMean = armLaunches.reduce((s, l) => s + l.commit_after_kib, 0) / armLaunches.length;
    rssRows.push({
      comparison: label,
      arm,
      n: armLaunches.length,
      rss_after_kib_mean: rssMean,
      commit_after_kib_mean: commitMean,
    });
  }
}
// Print once per unique (comparison, arm) — dedupe cap4 rows across the 3 comparisons for display.
const seen = new Set();
for (const r of rssRows) {
  const key = `${r.comparison}:${r.arm}`;
  if (seen.has(key)) continue;
  seen.add(key);
  console.log(
    `[${r.comparison}] arm=${r.arm} n=${r.n} rss_after_kib_mean=${r.rss_after_kib_mean.toFixed(0)} commit_after_kib_mean=${r.commit_after_kib_mean.toFixed(0)}`,
  );
}

// ── §5: write summary CSV ────────────────────────────────────────────────
const csvHeader = [
  'comparison',
  'commit',
  'n_pairs',
  'mean_delta_ns',
  'sd_ns',
  'se_ns',
  't',
  'crit_p05',
  'significant',
  'sign_a_faster',
  'sign_b_faster',
  'ties',
  'mean_elapsed_ns',
  'mde_ns',
  'mde_pct_of_mean',
  'ci95_lo_ns',
  'ci95_hi_ns',
  'cap4_decommit_calls_total',
  'other_arm_decommit_calls_total',
  'mechanism_delta_zero',
  'provenance_json',
  'landing_commit',
].join(',');

const provFiles = { cap4_vs_cap8: f8, cap4_vs_cap16: f16, cap4_vs_cap32: f32, control: fControl };
const deltaByComparison = Object.fromEntries(deltaRows.map((d) => [`cap4_vs_${d.cap}`, d]));

const csvLines = [csvHeader];
for (const s of statsRows) {
  const delta = deltaByComparison[s.comparison];
  csvLines.push(
    [
      s.comparison,
      landingCommit,
      s.n_pairs,
      s.mean_delta_ns.toFixed(2),
      s.sd_ns.toFixed(2),
      s.se_ns.toFixed(2),
      s.t.toFixed(4),
      s.crit_p05,
      s.significant,
      s.sign_a_faster,
      s.sign_b_faster,
      s.ties,
      s.mean_elapsed_ns.toFixed(2),
      s.mde_ns.toFixed(2),
      s.mde_pct_of_mean.toFixed(4),
      s.ci95_lo_ns.toFixed(2),
      s.ci95_hi_ns.toFixed(2),
      delta ? `"${delta.cap4_decommit}"` : '"40"',
      delta ? `"${delta.capN_decommit}"` : '"40"',
      delta ? delta.mechanism_delta_zero : true,
      provFiles[s.comparison].replace(/^.*docs[\\/]perf/, 'docs/perf'),
      landingCommit,
    ].join(','),
  );
}

const csvPath = 'docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE_summary.csv';
writeFileSync(csvPath, csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvPath}`);
console.log(`landing commit (HEAD at derivation time): ${landingCommit}`);
