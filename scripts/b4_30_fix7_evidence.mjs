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
// Identity: the `test_sha` column is the sha256 of the test file after the
// P2-4 NORMALIZATION + masking below, so a constant-only rebase
// (new budgets, same measurement logic) keeps old evidence valid while any
// logic change invalidates it. The producer ASSERTS the live normalized
// masked sha is accepted by MASKED_TEST_SHA_PINS (pin or a logicEquivalents
// successor). The verify gate `scripts/verify-b4-30-pin.mjs` re-checks the
// same pin cargo-free in CI (docs-consistency).
//
// NORMALIZATION (P2-4, fix8) — applied BEFORE masking and hashing:
//   1. CRLF -> LF (the pin must not depend on checkout eol; `.gitattributes`
//      sets no `eol` for this path);
//   2. WHOLE-LINE comments and blank lines are dropped: a line is kept iff
//      its trim() is non-empty and does not start with `//`.
// Why dropping comment lines cannot smuggle a logic change:
//   - Only WHOLE-line comments are dropped. A trailing `// ...` after code
//     stays, and the CODE part of the line is still hashed, so hiding a
//     logic change behind a trailing comment is impossible.
//   - Everything that is executable or declarative in Rust survives: every
//     line of real code, including `#[...]` attributes (`#[inline(never)]`
//     starts with `#`, not `//`), string literals, and multi-line
//     expressions. A logic change MUST occupy at least one non-comment,
//     non-blank line, and every such line is hashed — so any logic change
//     changes the hash.
//   - `///` / `//!` doc comments are real comments (rustdoc prose); this
//     file is an integration-test target, so doc comments never generate
//     doctests or code. Dropping them only allows comment-only edits to
//     pass — exactly the intended property (P3-1 comment fixes need no new
//     pin); they cannot carry logic.
//   - The masking regex still anchors `^const (BUDGET_\w+): usize = \d+;$`
//     on the normalized text, so budget-value rebases stay pin-neutral.
import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

const DIR = "docs/perf";
export const EVIDENCE_PATH = `${DIR}/b4_30_probes/b4_30_fix7_budget_evidence.csv`;
const EVIDENCE = EVIDENCE_PATH;
const MANIFEST = `${DIR}/b4_30_probes/b4_30_fix7_run_manifest.csv`;
export const TEST = "tests/b4_30_first_alloc_min_stack.rs";
export function normalizedMaskedTestSha(source) {
  const norm = source
    .replace(/\r\n/g, "\n")
    .split("\n")
    .filter((l) => {
      const t = l.trim();
      return t && !t.startsWith("//");
    })
    .join("\n");
  return createHash("sha256")
    .update(
      norm.replace(
        /^const (BUDGET_\w+): usize = \d+;$/gm,
        "const $1: usize = <BUDGET>;",
      ),
    )
    .digest("hex");
}
export const MASKED_TEST_SHA_PINS = {
  maskedTestSha: normalizedMaskedTestSha,
  // Normalized+masked sha of the MEASURED revision (git show
  // 5940f78f:tests/b4_30_first_alloc_min_stack.rs) — the exact logic that
  // produced every raw fix7 log.
  pin: "62baadba40db989ccd87200bda6eb34db86df6566ddd991adc78238f9eeeee92",
  // Normalized+masked shas of later revisions that are EQUIVALENT FOR NEED
  // (no change to the boundary bisect, the need formula, the stride
  // calibration, the anchors, or the budget() mapping), each with its
  // human-readable justification. Renamed/extended in fix9 from the former
  // `formatOnly` map: the semantic is "logic-equivalent for need", which is
  // strictly what the evidence depends on. ANY OTHER logic change is still a
  // pin violation (RC=1) — this map is an explicit whitelist, not an
  // escape hatch.
  logicEquivalents: {
    // rustfmt --edition 2021 reordered the `use` items (e.g.
    // `atomic::{..}` before `Mutex, OnceLock`); the SET of normalized
    // non-comment lines is byte-identical, only their order differs.
    "7b76596b0094181357498db5fa8ea8d37d259d76fa87896fdb073f76cb3a4070":
      "rustfmt import order (formatting only)",
    // fix9 (CI flake fix, job `test windows (production)`, commit
    // 153112ae): ONLY the post-assert runtime confirmation probe changed —
    // its target was raised from the measured boundary `fs` to
    // `fs + CONF_SLACK_WINDOWS/LINUX` (1024/256 B, justified from the raw
    // cross-process frame-placement jitter) and its window assert was
    // reformulated for the raised target. The confirmation probe runs AFTER
    // `need` is computed and does NOT participate in it: the boundary
    // bracket+bisect (boundary_search), the stride calibration, the control
    // boundary, and the `need = fs - fc + stride` formula are byte-for-byte
    // untouched, so the fix7 evidence and budgets remain valid without a
    // re-measure (proof: producer + generator re-run is byte-identical in
    // all need/budget columns).
    "eec4ab36653e46c00752603373793347127923fddd2a9b26b533d7360251da43":
      "confirmation probe target raised by platform SLACK constant; boundary bisect and need formula untouched; probe does not participate in need",
  },
  accepts(masked) {
    return masked === this.pin || Object.hasOwn(this.logicEquivalents, masked);
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
  const rustc = text.match(/toolchain=(rustc [^ ]+)/)?.[1];
  return { scenarios: out, rustc };
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
