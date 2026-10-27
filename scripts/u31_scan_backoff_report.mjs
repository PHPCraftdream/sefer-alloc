#!/usr/bin/env node
// U31 — owner-side sidecar-drain deferral: gate report.
// Derives every table of docs/perf/U31_SCAN_BACKOFF_GATE.md from the
// committed raws in docs/perf/u31_scan_backoff/ and asserts the arithmetic it
// prints. Thresholds: docs/perf/u31_scan_backoff/U31_PREREG.md (fixed before
// the runs; amendments 1-3 dated inside it):
//   1. growth16 n=131072 Ir <= 0.70 x base;
//   2. growth16 time: median ratio < 1 and CI upper < 1 (100 ABBA quads);
//   3. J1' v2 ablation net of null: pair16, pair64, larson effect <= +1 %
//      and CI upper <= +1.5 %; other kernels not (CI lower > 0 AND effect
//      > +2 %); an imprecise larson is judged on its 40-process replication
//      (amendment 2);
//   4. MT time (ph6b_mt_ab, 100 quads + 50 null): no cell with CI lower > 0
//      and effect above the largest null |effect|;
//   5. memory: segments_reserved_total and peak RSS (growth16 20 quads, MT
//      100 quads) paired median ratio <= 1.05;
//   oracle: growth16 n=131072 deferred > 0, discovery scans <= 128, scan
//   words <= 65 536.
// Every ratio is B/A (variant/base); bootstraps use a fixed-seed LCG so the
// output is deterministic.
//
// Usage: node scripts/u31_scan_backoff_report.mjs [--csv <summary.csv>]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import zlib from "node:zlib";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
const D = (...p) => path.join(root, "docs", "perf", "u31_scan_backoff", ...p);
let csvOut = null;
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === "--csv") csvOut = process.argv[++i];
  else throw new Error(`unknown argument ${process.argv[i]}`);
}
const assert = (c, m) => {
  if (!c) throw new Error(`assert: ${m}`);
};
const close = (a, b, m) => assert(Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(b)), `${m}: ${a} != ${b}`);

// ---- statistics -----------------------------------------------------------
const median = (a) => {
  assert(a.length > 0, "median of empty");
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const BOOT = 4000;
function bootstrapMedianCi(v, seed) {
  let x = seed >>> 0;
  const rnd = () => {
    x = (Math.imul(x, 1103515245) + 12345) >>> 0;
    return x / 4294967296;
  };
  const b = [];
  for (let i = 0; i < BOOT; i++) b.push(median(v.map(() => v[Math.floor(rnd() * v.length)])));
  b.sort((p, q) => p - q);
  return [b[Math.floor(0.025 * BOOT)], b[Math.ceil(0.975 * BOOT) - 1]];
}
const pct = (l) => (100 * l).toFixed(2);
// Four decimals below a 1 % effect so a real but small ratio never prints as 1.000.
const dp = (r) => (Math.abs(Math.log(r)) < 0.01 ? 4 : 3);
const fx = (r) => (r < 1 ? `быстрее в ${(1 / r).toFixed(dp(r))}×` : r > 1 ? `медленнее в ${r.toFixed(dp(r))}×` : "равно");
const mem = (r) => (r < 1 ? `меньше в ${(1 / r).toFixed(dp(r))}×` : r > 1 ? `больше в ${r.toFixed(dp(r))}×` : "равно");

const readJsonl = (p) => {
  const raw = p.endsWith(".gz") ? zlib.gunzipSync(fs.readFileSync(p)).toString("utf8") : fs.readFileSync(p, "utf8");
  return raw.split("\n").filter(Boolean).map((l) => JSON.parse(l));
};
function readCsv(p) {
  const [head, ...lines] = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
  const cols = head.split(",");
  return lines.map((l) => {
    const c = l.split(",");
    assert(c.length === cols.length, `${p}: column count`);
    return Object.fromEntries(cols.map((k, i) => [k, c[i]]));
  });
}

// Paired quads: per quad ln(sum B / sum A) of `get(row)`.
function pairedQuads(rows, kind, get, seed) {
  const q = new Map();
  for (const r of rows.filter((r) => r.kind === undefined || r.kind === kind)) {
    const e = q.get(r.quad) ?? { A: 0, B: 0, nA: 0, nB: 0 };
    e[r.arm] += get(r);
    e[`n${r.arm}`] += 1;
    q.set(r.quad, e);
  }
  const v = [];
  for (const [quad, e] of q) {
    assert(e.nA === 2 && e.nB === 2, `quad ${quad}: ${e.nA} A / ${e.nB} B runs`);
    v.push(Math.log(e.B / e.A));
  }
  const m = median(v);
  const [lo, hi] = bootstrapMedianCi(v, seed);
  assert(lo <= m && m <= hi, "median outside its CI");
  return { n: v.length, ln: m, lo, hi };
}

const csv = [["section", "version", "row", "metric", "value", "ci_lo", "ci_hi", "n", "verdict"]];
const out = [];
const P = (s = "") => out.push(s);

// ---- 1. callgrind growth16 ------------------------------------------------
function readCallgrind(name) {
  const t = fs.readFileSync(D(name), "utf8").split(/\r?\n/);
  const ir = {};
  const cnt = {};
  const deferred = {};
  for (const l of t) {
    let m = l.match(/^IR_TOTAL (sefer|mimalloc) (\d+) (\d+)$/);
    if (m) ir[`${m[1]}_${m[2]}`] = Number(m[3]);
    m = l.match(/^COUNTERS (\{.*\})$/);
    if (m) {
      const j = JSON.parse(m[1]);
      cnt[j.growth16_n] = j;
    }
    m = l.match(/^\{"growth16_n":(\d+),"u31_deferred_scans":(\d+)\}$/);
    if (m) deferred[m[1]] = Number(m[2]);
  }
  return { ir, cnt, deferred };
}
const NS = [32768, 65536, 131072];
const cg = { base: readCallgrind("_raw_callgrind_base.log"), v1: readCallgrind("_raw_callgrind_v1.log"), v2: readCallgrind("_raw_callgrind_v2.log") };
for (const v of ["base", "v1", "v2"]) for (const n of NS) assert(cg[v].ir[`sefer_${n}`] > 0 && cg[v].ir[`mimalloc_${n}`] > 0, `${v} ${n}: IR missing`);
P("### 1. growth16 — callgrind Ir (WSL, `ut_u31_probe`)");
P();
P("| n | base | v1 | v2 | mimalloc (v2 run) | v2 против base | v2 против mimalloc |");
P("|---|---|---|---|---|---|---|");
for (const n of NS) {
  const b = cg.base.ir[`sefer_${n}`];
  const v1 = cg.v1.ir[`sefer_${n}`];
  const v2 = cg.v2.ir[`sefer_${n}`];
  const mi = cg.v2.ir[`mimalloc_${n}`];
  const rb = v2 / b;
  const rm = v2 / mi;
  close(rb * b, v2, "ratio arithmetic");
  P(`| ${n} | ${b} | ${v1} | ${v2} | ${mi} | ${fx(rb)} | ${fx(rm)} |`);
  csv.push(["callgrind", "v2", `growth16_n${n}`, "ir_ratio_vs_base", rb, "", "", 1, ""]);
  csv.push(["callgrind", "v2", `growth16_n${n}`, "ir_ratio_vs_mimalloc", rm, "", "", 1, ""]);
  csv.push(["callgrind", "v1", `growth16_n${n}`, "ir_ratio_vs_base", v1 / b, "", "", 1, ""]);
}
const baseIr = cg.base.ir.sefer_131072;
const t1Limit = Math.floor(0.7 * baseIr);
P();
P(`base: sefer/mimalloc Ir при n = 131072 = ${(baseIr / cg.base.ir.mimalloc_131072).toFixed(2)}× (${baseIr}/${cg.base.ir.mimalloc_131072}). Порог 1: Ir ≤ ${t1Limit} (0.70 × ${baseIr}).`);
P();

P("### 2. Оракул активации (growth16 n = 131072, счётчики `bench-internals`)");
P();
P("| Счётчик | base | v1 | v2 | Порог |");
P("|---|---|---|---|---|");
const o = (v) => cg[v].cnt[131072];
P(`| find_segment | ${o("base").find_segment_calls} | ${o("v1").find_segment_calls} | ${o("v2").find_segment_calls} | — |`);
P(`| discovery-сканы (drain) | ${o("base").scan_calls} | ${o("v1").scan_calls} | ${o("v2").scan_calls} | ≤ 128 |`);
P(`| прочитано слов | ${o("base").scan_words} | ${o("v1").scan_words} | ${o("v2").scan_words} | ≤ 65 536 |`);
P(`| отложенные сканы | — | ${cg.v1.deferred[131072]} | ${cg.v2.deferred[131072]} | > 0 |`);
P();
for (const v of ["v1", "v2"]) assert(o(v).find_segment_calls === o("base").find_segment_calls, `${v}: discovery calls differ from base`);
for (const v of ["v1", "v2"]) assert(o(v).scan_calls + cg[v].deferred[131072] === o(v).find_segment_calls, `${v}: drains + deferred != discoveries`);
const oracle = (v) => cg[v].deferred[131072] > 0 && o(v).scan_calls <= 128 && o(v).scan_words <= 65536;

// ---- 3. growth16 time -----------------------------------------------------
function growth(v) {
  const rows = readJsonl(D(v, "growth_quads_131072.jsonl"));
  assert(rows.every((r) => r.n === 131072), `${v}: n`);
  const r = {};
  for (const f of ["total_ns", "alloc_ns", "free_ns"]) {
    r[f] = { ab: pairedQuads(rows, "ab", (x) => x[f], 0x310 + f.length), nul: pairedQuads(rows, "null", (x) => x[f], 0x311 + f.length) };
    assert(r[f].ab.n === 100 && r[f].nul.n === 50, `${v} ${f}: quad counts`);
  }
  return r;
}
const gr = { v1: growth("v1"), v2: growth("v2") };
P("### 3. growth16 n = 131072 — время, 100 четвёрок процессов ABBA + 50 null");
P();
P("| Версия | Мера | B/A [95 % CI] | null [95 % CI] | Итог |");
P("|---|---|---|---|---|");
for (const v of ["v1", "v2"])
  for (const f of ["total_ns", "alloc_ns", "free_ns"]) {
    const a = gr[v][f].ab;
    const n = gr[v][f].nul;
    const verdict = a.hi < 0 ? fx(Math.exp(a.ln)) : a.lo > 0 ? fx(Math.exp(a.ln)) : "равно";
    P(`| ${v} | ${f} | ${Math.exp(a.ln).toFixed(4)} [${Math.exp(a.lo).toFixed(4)}, ${Math.exp(a.hi).toFixed(4)}] | ${Math.exp(n.ln).toFixed(4)} [${Math.exp(n.lo).toFixed(4)}, ${Math.exp(n.hi).toFixed(4)}] | ${verdict} |`);
    csv.push(["growth_time", v, f, "ratio", Math.exp(a.ln), Math.exp(a.lo), Math.exp(a.hi), a.n, verdict]);
    csv.push(["growth_time", v, f, "null_ratio", Math.exp(n.ln), Math.exp(n.lo), Math.exp(n.hi), n.n, ""]);
  }
P();

// ---- 4. ablation ----------------------------------------------------------
const KERNELS = ["pair16", "pair64", "pair_rt", "bulk64", "bulk16k", "churn16", "churn64", "churn256", "churn1024", "vec512", "larson"];
const STRICT = ["pair16", "pair64", "larson"];
function ablation(v) {
  const rows = Object.fromEntries(readCsv(D(v, "vsnull_u31.csv")).map((r) => [r.kernel, r]));
  const rep = readCsv(D(v, "vsnull_u31_larson40.csv"));
  assert(rep.length === 1 && rep[0].kernel === "larson" && Number(rep[0].reps) === 40, `${v}: larson replication`);
  const check = (r, reps) => {
    assert(r.variant === "u31_scan_backoff" && Number(r.reps) === reps, `${v} ${r.kernel}: identity`);
    close(Math.exp(+r.effect), +r.ratio, `${v} ${r.kernel}: ratio != exp(effect)`);
    close(+r.variant_ab_median - +r.null_ab_median, +r.effect, `${v} ${r.kernel}: effect != variant - null`);
  };
  for (const k of KERNELS) {
    assert(rows[k], `${v}: kernel ${k} missing`);
    check(rows[k], 20);
  }
  check(rep[0], 40);
  const pass = {};
  for (const k of KERNELS) {
    let r = rows[k];
    if (k === "larson" && r.precise !== "yes") r = rep[0];
    const e = +r.effect;
    const lo = +r.effect_ci_lo;
    const hi = +r.effect_ci_hi;
    pass[k] = STRICT.includes(k) ? e <= 0.01 && hi <= 0.015 : !(lo > 0 && e > 0.02);
  }
  return { rows, rep: rep[0], pass };
}
const ab = { v1: ablation("v1"), v2: ablation("v2") };
P("### 4. Ядра судьи J1′ v2 — эффект за вычетом null (20 процессов × 2000 блоков)");
P();
P("| Ядро | v1 эффект % [CI] | v2 эффект % [CI] | v2 итог | v2 порог |");
P("|---|---|---|---|---|");
const abVerdict = (r) => {
  if (r.precise !== "yes") return "неточно";
  if (+r.effect_ci_hi < 0 || +r.effect_ci_lo > 0) return fx(+r.ratio);
  return "равно";
};
for (const k of KERNELS) {
  const a = ab.v1.rows[k];
  const b = ab.v2.rows[k];
  P(`| ${k} | ${pct(+a.effect)} [${pct(+a.effect_ci_lo)}, ${pct(+a.effect_ci_hi)}] | ${pct(+b.effect)} [${pct(+b.effect_ci_lo)}, ${pct(+b.effect_ci_hi)}] | ${abVerdict(b)} | ${ab.v2.pass[k] ? "✓" : "✗"} |`);
  for (const v of ["v1", "v2"]) {
    const r = ab[v].rows[k];
    csv.push(["ablation", v, k, "effect", +r.effect, +r.effect_ci_lo, +r.effect_ci_hi, 20, abVerdict(r)]);
  }
}
for (const v of ["v1", "v2"]) {
  const r = ab[v].rep;
  P(`| larson, повтор 40 процессов (${v}) | | ${pct(+r.effect)} [${pct(+r.effect_ci_lo)}, ${pct(+r.effect_ci_hi)}] | ${abVerdict(r)} | ${ab[v].pass.larson ? "✓" : "✗"} |`);
  csv.push(["ablation", v, "larson_rep40", "effect", +r.effect, +r.effect_ci_lo, +r.effect_ci_hi, 40, abVerdict(r)]);
}
P();
// Activation in the judge kernels (per process, ab_variant, 2000 blocks).
function counters(v) {
  const m = {};
  for (const l of fs.readFileSync(D(v, "abl_counters.txt"), "utf8").split(/\r?\n/).filter(Boolean)) {
    const k = l.split(" ")[0];
    const g = Object.fromEntries([...l.matchAll(/(\w+)=(\d+)/g)].map((x) => [x[1], Number(x[2])]));
    assert(g.base_find === g.var_find, `${v} ${k}: discovery calls differ`);
    assert(g.var_scan + g.var_deferred === g.var_find, `${v} ${k}: drains + deferred != discoveries`);
    m[k] = g;
  }
  for (const k of KERNELS) assert(m[k], `${v}: counters for ${k} missing`);
  return m;
}
const ac = { v1: counters("v1"), v2: counters("v2") };
P("Активация в ядрах судьи (на процесс, `ab_variant`, 2000 блоков): discovery-вызовов / из них отложено (v1, v2).");
P();
P("| Ядро | discovery | слов base | v1 отложено | v2 отложено | v2 слов |");
P("|---|---|---|---|---|---|");
for (const k of KERNELS) P(`| ${k} | ${ac.v2[k].var_find} | ${ac.v2[k].base_words} | ${ac.v1[k].var_deferred} | ${ac.v2[k].var_deferred} | ${ac.v2[k].var_words} |`);
P();

// ---- 5. MT time -----------------------------------------------------------
const CELLS = ["larson_T1", "mstress_T1", "larson_T2", "mstress_T2", "larson_T4", "mstress_T4", "larson_T8", "mstress_T8"];
function mt(v) {
  const rows = readJsonl(D(v, "mt_quads.jsonl.gz"));
  const res = {};
  for (const c of CELLS) {
    const get = (r) => r.cells[c].ns / r.cells[c].ops;
    res[c] = { ab: pairedQuads(rows, "ab", get, 0x5150 + c.length), nul: pairedQuads(rows, "null", get, 0x5151 + c.length) };
    assert(res[c].ab.n === 100 && res[c].nul.n === 50, `${v} ${c}: quad counts`);
  }
  const maxNull = Math.max(...CELLS.map((c) => Math.abs(res[c].nul.ln)));
  const seg = { A: [], B: [] };
  for (const r of rows.filter((r) => r.kind === "ab")) seg[r.arm].push(r.cells.segments_reserved_total);
  return { res, maxNull, segA: median(seg.A), segB: median(seg.B), segAmax: Math.max(...seg.A), segBmax: Math.max(...seg.B) };
}
const mtr = { v1: mt("v1"), v2: mt("v2") };
P("### 5. Многопоточность — время (`ph6b_mt_ab`, 100 четвёрок ABBA + 50 null; t = ns/op)");
P();
P("| Ячейка | v1 % [CI] | v2 null % | v2 % [CI] | v2 итог |");
P("|---|---|---|---|---|");
const mtSlow = (v, c) => mtr[v].res[c].ab.lo > 0 && mtr[v].res[c].ab.ln > mtr[v].maxNull;
for (const c of CELLS) {
  const a = mtr.v1.res[c].ab;
  const b = mtr.v2.res[c].ab;
  const n = mtr.v2.res[c].nul;
  const verdict = b.hi < 0 || b.lo > 0 ? fx(Math.exp(b.ln)) : "равно";
  P(`| ${c} | ${pct(a.ln)} [${pct(a.lo)}, ${pct(a.hi)}] | ${pct(n.ln)} | ${pct(b.ln)} [${pct(b.lo)}, ${pct(b.hi)}] | ${verdict} |`);
  for (const v of ["v1", "v2"]) {
    const x = mtr[v].res[c].ab;
    csv.push(["mt_time", v, c, "ln_ratio", x.ln, x.lo, x.hi, x.n, mtSlow(v, c) ? "SLOWER" : "ok"]);
  }
}
P();
P(`Наибольший |null| по ячейкам: v1 ${pct(mtr.v1.maxNull)} %, v2 ${pct(mtr.v2.maxNull)} %.`);
P();

// ---- 6. memory --------------------------------------------------------------
function rss(v) {
  const g = readJsonl(D(v, "rss_quads.jsonl"));
  const m = readJsonl(D(v, "rss_mt100.jsonl"));
  return {
    growth: pairedQuads(g, undefined, (r) => r.growth16_maxrss_kib, 0x7701),
    mt20: pairedQuads(g, undefined, (r) => r.mt_maxrss_kib, 0x7702),
    mt100: pairedQuads(m, undefined, (r) => r.mt_maxrss_kib, 0x7703),
  };
}
const rs = { v1: rss("v1"), v2: rss("v2") };
for (const v of ["v1", "v2"]) assert(rs[v].growth.n === 20 && rs[v].mt20.n === 20 && rs[v].mt100.n === 100, `${v}: rss quad counts`);
P("### 6. Память — пиковый RSS (WSL `/usr/bin/time`, ABBA, парная медиана B/A) и сегменты");
P();
P("| Мера | v1 [CI] | v2 [CI] | v2 итог | Порог ≤ 1.05 |");
P("|---|---|---|---|---|");
const rRow = (label, key) => {
  const a = rs.v1[key];
  const b = rs.v2[key];
  const ok = (x) => Math.exp(x.ln) <= 1.05;
  P(`| ${label} | ${Math.exp(a.ln).toFixed(4)} [${Math.exp(a.lo).toFixed(4)}, ${Math.exp(a.hi).toFixed(4)}] | ${Math.exp(b.ln).toFixed(4)} [${Math.exp(b.lo).toFixed(4)}, ${Math.exp(b.hi).toFixed(4)}] | ${mem(Math.exp(b.ln))} | v1 ${ok(a) ? "✓" : "✗"}, v2 ${ok(b) ? "✓" : "✗"} |`);
  for (const v of ["v1", "v2"]) {
    const x = rs[v][key];
    csv.push(["memory", v, key, "rss_ratio", Math.exp(x.ln), Math.exp(x.lo), Math.exp(x.hi), x.n, ok(x) ? "ok" : "FAIL"]);
  }
};
rRow("growth16 peak RSS, 20 четвёрок", "growth");
rRow("MT peak RSS, 20 четвёрок", "mt20");
rRow("MT peak RSS, 100 четвёрок", "mt100");
P(`| segments_reserved_total (медиана, макс.) | ${mtr.v1.segB}/${mtr.v1.segA}, ${mtr.v1.segBmax}/${mtr.v1.segAmax} | ${mtr.v2.segB}/${mtr.v2.segA}, ${mtr.v2.segBmax}/${mtr.v2.segAmax} | ${mem(mtr.v2.segB / mtr.v2.segA)} | v1 ${mtr.v1.segB / mtr.v1.segA <= 1.05 ? "✓" : "✗"}, v2 ${mtr.v2.segB / mtr.v2.segA <= 1.05 ? "✓" : "✗"} |`);
for (const v of ["v1", "v2"]) csv.push(["memory", v, "segments_reserved_total", "median_ratio", mtr[v].segB / mtr[v].segA, "", "", 200, ""]);
P();

// ---- verdicts ---------------------------------------------------------------
function verdict(v) {
  const t = {
    t1: cg[v].ir.sefer_131072 <= t1Limit,
    t2: Math.exp(gr[v].total_ns.ab.ln) < 1 && gr[v].total_ns.ab.hi < 0,
    t3: KERNELS.every((k) => ab[v].pass[k]),
    t4: CELLS.every((c) => !mtSlow(v, c)),
    t5:
      mtr[v].segB / mtr[v].segA <= 1.05 &&
      Math.exp(rs[v].growth.ln) <= 1.05 &&
      Math.exp(rs[v].mt100.ln) <= 1.05,
    oracle: oracle(v),
  };
  t.go = Object.values(t).every(Boolean);
  return t;
}
const V = { v1: verdict("v1"), v2: verdict("v2") };
P("### 7. Вердикт по предрегистрации");
P();
P("| Порог | v1 | v2 |");
P("|---|---|---|");
const mark = (b) => (b ? "✓" : "✗");
for (const [k, label] of [
  ["t1", "1. growth16 Ir ≤ 0.70 × base"],
  ["t2", "2. growth16 время: ratio < 1, CI < 1"],
  ["t3", "3. ядра J1′ без вреда"],
  ["t4", "4. MT время без регресса"],
  ["t5", "5. память ≤ 1.05×"],
  ["oracle", "оракул активации"],
])
  P(`| ${label} | ${mark(V.v1[k])} | ${mark(V.v2[k])} |`);
P(`| **Итог** | **${V.v1.go ? "GO" : "NO-GO"}** | **${V.v2.go ? "GO" : "NO-GO"}** |`);
// The published verdicts (amendment 3 and this report) must match the data.
assert(!V.v1.go && !V.v1.t5 && V.v1.t1 && V.v1.t2 && V.v1.t3 && V.v1.t4 && V.v1.oracle, "v1 must be NO-GO on threshold 5 only");
assert(V.v2.go, "v2 must be GO");
for (const v of ["v1", "v2"]) for (const [k, b] of Object.entries(V[v])) csv.push(["verdict", v, k, "pass", b ? 1 : 0, "", "", "", b ? "PASS" : "FAIL"]);

console.log(out.join("\n"));
if (csvOut) {
  // Measured source identity (run_identity.txt): v2 variant commit and base.
  const id = fs.readFileSync(D("run_identity.txt"), "utf8");
  const v2sha = id.match(/^v2 commit: ([0-9a-f]{40})\r?$/m)?.[1];
  const basesha = id.match(/^base: main ([0-9a-f]{40})/m)?.[1];
  assert(v2sha && basesha, "run_identity.txt: full SHAs");
  const head = `# commit_sha=${v2sha} base_sha=${basesha}\n`;
  fs.writeFileSync(csvOut, head + csv.map((r) => r.join(",")).join("\n") + "\n");
  console.error(`# wrote ${csvOut} (${csv.length - 1} rows)`);
}
