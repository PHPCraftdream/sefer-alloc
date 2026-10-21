#!/usr/bin/env node
// B4-30 fix7 evidence producer: parses the committed RAW run logs
// (docs/perf/_raw_b4_30_fix7_<cell>.run<k>.log, one full
// `cargo test --test b4_30_first_alloc_min_stack -- --nocapture` invocation
// per log) plus the run manifest (machine identity per log) and derives
// docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv — the WORST (max
// need) run per (os, profile, features, scenario), requiring >= MIN_RUNS
// runs per cell. The budget generator (scripts/b4_30_stack_table.mjs)
// consumes that CSV; this script is its only producer.
//
// Identity: the `test_sha` column is the sha256 of the test file with the
// `const BUDGET_*: usize = <N>;` VALUES masked, so a constant-only rebase
// (new budgets, same measurement logic) keeps old evidence valid while any
// logic change invalidates it. The producer ASSERTS the live masked sha
// equals the pinned MASKED_TEST_SHA below.
import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

const DIR = "docs/perf";
const EVIDENCE = `${DIR}/b4_30_probes/b4_30_fix7_budget_evidence.csv`;
const MANIFEST = `${DIR}/b4_30_probes/b4_30_fix7_run_manifest.csv`;
const TEST = "tests/b4_30_first_alloc_min_stack.rs";
// sha256 of tests/b4_30_first_alloc_min_stack.rs with every
// `const BUDGET_*: usize = <number>;` line replaced by
// `const <NAME>: usize = <BUDGET>;` — the revision of the MEASUREMENT
// LOGIC that produced every raw fix7 log (constants masked so rebasing
// budgets alone does not require a re-measure).
export const MASKED_TEST_SHA_PINS = {
  maskedTestSha: (source) =>
    createHash("sha256")
      .update(
        source.replace(
          /^const (BUDGET_\w+): usize = \d+;$/gm,
          "const $1: usize = <BUDGET>;",
        ),
      )
      .digest("hex"),
  pin: "9f3fd8848335e176aaa17962645c5d8304b143b9ea7983386f147efc33bbdf33",
  // Masked shas of later revisions whose diff against the pinned one is
  // formatting only (no measurement-logic change), each with its diff.
  formatOnly: {
    // rustfmt --edition 2021 import order: `atomic::{..}` before `Mutex, OnceLock`.
    "704c9a44377acffa2a5bec95a2b3ba3d956e046c9b3f74da3e611d70a31cc400": "rustfmt import order",
  },
  accepts(masked) {
    return masked === this.pin || Object.hasOwn(this.formatOnly, masked);
  },
};
const MIN_RUNS = 2;
const SCENARIOS = ["small", "large", "foreign-free"];

const maskedSha = MASKED_TEST_SHA_PINS.maskedTestSha(readFileSync(TEST, "utf8"));
if (MASKED_TEST_SHA_PINS.pin !== "PENDING") {
  if (!MASKED_TEST_SHA_PINS.accepts(maskedSha))
    throw Error(
      `masked test sha ${maskedSha} != pinned measurement-logic revision ${MASKED_TEST_SHA_PINS.pin} — the test's measurement logic changed; re-measure all 8 cells and refresh the raw logs + pin`,
    );
}

// ---- manifest -------------------------------------------------------------
const manText = readFileSync(MANIFEST, "utf8").trim();
const manLines = manText.split(/\r?\n/).filter(l => l.trim() && !l.startsWith("#"));
const manHeader = manLines[0].split(",");
const needCols = ["log", "os", "profile", "features", "cpu", "os_build"];
for (const c of needCols)
  if (!manHeader.includes(c)) throw Error(`manifest: missing column ${c}`);
const runs = [];
for (const line of manLines.slice(1)) {
  if (!line.trim() || line.startsWith("#")) continue;
  const f = line.split(",");
  const r = {};
  for (const h of manHeader) r[h] = f[manHeader.indexOf(h)];
  runs.push(r);
}
if (runs.length < 8 * MIN_RUNS)
  throw Error(`manifest: expected >= ${8 * MIN_RUNS} runs, got ${runs.length}`);

// ---- parse each raw log ---------------------------------------------------
const parseDetails = (text) => {
  const out = {};
  for (const m of text.matchAll(
    /\[b4-30\] scenario=(\S+) details need=(\d+) budget=(\d+) margin=(-?\d+) fs=(\d+) fc=(\d+) stride=(\d+) boundary=(\S+) control=(\S+) enforce=(\d+)/g,
  )) {
    if (out[m[1]]) throw Error(`duplicate details marker for ${m[1]} in one log`);
    out[m[1]] = {
      need: Number(m[2]),
      fs: Number(m[5]),
      fc: Number(m[6]),
      stride: Number(m[7]),
      boundary: m[8],
      control: m[9],
      enforce: Number(m[10]),
    };
  }
  const tool = text.match(/\[b4-30\] toolchain=(\S+ \S+)?/);
  return { scenarios: out, rustc: text.match(/toolchain=(rustc [^ ]+)/)?.[1] };
};

const byCell = {}; // key os/profile/features -> {scenario: [{need,...,cpu,os_build}]}
for (const run of runs) {
  const text = readFileSync(`${DIR}/${run.log}`, "utf8");
  const { scenarios, rustc } = parseDetails(text);
  for (const s of SCENARIOS) {
    const d = scenarios[s];
    if (!d) throw Error(`${run.log}: no details marker for scenario ${s}`);
    if (d.enforce !== 0)
      throw Error(`${run.log}: evidence runs must be measure-only (enforce=0)`);
    const key = `${run.os}/${run.profile}/${run.features}`;
    (byCell[key] ??= {})[s] ??= [];
    byCell[key][s].push({ ...d, rustc, cpu: run.cpu, os_build: run.os_build, log: run.log });
  }
}

// ---- worst per cell, >= MIN_RUNS ------------------------------------------
const lines = [
  "# derived by scripts/b4_30_fix7_evidence.mjs from docs/perf/_raw_b4_30_fix7_*.log (worst = max need run per cell/scenario; >= 2 independent full runs per cell)",
  `# masked test logic sha256=${MASKED_TEST_SHA_PINS.pin} (test file with BUDGET_* values masked)`,
  "os,profile,features,scenario,need,fs,fc,stride,boundary,control,rustc,test_sha,runs,cpu,os_build",
];
const cells = Object.keys(byCell);
if (cells.length !== 8) throw Error(`expected 8 cells, got ${cells.length}: ${cells}`);
for (const key of cells.sort()) {
  const [os, profile, features] = key.split("/");
  for (const s of SCENARIOS) {
    const list = byCell[key][s];
    if (list.length < MIN_RUNS)
      throw Error(`${key}/${s}: only ${list.length} runs (< ${MIN_RUNS})`);
    const worst = list.reduce((a, b) => (b.need > a.need ? b : a));
    lines.push(
      [os, profile, features, s, worst.need, worst.fs, worst.fc, worst.stride,
        worst.boundary, worst.control, worst.rustc, MASKED_TEST_SHA_PINS.pin, list.length,
        worst.cpu, worst.os_build].join(","),
    );
  }
}
writeFileSync(EVIDENCE, lines.join("\n") + "\n");
console.log(`wrote ${EVIDENCE}: ${lines.length - 3} rows; masked test sha ${maskedSha}`);
