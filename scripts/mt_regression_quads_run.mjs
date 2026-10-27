// MT regression check runner: examples/ph6b_mt_ab.rs built at two commits (A, B), run as quads in
// ABBA / BAAB order (alternating per quad) so both arms share the machine noise; then null quads
// with A in both arms. Each line of <out> is one run: quad, kind (ab|null), slot, arm, cells.
// Checks the harness's own sanity counters (config_conflicts == 0, segments_reserved_total > 0).
// Usage: node scripts/mt_regression_quads_run.mjs <A.exe> <B.exe> <quads> <null_quads> <out.jsonl>
import { execFileSync } from "node:child_process";
import fs from "node:fs";
const [,, aExe, bExe, quads, nullQuads, out] = process.argv;
const run = (exe) => {
  const t = execFileSync(exe, { encoding: "utf8", timeout: 120000 });
  const cells = {};
  for (const l of t.split(/\r?\n/)) {
    let m = l.match(/^RESULT mt_ns workload=(\w+) T=(\d+) ns=(\d+)$/);
    if (m) { (cells[`${m[1]}_T${m[2]}`] ??= {}).ns = Number(m[3]); continue; }
    m = l.match(/^RESULT mt_ops workload=(\w+) T=(\d+) ops=(\d+)$/);
    if (m) { (cells[`${m[1]}_T${m[2]}`] ??= {}).ops = Number(m[3]); continue; }
    m = l.match(/^RESULT (segments_reserved_total|config_conflicts)=(\d+)$/);
    if (m) cells[m[1]] = Number(m[2]);
  }
  if (cells.config_conflicts !== 0 || !(cells.segments_reserved_total > 0)) throw new Error(`sanity: ${JSON.stringify(cells)}`);
  return cells;
};
const fd = fs.openSync(out, "w");
const emit = (o) => fs.writeSync(fd, JSON.stringify(o) + "\n");
const total = Number(quads) + Number(nullQuads);
for (let q = 0; q < total; q++) {
  const isNull = q >= Number(quads);
  const B = isNull ? aExe : bExe;
  const order = q % 2 === 0 ? ["A", "B", "B", "A"] : ["B", "A", "A", "B"];
  order.forEach((arm, slot) => emit({ quad: q, kind: isNull ? "null" : "ab", slot, arm, cells: run(arm === "A" ? aExe : B) }));
}
fs.closeSync(fd);
console.log(`done ${total} quads -> ${out}`);
