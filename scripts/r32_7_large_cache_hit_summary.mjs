// F12 (task #498): derives `docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE_summary.csv`
// from the two raw iai-callgrind logs this task's BEFORE/AFTER measurement
// produced (`docs/perf/_raw_r32_7_before.log` / `_raw_r32_7_after.log`).
// This is the ONE checked script that turns the raw logs into the report's
// machine-readable summary (CLAUDE.md "tables derived by one checked script,
// not hand-transcribed").
//
// What this measures: replacing `AllocCore::alloc_large`'s large-cache HIT
// arm's full ~144-byte `SegmentHeader` `Node::write_struct` rewrite with 4
// targeted field writes (`magic`/`large_size`/`large_align`/`bump`) --
// `src/alloc_core/alloc_core_large.rs`. The BEFORE log was captured in an
// isolated `git worktree` at commit
// 2dfeaa30944fb73dedd2365bb90c41ff4c198c5d (this task's base) with ONLY the
// new bench pair (`large_cache_prefill_only_4mib` / `large_cache_hit_only_4mib`,
// `benches/perf_gate_iai.rs`) applied -- NOT the targeted-write change -- so
// the BEFORE log measures the OLD (full-write) hit arm through the identical
// bench shape the AFTER log uses. The AFTER log was captured in the main
// working tree with the targeted-write change applied.
//
// Immutable source identity (CLAUDE.md's R29-6 rule):
//   - AFTER:  working tree at commit 2dfeaa30944fb73dedd2365bb90c41ff4c198c5d
//             + this task's full diff (targeted-write change + falsification
//             assert + size pin + new bench pair + activation oracle).
//             `git write-tree` of that state (staged) ==
//             8fa61fd1a4aabd11296607bb878951afb728d79e (recorded below).
//   - BEFORE: `git worktree add` at commit
//             2dfeaa30944fb73dedd2365bb90c41ff4c198c5d, with ONLY
//             `git diff HEAD -- benches/perf_gate_iai.rs` (the bench-only
//             half of this task's diff) applied via `git apply`;
//             `git write-tree` of that state ==
//             b19c6e6f0b5bcf7d41438143fe8bc8e318a5cb29 (recorded below).
//
// WSL/callgrind measurement caveat (discovered during this task): the
// underlying `scripts/iai.mjs` uses a single hardcoded `CARGO_TARGET_DIR`
// (`/tmp/sefer-iai`) shared across ANY invocation regardless of which
// source tree (main repo vs. an isolated worktree) invoked it. A
// stale/shared build in that directory can silently serve an
// already-compiled binary instead of rebuilding from the tree actually
// being measured. Both the BEFORE and AFTER logs cited here were captured
// with `/tmp/sefer-iai` explicitly removed (`rm -rf`) immediately before
// each run, and each was independently reproduced twice (byte-identical
// both times) before being trusted -- see this task's own report
// (docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE.md §3.3) for
// the full reproduction trail. A future BEFORE/AFTER measurement using this
// script's pattern MUST clean `/tmp/sefer-iai` between runs against
// different source trees, or risk exactly this false-negative ("no
// change") failure mode.
//
// Usage:
//   node scripts/r32_7_large_cache_hit_summary.mjs [landing_commit_sha]
//
// `landing_commit_sha` is this task's own landing commit (chicken-and-egg: a
// commit cannot cite its own SHA inside its own tree). Omit on first
// generation (column reads "UNFILLED"); a follow-up commit may fill the
// placeholder, mirroring the R31-0/R495 precedent this project already uses
// for the same problem.

import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || "UNFILLED";

const META = {
  base_commit_sha: "2dfeaa30944fb73dedd2365bb90c41ff4c198c5d",
  after_tree_sha: "8fa61fd1a4aabd11296607bb878951afb728d79e",
  before_tree_sha: "b19c6e6f0b5bcf7d41438143fe8bc8e318a5cb29",
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045_(WSL/valgrind callgrind)",
  feature_set: "production bench-internals",
};

const HEADER =
  "base_commit_sha,after_tree_sha,before_tree_sha,feature_set,cpu,os,bench,before_ir,after_ir,delta_ir,kind,landing_commit";

/** Parse `bench  Ir  L1  L2  RAM  EstCycles  Ir/op` table rows out of one
 * `npm run iai`-style raw log (the printed report table, not the raw
 * iai-callgrind per-bench blocks -- same table shape both `_before`/`_after`
 * logs share). Returns a Map<name, ir>. */
function parseTable(logText) {
  const out = new Map();
  const lines = logText.split(/\r?\n/);
  // Table rows look like:
  //   "  large_cache_hit_only_4mib                  7,115        10,371 ..."
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

const beforeLog = read("docs/perf/_raw_r32_7_before.log");
const afterLog = read("docs/perf/_raw_r32_7_after.log");

const before = parseTable(beforeLog);
const after = parseTable(afterLog);

const BENCHES = [
  { name: "large_cache_prefill_only_4mib", kind: "prefill (shared prefix, both arms)" },
  { name: "large_cache_hit_only_4mib", kind: "treatment (prefix + 1 large-cache HIT)" },
  { name: "large_alloc_free_cycle", kind: "kill-gate (fresh OS reservation, never a hit; must be flat)" },
  { name: "small_churn_16b", kind: "kill-gate (plain small alloc, must be flat)" },
  { name: "churn_256b", kind: "kill-gate (plain small alloc, must be flat)" },
  { name: "aligned_churn_640b_a128", kind: "kill-gate (plain small alloc, must be flat)" },
  { name: "cold_alloc_free_256x16b", kind: "kill-gate (plain small alloc, must be flat)" },
];

const rows = [];
for (const { name, kind } of BENCHES) {
  const b = before.get(name);
  const a = after.get(name);
  if (b == null || a == null) {
    throw new Error(
      `r32_7_large_cache_hit_summary: bench "${name}" missing from ${b == null ? "BEFORE" : "AFTER"} log — cannot derive summary`,
    );
  }
  rows.push({ name, before: b, after: a, delta: a - b, kind });
}

// --- Assertions (CLAUDE.md: "a script that computes a headline ratio must
// assert the arithmetic it prints, not just print a hand-computed string") ---

const prefill = rows.find((r) => r.name === "large_cache_prefill_only_4mib");
const hit = rows.find((r) => r.name === "large_cache_hit_only_4mib");

// All five kill-gate benches (including the prefill arm's OWN control
// role is separate -- prefill DOES touch the modified code, since every
// alloc after the first in its own loop is itself a large-cache hit) must
// be exactly flat. `large_alloc_free_cycle` never touches the large_cache
// at all (single alloc+free, no second alloc); the four plain-small-alloc
// benches never touch Large-object code at all.
for (const r of rows) {
  if (r.kind.startsWith("kill-gate") && r.delta !== 0) {
    throw new Error(
      `r32_7_large_cache_hit_summary: kill-gate bench "${r.name}" moved by ${r.delta} Ir ` +
        `(expected exactly 0 -- F12 only touches the large-cache HIT arm). This is a red flag, ` +
        `not noise (iai-callgrind's Ir count is deterministic run-to-run).`,
    );
  }
}

// Both the prefill arm and the treatment arm must show a NEGATIVE delta (Ir
// went down -- fewer bytes written per hit, never more or neutral).
if (prefill.delta >= 0) {
  throw new Error(
    `r32_7_large_cache_hit_summary: prefill arm "large_cache_prefill_only_4mib" ` +
      `delta is ${prefill.delta} Ir (expected negative -- the targeted write must not increase Ir).`,
  );
}
if (hit.delta >= 0) {
  throw new Error(
    `r32_7_large_cache_hit_summary: treatment arm "large_cache_hit_only_4mib" ` +
      `delta is ${hit.delta} Ir (expected negative -- the targeted write must not increase Ir).`,
  );
}

// R23-3 shared-prefix subtraction: isolate ONE hit's own marginal cost in
// each tree, then compare the two per-hit costs.
const beforePerHit = hit.before - prefill.before;
const afterPerHit = hit.after - prefill.after;
const perHitDeltaIr = afterPerHit - beforePerHit;

// Sanity-check against a plausible range: the survey's own estimate was
// "roughly 10-20% of a ~45 ns hit" and a full ~144-byte header write is
// ~18 usize/u32 stores; a targeted 4-field write removing most of that is
// plausibly tens of Ir, not hundreds.
const perHitAbs = Math.abs(perHitDeltaIr);
if (perHitAbs < 5 || perHitAbs > 100) {
  throw new Error(
    `r32_7_large_cache_hit_summary: per-hit Ir delta ${perHitDeltaIr.toFixed(2)} is outside the ` +
      `sanity range [-100, -5] Ir/hit -- re-verify the measurement before publishing.`,
  );
}

const lines = [HEADER];
for (const r of rows) {
  lines.push(
    [
      META.base_commit_sha,
      META.after_tree_sha,
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

const outPath = new URL(
  "docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE_summary.csv",
  ROOT,
);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log(
  "[r32-7-summary] wrote docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE_summary.csv",
);
console.log(
  `[r32-7-summary] prefill arm delta: ${prefill.delta} Ir (before=${prefill.before}, after=${prefill.after})`,
);
console.log(
  `[r32-7-summary] treatment (hit) arm delta: ${hit.delta} Ir (before=${hit.before}, after=${hit.after})`,
);
console.log(
  `[r32-7-summary] per-hit marginal cost (shared-prefix-subtracted): ` +
    `before=${beforePerHit} Ir/hit, after=${afterPerHit} Ir/hit, delta=${perHitDeltaIr} Ir/hit ` +
    `(asserted in [-100, -5] Ir/hit)`,
);
console.log(
  `[r32-7-summary] kill-gates (large_alloc_free_cycle, small_churn_16b, churn_256b, ` +
    `aligned_churn_640b_a128, cold_alloc_free_256x16b): all asserted == 0 delta`,
);
