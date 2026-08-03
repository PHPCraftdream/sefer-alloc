// R33-12 / R32-3 backfill (task #517): derives
// `docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv` from the
// two raw iai-callgrind logs this task's BEFORE/AFTER re-measurement produced
// (`docs/perf/_raw_r32_3_realloc_before.log` /
// `_raw_r32_3_realloc_after.log`). This is the ONE checked script that turns
// the raw logs into the report's machine-readable summary (CLAUDE.md "tables
// derived by one checked script, not hand-transcribed").
//
// WHY THIS EXISTS. R32-3 (commit 5d72bc6, task #494) is the round's only
// `perf(runtime)` SHIPPING change whose verdict rested on measured numbers
// that existed ONLY in its own commit message — no `docs/perf/R32_3_*.md`,
// no `_summary.csv`, no `_raw_*.log`, no derive script (finding F10 [P3] in
// docs/reviews/2026-08-03-round32-readonly-review.md §7). CLAUDE.md's R22-14
// boundary rule's stated TEST is "does the verdict rest on a number obtained
// by RUNNING SOMETHING" — and here it plainly does. This task (R33-12) does
// NOT re-decide R32-3's already-shipped change; it makes the already-published
// numbers reproducible from committed artifacts, exactly the precedent the
// R22-14 rule set for R21-2's throwaway probe (promoting a reproducible
// measurement to a small permanent committed example).
//
// What this measures: `HeapCore::realloc`'s own-segment move leg and
// `try_promote_to_large` both already hold `base` (proven ours & live via an
// earlier `contains_base(base)` in the same call), but their closing dealloc
// went through `self.dealloc(ptr, old_layout)` -> dealloc_routing, which
// RECOMPUTES `os::segment_base_of_ptr(ptr)` (~9.03 Ir) and RE-RUNS
// `contains_base` (~8.2-12.0 Ir) from scratch. The fix routes both call sites
// through `dealloc_own_thread_with_base(ptr, old_layout, base)` directly,
// skipping the redundant recompute. See commit 5d72bc6's full message for the
// correctness argument (the OLD block at `ptr` is still LIVE until that
// closing dealloc, so its segment's live_count stays > 0 throughout, and
// contains_base(base) is still true exactly as when first checked).
//
// The BEFORE log was captured in an isolated `git worktree` at commit
// f3020fdb5e0f0dcd41f3bc46a3d2ab44e5fce0df (R32-3's parent, = main's HEAD
// immediately before 5d72bc6 landed); the AFTER log in an isolated worktree
// at commit 5d72bc633193938181e2d06f8c584617ebaecf42 (R32-3 itself). No
// bench-file changes were needed between the two — the pre-existing
// `realloc_grow` + four churn benches in `benches/perf_gate_iai.rs` already
// drive the changed code path, so BEFORE and AFTER are measured through
// byte-identical bench source.
//
// REPRODUCTION TRAP (encountered and fixed during this task): `scripts/iai.mjs`
// uses a fixed `/tmp/sefer-iai` target dir. Measuring two different commits via
// two worktrees back-to-back WITHOUT wiping that target makes cargo reuse the
// FIRST run's compiled sefer-alloc artifact for the SECOND run (the two
// worktrees are different paths but share the target; cargo's fingerprint can
// conclude the benchmark is up-to-date and skip recompiling sefer-alloc). The
// symptom is the AFTER run finishing in ~2s ("Finished", no "Compiling
// sefer-alloc") and reporting `(No change)` against the BEFORE baseline — a
// FALSE non-reproduction, because the AFTER binary was actually the BEFORE
// binary. FIX: `rm -rf /tmp/sefer-iai` between the two worktree runs so each
// forces a full recompile from its own source. Both runs here were captured
// with a wiped target and show `Instructions: N|N/A` (fresh, no baseline
// contamination). See the report's §3.3 for the exact commands.
//
// Immutable source identity (CLAUDE.md's R29-6 rule):
//   - BEFORE: `git worktree add ../sefer-alloc-r517-before
//             f3020fdb5e0f0dcd41f3bc46a3d2ab44e5fce0df`, no changes applied.
//   - AFTER:  `git worktree add ../sefer-alloc-r517-after
//             5d72bc633193938181e2d06f8c584617ebaecf42`, no changes applied.
//   Both are clean landed commits (no working-tree diff), so no patch hash or
//   tree SHA is needed beyond the commit SHAs themselves.
//
// Usage:
//   node scripts/r32_3_realloc_redundant_contains_base_summary.mjs [doc_commit_sha]
//
// `doc_commit_sha` is THIS documentation task's own landing commit
// (chicken-and-egg: a commit cannot cite its own SHA inside its own tree). If
// omitted, the SHA is derived from `git rev-parse HEAD` at run time (R33-8; see
// scripts/r495_stamp_removal_summary.mjs / r496_perclass_repr_c_summary.mjs),
// so re-running the script reproduces the column without a hand-edited
// follow-up commit.

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const docCommit = process.argv[2] || execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

const META = {
  before_commit_sha: "f3020fdb5e0f0dcd41f3bc46a3d2ab44e5fce0df",
  after_commit_sha: "5d72bc633193938181e2d06f8c584617ebaecf42",
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045_(WSL/valgrind callgrind)",
  feature_set: "production bench-internals",
};

const HEADER =
  "before_commit_sha,after_commit_sha,feature_set,cpu,os,bench,before_ir,after_ir,delta_ir,kind,doc_commit";

/** Parse iai-callgrind's per-bench "perf_gate_iai::perf_gate::<name>" header +
 * "Instructions: <n>|..." block pairs out of one raw log. Returns a
 * Map<name, ir>. This is the same shape r496_perclass_repr_c_summary.mjs's
 * parsePerBenchBlocks uses; preferred over the printed summary table because
 * every bench iai-callgrind actually ran gets its own raw block regardless of
 * the `--filter` the runner passed. */
function parseLog(logText) {
  const out = new Map();
  const re = /perf_gate_iai::perf_gate::([A-Za-z_]\w*)\r?\n\s+Instructions:\s+(\d+)\|/g;
  let m;
  while ((m = re.exec(logText)) !== null) {
    if (!out.has(m[1])) out.set(m[1], Number(m[2]));
  }
  return out;
}

const before = parseLog(read("docs/perf/_raw_r32_3_realloc_before.log"));
const after = parseLog(read("docs/perf/_raw_r32_3_realloc_after.log"));

const BENCHES = [
  { name: "realloc_grow", kind: "treatment (16 geometric realloc-grow steps via HeapCore::realloc move leg)" },
  { name: "small_churn_16b", kind: "kill-gate (plain alloc, never calls realloc, must be flat)" },
  { name: "medium_class_dealloc_churn_16b", kind: "kill-gate (plain alloc dealloc churn, must be flat)" },
  { name: "churn_256b", kind: "kill-gate (plain alloc, never calls realloc, must be flat)" },
  { name: "churn_write_256b", kind: "kill-gate (plain alloc write churn, never calls realloc, must be flat)" },
];

const rows = [];
for (const { name, kind } of BENCHES) {
  const b = before.get(name);
  const a = after.get(name);
  if (b == null || a == null) {
    throw new Error(
      `r32_3_summary: bench "${name}" missing from ${b == null ? "BEFORE" : "AFTER"} log — cannot derive summary`,
    );
  }
  rows.push({ name, before: b, after: a, delta: a - b, kind });
}

// --- Assertions (CLAUDE.md: "a script that computes a headline ratio must
// assert the arithmetic it prints, not just print a hand-computed string") ---

const grow = rows.find((r) => r.name === "realloc_grow");

// The four churn kill-gate benches (which never call realloc) must be EXACTLY
// flat -- R32-3 touches only the realloc / try_promote_to_large closing
// dealloc, never the plain-alloc churn path. iai-callgrind's Ir count is
// deterministic run-to-run, so any nonzero delta here is a red flag, not noise.
for (const r of rows) {
  if (r.kind.startsWith("kill-gate") && r.delta !== 0) {
    throw new Error(
      `r32_3_summary: kill-gate bench "${r.name}" moved by ${r.delta} Ir ` +
        `(expected exactly 0 -- R32-3 does not touch the plain-alloc churn path). ` +
        `This is a red flag: iai-callgrind's Ir count is deterministic run-to-run, ` +
        `so a nonzero delta means either the logs were swapped, the wrong commit was ` +
        `measured, or the shared-target-dir trap (see header) contaminated one run.`,
    );
  }
}

// Treatment arm: must show a NEGATIVE delta (Ir went down -- the redundant
// segment_base_of_ptr + contains_base recompute was removed, not added).
if (grow.delta >= 0) {
  throw new Error(
    `r32_3_summary: treatment arm "realloc_grow" delta is ${grow.delta} Ir ` +
      `(expected negative -- removing an instruction sequence must not increase Ir).`,
  );
}

// Assert the headline arithmetic the report publishes: the commit message
// cites 492,694 -> 492,574 (-120 Ir), and this re-measurement reproduces that
// EXACTLY. A future re-run on a different toolchain that does NOT reproduce
// -120 SHOULD fail here loudly (prompting investigation), not silently publish
// a different number under the same "-120" claim.
const STATED_DELTA = -120;
if (grow.delta !== STATED_DELTA) {
  throw new Error(
    `r32_3_summary: realloc_grow delta is ${grow.delta} Ir, but the report/commit ` +
      `cite ${STATED_DELTA} Ir (492,694 -> 492,574). The committed raw logs no longer ` +
      `reproduce the published number -- investigate (toolchain drift? wrong logs ` +
      `committed?) before trusting either figure.`,
  );
}

// Per-step sanity: -120 Ir / 16 realloc-grow steps = -7.5 Ir/step, matching the
// commit message's own "~7.5 Ir/step x 16 steps" decomposition.
const GROW_STEPS = 16; // benches/perf_gate_iai.rs realloc_grow: 16 geometric doublings
const perStep = grow.delta / GROW_STEPS;
if (perStep !== -7.5) {
  throw new Error(
    `r32_3_summary: per-step Ir is ${perStep} (expected exactly -7.5 = ${STATED_DELTA}/${GROW_STEPS}).`,
  );
}

const lines = [HEADER];
for (const r of rows) {
  lines.push(
    [
      META.before_commit_sha,
      META.after_commit_sha,
      META.feature_set,
      META.cpu,
      META.os,
      r.name,
      r.before,
      r.after,
      r.delta,
      `"${r.kind}"`,
      docCommit,
    ].join(","),
  );
}

const outPath = new URL("docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv", ROOT);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log("[r32_3-summary] wrote docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv");
console.log(
  `[r32_3-summary] treatment (realloc_grow): ${grow.before} -> ${grow.after} Ir = ${grow.delta} Ir ` +
    `(asserted == ${STATED_DELTA}; ${perStep} Ir/step over ${GROW_STEPS} steps, asserted == -7.5)`,
);
console.log(
  `[r32_3-summary] kill-gates (small_churn_16b, medium_class_dealloc_churn_16b, churn_256b, ` +
    `churn_write_256b): all asserted == 0 delta (byte-exact, no codegen perturbation outside realloc)`,
);
