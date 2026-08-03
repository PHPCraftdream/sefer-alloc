// F7 (task #495): derives `docs/perf/R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE_summary.csv`
// from the two raw iai-callgrind logs this task's BEFORE/AFTER measurement
// produced (`docs/perf/_raw_r495_stamp_removal_before.log` /
// `_raw_r495_stamp_removal_after.log`). This is the ONE checked script that
// turns the raw logs into the report's machine-readable summary (CLAUDE.md
// "tables derived by one checked script, not hand-transcribed").
//
// What this measures: removing the redundant `stamp_segment_owner` call from
// `HeapCore::alloc_small_zeroed_via_magazine`'s magazine-HIT arm
// (`src/registry/heap_core_alloc.rs`) -- see this file's own git-log entry
// for the full enumeration/justification. The BEFORE log was captured in an
// isolated `git worktree` at commit 5d72bc633193938181e2d06f8c584617ebaecf42
// (this task's base) with ONLY the new bench pair
// (`alloc_zeroed_magazine_prefill_only_16b` /
// `alloc_zeroed_magazine_hit_only_16b`, benches/perf_gate_iai.rs) applied --
// NOT the stamp removal -- so the BEFORE log measures the OLD (stamp-present)
// hit arm through the identical bench shape the AFTER log uses. The AFTER
// log was captured in the main working tree with the stamp removed.
//
// Immutable source identity (CLAUDE.md's R29-6 rule):
//   - AFTER:  working tree at commit 5d72bc633193938181e2d06f8c584617ebaecf42
//             + this task's full diff (stamp removal + new bench pair).
//   - BEFORE: `git worktree add` at commit
//             5d72bc633193938181e2d06f8c584617ebaecf42, with ONLY
//             `git diff -- benches/perf_gate_iai.rs` (the bench-only half of
//             this task's diff) applied; `git write-tree` of that state ==
//             3dfa28c9532427cbf00e4204f98c80d783056608 (recorded below).
//
// Usage:
//   node scripts/r495_stamp_removal_summary.mjs [landing_commit_sha]
//
// `landing_commit_sha` is this task's own landing commit (chicken-and-egg: a
// commit cannot cite its own SHA inside its own tree). If omitted, the SHA is
// derived from `git rev-parse HEAD` at run time (R33-8; see
// scripts/r33_6_decay_throttle_retention_summary.mjs), so re-running the script
// reproduces the column without a hand-edited follow-up commit.

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

const META = {
  base_commit_sha: "5d72bc633193938181e2d06f8c584617ebaecf42",
  before_tree_sha: "3dfa28c9532427cbf00e4204f98c80d783056608",
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045_(WSL/valgrind callgrind)",
  feature_set: "production bench-internals virgin-zero-skip",
};

const HEADER =
  "base_commit_sha,before_tree_sha,feature_set,cpu,os,bench,before_ir,after_ir,delta_ir,kind,landing_commit";

/** Parse `bench  Ir  L1  L2  RAM  EstCycles  Ir/op` table rows out of one
 * `npm run iai`-style raw log (the printed report table, not the raw
 * iai-callgrind per-bench blocks -- same table shape both `_before`/`_after`
 * logs share). Returns a Map<name, ir>. */
function parseTable(logText) {
  const out = new Map();
  const lines = logText.split(/\r?\n/);
  // Table rows look like:
  //   "  alloc_zeroed_magazine_hit_only_16b             8,431        10,996 ..."
  // i.e. leading whitespace, a bench name (word chars/underscore), then
  // whitespace-separated numeric columns (first one is Ir, may have commas).
  const rowRe = /^\s{2}([A-Za-z_][\w]*)\s{2,}([\d,]+)\s{2,}[\d,]+\s{2,}[\d,]+\s{2,}[\d,]+\s{2,}[\d,]+/;
  for (const line of lines) {
    const m = rowRe.exec(line);
    if (m) {
      out.set(m[1], Number(m[2].replace(/,/g, "")));
    }
  }
  return out;
}

const beforeLog = read("docs/perf/_raw_r495_stamp_removal_before.log");
const afterLog = read("docs/perf/_raw_r495_stamp_removal_after.log");

const before = parseTable(beforeLog);
const after = parseTable(afterLog);

const BENCHES = [
  { name: "alloc_zeroed_magazine_prefill_only_16b", kind: "control (untouched by F7)" },
  { name: "alloc_zeroed_magazine_hit_only_16b", kind: "treatment (16 magazine hits via alloc_zeroed)" },
  { name: "small_churn_16b", kind: "kill-gate (plain alloc, must be flat)" },
  { name: "churn_256b", kind: "kill-gate (plain alloc, must be flat)" },
  { name: "aligned_churn_640b_a128", kind: "kill-gate (plain alloc, must be flat)" },
  { name: "cold_alloc_free_256x16b", kind: "kill-gate (plain alloc, must be flat)" },
];

const rows = [];
for (const { name, kind } of BENCHES) {
  const b = before.get(name);
  const a = after.get(name);
  if (b == null || a == null) {
    throw new Error(
      `r495_stamp_removal_summary: bench "${name}" missing from ${b == null ? "BEFORE" : "AFTER"} log — cannot derive summary`,
    );
  }
  rows.push({ name, before: b, after: a, delta: a - b, kind });
}

// --- Assertions (CLAUDE.md: "a script that computes a headline ratio must
// assert the arithmetic it prints, not just print a hand-computed string") ---

const prefill = rows.find((r) => r.name === "alloc_zeroed_magazine_prefill_only_16b");
const hit = rows.find((r) => r.name === "alloc_zeroed_magazine_hit_only_16b");

// The control arm (prefill; plain `alloc`/`dealloc` only, never touches
// `alloc_zeroed`) must be EXACTLY flat -- it is untouched code.
if (prefill.delta !== 0) {
  throw new Error(
    `r495_stamp_removal_summary: control arm "alloc_zeroed_magazine_prefill_only_16b" ` +
      `moved by ${prefill.delta} Ir -- expected exactly 0 (this arm never calls alloc_zeroed). ` +
      `Investigate before trusting the treatment-arm delta.`,
  );
}

// The four plain-`alloc` kill-gate benches must be exactly flat too (F7 only
// touches `alloc_small_zeroed_via_magazine`, never plain `alloc`).
for (const r of rows) {
  if (r.kind.startsWith("kill-gate") && r.delta !== 0) {
    throw new Error(
      `r495_stamp_removal_summary: kill-gate bench "${r.name}" moved by ${r.delta} Ir ` +
        `(expected exactly 0 -- F7 does not touch plain \`alloc\`). This is a red flag, not noise ` +
        `(iai-callgrind's Ir count is deterministic run-to-run).`,
    );
  }
}

// Treatment arm: must show a NEGATIVE delta (Ir went down -- the stamp was
// removed, not added or left neutral).
if (hit.delta >= 0) {
  throw new Error(
    `r495_stamp_removal_summary: treatment arm "alloc_zeroed_magazine_hit_only_16b" ` +
      `delta is ${hit.delta} Ir (expected negative -- removing an instruction sequence must not increase Ir).`,
  );
}

const MAGAZINE_FILL = 16; // matches benches/perf_gate_iai.rs's MAGAZINE_FILL const
const perHitDeltaIr = hit.delta / MAGAZINE_FILL;
// Sanity-check against the task brief's own predicted range (12-18 Ir/hit).
const perHitAbs = Math.abs(perHitDeltaIr);
if (perHitAbs < 5 || perHitAbs > 30) {
  throw new Error(
    `r495_stamp_removal_summary: per-hit Ir delta ${perHitDeltaIr.toFixed(2)} is outside the ` +
      `sanity range [5, 30] Ir/hit the task brief predicted (12-18) -- re-verify the measurement ` +
      `before publishing.`,
  );
}

const lines = [HEADER];
for (const r of rows) {
  lines.push(
    [
      META.base_commit_sha,
      META.before_tree_sha,
      META.feature_set,
      META.cpu,
      META.os,
      r.name,
      r.before,
      r.after,
      r.delta,
      `"${r.kind}"`,
      landingCommit,
    ].join(","),
  );
}

const outPath = new URL("docs/perf/R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE_summary.csv", ROOT);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log("[r495-summary] wrote docs/perf/R32_4_ALLOC_ZEROED_MAGAZINE_HIT_STAMP_REMOVAL_GATE_summary.csv");
console.log(
  `[r495-summary] control (prefill) delta: ${prefill.delta} Ir (asserted == 0)`,
);
console.log(
  `[r495-summary] treatment (hit) delta: ${hit.delta} Ir over ${MAGAZINE_FILL} hits ` +
    `= ${perHitDeltaIr.toFixed(2)} Ir/hit removed (asserted in [-30, -5] Ir/hit)`,
);
console.log(
  `[r495-summary] kill-gates (small_churn_16b, churn_256b, aligned_churn_640b_a128, ` +
    `cold_alloc_free_256x16b): all asserted == 0 delta`,
);
