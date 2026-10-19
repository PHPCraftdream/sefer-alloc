// Generator for docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv plus the
// markdown tables of the report's §1 printed to stdout.
//
// Usage (from repo root):
//   node scripts/b4_collapse_summary.mjs [bundle]
// `bundle` defaults to docs/perf/_raw_b4_collapse_series.log.gz: every series
// log concatenated, each preceded by a `=== FILE <name>` line, gzip-compressed.
//
// Parses the RESULT_JSON line of every listed series log (t=4 pass only;
// ops_per_sec in JSON is ops/s x100) and emits rows:
//   series,binary,run,threads,ops,secs_ms,ops_per_sec_x100
//
// Median method: interpolated median (linear interpolation between the two
// middle order statistics; for even n it is the mean of the two middle
// values). Collapse = ops/s < 10M; degradation = 10M <= ops/s < 20M.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";
import { REPO_ROOT } from "./lib.mjs";

const BUNDLE = process.argv[2] ?? "docs/perf/_raw_b4_collapse_series.log.gz";
const logs = new Map();
{
  const text = gunzipSync(readFileSync(join(REPO_ROOT, BUNDLE))).toString("utf8");
  const parts = text.split(/^=== FILE (\S+)\r?\n/m);
  for (let i = 1; i < parts.length; i += 2) logs.set(parts[i], parts[i + 1]);
}
const readLog = (name) => {
  const t = logs.get(name);
  if (t === undefined) throw new Error(`missing ${name} in ${BUNDLE}`);
  return t;
};

const plans = [
  { glob: /^s2_(warm|cold)_\d+\.log$/, series: "s2-prefix", binary: "pre-fix" },
  { glob: /^s3_(warm|cold)_\d+\.log$/, series: "s3-prefix", binary: "pre-fix counters" },
  { glob: /^s4_warm_\d+\.log$/, series: "s4-fix", binary: "pre-fix counters" },
  { glob: /^s5_(warm|cold)_\d+\.log$/, series: "s5-fix1", binary: "fix1" },
  { glob: /^s6_warm_\d+\.log$/, series: "s6-fix1", binary: "fix1 counters" },
  { glob: /^s7_(warm|cold)_\d+\.log$/, series: "s7-fix1+2", binary: "fix1+2" },
  { glob: /^s7_cnt_\d+\.log$/, series: "s7-cnt-fix1+2", binary: "fix1+2 counters-build" },
];

// series key -> { label, build, rows: [{run, ops_per_sec}] }
const seriesMap = new Map();
function reg(series, binary, run, pass) {
  if (!seriesMap.has(series)) seriesMap.set(series, { binary, rows: [] });
  const temp = /cold/.test(run) ? "cold" : "warm";
  seriesMap.get(series).rows.push({ run, temp, mops: pass.ops_per_sec / 100 / 1e6 });
}

for (const f of [...logs.keys()].sort()) {
  const plan = plans.find((p) => p.glob.test(f));
  if (!plan) continue;
  const text = readLog(f);
  const m = text.match(/RESULT_JSON (.*)/);
  if (!m) {
    console.error(`no RESULT_JSON in ${f}`);
    continue;
  }
  const json = JSON.parse(m[1]);
  const pass = json.passes.find((p) => p.threads === 4);
  if (!pass) {
    console.error(`no t=4 pass in ${f}`);
    continue;
  }
  reg(plan.series, plan.binary, f, pass);
}

const header = "series,binary,run,threads,ops,secs_ms,ops_per_sec_x100";
const rows = [];
for (const [series, s] of seriesMap) {
  for (const r of [...s.rows].sort((a, b) => a.run.localeCompare(b.run))) {
    const text = readLog(r.run);
    const j = JSON.parse(text.match(/RESULT_JSON (.*)/)[1]);
    const p = j.passes.find((p) => p.threads === 4);
    rows.push(`${series},${s.binary},${r.run},4,${p.ops},${p.secs},${p.ops_per_sec}`);
  }
}
writeFileSync(
  join(REPO_ROOT, "docs/perf/B4_MT_COLLAPSE_DIAGNOSIS_summary.csv"),
  header + "\n" + rows.join("\n") + "\n",
);
console.log(`rows=${rows.length}`);

function median(sorted) {
  const n = sorted.length;
  if (n % 2 === 1) return sorted[(n - 1) / 2];
  return (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
}
const fmt = (x) => x.toFixed(2);

// group sX_(cold|warm) series into families for the §1 table
const families = new Map();
for (const [series, s] of seriesMap) {
  const m = series.match(/^(s\d+)-/);
  if (!m) continue;
  const key = series.includes("cnt") ? `${m[1]}_cnt` : m[1];
  if (!families.has(key)) families.set(key, { binary: s.binary, byTemp: {} });
  for (const temp of ["cold", "warm"]) {
    const list = s.rows.filter((r) => r.temp === temp);
    if (list.length) fam_add(families.get(key), temp, list);
  }
}
function fam_add(fam, temp, list) {
  fam.byTemp[temp] = list;
}

console.log("| серия | сборка | прогонов | min | median | max (M ops/s) | коллапсов (<10M) | деградированных (10–20M) |");
console.log("|---|---|---|---|---|---|---|---|");
for (const key of [...families.keys()].sort()) {
  const fam = families.get(key);
  for (const temp of ["cold", "warm"]) {
    const list = fam.byTemp[temp];
    if (!list) continue;
    const vals = list.map((r) => r.mops).sort((a, b) => a - b);
    const med = median(vals);
    const collapsed = list.filter((r) => r.mops < 10);
    const degraded = list.filter((r) => r.mops >= 10 && r.mops < 20);
    const cd = collapsed.length
      ? `${collapsed.length} (${collapsed.map((r) => `${r.run.replace(/\.log$/, "")}:${fmt(r.mops)}`).join(", ")})`
      : "0";
    const dd = degraded.length
      ? `${degraded.length} (${degraded.map((r) => fmt(r.mops)).join("/")})`
      : "0";
    console.log(
      `| ${key}_${temp} | ${fam.binary} | ${list.length} | ${fmt(vals[0])} | ${fmt(med)} | ${fmt(vals[vals.length - 1])} | ${cd} | ${dd} |`,
    );
  }
}

// counters maxima for counters-series
const COUNTER_SERIES = [
  ["s4-fix", "s4_warm"],
  ["s6-fix1", "s6_warm"],
  ["s7-cnt-fix1+2", "s7_cnt"],
];
console.log("\ncounters maxima (counters_delta t=4):");
console.log("| серия | max b4_find_segment_calls (прогон) | max b4_scan_calls (прогон) | max b4_scan_words (прогон) |");
console.log("|---|---|---|---|");
for (const [series, label] of COUNTER_SERIES) {
  const s = seriesMap.get(series);
  if (!s) continue;
  let max = { find: 0, scan: 0, words: 0, findSrc: "", scanSrc: "", wordsSrc: "" };
  for (const r of s.rows) {
    const j = JSON.parse(readLog(r.run).match(/RESULT_JSON (.*)/)[1]);
    const p = j.passes.find((p) => p.threads === 4);
    const cd = p.counters_delta ?? {};
    if ((cd.b4_find_segment_calls ?? 0) > max.find) max = { ...max, find: cd.b4_find_segment_calls, findSrc: r.run };
    if ((cd.b4_scan_calls ?? 0) > max.scan) max = { ...max, scan: cd.b4_scan_calls, scanSrc: r.run };
    if ((cd.b4_scan_words ?? 0) > max.words) max = { ...max, words: cd.b4_scan_words, wordsSrc: r.run };
  }
  console.log(
    `| ${label} | ${max.find.toLocaleString("en-US")} (${max.findSrc.replace(/\.log$/, "")}) | ${max.scan.toLocaleString("en-US")} (${max.scanSrc.replace(/\.log$/, "")}) | ${max.words.toLocaleString("en-US")} (${max.wordsSrc.replace(/\.log$/, "")}) |`,
  );
}

// §2: per-counter t=4 deltas for the named normal and collapsed runs. The
// probe prints only non-zero deltas, so a counter absent from a run is 0.
const GROUPS = {
  normal: ["s4_warm_11.log", "s4_warm_14.log", "s4_warm_09.log"],
  collapsed: ["s4_warm_06.log", "s4_warm_02.log", "s4_warm_15.log"],
};
const deltas = (run) => {
  const j = JSON.parse(readLog(run).match(/RESULT_JSON (.*)/)[1]);
  return j.passes.find((p) => p.threads === 4).counters_delta ?? {};
};
const keys = new Set();
const byGroup = {};
for (const [g, runs] of Object.entries(GROUPS)) {
  byGroup[g] = runs.map(deltas);
  for (const d of byGroup[g]) for (const k of Object.keys(d)) keys.add(k);
}
const range = (list, k) => {
  const v = list.map((d) => d[k] ?? 0);
  const lo = Math.min(...v), hi = Math.max(...v);
  const f = (x) => x.toLocaleString("en-US");
  return lo === hi ? f(lo) : `${f(lo)}–${f(hi)}`;
};
console.log(`\ncounters t=4 delta: normal = ${GROUPS.normal.join(", ")}; collapsed = ${GROUPS.collapsed.join(", ")}`);
console.log("| счётчик (delta/проход t=4) | норма | коллапс |");
console.log("|---|---|---|");
for (const k of [...keys].sort()) {
  console.log(`| ${k} | ${range(byGroup.normal, k)} | ${range(byGroup.collapsed, k)} |`);
}

// End-of-run totals that must stay zero across every s4 run.
const s4runs = [...logs.keys()].filter((f) => /^s4_warm_\d+\.log$/.test(f));
for (const k of ["fallback_lock_acquisitions", "foreign_or_unroutable_frees", "segments_released_total"]) {
  const v = s4runs.map((f) => JSON.parse(readLog(f).match(/RESULT_JSON (.*)/)[1]).counters_end?.[k]);
  if (v.some((x) => x === undefined)) throw new Error(`${k} missing in counters_end`);
  console.log(`| ${k} (counters_end, max over all ${s4runs.length} s4 runs) | ${Math.max(...v)} | ${Math.max(...v)} |`);
}
