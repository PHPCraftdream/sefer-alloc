#!/usr/bin/env node
// G1-K7 Addendum 6, run 3 — RSS decision (rule 3) and the run-3 part of the
// evidence (rule 5).
//
// Input: <dir>/judge.jsonl (examples/g1_k7_rss_judge.rs, one line per
// process, tagged with rep and bin by run3.sh) and <dir>/w4.log
// (examples/rss_probe.rs output, "== rep R k5|k7" headers).
//
// Per (workload, snapshot): statistic = median of the 5 repetitions. Noise
// N = max(64 KiB, max_i |K5_i − median K5|). Loss for arm v5 at budget E:
// median(v5, E) − median(K5) > N + A, where A = 0 for E = 0; otherwise
// A = E + the arm's median overflow committed record bytes at S2 / P0 / P1 /
// P2, A = E + the median of the maximum committed bytes over the per-pass
// checkpoints at S1 (peak), and A = 0 at S3 (teardown releases the record
// pages). W4 (rss_probe, K5 vs v5 at the default E, unchanged probe): A = 0
// at its peak and final snapshots. mimalloc is published, not judged.
//
// Evidence checked here (run-3 arms; P3 / P4 cover the per-kernel oracle of
// all 13 kernels): 5 repetitions per (arm, workload); every RSS value > 0;
// config_conflicts_delta = 0; K7 arms: requested = E, resolved = E rounded
// down to 256 B, effective = resolved; E = 0: no overflow parks; E > 0:
// parks > 0 on every workload, reloads >= 0.9 · parks on bulk16k_32 (W3, S2)
// and on bulk16k (W5 fill, P0), budget refusals > 0 or overflow bytes = the resolved budget (ring-full
// gating by the class latch) on bulk64k (W2, S2),
// decay_flushed = 0 on the bulk kernels (W2 / W3 at S2, W5 fill at P0), the
// W5 fill at a cohort steady state (its last 4 per-pass overflow byte
// counts equal — a plateau, not necessarily a full budget); K5 and
// mimalloc report zero overflow counters. W4 has no budget read-back (the
// probe is unchanged and uses SeferAlloc::new(), i.e. the default budget by
// construction); its evidence is the binary identity in identity.txt.
//
// Usage: node scripts/g1_k7_a6_rss.mjs <dir> [--csv <path>]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const dir = process.argv[2];
if (!dir) throw new Error("usage: g1_k7_a6_rss.mjs <dir> [--csv <path>]");
const ci = process.argv.indexOf("--csv");
const csvPath = ci > 0 ? process.argv[ci + 1] : null;

const REPS = 5;
const KIB = 1024;
const NOISE_FLOOR = 64 * KIB;
const SAT = 4;
const E_ARMS = [0, 524288, 1048576, 2097152];
const WORKLOADS = ["W1", "W2", "W3", "W5i", "W5c"];
const SNAPS = {
  W1: ["S1", "S2", "S3"],
  W2: ["S1", "S2", "S3"],
  W3: ["S1", "S2", "S3"],
  W5i: ["P0", "P2", "S1", "S2", "S3"],
  W5c: ["P0", "P1", "S1", "S2", "S3"],
};
const COUNTERS = ["ovf_bytes", "parks", "reloads", "budget_refusals", "os_refusals", "decay_flushed", "ingress_checks", "decommit_calls", "committed", "arms", "disarms"];

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const fail = (m) => {
  throw new Error(m);
};
const nat = (v) => Number.isInteger(v) && v >= 0;
const pos = (v, tag) => {
  if (!(Number.isFinite(v) && v > 0)) fail(`${tag}: RSS ${v}`);
  return v;
};

const rows = fs
  .readFileSync(path.join(dir, "judge.jsonl"), "utf8")
  .split(/\r?\n/)
  .filter((l) => l.trim())
  .map((l) => JSON.parse(l));

const rss = (r, s) => pos(r[s]?.rss, `${r.bin} ${r.workload} rep ${r.rep} ${s}`);
const committed = (r, s) => (s === "S1" ? r.S1.committed_max : s === "S3" ? 0 : r[s].committed);

const byArm = new Map();
for (const r of rows) {
  if (r.config_conflicts_delta !== 0) fail(`${r.bin} ${r.workload} rep ${r.rep}: config conflict`);
  const k = `${r.bin}|${r.workload}`;
  if (!byArm.has(k)) byArm.set(k, []);
  byArm.get(k).push(r);
}
const arms = ["k5", ...E_ARMS.map((e) => `k7_${e}`), "mi"];
for (const a of arms)
  for (const w of WORKLOADS) {
    const rs = byArm.get(`${a}|${w}`) ?? [];
    if (rs.length !== REPS) fail(`${a} ${w}: ${rs.length} repetitions, want ${REPS}`);
    if (new Set(rs.map((r) => r.rep)).size !== REPS) fail(`${a} ${w}: repeated rep`);
    for (const r of rs) {
      for (const s of SNAPS[w]) rss(r, s);
      const tag = `${a} ${w} rep ${r.rep}`;
      if (!nat(r.S1.committed_max)) fail(`${tag}: S1.committed_max ${r.S1.committed_max}`);
      for (const s of [...SNAPS[w].filter((x) => x !== "S1" && x !== "S3")])
        for (const c of COUNTERS) if (!nat(r[s][c])) fail(`${tag} ${s}: ${c} = ${r[s][c]}`);
      if (w.startsWith("W5") && !(Array.isArray(r.fill_bytes) && r.fill_bytes.every(nat))) fail(`${tag}: fill_bytes`);
    }
  }

// Evidence.
const live = (w) => SNAPS[w].filter((s) => s !== "S1" && s !== "S3");
for (const e of E_ARMS)
  for (const w of WORKLOADS)
    for (const r of byArm.get(`k7_${e}|${w}`)) {
      const tag = `k7_${e} ${w} rep ${r.rep}`;
      if (r.arm !== "k7") fail(`${tag}: arm ${r.arm}`);
      if (r.budget_requested !== e) fail(`${tag}: requested ${r.budget_requested}`);
      if (r.budget_resolved !== Math.floor(e / 256) * 256) fail(`${tag}: resolved ${r.budget_resolved}`);
      if (r.budget_effective !== r.budget_resolved) fail(`${tag}: effective ${r.budget_effective} != resolved`);
      const s2 = r.S2;
      if (e === 0) {
        for (const s of live(w)) if (r[s].parks !== 0 || r[s].ovf_bytes !== 0) fail(`${tag} ${s}: overflow active at E = 0`);
        continue;
      }
      if (!(s2.parks > 0)) fail(`${tag}: no overflow parks`);
      const near = (c) => c.reloads >= 0.9 * c.parks;
      if (w === "W3" && !near(s2)) fail(`${tag}: reloads ${s2.reloads} vs parks ${s2.parks}`);
      // Rule 5: budget refusals OR ring-full gating (the overflow holds its whole
      // budget and the class latch stops further park attempts).
      if (w === "W2" && !(s2.budget_refusals > 0 || s2.ovf_bytes === r.budget_resolved))
        fail(`${tag}: bulk64k neither refused nor reached the budget`);
      if ((w === "W2" || w === "W3") && s2.decay_flushed !== 0) fail(`${tag}: decay_flushed ${s2.decay_flushed} on a bulk kernel`);
      if (w === "W5i" || w === "W5c") {
        if (!near(r.P0)) fail(`${tag}: fill reloads ${r.P0.reloads} vs parks ${r.P0.parks}`);
        if (r.P0.decay_flushed !== 0) fail(`${tag}: decay_flushed ${r.P0.decay_flushed} during the bulk16k fill`);
        const t = r.fill_bytes.slice(-SAT);
        if (t.length !== SAT || t.some((b) => b !== t[0])) fail(`${tag}: fill not at a cohort steady state (${t.join(",")})`);
      }
    }
for (const a of ["k5", "mi"])
  for (const w of WORKLOADS)
    for (const r of byArm.get(`${a}|${w}`))
      for (const s of [...live(w), "S2"])
        if (COUNTERS.some((c) => r[s][c] !== 0)) fail(`${a} ${w} ${s}: overflow counters on a non-K7 arm`);

console.log(`# G1-K7 A6 run 3 — RSS, ${REPS} repetitions, statistic = median; N = max(64 KiB, max |K5_i − median K5|)`);
console.log("");
console.log("## Evidence (K7 arms, medians at S2; W5 rows at P0, the end of the fill)");
console.log("");
console.log("| arm | workload | requested | resolved | effective | parks | reloads | budget_refusals | os_refusals | decay_flushed | arms | disarms | fill bytes (last) |");
console.log("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const e of E_ARMS)
  for (const w of WORKLOADS) {
    const rs = byArm.get(`k7_${e}|${w}`);
    const at = w.startsWith("W5") ? "P0" : "S2";
    const m = (f) => median(rs.map((r) => r[at][f]));
    const fb = w.startsWith("W5") ? median(rs.map((r) => r.fill_bytes.at(-1))) : "-";
    console.log(`| k7_${e} | ${w} | ${rs[0].budget_requested} | ${rs[0].budget_resolved} | ${rs[0].budget_effective} | ${m("parks")} | ${m("reloads")} | ${m("budget_refusals")} | ${m("os_refusals")} | ${m("decay_flushed")} | ${m("arms")} | ${m("disarms")} | ${fb} |`);
  }
console.log("");

const out = [];
const losses = [];
const mb = (b) => (b / 1048576).toFixed(2);
console.log("## RSS");
console.log("");
console.log("| workload | snapshot | K5 MiB | N KiB | E | v5 MiB | Δ KiB | A KiB | committed KiB | overflow bytes KiB | decay_flushed | loss | mimalloc MiB |");
console.log("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|");
for (const w of WORKLOADS)
  for (const s of SNAPS[w]) {
    const k5 = byArm.get(`k5|${w}`).map((r) => rss(r, s));
    const mk5 = median(k5);
    const noise = Math.max(NOISE_FLOOR, ...k5.map((x) => Math.abs(x - mk5)));
    const mmi = median(byArm.get(`mi|${w}`).map((r) => rss(r, s)));
    for (const e of E_ARMS) {
      const rs = byArm.get(`k7_${e}|${w}`);
      const mv = median(rs.map((r) => rss(r, s)));
      const com = median(rs.map((r) => committed(r, s)));
      const isLive = s !== "S1" && s !== "S3";
      const held = isLive ? median(rs.map((r) => r[s].ovf_bytes)) : 0;
      const decay = isLive ? median(rs.map((r) => r[s].decay_flushed)) : 0;
      const allow = e === 0 || s === "S3" ? 0 : e + com;
      const delta = mv - mk5;
      const loss = delta > noise + allow;
      if (loss) losses.push(`${w} ${s} E=${e}`);
      out.push({ w, s, e, mk5, noise, mv, delta, allow, com, held, decay, loss, mmi });
      console.log(
        `| ${w} | ${s} | ${mb(mk5)} | ${(noise / KIB).toFixed(0)} | ${e} | ${mb(mv)} | ${(delta / KIB).toFixed(0)} | ${(allow / KIB).toFixed(0)} | ${(com / KIB).toFixed(0)} | ${(held / KIB).toFixed(0)} | ${decay} | ${loss ? "LOSS" : "no"} | ${mb(mmi)} |`,
      );
    }
  }

// W4: rss_probe, K5 vs v5 at the default E.
const w4 = { k5: { peak: [], fin: [] }, k7: { peak: [], fin: [] } };
let cur = null;
for (const l of fs.readFileSync(path.join(dir, "w4.log"), "utf8").split(/\r?\n/)) {
  const h = l.match(/^== rep (\d+) (k5|k7)$/);
  if (h) cur = h[2];
  const p = l.match(/Peak RSS\s*:\s*([\d.]+) MB/);
  const f = l.match(/Final RSS\s*:\s*([\d.]+) MB/);
  if (p) w4[cur].peak.push(pos(Number(p[1]) * 1048576, `W4 ${cur} peak`));
  if (f) w4[cur].fin.push(pos(Number(f[1]) * 1048576, `W4 ${cur} final`));
}
for (const t of ["k5", "k7"])
  for (const k of ["peak", "fin"]) if (w4[t][k].length !== REPS) fail(`W4 ${t} ${k}: ${w4[t][k].length} values`);
console.log("");
console.log("| W4 snapshot | K5 MiB | N KiB | v5 (default E) MiB | Δ KiB | loss |");
console.log("|---|---:|---:|---:|---:|---|");
for (const k of ["peak", "fin"]) {
  const mk5 = median(w4.k5[k]);
  const noise = Math.max(NOISE_FLOOR, ...w4.k5[k].map((x) => Math.abs(x - mk5)));
  const mv = median(w4.k7[k]);
  const delta = mv - mk5;
  const loss = delta > noise;
  if (loss) losses.push(`W4 ${k}`);
  out.push({ w: "W4", s: k, e: 1048576, mk5, noise, mv, delta, allow: 0, com: 0, held: 0, decay: 0, loss, mmi: "" });
  console.log(`| ${k} | ${mb(mk5)} | ${(noise / KIB).toFixed(0)} | ${mb(mv)} | ${(delta / KIB).toFixed(0)} | ${loss ? "LOSS" : "no"} |`);
}

if (csvPath) {
  const head = "workload,snapshot,e,k5_median,noise,v5_median,delta,allowance,committed,overflow_bytes,decay_flushed,loss,mimalloc_median";
  const body = out.map((o) => [o.w, o.s, o.e, o.mk5, o.noise, o.mv, o.delta, o.allow, o.com, o.held, o.decay, o.loss ? "yes" : "no", o.mmi].join(","));
  fs.writeFileSync(csvPath, [head, ...body].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvPath)}`);
}
console.log("");
console.log("## RSS decision (Addendum 6 rule 3)");
console.log("evidence: every run-3 check above passed (W4: default budget by construction, no read-back)");
console.log(`losses: ${losses.length ? losses.join(", ") : "none"}`);
console.log(`RSS: ${losses.length ? "FAIL — NO-GO" : "PASS"}`);
