// U31 growth16 wall-clock runner: two executables (A = base, B = variant), quads in
// ABBA / BAAB order (alternating per quad) so both arms share the machine noise, then null
// quads with A in both arms. One growth16 body per process. Each output line is one run.
// Usage: node growth_quads.mjs <A.exe> <B.exe> <n> <quads> <null_quads> <out.jsonl>
import { execFileSync } from "node:child_process";
import fs from "node:fs";
const [,, aExe, bExe, n, quads, nullQuads, out] = process.argv;
const run = (exe) => {
  const t = execFileSync(exe, [n], { encoding: "utf8", timeout: 120000 });
  const m = t.match(/^RESULT growth16 n=(\d+) alloc_ns=(\d+) free_ns=(\d+) total_ns=(\d+)\r?$/m);
  if (!m || m[1] !== n) throw new Error(`bad output: ${t}`);
  return { alloc_ns: Number(m[2]), free_ns: Number(m[3]), total_ns: Number(m[4]) };
};
const fd = fs.openSync(out, "w");
const total = Number(quads) + Number(nullQuads);
for (let q = 0; q < total; q++) {
  const isNull = q >= Number(quads);
  const order = q % 2 === 0 ? ["A", "B", "B", "A"] : ["B", "A", "A", "B"];
  order.forEach((arm, slot) => {
    const exe = arm === "A" ? aExe : (isNull ? aExe : bExe);
    fs.writeSync(fd, JSON.stringify({ quad: q, kind: isNull ? "null" : "ab", slot, arm, n: Number(n), ...run(exe) }) + "\n");
  });
}
fs.closeSync(fd);
