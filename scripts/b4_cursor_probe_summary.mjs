// Path-activation summary for the B4 §11 per-class discovery cursor
// (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS.md §11.6): per workload and thread
// count, the median and maximum of `b4_find_segment_calls` (discovery
// entries) and `b4_cursor_hits` (cursor probes that served a block) per
// probe pass, for each bundle given.
//
// Usage (from repo root):
//   node scripts/b4_cursor_probe_summary.mjs <label>=<bundle.log.gz> ...
// A bundle is gzip of `=== FILE <name>` sections, each an
// `examples/b4_collapse_probe` log (one RESULT_JSON line per process).
// Median method: lower median (element floor(n/2) of the sorted values).
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";

const args = process.argv.slice(2);
if (!args.length) throw new Error("usage: node scripts/b4_cursor_probe_summary.mjs <label>=<bundle.log.gz> ...");
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
const max = (a) => Math.max(...a);
console.log("| bundle | workload | T | runs | find_segment median | find_segment max | cursor hits median | cursor hits max |");
console.log("|---|---|---:|---:|---:|---:|---:|---:|");
for (const arg of args) {
  const [label, path] = arg.split("=");
  const text = gunzipSync(readFileSync(path)).toString("utf8");
  const cells = new Map();
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith("RESULT_JSON ")) continue;
    const j = JSON.parse(line.slice("RESULT_JSON ".length));
    for (const p of j.passes) {
      const key = `${j.workload}|${p.threads}`;
      const c = cells.get(key) ?? { find: [], hits: [] };
      c.find.push(p.counters_delta.b4_find_segment_calls ?? 0);
      c.hits.push(p.counters_delta.b4_cursor_hits ?? 0);
      cells.set(key, c);
    }
  }
  for (const [key, c] of [...cells].sort()) {
    const [w, t] = key.split("|");
    if (c.find.length !== c.hits.length) throw new Error("ragged cell");
    console.log(`| ${label} | ${w} | ${t} | ${c.find.length} | ${med(c.find)} | ${max(c.find)} | ${med(c.hits)} | ${max(c.hits)} |`);
  }
}
