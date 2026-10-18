// Summarize a local wall-clock A/B bundle: per cell, median SeferAlloc on side A
// and side B, B vs A, mimalloc on side B, and the min..max spread of each side.
//
// Usage (from repo root):
//   node scripts/ab_local_parse.mjs docs/perf/<bundle>.log.gz
// The bundle is gzip of `=== FILE <name>` sections: `mm_<A|B>_<round>.log`
// (examples/malloc_macro output) and `ga_<A|B>_<round>.log` (criterion
// `global_alloc` bench output). MT cells are M ops/s (higher is better), ST
// cells are criterion's median-estimate time in ns (lower is better).
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";

const bundle = process.argv[2];
if (!bundle) throw new Error("usage: node scripts/ab_local_parse.mjs <bundle.log.gz>");
const text = gunzipSync(readFileSync(bundle)).toString("utf8");
const parts = text.split(/^=== FILE (\S+)\r?\n/m);
const files = new Map();
for (let i = 1; i < parts.length; i += 2) files.set(parts[i], parts[i + 1]);

const med = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const UNIT = { ns: 1, "µs": 1e3, us: 1e3, ms: 1e6, s: 1e9 };
const cells = {};
const put = (k, side, arm, v) => (((cells[k] ??= {})[side] ??= {})[arm] ??= []).push(v);
for (const [f, body] of files) {
  let m = f.match(/^mm_([AB])_\d+\.log$/);
  if (m) {
    let wl;
    for (const l of body.split(/\r?\n/)) {
      const w = l.match(/workload: (\S+)\s+\(mode: (\S+)\)/);
      if (w) {
        wl = `${w[1]}/${w[2]}`;
        continue;
      }
      const r = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M/);
      if (r && wl) {
        const k = `MT ${wl} T=${r[1]} (Mops)`;
        put(k, m[1], "sefer", +r[2]);
        put(k, m[1], "mimalloc", +r[3]);
      }
    }
    continue;
  }
  m = f.match(/^ga_([AB])_\d+\.log$/);
  if (m) {
    for (const x of body.matchAll(/^(global_alloc\S*)\s+time:\s+\[[\d.]+ \S+ ([\d.]+) (ns|µs|us|ms|s) [\d.]+ \S+\]/gm)) {
      const p = x[1].split("/");
      const [k, arm] =
        p[1] === "manual_realloc_sim" ? ["ST manual_realloc_sim (ns)", p[2]] : [`ST ${p[0]}/${p[2]} (ns)`, p[1]];
      const a = arm === "SeferAlloc" ? "sefer" : arm === "mimalloc" ? "mimalloc" : "system";
      put(k, m[1], a, +x[2] * UNIT[x[3]]);
    }
  }
}
const word = (r) => (r >= 1.05 ? `faster x${r.toFixed(2)}` : r <= 0.95 ? `slower x${(1 / r).toFixed(2)}` : "~equal");
const p4 = (x) => x.toPrecision(4);
const span = (a) => `${p4(Math.min(...a))}..${p4(Math.max(...a))}`;
console.log("| cell | Sefer A median (min..max) | Sefer B median (min..max) | B vs A | mimalloc B median | B vs mimalloc | n A/B |");
console.log("|---|---:|---:|---|---:|---|---:|");
for (const [k, s] of Object.entries(cells).sort()) {
  if (!s.A?.sefer || !s.B?.sefer) continue;
  const hi = k.includes("Mops");
  const sa = med(s.A.sefer), sb = med(s.B.sefer), mb = med(s.B.mimalloc);
  const ba = hi ? sb / sa : sa / sb; // >1: B faster than A
  const vsMi = hi ? sb / mb : mb / sb; // >1: B faster than mimalloc
  console.log(
    `| ${k} | ${p4(sa)} (${span(s.A.sefer)}) | ${p4(sb)} (${span(s.B.sefer)}) | ${word(ba)} | ${p4(mb)} | ${word(vsMi)} | ${s.A.sefer.length}/${s.B.sefer.length} |`,
  );
}
