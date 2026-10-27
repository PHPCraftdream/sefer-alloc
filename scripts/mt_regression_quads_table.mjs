// MT regression check (examples/ph6b_mt_ab.rs, real #[global_allocator]): table from the
// quads written by scripts/mt_regression_quads_run.mjs. Per cell (workload x T): median over
// quads of ln(sum t_B / sum t_A), t = ns/op of one run, the two runs of each arm in a quad
// summed; 95 % bootstrap over quads; null = the same exe in both arms.
// Usage: node scripts/mt_regression_quads_table.mjs <quads.jsonl[.gz]> [summary.csv]
import fs from "node:fs";
import zlib from "node:zlib";
const [,, inp, csvOut] = process.argv;
const buf = fs.readFileSync(inp);
const rows = (inp.endsWith(".gz") ? zlib.gunzipSync(buf) : buf).toString("utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
const CELLS = ["larson_T1", "mstress_T1", "larson_T2", "mstress_T2", "larson_T4", "mstress_T4", "larson_T8", "mstress_T8"];
let seed = 0x4d54_5251;
const rnd = () => ((seed = (seed * 1103515245 + 12345) >>> 0) / 2 ** 32);
const med = (a) => { const s = [...a].sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const boot = (a) => { const m = []; for (let i = 0; i < 2000; i++) m.push(med(a.map(() => a[Math.floor(rnd() * a.length)]))); m.sort((x, y) => x - y); return [m[49], m[1949]]; };
const diffs = (kind, cell) => {
  const q = {};
  for (const r of rows.filter((r) => r.kind === kind)) {
    const c = r.cells[cell]; if (!c || !(c.ops > 0)) throw new Error(`missing ${cell} in quad ${r.quad}`);
    ((q[r.quad] ??= { A: [], B: [] })[r.arm]).push(c.ns / c.ops);
  }
  const list = Object.values(q);
  if (list.length !== (kind === "ab" ? nAB : nN)) throw new Error(`${cell}: quad count`);
  return list.map(({ A, B }) => { if (A.length !== 2 || B.length !== 2) throw new Error("quad shape"); return Math.log((B[0] + B[1]) / (A[0] + A[1])); });
};
const word = (x) => (x < 0 ? `быстрее в ${Math.exp(-x).toFixed(3)}×` : `медленнее в ${Math.exp(x).toFixed(3)}×`);
const nAB = new Set(rows.filter((r) => r.kind === "ab").map((r) => r.quad)).size, nN = new Set(rows.filter((r) => r.kind === "null").map((r) => r.quad)).size;
if (nAB < 20 || nN < 10) throw new Error("too few quads");
console.log(`# MT regression: B = main after depot+B1 vs A = main before depot; ${nAB} AB quads, ${nN} A/A null quads; statistic = median over quads of ln(sum t_B / sum t_A), t = ns/op; 95 % bootstrap over quads`);
console.log("| Ячейка | A/A null % [CI] | B/A % [CI] | B против A |");
console.log("|---|---|---|---|");
const out = ["cell,null_median,null_ci_lo,null_ci_hi,ab_median,ab_ci_lo,ab_ci_hi,quads_ab,quads_null"];
for (const cell of CELLS) {
  const n = diffs("null", cell), d = diffs("ab", cell);
  const [nl, nh] = boot(n), [dl, dh] = boot(d), nm = med(n), dm = med(d);
  const p = (x) => (x * 100).toFixed(2);
  const verdict = dl <= 0 && dh >= 0 ? "равно" : word(dm);
  console.log(`| ${cell} | ${p(nm)} [${p(nl)}, ${p(nh)}] | ${p(dm)} [${p(dl)}, ${p(dh)}] | ${verdict} |`);
  out.push([cell, nm, nl, nh, dm, dl, dh, d.length, n.length].join(","));
}
if (csvOut) fs.writeFileSync(csvOut, out.join("\n") + "\n");
