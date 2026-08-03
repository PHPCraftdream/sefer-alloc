// F4/R32-5 (task #496): derives `docs/perf/R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv`
// from the four raw iai-callgrind logs this task's BEFORE/AFTER measurement
// produced:
//   docs/perf/_raw_r496_repr_c_before_production.log
//   docs/perf/_raw_r496_repr_c_after_production.log
//   docs/perf/_raw_r496_repr_c_before_virginzeroskip.log
//   docs/perf/_raw_r496_repr_c_after_virginzeroskip.log
// This is the ONE checked script that turns the raw logs into the report's
// machine-readable summary (CLAUDE.md "tables derived by one checked
// script, not hand-transcribed").
//
// What this measures: adding `#[repr(C)]` to `PerClass`
// (`src/registry/tcache.rs`) and reordering its declared fields to
// `count, virgin_mask, slots` so the struct's own doc comment's claimed
// "count and slots share one 64-byte cache line" property is actually
// delivered under `#[repr(C)]`'s declaration-order layout guarantee — see
// this file's own git-log entry / `docs/perf/R32_5_PERCLASS_REPR_C_LAYOUT_FIX_GATE.md`
// for the full offset derivation (offset_of(count) went from 128 to 0,
// offset_of(slots) went from 0 to 8; struct size unchanged at 136 bytes in
// both feature configurations).
//
// The BEFORE logs were captured in an isolated `git worktree` at commit
// 62e217fa1ca599d5903fb519c16ab9f0af55a7e0 (this task's base, HEAD at the
// time this task started) with NO changes applied (`PerClass` still
// repr(Rust)) -- both BEFORE logs measure through byte-identical bench
// source to their AFTER counterpart (no bench-file diff was needed: the
// pre-existing `alloc_magazine_prefill_only_16b` / `_hit_only_16b` and
// `alloc_zeroed_magazine_prefill_only_16b` / `_hit_only_16b` pairs already
// isolate exactly the magazine push/pop path this fix targets -- see
// `benches/perf_gate_iai.rs`'s R23-3/F7 module notes).
//
// Immutable source identity (CLAUDE.md's R29-6 rule):
//   - BEFORE: `git worktree add` at commit
//             62e217fa1ca599d5903fb519c16ab9f0af55a7e0, no changes applied.
//   - AFTER:  working tree at the same base commit + this task's full diff
//             to `src/registry/tcache.rs` (no bench-file changes). Patch
//             hash (`git diff -- src/registry/tcache.rs | sha256sum`):
//             64d6a1ab3c0a0d861e8d52574bdcd2610ea003bf4744d9e90df1d44b8a54cbc9
//
// Usage:
//   node scripts/r496_perclass_repr_c_summary.mjs [landing_commit_sha]
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
  base_commit_sha: "62e217fa1ca599d5903fb519c16ab9f0af55a7e0",
  after_patch_sha256: "64d6a1ab3c0a0d861e8d52574bdcd2610ea003bf4744d9e90df1d44b8a54cbc9",
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045_(WSL/valgrind callgrind)",
};

const HEADER =
  "base_commit_sha,after_patch_sha256,feature_set,cpu,os,bench,before_ir,after_ir,delta_ir,kind,landing_commit";

/** Parse `bench  Ir  L1  L2  RAM  EstCycles  Ir/op` table rows out of one
 * `npm run iai`-style raw log (the printed report table, not the raw
 * iai-callgrind per-bench blocks). Returns a Map<name, ir>. */
function parseTable(logText) {
  const out = new Map();
  const lines = logText.split(/\r?\n/);
  const rowRe = /^\s{2}([A-Za-z_][\w]*)\s{2,}([\d,]+)\s{2,}[\d,]+\s{2,}[\d,]+\s{2,}[\d,]+\s{2,}[\d,]+/;
  for (const line of lines) {
    const m = rowRe.exec(line);
    if (m) {
      out.set(m[1], Number(m[2].replace(/,/g, "")));
    }
  }
  return out;
}

/** Fallback parse: pull `Ir` straight from iai-callgrind's own per-bench
 * "perf_gate_iai::perf_gate::<name>" / "Instructions: <n>" block pair. Used
 * to backfill any bench the printed summary table's `--filter` happened to
 * exclude (the table only lists benches passed as CLI args to
 * `scripts/iai.mjs`, but every bench iai-callgrind actually RAN still gets
 * its own raw per-bench block regardless of that filter) -- merged into,
 * never overriding, `parseTable`'s result. */
function parsePerBenchBlocks(logText) {
  const out = new Map();
  const re = /perf_gate_iai::perf_gate::([A-Za-z_]\w*)\r?\n\s+Instructions:\s+(\d+)\|/g;
  let m;
  while ((m = re.exec(logText)) !== null) {
    if (!out.has(m[1])) out.set(m[1], Number(m[2]));
  }
  return out;
}

function parseLog(logText) {
  const merged = parsePerBenchBlocks(logText);
  const table = parseTable(logText);
  for (const [k, v] of table) merged.set(k, v); // table values are the same source; prefer if present
  return merged;
}

const prodBefore = parseLog(read("docs/perf/_raw_r496_repr_c_before_production.log"));
const prodAfter = parseLog(read("docs/perf/_raw_r496_repr_c_after_production.log"));
const vzsBefore = parseLog(read("docs/perf/_raw_r496_repr_c_before_virginzeroskip.log"));
const vzsAfter = parseLog(read("docs/perf/_raw_r496_repr_c_after_virginzeroskip.log"));

const PROD_FEATURES = "production bench-internals";
const VZS_FEATURES = "production bench-internals virgin-zero-skip";

const PROD_BENCHES = [
  { name: "alloc_magazine_prefill_only_16b", kind: "control (shared prefill prefix)" },
  { name: "alloc_magazine_hit_only_16b", kind: "treatment (16 magazine hits via alloc)" },
  { name: "small_churn_16b", kind: "kill-gate (plain alloc, absolute Ir shifts w/ bootstrap)" },
  { name: "churn_256b", kind: "kill-gate (plain alloc, absolute Ir shifts w/ bootstrap)" },
  { name: "aligned_churn_640b_a128", kind: "kill-gate (plain alloc, absolute Ir shifts w/ bootstrap)" },
  { name: "cold_alloc_free_256x16b", kind: "kill-gate (carve path, absolute Ir shifts w/ bootstrap)" },
  { name: "large_alloc_free_cycle", kind: "bootstrap proxy (process-wide one-time init cost, B)" },
];

const VZS_BENCHES = [
  { name: "alloc_magazine_prefill_only_16b", kind: "control (shared prefill prefix)" },
  { name: "alloc_magazine_hit_only_16b", kind: "treatment (16 magazine hits via alloc)" },
  { name: "alloc_zeroed_magazine_prefill_only_16b", kind: "control (shared prefill prefix, calloc-shaped)" },
  { name: "alloc_zeroed_magazine_hit_only_16b", kind: "treatment (16 magazine hits via alloc_zeroed)" },
  { name: "small_churn_16b", kind: "kill-gate (plain alloc, absolute Ir shifts w/ bootstrap)" },
  { name: "churn_256b", kind: "kill-gate (plain alloc, absolute Ir shifts w/ bootstrap)" },
  { name: "large_alloc_free_cycle", kind: "bootstrap proxy (process-wide one-time init cost, B)" },
];

function buildRows(before, after, benches, featureSet) {
  const rows = [];
  for (const { name, kind } of benches) {
    const b = before.get(name);
    const a = after.get(name);
    if (b == null || a == null) {
      throw new Error(
        `r496_perclass_repr_c_summary: bench "${name}" (${featureSet}) missing from ` +
          `${b == null ? "BEFORE" : "AFTER"} log — cannot derive summary`,
      );
    }
    rows.push({ name, before: b, after: a, delta: a - b, kind, featureSet });
  }
  return rows;
}

const prodRows = buildRows(prodBefore, prodAfter, PROD_BENCHES, PROD_FEATURES);
const vzsRows = buildRows(vzsBefore, vzsAfter, VZS_BENCHES, VZS_FEATURES);
const rows = [...prodRows, ...vzsRows];

// ---------------------------------------------------------------------------
// Self-asserting checks (CLAUDE.md's "assert the arithmetic it prints, not
// just print a hand-computed string" rule).
// ---------------------------------------------------------------------------

function byName(rowSet, name) {
  const r = rowSet.find((x) => x.name === name);
  if (!r) throw new Error(`r496_perclass_repr_c_summary: internal error, missing row "${name}"`);
  return r;
}

// 1. Path-activation-oracle-derived isolation: subtract the shared prefill
//    prefix from the hit arm to get the TRUE per-16-hits marginal cost,
//    independent of the process-bootstrap constant that shifted between
//    BEFORE/AFTER (see finding below). This isolated delta must be exactly
//    0 Ir in BOTH feature configurations -- the fix changes WHERE `count`/
//    `slots` live, not the number of instructions the hit-path executes.
const prodPrefill = byName(prodRows, "alloc_magazine_prefill_only_16b");
const prodHit = byName(prodRows, "alloc_magazine_hit_only_16b");
const prodIsolatedBefore = prodHit.before - prodPrefill.before;
const prodIsolatedAfter = prodHit.after - prodPrefill.after;
const prodIsolatedDelta = prodIsolatedAfter - prodIsolatedBefore;
if (prodIsolatedDelta !== 0) {
  throw new Error(
    `r496_perclass_repr_c_summary: plain-alloc isolated magazine-hit delta is ` +
      `${prodIsolatedDelta} Ir (expected exactly 0 -- re-verify before publishing a ` +
      `"no per-op Ir change" claim).`,
  );
}

const vzsPrefill = byName(vzsRows, "alloc_zeroed_magazine_prefill_only_16b");
const vzsHit = byName(vzsRows, "alloc_zeroed_magazine_hit_only_16b");
const vzsIsolatedBefore = vzsHit.before - vzsPrefill.before;
const vzsIsolatedAfter = vzsHit.after - vzsPrefill.after;
const vzsIsolatedDelta = vzsIsolatedAfter - vzsIsolatedBefore;
if (vzsIsolatedDelta !== 0) {
  throw new Error(
    `r496_perclass_repr_c_summary: alloc_zeroed isolated magazine-hit delta is ` +
      `${vzsIsolatedDelta} Ir (expected exactly 0 -- re-verify before publishing a ` +
      `"no per-op Ir change" claim).`,
  );
}

// 2. The "flat-workload" shift: every plain-churn kill-gate bench (whose
//    OWN loop body touches no `PerClass` field the fix's declared-order
//    reorder didn't already touch identically -- these are single-size-class,
//    single-magazine loops) moves by the EXACT SAME constant within a
//    feature set. This is what proves the earlier "why did small_churn_16b
//    move at all" observation is a uniform process/codegen-wide shift
//    rather than a per-op regression concentrated in one bench, without
//    asserting it in prose alone. `large_alloc_free_cycle` (the bootstrap
//    proxy) is reported separately below WITHOUT this exact-match
//    assertion: its own body (one 4 MiB OS-backed alloc+free) is a
//    structurally different workload from the churn benches (it never
//    touches the small-class `Tcache`/`PerClass` array at all beyond the
//    one-time `HeapCore::new()` zero-init every SeferAlloc bench shares),
//    so a small residual (13 Ir, production; see report) between its delta
//    and the churn benches' shared delta is expected, not a contradiction.
const prodChurnDelta = byName(prodRows, "small_churn_16b").delta;
for (const name of ["churn_256b", "aligned_churn_640b_a128"]) {
  const r = byName(prodRows, name);
  if (r.delta !== prodChurnDelta) {
    throw new Error(
      `r496_perclass_repr_c_summary: "${name}" delta (${r.delta}) does not match ` +
        `small_churn_16b's delta (${prodChurnDelta}) under production -- the "uniform ` +
        `process-wide shift, not a per-op regression" claim does not hold; re-investigate ` +
        `before publishing.`,
    );
  }
}
const prodBootstrapDelta = byName(prodRows, "large_alloc_free_cycle").delta;

const vzsChurnDelta = byName(vzsRows, "small_churn_16b").delta;
{
  const r = byName(vzsRows, "churn_256b");
  if (r.delta !== vzsChurnDelta) {
    throw new Error(
      `r496_perclass_repr_c_summary: "churn_256b" delta (${r.delta}) does not match ` +
        `small_churn_16b's delta (${vzsChurnDelta}) under virgin-zero-skip -- the "uniform ` +
        `process-wide shift, not a per-op regression" claim does not hold; re-investigate ` +
        `before publishing.`,
    );
  }
}
const vzsBootstrapDelta = byName(vzsRows, "large_alloc_free_cycle").delta;

const lines = [HEADER];
for (const r of rows) {
  lines.push(
    [
      META.base_commit_sha,
      META.after_patch_sha256,
      `"${r.featureSet}"`,
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

const outPath = new URL("docs/perf/R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv", ROOT);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log("[r496-summary] wrote docs/perf/R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv");
console.log(
  `[r496-summary] plain-alloc isolated magazine-hit (16 hits) delta: ${prodIsolatedDelta} Ir ` +
    `(asserted == 0; before=${prodIsolatedBefore} after=${prodIsolatedAfter})`,
);
console.log(
  `[r496-summary] alloc_zeroed isolated magazine-hit (16 hits) delta: ${vzsIsolatedDelta} Ir ` +
    `(asserted == 0; before=${vzsIsolatedBefore} after=${vzsIsolatedAfter})`,
);
console.log(
  `[r496-summary] production churn-bench shared delta (small_churn_16b/churn_256b/` +
    `aligned_churn_640b_a128): ${prodChurnDelta} Ir (asserted to match exactly across all three); ` +
    `large_alloc_free_cycle (bootstrap proxy, NOT asserted to match — different workload shape) ` +
    `delta: ${prodBootstrapDelta} Ir`,
);
console.log(
  `[r496-summary] virgin-zero-skip churn-bench shared delta (small_churn_16b/churn_256b): ` +
    `${vzsChurnDelta} Ir (asserted to match exactly); large_alloc_free_cycle (bootstrap proxy, ` +
    `NOT asserted to match) delta: ${vzsBootstrapDelta} Ir`,
);
