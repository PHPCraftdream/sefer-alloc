// R32-11 (task #502, F10): derives `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv`
// from `scripts/paired-ab-runner.mjs`'s own provenance JSON files
// (`docs/perf/paired_ab_runs/*.json`) — the CLEAN-TIMING before/after
// comparisons for both regimes (multiple independent trials each), plus
// their same-vs-same controls. This is the ONE checked script that turns
// the raw paired-ab-runner provenance into the report's machine-readable
// summary + asserts the report's headline percentages/ratios (CLAUDE.md
// "tables derived by one checked script, not hand-transcribed" + "a script
// that computes a headline ratio must assert the arithmetic it prints").
//
// Usage:
//   node scripts/r32_11_shadow_head_summary.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

// Every CLEAN-TIMING (post-fix, timing-only `after` binary) provenance file
// this report cites — all timestamps at or after 2026-08-02T19:01Z, the
// point the harness switched from the oracle-bearing binary (which
// contaminated timing with its own counter RMW cost, see the report's
// Finding 1) to the timing-only `after_timing.exe` binary. Earlier
// timestamps (18:49-19:00Z) used the CONTAMINATED oracle-bearing binary and
// are cited SEPARATELY in the report's Finding 1 narrative, not included
// here as evidence for the corrected headline.
const FILES = {
  favorable_trial1: "docs/perf/paired_ab_runs/2026-08-02T19-18-32-844Z.json",
  favorable_trial2: "docs/perf/paired_ab_runs/2026-08-02T19-21-31-112Z.json",
  favorable_trial3: "docs/perf/paired_ab_runs/2026-08-02T19-52-59-897Z.json",
  favorable_before_control: "docs/perf/paired_ab_runs/2026-08-02T19-20-15-598Z.json",
  favorable_after_control: "docs/perf/paired_ab_runs/2026-08-02T19-19-53-674Z.json",
  adversarial_trial1: "docs/perf/paired_ab_runs/2026-08-02T19-19-29-944Z.json",
  adversarial_trial2: "docs/perf/paired_ab_runs/2026-08-02T19-22-26-516Z.json",
  adversarial_trial3_noisy: "docs/perf/paired_ab_runs/2026-08-02T19-54-14-023Z.json",
  adversarial_trial3_n30_rerun: "docs/perf/paired_ab_runs/2026-08-02T19-58-37-723Z.json",
  adversarial_trial4: "docs/perf/paired_ab_runs/2026-08-02T20-00-14-009Z.json",
  adversarial_after_control: "docs/perf/paired_ab_runs/2026-08-02T19-21-10-197Z.json",
};

function loadComparison(relPath) {
  const doc = JSON.parse(read(relPath));
  return { doc, cmp: doc.comparisons[0] };
}

function mean(arr) {
  return arr.reduce((a, b) => a + b, 0) / arr.length;
}

const TOTAL_PUSHES = 200000; // fixed by both harness variants (PRODUCERS * BLOCKS_PER_PRODUCER)

const rows = [];

for (const [label, path] of Object.entries(FILES)) {
  const { doc, cmp } = loadComparison(path);
  const meanA = mean(cmp.samples_a_ns);
  const meanB = mean(cmp.samples_b_ns);
  const nsPerPushA = meanA / TOTAL_PUSHES;
  const nsPerPushB = meanB / TOTAL_PUSHES;
  rows.push({
    label,
    arm_a: cmp.arm_a,
    arm_b: cmp.arm_b,
    n: cmp.pairs,
    mean_a_ns: meanA.toFixed(1),
    mean_b_ns: meanB.toFixed(1),
    ns_per_push_a: nsPerPushA.toFixed(4),
    ns_per_push_b: nsPerPushB.toFixed(4),
    delta_ns: (meanA - meanB).toFixed(1),
    pct_change: (((meanB - meanA) / meanA) * 100).toFixed(2),
    t: cmp.paired_t_test.t.toFixed(3),
    crit: cmp.paired_t_test.crit,
    significant: cmp.paired_t_test.significant,
    sign_a_faster: cmp.sign_test.aFaster,
    sign_b_faster: cmp.sign_test.bFaster,
    git_commit: doc.git_commit,
    timestamp: doc.timestamp,
  });
}

// ---------------------------------------------------------------------------
// Assert the report's headline claims (CLAUDE.md rule 6: a script that
// computes a headline ratio must assert the arithmetic it prints).
// ---------------------------------------------------------------------------

function assertRow(label, { minSignB, requireSignificant }) {
  const row = rows.find((r) => r.label === label);
  if (!row) throw new Error(`missing row ${label}`);
  if (!(Number(row.pct_change) < 0)) {
    throw new Error(`ASSERT FAILED: ${label} pct_change must be < 0 (AFTER faster), got ${row.pct_change}`);
  }
  if (row.sign_b_faster < minSignB) {
    throw new Error(
      `ASSERT FAILED: ${label} sign test must be >= ${minSignB} after-faster, got ${row.sign_a_faster}/${row.sign_b_faster}`,
    );
  }
  if (requireSignificant && !row.significant) {
    throw new Error(`ASSERT FAILED: ${label} must be statistically significant (t past crit)`);
  }
  return row;
}

// Favorable: all 3 trials must show AFTER faster, all 3 must reach
// significance, sign test must be the full 20/0 in favor of AFTER.
const favTrials = ["favorable_trial1", "favorable_trial2", "favorable_trial3"].map((l) =>
  assertRow(l, { minSignB: 20, requireSignificant: true }),
);

// Adversarial: all trials must show AFTER faster (direction consistent) and
// a lopsided sign test (>= 14/20 minimum observed, most trials >= 17/20);
// NOT all trials are required to reach t-test significance (trial 3's high
// variance under shared-host contention is honestly reported, not asserted
// away — see the report's own §3.2).
const advTrial1 = assertRow("adversarial_trial1", { minSignB: 17, requireSignificant: true });
const advTrial2 = assertRow("adversarial_trial2", { minSignB: 17, requireSignificant: true });
const advTrial3 = assertRow("adversarial_trial3_noisy", { minSignB: 14, requireSignificant: false });
const advTrial3n30 = assertRow("adversarial_trial3_n30_rerun", { minSignB: 28, requireSignificant: false });
const advTrial4 = assertRow("adversarial_trial4", { minSignB: 17, requireSignificant: true });

// All same-vs-same controls must be NOT significant (t < crit).
for (const label of ["favorable_before_control", "favorable_after_control", "adversarial_after_control"]) {
  const row = rows.find((r) => r.label === label);
  if (row.significant) {
    throw new Error(
      `ASSERT FAILED: same-vs-same control '${label}' must NOT be statistically significant (t=${row.t} >= crit=${row.crit}) — this would indicate a harness reliability problem, not a real effect`,
    );
  }
}

console.log("Favorable regime (3/3 trials significant, sign test 20/0 every time):");
for (const r of favTrials) {
  console.log(`  ${r.ns_per_push_a} -> ${r.ns_per_push_b} ns/push (${r.pct_change}%, t=${r.t}, sign=${r.sign_a_faster}/${r.sign_b_faster})`);
}
console.log("Adversarial regime (3/5 trials significant by t-test; sign test favors AFTER in ALL 5):");
for (const r of [advTrial1, advTrial2, advTrial3, advTrial3n30, advTrial4]) {
  console.log(`  ${r.ns_per_push_a} -> ${r.ns_per_push_b} ns/push (${r.pct_change}%, t=${r.t}, sig=${r.significant}, sign=${r.sign_a_faster}/${r.sign_b_faster}, n=${r.n})`);
}
console.log("All 3 same-vs-same controls: NOT statistically significant (harness reliability confirmed).");

// ---------------------------------------------------------------------------
// Write the summary CSV.
// ---------------------------------------------------------------------------

const header = [
  "label",
  "arm_a",
  "arm_b",
  "n",
  "mean_a_ns",
  "mean_b_ns",
  "ns_per_push_a",
  "ns_per_push_b",
  "delta_ns",
  "pct_change",
  "t",
  "crit",
  "significant",
  "sign_a_faster",
  "sign_b_faster",
  "git_commit",
  "timestamp",
];

const lines = [
  `# R32-11 (task #502, F10) summary — landing_commit=${landingCommit}`,
  `# cpu=Intel_Core_i7-11800H_2.30GHz os=Windows_10_Pro_10.0.19045 feature_set_before="alloc-global alloc-xthread bench-internals" feature_set_after="alloc-global alloc-xthread (timing-only, no bench-internals)"`,
  header.join(","),
  ...rows.map((r) => header.map((h) => r[h]).join(",")),
];

writeFileSync(
  new URL("docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv", ROOT),
  lines.join("\n") + "\n",
);
console.log("Wrote docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE_summary.csv");
