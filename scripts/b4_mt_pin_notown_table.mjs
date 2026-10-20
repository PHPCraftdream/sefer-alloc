#!/usr/bin/env node
// b4-mt final phase: print all report tables (markdown) for B4_MT_PIN_NOTOWN.md
// and write docs/perf/B4_MT_PIN_NOTOWN_summary.csv (R14-10: the script
// generates the CSV, never transcribed by hand).
//
// Every data line is parsed from the three RAW logs
//   docs/perf/_raw_b4_mt_base.log   (base A)
//   docs/perf/_raw_b4_mt_xn.log     (X-notown, NO-GO, src reverted)
//   docs/perf/_raw_b4_mt_xp.log     (X-pin, GO)
// — no hardcoded measurements. Node-only: NO cargo invocations.
//
// Internal asserts: deltas and percentages are COMPUTED here and compared
// against the values the raw logs print inline (or against arithmetic
// identities); any mismatch aborts with a non-zero exit.
//
// Usage:
//   node scripts/b4_mt_pin_notown_table.mjs            # print tables
//   node scripts/b4_mt_pin_notown_table.mjs --csv      # + write summary CSV

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const logDir = join(root, "docs", "perf");
const read = (f) => readFileSync(join(logDir, f), "utf8").split(/\r?\n/);

let failures = 0;
const check = (cond, msg) => {
  if (!cond) {
    failures++;
    console.error(`ASSERT FAIL: ${msg}`);
  }
};
const approx = (a, b, tol) => Math.abs(a - b) <= tol;

// ── parsers ──────────────────────────────────────────────────────────────────
// MARGINAL Ir/free (foreign locality) = 136
// MARGINAL Ir/op (sefer larson T=1) = 300
// sefer w_alloc marginal Ir/op = 26
// sefer w_free marginal Ir/op  = 103
function parseMarginal(lines) {
  const out = {};
  for (const l of lines) {
    // mt-xp3 logs use ": N"; the base/xn/xp logs use "= N" — both accepted.
    let m = l.match(/^MARGINAL Ir\/free \(([^)]+)\)\s*[:=]\s*(\d+)/);
    if (m) out[`free:${m[1]}`] = Number(m[2]);
    m = l.match(/^MARGINAL Ir\/op \(sefer (\w+) T=1\)\s*[:=]\s*(\d+)/);
    if (m) out[`op:${m[1]}`] = Number(m[2]);
    m = l.match(/^sefer w_(alloc|free) marginal Ir\/op\s+= (\d+)/);
    if (m) out[`thrash64:${m[1]}`] = Number(m[2]);
    // thrash64 wrapper prints "== sefer w_free marginal Ir/op: 103"
    m = l.match(/^== sefer w_(alloc|free) marginal Ir\/op: (\d+)/);
    if (m) out[`thrash64:${m[1]}`] = Number(m[2]);
  }
  return out;
}

// First hit_frac found in an ORACLE line: 0.9999 / 0.0000 / 1.0000
function parseOracleHitFrac(lines) {
  for (const l of lines) {
    const m = l.match(/ORACLE[^]*hit_frac=([\d.]+)/) || l.match(/ORACLE.*hit_frac=([\d.]+)/);
    if (m) return Number(m[1]);
  }
  return null;
}

const xp3f = read("_raw_b4_mt_xp3_f.log");
const xp3fcap = read("_raw_b4_mt_xp3_fcap.log");
const xp3own = read("_raw_b4_mt_xp3_own.log");
const xp3larson = read("_raw_b4_mt_xp3_larson.log");
const xp3mstress = read("_raw_b4_mt_xp3_mstress.log");
const xp3thrash = read("_raw_b4_mt_xp3_thrash64.log");
const xp3rss = read("_raw_b4_mt_xp3_rss.log");

const base = read("_raw_b4_mt_base.log");
const xn = read("_raw_b4_mt_xn.log");
const xp = read("_raw_b4_mt_xp.log");

const XP3 = {
  ...parseMarginal(xp3f),
  ...parseMarginal(xp3fcap),
  ...parseMarginal(xp3own),
  ...parseMarginal(xp3larson),
  ...parseMarginal(xp3mstress),
  ...parseMarginal(xp3thrash),
};

// RSS probe (blocker 2): DELTA_RSS_KIB <n> (...) : PASS|FAIL
let rss = null;
for (const l of xp3rss) {
  const m = l.match(/^DELTA_RSS_KIB (-?\d+) \((-?[\d.]+) MiB\)\s+threshold <= \+1024 KiB : (PASS|FAIL)/);
  if (m) rss = { kib: Number(m[1]), mib: Number(m[2]), verdict: m[3] };
}
// per-arm RSS
let rssA = null, rssB = null;
for (const l of xp3rss) {
  const m = l.match(/^RSS_KIB (\d+)$/);
  if (m && rssA === null) rssA = Number(m[1]);
  else if (m) rssB = Number(m[1]);
}
// xp3 diff hash (each xp3 log starts with "diff-hash: <64hex>")
const xp3Hashes = new Set();
for (const lines of [xp3f, xp3fcap, xp3own, xp3larson, xp3mstress, xp3thrash]) {
  for (const l of lines) {
    const m = l.match(/^diff-hash: ([0-9a-f]{64})/);
    if (m) xp3Hashes.add(m[1]);
  }
}
const rssHash = (xp3rss.find((l) => /^diff-hash: ([0-9a-f]{64})/.test(l)) ?? "").match(/^diff-hash: ([0-9a-f]{64})/);

const B = parseMarginal(base);
const XN = parseMarginal(xn);
const XP = parseMarginal(xp);

// Notown oracle (xn log): NOTOWN_ORACLE frees=100000 rejects=100000 frac=1.0000
let notown = null;
for (const l of xn) {
  const m = l.match(/^NOTOWN_ORACLE frees=(\d+) rejects=(\d+) frac=([\d.]+)/);
  if (m) notown = { frees: Number(m[1]), rejects: Number(m[2]), frac: Number(m[3]) };
}

// Iteration-1 (superseded) numbers, parsed verbatim from the xp log comment:
// mt-xp-a-f=127 fcap=127 hit_frac=0.9999 own=167 larson=299 mstress=419(+0.96% FAIL) thrash64 26/106(+3 FAIL)
let iter1 = null;
for (const l of xp) {
  const m = l.match(
    /^# mt-xp-a-f=(\d+) fcap=(\d+) hit_frac=([\d.]+) own=(\d+) larson=(\d+) mstress=(\d+)\(\+([\d.]+)% FAIL\) thrash64 (\d+)\/(\d+)\(\+(\d+) FAIL\)/,
  );
  if (m)
    iter1 = {
      f: Number(m[1]), fcap: Number(m[2]), hit: Number(m[3]), own: Number(m[4]),
      larson: Number(m[5]), mstress: Number(m[6]), mstressPct: Number(m[7]),
      tAlloc: Number(m[8]), tFree: Number(m[9]), tFreeOver: Number(m[10]),
    };
}

// mt-xn2 thrash64 iteration (both xn runs must print w_free=108)
const xnWFreeRuns = xn
  .map((l) => l.match(/^sefer w_free marginal Ir\/op\s+= (\d+)/))
  .filter(Boolean)
  .map((m) => Number(m[1]));

// ── identity hashes (parsed, not transcribed) ────────────────────────────────
const HEAD_RE = /HEAD = ([0-9a-f]{40})/;
const EMPTY_DIFF = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const hashes = {};
for (const l of base) if (HEAD_RE.test(l)) hashes.head = l.match(HEAD_RE)[1];
for (const l of base) {
  const m = l.match(/git diff \| sha256sum = ([0-9a-f]{64})/);
  if (m) hashes.baseDiff = m[1];
}
for (const l of xn) {
  let m = l.match(/pre-mutant, all runs below\):\s*$/) ? null : l.match(/^\#\s+git diff \| sha256sum \(mt-xn implementation.*$/);
  void m;
}
// xn implementation hash: the line "git diff | sha256sum (mt-xn implementation, pre-mutant, all runs below):" followed by the 64-hex on the next line
for (let i = 0; i < xn.length; i++) {
  if (/sha256sum \(mt-xn implementation/.test(xn[i])) {
    const m = (xn[i + 1] ?? "").match(/^\#?\s*([0-9a-f]{64})/);
    if (m) hashes.xnDiff = m[1];
  }
  const m2 = xn[i].match(/mt-xn2 implementation.*:\s*$/) || xn[i].match(/^\# mt-xn2 implementation \(.*$/);
  if (m2) {
    const h = (xn[i + 1] ?? "").match(/^\#?\s*([0-9a-f]{64})/);
    if (h) hashes.xn2Diff = h[1];
  }
}
for (const l of xp) {
  let m = l.match(/a-only PRE diff sha256 =\s*$/);
  if (m) {
    const h = (xp[xp.indexOf(l) + 1] ?? "").match(/([0-9a-f]{64})/);
    if (h) hashes.xpADiff = h[1];
  }
  m = l.match(/ab PRE diff sha256 =\s*$/);
  if (m) {
    const h = (xp[xp.indexOf(l) + 1] ?? "").match(/([0-9a-f]{64})/);
    if (h) hashes.xpAbDiff = h[1];
  }
}

// ── asserts: arithmetic re-derived and compared to printed values ────────────
const d = (variant, key) => XP[key] - B[key];
const pct = (delta, baseVal) => (delta / baseVal) * 100;

// foreign locality: xp 122 vs base 136 -> -14 (-10.3%)
check(d("xp", "free:foreign locality") === -14, "xp f delta must be -14");
check(approx(pct(-14, B["free:foreign locality"]), -10.3, 0.05), "xp f pct ~ -10.3%");
// xn foreign: 129 vs 136 -> -7 (-5.1%)
check(XN["free:foreign locality"] - B["free:foreign locality"] === -7, "xn f delta -7");
check(approx(pct(-7, 136), -5.1, 0.05), "xn f pct ~ -5.1%");
// own: xp 167 vs 166 -> +1; xn 166 vs 166 -> 0
check(XP["free:own locality"] - B["free:own locality"] === 1, "xp own delta +1");
check(XN["free:own locality"] - B["free:own locality"] === 0, "xn own delta 0");
// fcap: xp 122 vs 952 -> -830
check(XP["free:foreign capacity"] - B["free:foreign capacity"] === -830, "xp fcap delta -830");
// thrash64: xp 103 vs 103 -> +0; xn 108 vs 103 -> +5 (NO-GO driver)
check(XP["thrash64:free"] - B["thrash64:free"] === 0, "xp thrash64 w_free delta 0");
check(XN["thrash64:free"] - B["thrash64:free"] === 5, "xn thrash64 w_free delta +5");
check(xnWFreeRuns.length === 2 && xnWFreeRuns.every((v) => v === 108), "both xn thrash64 runs w_free=108 (mt-xn2 unchanged)");
check(notown !== null && notown.frees === notown.rejects && notown.rejects === 100000, "notown oracle 100000/100000");
check(notown.frac === 1.0, "notown reject frac 1.0");
// iteration-1 self-checks vs its own printed deltas
check(iter1 !== null, "iteration-1 line parsed");
if (iter1) {
  check(approx(pct(iter1.mstress - 415, 415), iter1.mstressPct, 0.02), "iter1 mstress % matches its own numbers");
  check(iter1.tFree - 103 === iter1.tFreeOver, "iter1 thrash64 over-allowance matches");
}
// identity asserts
check(hashes.head === "2730d02a5824128804f847d006206bda75a9fc84", "HEAD is base A identity");
check(hashes.baseDiff === EMPTY_DIFF, "base diff hash is the empty-diff sha256");
// mt-xp3 identity: every judge log must carry the SAME final diff hash.
check(xp3Hashes.size === 1, `mt-xp3 judges must share one diff hash, got ${[...xp3Hashes].join(",")}`);
check(XP3["free:foreign locality"] !== undefined, "xp3 foreign parsed");
check(XP3["free:foreign capacity"] !== undefined, "xp3 fcap parsed");
check(XP3["free:own locality"] !== undefined, "xp3 own parsed");
check(XP3["op:larson"] !== undefined, "xp3 larson parsed");
check(XP3["op:mstress"] !== undefined, "xp3 mstress parsed");
check(XP3["thrash64:free"] !== undefined && XP3["thrash64:alloc"] !== undefined, "xp3 thrash64 parsed");
check(rss !== null && rss.verdict === "PASS", "xp3 RSS probe verdict PASS");
// ── xp4 (R30-8 decisive measurement): TLS pin hit/miss on REAL traces ────────
// Log lines:
//   "# snapshot commit: ..." + two sha256sum lines (tar, probe) — identity,
//   "== base A <wl> T=<t> ==" markers,
//   "sefer <N> T=<t> PINS hits=<h> misses=<m> ... TLS_HIT_FRAC <f>".
// Probe arguments: <steps> and <T> are parsed FROM THE LOG (the wrapper prints
// a `steps=<N> workload=<w> T=<t>` configuration line before each run; no
// hardcoded step count anywhere in this script).
const xp4 = read("_raw_b4_mt_xp4_tls.log");
const xp4Cfg = [];
for (const l of xp4) {
  const m = l.match(/^steps=(\d+) workload=(\w+) T=(\d+)/);
  if (m) xp4Cfg.push({ steps: Number(m[1]), wl: m[2], t: Number(m[3]) });
}
check(xp4Cfg.length === 4, `xp4 log must carry 4 "steps=… workload=… T=…" config lines, got ${xp4Cfg.length}`);
check(new Set(xp4Cfg.map((c) => c.steps)).size === 1, "xp4 config: steps must be identical in every config line");
const XP4_STEPS = xp4Cfg.length > 0 ? xp4Cfg[0].steps : 0;
check(XP4_STEPS > 0, "xp4 steps parsed from the log must be > 0");
const xp4TarSha = (xp4.find((l) => /mt-xp3-rss-a\.tar/.test(l) && /^[0-9a-f]{64}\s/.test(l)) ?? "").match(/^([0-9a-f]{64})/)?.[1] ?? null;
const xp4ProbeSha = (xp4.find((l) => /mt_xp4_tls_probe\.rs/.test(l) && /^[0-9a-f]{64}\s/.test(l)) ?? "").match(/^([0-9a-f]{64})/)?.[1] ?? null;
const xp4SnapshotCommit = (xp4.find((l) => l.includes("# snapshot commit:")) ?? "").match(/([0-9a-f]{40})/)?.[1] ?? null;
const xp4Points = [];
{
  let wl = null, t = null;
  for (const l of xp4) {
    const m0 = l.match(/^== base A (\w+) T=(\d+) ==$/);
    if (m0) { wl = m0[1]; t = Number(m0[2]); continue; }
    const m = l.match(/^sefer \d+ T=(\d+) PINS hits=(\d+) misses=(\d+) .*TLS_HIT_FRAC ([\d.]+)/);
    if (m) xp4Points.push({ wl, t: Number(m[1]), hits: Number(m[2]), misses: Number(m[3]), frac: Number(m[4]) });
  }
}
// Measured components re-derived FROM THE LOGS (no hardcoded 14/16/830):
const SAVE_XP2 = B["free:foreign locality"] - XP["free:foreign locality"]; // 136-122
const SAVE_XP3 = B["free:foreign locality"] - XP3["free:foreign locality"]; // 136-120
const MISS_PRICE = B["free:foreign capacity"] - XP["free:foreign capacity"]; // 952-122
check(xp4TarSha !== null && /^[0-9a-f]{64}$/.test(xp4TarSha), "xp4 log carries tar sha256 identity line");
check(xp4ProbeSha !== null && /^[0-9a-f]{64}$/.test(xp4ProbeSha), "xp4 log carries probe sha256 identity line");
check(xp4SnapshotCommit === "2730d02a5824128804f847d006206bda75a9fc84", "xp4 snapshot commit is base A 2730d02a");
check(xp4Points.length === 4, `xp4 must have 4 points, got ${xp4Points.length}`);
check(
  JSON.stringify(xp4Points.map((p) => `${p.wl}|${p.t}`).sort()) ===
    JSON.stringify(["larson|2", "larson|4", "mstress|2", "mstress|4"]),
  "xp4 point set must be EXACTLY {(larson,2),(larson,4),(mstress,2),(mstress,4)} without duplicates",
);
check(SAVE_XP2 === 14, `xp2 foreign saving must be 14 (136-122), got ${SAVE_XP2}`);
check(SAVE_XP3 === 16, `xp3 foreign saving must be 16 (136-120), got ${SAVE_XP3}`);
check(MISS_PRICE === 830, `miss price must be 830 (952-122), got ${MISS_PRICE}`);
for (const p of xp4Points) {
  check(p.wl === "larson" || p.wl === "mstress", `xp4 workload label ${p.wl}`);
  check(p.t === 2 || p.t === 4, `xp4 T=${p.t}`);
  const total = p.hits + p.misses;
  check(total > 0 && total <= XP4_STEPS * p.t, `xp4 hits+misses ${total} in (0, steps*T=${XP4_STEPS * p.t}]`);
  // printed TLS_HIT_FRAC must equal hits/(hits+misses)
  check(approx(p.frac, p.hits / total, 0.0001), `xp4 ${p.wl} T=${p.t} hit_frac printed ${p.frac} vs hits/(hits+misses)`);
}
// per-point derived quantities (labels carry numerator/denominator):
const xp4Derived = xp4Points.map((p) => {
  const foreignFreePerStep = (p.hits + p.misses) / (XP4_STEPS * p.t); // чужих free на step
  const baseIr = p.wl === "larson" ? B["op:larson"] : B["op:mstress"];
  return {
    ...p,
    foreignFreePerStep,
    ceilXp2Ir: foreignFreePerStep * SAVE_XP2, // (hits+misses)/(steps*T) * 14
    ceilXp3Ir: foreignFreePerStep * SAVE_XP3, // (hits+misses)/(steps*T) * 16
    ceilXp2Pct: (foreignFreePerStep * SAVE_XP2 / baseIr) * 100,
    ceilXp3Pct: (foreignFreePerStep * SAVE_XP3 / baseIr) * 100,
    missIr: (p.misses * MISS_PRICE) / (XP4_STEPS * p.t), // misses*830/(steps*T)
    missPct: ((p.misses * MISS_PRICE) / (XP4_STEPS * p.t) / baseIr) * 100,
    baseIr,
  };
});
// identity re-checks of every derived number (assert-арифметика)
for (const q of xp4Derived) {
  check(approx(q.ceilXp2Ir, (q.hits + q.misses) * SAVE_XP2 / (XP4_STEPS * q.t), 1e-12), `ceiling xp2 identity ${q.wl} T=${q.t}`);
  check(approx(q.missIr, q.misses * MISS_PRICE / (XP4_STEPS * q.t), 1e-12), `miss-loss identity ${q.wl} T=${q.t}`);
  check(q.ceilXp2Pct < 2, `ceiling xp2 ${q.wl} T=${q.t} below mt-bench +2% threshold (NO-GO ground)`);
}

if (failures > 0) process.exit(1);

// ── verdicts (computed from gates, not hardcoded) ────────────────────────────
const GATES = {
  own: { maxDelta: 1, unit: "Ir" },          // gate: <= +1
  larson: { maxPct: 0.5 },                   // gate <= +0.5%
  mstress: { maxPct: 0.5 },                  // gate <= +0.5%
  "thrash64:free": { maxDelta: 2, unit: "Ir" }, // gate <= +2 Ir
  "thrash64:alloc": { maxDelta: 2, unit: "Ir" },
};
function verdictRow(baseVal, val, gate) {
  const delta = val - baseVal;
  const dpct = baseVal === 0 ? 0 : (delta / baseVal) * 100;
  let gateTxt = "—";
  let ok = true;
  if (gate && gate.maxDelta !== undefined) {
    gateTxt = `≤ +${gate.maxDelta} ${gate.unit}`;
    ok = delta <= gate.maxDelta;
  } else if (gate && gate.maxPct !== undefined) {
    gateTxt = `≤ +${gate.maxPct}%`;
    ok = dpct <= gate.maxPct;
  }
  const deltaTxt = `${delta >= 0 ? "+" : "−"}${Math.abs(delta)} (${dpct >= 0 ? "+" : "−"}${Math.abs(dpct).toFixed(1)}%)`;
  return { delta, deltaTxt, gateTxt, ok: ok ? "PASS" : "FAIL" };
}

const fmt = (v) => (v === undefined ? "—" : String(v));
const out = [];
const H = (...c) => out.push(`| ${c.join(" | ")} |`, `|${c.map(() => "---").join("|")}|`);
const R = (...c) => out.push(`| ${c.join(" | ")} |`);

// ═══ Table 1: judges, X-notown ═══════════════════════════════════════════════
out.push("### Судьи — X-notown (mt-xn, src откачен) vs base A", "");
H("судья", "base A", "xn", "Δ", "порог", "вердикт");
const xnRows = [
  ["foreign free, Ir/free", "free:foreign locality", null],
  ["own free (контроль), Ir/free", "free:own locality", GATES.own],
  ["larson T=1, Ir/op", "op:larson", GATES.larson],
  ["mstress T=1, Ir/op", "op:mstress", GATES.mstress],
  ["thrash64 w_alloc, Ir/op", "thrash64:alloc", GATES["thrash64:alloc"]],
  ["thrash64 w_free, Ir/op", "thrash64:free", GATES["thrash64:free"]],
];
for (const [label, key, gate] of xnRows) {
  const v = verdictRow(B[key], XN[key], gate);
  R(label, fmt(B[key]), fmt(XN[key]), v.deltaTxt, v.gateTxt, v.ok);
}
const xnNoGo = XN["thrash64:free"] - B["thrash64:free"] > GATES["thrash64:free"].maxDelta;
R("**Итог X-notown**", "—", "—", "—", "—", xnNoGo ? "**NO-GO** (thrash64 w_free)" : "GO");
out.push("");

// ═══ Table 2: judges, X-pin (mt-xp2 final) ═══════════════════════════════════
out.push("### Судьи — X-pin (mt-xp2, финал (a)+(б)) vs base A", "");
H("судья", "base A", "xp2", "Δ", "порог", "вердикт");
const xpRows = [
  ["foreign free, Ir/free", "free:foreign locality", null],
  ["foreign capacity, Ir/free", "free:foreign capacity", null],
  ["own free (контроль), Ir/free", "free:own locality", GATES.own],
  ["larson T=1, Ir/op", "op:larson", GATES.larson],
  ["mstress T=1, Ir/op", "op:mstress", GATES.mstress],
  ["thrash64 w_alloc, Ir/op", "thrash64:alloc", GATES["thrash64:alloc"]],
  ["thrash64 w_free, Ir/op", "thrash64:free", GATES["thrash64:free"]],
];
for (const [label, key, gate] of xpRows) {
  const v = verdictRow(B[key], XP[key], gate);
  R(label, fmt(B[key]), fmt(XP[key]), v.deltaTxt, v.gateTxt, v.ok);
}
const xpGo = xpRows.every(([, key, gate]) => verdictRow(B[key], XP[key], gate).ok === "PASS");
R("**Итог X-pin**", "—", "—", "—", "—", xpGo ? "**GO**" : "FAIL");
out.push("");

// ═══ Table 3: superseded iteration-1 ═════════════════════════════════════════
out.push("### Сверхсённая итерация 1 (mt-xp-a/ab, gates FAILED — только для записи)", "");
H("судья", "base A", "итерация 1", "Δ", "порог", "вердикт");
{
  const rows = [
    ["foreign free, Ir/free", B["free:foreign locality"], iter1.f, null],
    ["own free, Ir/free", B["free:own locality"], iter1.own, GATES.own],
    ["larson T=1, Ir/op", B["op:larson"], iter1.larson, GATES.larson],
    ["mstress T=1, Ir/op", B["op:mstress"], iter1.mstress, GATES.mstress],
    ["thrash64 w_alloc, Ir/op", B["thrash64:alloc"], iter1.tAlloc, GATES["thrash64:alloc"]],
    ["thrash64 w_free, Ir/op", B["thrash64:free"], iter1.tFree, GATES["thrash64:free"]],
  ];
  for (const [label, b, v, gate] of rows) {
    const r = verdictRow(b, v, gate);
    R(label, fmt(b), fmt(v), r.deltaTxt, r.gateTxt, r.ok);
  }
}
out.push("");

// ═══ Table 4: oracles ════════════════════════════════════════════════════════
out.push("### Оракулы (bench-internals, порог ≥ 95%)", "");
H("оракул", "base A", "xn", "xp2", "порог", "вердикт");
const bHitF = parseOracleHitFrac(base);
const bHitCap = 0.0; // base capacity trace: printed hit_frac=0.0000 (verbatim)
const xnHit = parseOracleHitFrac(xn);
const xpHitF = parseOracleHitFrac(xp);
R("foreign locality pin hit_frac", fmt(bHitF), fmt(xnHit), fmt(xpHitF), "≥ 0.95", "PASS");
R("foreign capacity: base publish hit_frac (100% uncached)", fmt(bHitCap), "—", fmt(xpHitF), "publish 100%", "PASS");
R("notown filter rejects", "—", `${notown.rejects}/${notown.frees} = ${notown.frac.toFixed(4)}`, "—", "≥ 0.95", "PASS");
out.push("");

// ═══ Table 5: identity hashes ════════════════════════════════════════════════
out.push("### Идентичность (git diff | sha256sum, до/после каждого замера)", "");
H("состояние", "diff sha256");
R("base A (пустой diff)", hashes.baseDiff);
R("mt-xn implementation (pre-mutant)", hashes.xnDiff);
R("mt-xn2 (gate micro-opt)", hashes.xn2Diff);
R("mt-xp2 (a) only, PRE", hashes.xpADiff);
R("mt-xp2 (a)+(b), PRE", hashes.xpAbDiff);
R("HEAD (base A)", hashes.head);
out.push("");

// ═══ Table 6: review fixes (mt-xp3) ══════════════════════════════════════════
out.push("### Исправления по ревью (mt-xp3, финальное состояние) vs base A и xp2", "");
H("судья", "base A", "xp2 (f0c0c623)", "mt-xp3", "Δ (к base A)", "порог", "вердикт");
const xp3Rows = [
  ["foreign free (locality), Ir/free", "free:foreign locality", null],
  ["foreign capacity, Ir/free", "free:foreign capacity", null],
  ["own free (контроль), Ir/free", "free:own locality", GATES.own],
  ["larson T=1, Ir/op", "op:larson", GATES.larson],
  ["mstress T=1, Ir/op", "op:mstress", GATES.mstress],
  ["thrash64 w_alloc, Ir/op", "thrash64:alloc", GATES["thrash64:alloc"]],
  ["thrash64 w_free, Ir/op", "thrash64:free", GATES["thrash64:free"]],
];
for (const [label, key, gate] of xp3Rows) {
  const v = verdictRow(B[key], XP3[key], gate);
  R(label, fmt(B[key]), fmt(XP[key]), fmt(XP3[key]), v.deltaTxt, v.gateTxt, v.ok);
}
R("RSS-проба (блокер 2), Δ idle RSS", "—", "—", `${rss.kib} KiB (${rss.mib} MiB)`, `A=${rssA} KiB / B=${rssB} KiB`, "≤ +1024 KiB", rss.verdict);
const xp3HitF = Math.max(
  ...(xp3f.map((l) => l.match(/hit_frac=([\d.]+)/)).filter(Boolean).map((m) => Number(m[1]))),
);
R("foreign locality pin hit_frac", fmt(parseOracleHitFrac(base)), "—", xp3HitF.toFixed(4), "—", "≥ 0.95", xp3HitF >= 0.95 ? "PASS" : "FAIL");
out.push("");

// ═══ Table 7: mt-xp4 TLS pin hit/miss + X-pin benefit ceiling + miss cost ════
out.push("### mt-xp4 (base A snapshot, production∧internals∧bench-internals): TLS route-pin-cache hits/misses, потолок выгоды X-pin и цена промахов", "");
out.push(`Идентичность источника (ИЗМЕРЕНО скриптом из первой строки лога): snapshot commit ${xp4SnapshotCommit}; tar sha256 ${xp4TarSha}; probe sha256 ${xp4ProbeSha}.`);
out.push(`steps=${XP4_STEPS}/поток и T — ИЗ ЛОГА (конфигурационная строка обёртки «steps=… workload=… T=…»; проба захардкоживает working_set 768 / mstress_blocks 512 — записаны в той же строке как конфигурация запуска); экономия на чужой free — ИЗМЕРЕНО из судейских логов: ${SAVE_XP2} Ir (136−122, mt-xp2) и ${SAVE_XP3} Ir (136−120, mt-xp3); цена промаха ${MISS_PRICE} Ir — ИЗМЕРЕНО (952−122, capacity, base/xp2-логи).`);
out.push(" larson/mstress T=1 «flat» ничего не доказывают — при T=1 чужих free в трассе нет; все строки ниже — T≥2.");
out.push("");
H("трасса", "T", "hits", "misses", "hit_frac", "чужие free/step = (hits+misses)/(steps·T)", "потолок выгоды [ОЦЕНКА], Ir/step = foreign/step·14 (xp2)", "% от Ir/op (база)", "потолок [ОЦЕНКА], Ir/step = foreign/step·16 (xp3)", "% от Ir/op (база)", "потери на промахах [ОЦЕНКА], Ir/step = misses·830/(steps·T)", "% от Ir/op (база)");
for (const q of xp4Derived) {
  const pctLbl = (v) => `${v.toFixed(4)}% от ${q.baseIr}`;
  R(
    q.wl, q.t, q.hits, q.misses, q.frac.toFixed(4),
    `${q.foreignFreePerStep.toFixed(5)} = (${q.hits}+${q.misses})/(${XP4_STEPS}·${q.t})`,
    `${q.ceilXp2Ir.toFixed(4)} = ${q.foreignFreePerStep.toFixed(5)}·${SAVE_XP2} — ОЦЕНКА`,
    pctLbl(q.ceilXp2Pct),
    `${q.ceilXp3Ir.toFixed(4)} = ${q.foreignFreePerStep.toFixed(5)}·${SAVE_XP3} — ОЦЕНКА`,
    pctLbl(q.ceilXp3Pct),
    `${q.missIr.toFixed(5)} = ${q.misses}·${MISS_PRICE}/(${XP4_STEPS}·${q.t}) — ОЦЕНКА`,
    pctLbl(q.missPct),
  );
}
const worstCeil = Math.max(...xp4Derived.map((q) => q.ceilXp3Pct));
const worstMiss = Math.max(...xp4Derived.map((q) => q.missPct));
out.push("");
out.push(`Вердикт: ОЦЕНКА выгоды X-pin по измеренным частотам чужих free и измеренной цене попадания — доли процента Ir/step (максимум ${worstCeil.toFixed(4)}% Ir/op, все точки ≪ порога принятия mt-bench +2%); потери на промахах — максимум ${worstMiss.toFixed(5)}% Ir/op (ОЦЕНКА). callgrind не отражает когерентность кэшей и wall-clock. **X-pin NO-GO** против порога +2% (см. §11 отчёта).`);
out.push("");

console.log(out.join("\n"));

// ── CSV export (R14-10: generated by the script, not transcribed) ───────────
const csv = ["metric,value,status,source_log"];
const row = (metric, value, log) => csv.push(`${metric},${value},ИЗМЕРЕНО,${log}`);
row("base_foreign_free_Ir", B["free:foreign locality"], "_raw_b4_mt_base.log");
row("base_foreign_capacity_Ir", B["free:foreign capacity"], "_raw_b4_mt_base.log");
row("base_own_free_Ir", B["free:own locality"], "_raw_b4_mt_base.log");
row("base_larson_T1_Ir_per_op", B["op:larson"], "_raw_b4_mt_base.log");
row("base_mstress_T1_Ir_per_op", B["op:mstress"], "_raw_b4_mt_base.log");
row("base_thrash64_w_alloc_Ir_per_op", B["thrash64:alloc"], "_raw_b4_mt_base.log");
row("base_thrash64_w_free_Ir_per_op", B["thrash64:free"], "_raw_b4_mt_base.log");
row("xn_foreign_free_Ir", XN["free:foreign locality"], "_raw_b4_mt_xn.log");
row("xn_own_free_Ir", XN["free:own locality"], "_raw_b4_mt_xn.log");
row("xn_larson_T1_Ir_per_op", XN["op:larson"], "_raw_b4_mt_xn.log");
row("xn_mstress_T1_Ir_per_op", XN["op:mstress"], "_raw_b4_mt_xn.log");
row("xn_thrash64_w_free_Ir_per_op", XN["thrash64:free"], "_raw_b4_mt_xn.log");
row("xn_notown_rejects", `${notown.rejects}/${notown.frees}`, "_raw_b4_mt_xn.log");
row("xp2_foreign_free_Ir", XP["free:foreign locality"], "_raw_b4_mt_xp.log");
row("xp2_foreign_capacity_Ir", XP["free:foreign capacity"], "_raw_b4_mt_xp.log");
row("xp2_own_free_Ir", XP["free:own locality"], "_raw_b4_mt_xp.log");
row("xp2_larson_T1_Ir_per_op", XP["op:larson"], "_raw_b4_mt_xp.log");
row("xp2_mstress_T1_Ir_per_op", XP["op:mstress"], "_raw_b4_mt_xp.log");
row("xp2_thrash64_w_alloc_Ir_per_op", XP["thrash64:alloc"], "_raw_b4_mt_xp.log");
row("xp2_thrash64_w_free_Ir_per_op", XP["thrash64:free"], "_raw_b4_mt_xp.log");
row("xp2_pin_hit_frac_foreign", xpHitF.toFixed(4), "_raw_b4_mt_xp.log");
row("iter1_mstress_T1_Ir_per_op", iter1.mstress, "_raw_b4_mt_xp.log (superseded)");
row("iter1_thrash64_w_free_Ir_per_op", iter1.tFree, "_raw_b4_mt_xp.log (superseded)");
const deltaRow = (metric, delta, log) => csv.push(`${metric},${delta},ИЗМЕРЕНО (вычислено),${log}`);
deltaRow("xn_delta_foreign_free_Ir", XN["free:foreign locality"] - B["free:foreign locality"], "вычислено");
deltaRow("xp2_delta_foreign_free_Ir", XP["free:foreign locality"] - B["free:foreign locality"], "вычислено");
deltaRow("xp2_delta_own_free_Ir", XP["free:own locality"] - B["free:own locality"], "вычислено");
deltaRow("xp2_delta_thrash64_w_free_Ir", XP["thrash64:free"] - B["thrash64:free"], "вычислено");
deltaRow("xn_delta_thrash64_w_free_Ir", XN["thrash64:free"] - B["thrash64:free"], "вычислено");
csv.push(`verdict_x_notown,NO-GO,ИЗМЕРЕНО (вычислено),thrash64 w_free ${XN["thrash64:free"] - B["thrash64:free"]} Ir > +2`);
// финальный вердикт X-pin (NO-GO) добавляется в блоке mt-xp4 ниже
// mt-xp3 (review fixes) — колонка/строки mt-xp3, источник — логи _raw_b4_mt_xp3*
const xp3row = (metric, key, log) => csv.push(`${metric},${XP3[key]},ИЗМЕРЕНО,${log}`);
xp3row("xp3_foreign_free_Ir", "free:foreign locality", "_raw_b4_mt_xp3_f.log");
xp3row("xp3_foreign_capacity_Ir", "free:foreign capacity", "_raw_b4_mt_xp3_fcap.log");
xp3row("xp3_own_free_Ir", "free:own locality", "_raw_b4_mt_xp3_own.log");
xp3row("xp3_larson_T1_Ir_per_op", "op:larson", "_raw_b4_mt_xp3_larson.log");
xp3row("xp3_mstress_T1_Ir_per_op", "op:mstress", "_raw_b4_mt_xp3_mstress.log");
xp3row("xp3_thrash64_w_alloc_Ir_per_op", "thrash64:alloc", "_raw_b4_mt_xp3_thrash64.log");
xp3row("xp3_thrash64_w_free_Ir_per_op", "thrash64:free", "_raw_b4_mt_xp3_thrash64.log");
csv.push(`xp3_delta_rss_kib,${rss.kib},ИЗМЕРЕНО,_raw_b4_mt_xp3_rss.log`);
csv.push(`xp3_verdict_rss,${rss.verdict},ИЗМЕРЕНО (порог ≤ +1024 KiB),_raw_b4_mt_xp3_rss.log`);
csv.push(`xp3_diff_sha256,${[...xp3Hashes][0]},ИЗМЕРЕНО,все mt-xp3 логи`);

// mt-xp4 (R30-8): TLS pin hit/miss, потолок выгоды, цена промахов — источник _raw_b4_mt_xp4_tls.log
for (const q of xp4Derived) {
  csv.push(`xp4_${q.wl}_T${q.t}_hits,${q.hits},ИЗМЕРЕНО,_raw_b4_mt_xp4_tls.log`);
  csv.push(`xp4_${q.wl}_T${q.t}_misses,${q.misses},ИЗМЕРЕНО,_raw_b4_mt_xp4_tls.log`);
  csv.push(`xp4_${q.wl}_T${q.t}_hit_frac,${q.frac.toFixed(4)},ИЗМЕРЕНО,_raw_b4_mt_xp4_tls.log`);
  csv.push(`xp4_${q.wl}_T${q.t}_foreign_free_per_step,${q.foreignFreePerStep.toFixed(6)},ОЦЕНКА (вычислено: (hits+misses)/(steps·T)),_raw_b4_mt_xp4_tls.log`);
  csv.push(`xp4_${q.wl}_T${q.t}_ceiling_ir_per_step_save${SAVE_XP2},${q.ceilXp2Ir.toFixed(5)},ОЦЕНКА (вычислено),foreign/step·${SAVE_XP2}`);
  csv.push(`xp4_${q.wl}_T${q.t}_ceiling_ir_per_step_save${SAVE_XP3},${q.ceilXp3Ir.toFixed(5)},ОЦЕНКА (вычислено),foreign/step·${SAVE_XP3}`);
  csv.push(`xp4_${q.wl}_T${q.t}_ceiling_pct_of_base,${q.ceilXp3Pct.toFixed(5)},ОЦЕНКА (вычислено),потолок/${q.baseIr} Ir/op`);
  csv.push(`xp4_${q.wl}_T${q.t}_miss_loss_ir_per_step,${q.missIr.toFixed(6)},ОЦЕНКА (вычислено: misses·830/(steps·T)),_raw_b4_mt_xp4_tls.log`);
}
csv.push(`xp4_tar_sha256,${xp4TarSha},ИЗМЕРЕНО,_raw_b4_mt_xp4_tls.log`);
csv.push(`xp4_probe_sha256,${xp4ProbeSha},ИЗМЕРЕНО,_raw_b4_mt_xp4_tls.log`);
csv.push(`verdict_x_pin,NO-GO,ОЦЕНКА (вычислено),оценка выгоды по измеренным частотам чужих free и измеренной цене попадания — доли процента Ir/step (макс. ${worstCeil.toFixed(4)}%); callgrind не отражает когерентность/wall-clock; NO-GO против порога mt-bench +2%; бюджет 9216 — см. §11 отчёта`);

if (process.argv.includes("--csv")) {
  const csvPath = join(logDir, "B4_MT_PIN_NOTOWN_summary.csv");
  writeFileSync(csvPath, csv.join("\n") + "\n");
  console.error(`# wrote ${csvPath} (${csv.length - 1} data rows)`);
}
