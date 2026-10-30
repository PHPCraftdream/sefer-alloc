#!/usr/bin/env node
// SC-6 counting oracle report (docs/perf/g1_scan/SC6_ORACLE.md): per
// (workload, T) remote publications, upper-bound notifications, and the
// projected A-exact producer Ir per operation from the hot check (+6) and the
// notification (+24) budgets.
// Usage: node scripts/g1_sc6_oracle_report.mjs <dir> [summary.csv]

import fs from "node:fs";
import process from "node:process";

const dir = process.argv[2];
const HOT = 6;
const NOTIFY = 24;
const rows = [];
for (const f of fs.readdirSync(dir).filter((n) => /^ph6b_r\d+\.txt$/.test(n)).sort()) {
  const run = f.match(/r(\d+)/)[1];
  const text = fs.readFileSync(`${dir}/${f}`, "utf8");
  const ops = {};
  for (const m of text.matchAll(/RESULT mt_ops workload=(\w+) T=(\d+) ops=(\d+)/g)) ops[`${m[1]} ${m[2]}`] = Number(m[3]);
  for (const m of text.matchAll(/RESULT g1_oracle workload=(\w+) T=(\d+) publishes=(\d+) notifies=(\d+)/g)) {
    const k = `${m[1]} ${m[2]}`;
    const p = Number(m[3]);
    const n = Number(m[4]);
    if (!(k in ops)) throw new Error(`no ops for ${k} in ${f}`);
    const irOp = (HOT * p + NOTIFY * n) / ops[k];
    if (Math.abs(irOp * ops[k] - (HOT * p + NOTIFY * n)) > 1e-6) throw new Error("arithmetic");
    rows.push({ run, workload: m[1], t: Number(m[2]), ops: ops[k], p, n, share: p ? n / p : 0, irOp, notifyIrOp: (NOTIFY * n) / ops[k] });
  }
}
const out = [
  "| Run | Workload | T | ops | remote publications | notifications (upper bound) | notifications / publications | projected Ir/op (6·pub + 24·notify) / ops | of which notify |",
  "|---|---|---:|---:|---:|---:|---:|---:|---:|",
];
for (const r of rows) {
  out.push(`| ${r.run} | ${r.workload} | ${r.t} | ${r.ops} | ${r.p} | ${r.n} | ${(100 * r.share).toFixed(2)} % (${r.n}/${r.p}) | ${r.irOp.toFixed(4)} | ${r.notifyIrOp.toFixed(4)} |`);
}
console.log(out.join("\n"));
if (process.argv[3]) {
  const csv = ["run,workload,threads,ops,publications,notifications,notify_share,projected_ir_per_op,notify_ir_per_op"];
  for (const r of rows) csv.push([r.run, r.workload, r.t, r.ops, r.p, r.n, r.share.toFixed(5), r.irOp.toFixed(5), r.notifyIrOp.toFixed(5)].join(","));
  fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
}
