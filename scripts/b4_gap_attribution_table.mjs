#!/usr/bin/env node
// b4_gap_attribution_table.mjs — v2 (chunk R3, b4-gap): B4 gap attribution table
// (sefer vs mimalloc, time gap vs callgrind toggle-window model).
//
// HONEST PROVENANCE NOTE: the classification thresholds below (0.1 and 0.7) were
// fixed by the owner/orchestrator BEFORE any measurement. This script (v2) was
// WRITTEN AFTER the first (full-process-window) measurements of the previous
// iteration; iteration history is documented in docs/perf/B4_GAP_ATTRIBUTION.md.
// The v1 full-process methodology was rejected in review; v2 uses toggle windows
// inside gp_kernel_probe::kernel_* only.
//
// Input (CLI args only, NO compilation, NO benchmarking):
//   $1 docs/perf/B4_INTERLEAVE_JUDGE_summary.csv        (judge J1', Windows)
//   $2 gp_r2_marginals_per_op.csv                        (callgrind toggle, WSL, per-op marginals p80-p40)
//   $3 gp_r2_sefer_events_per_op.csv                     (bench-internals shim events)
//   $4 gp_r2_sefer_calls_per_op.csv                      (callgrind calls= in toggle windows)
//   $5 gp_r2_func_breakdown.csv                          (callgrind per-function self Ir)
//   $6 [optional] gp_callgrind_marginals_per_op.csv      (v1 full-process, comparison only)
//
// EstCycles is PRIMARY-computed here from the marginal columns:
//   EstCycles = Ir + 10*(D1mr+D1mw+I1mr) + 100*(DLmr+DLmw+ILmr) + 10*Bcm.
// Classification criteria — fixed by the owner BEFORE the runs. DO NOT change:
//   time_ratio = exp(ab_median) (judge; mi/sefer);
//   ir_ratio    = per-op Ir(mi)/Ir(sefer); est_ratio = EstCycles(mi)/EstCycles(sefer);
//   class "instructions": |ln(est_ratio) - ln(time_ratio)| <= 0.1 AND |ln(ir_ratio)| >= 0.7*|ln(time_ratio)|;
//   class "memory":       |ln(est_ratio) - ln(time_ratio)| <= 0.1 AND |ln(ir_ratio)| <  0.7*|ln(time_ratio)|;
//   otherwise             "not_explained".
// Guards: |ln(time_ratio)| < 1e-9 or ir_ratio <= 0 -> hard error, never classify.
import { readFileSync, writeFileSync } from 'node:fs';

const args = process.argv.slice(2);
if (args.length !== 5 && args.length !== 6) {
  console.error('usage: node scripts/b4_gap_attribution_table.mjs <judge_summary.csv> <r2_marginals_per_op.csv> <r2_sefer_events_per_op.csv> <r2_sefer_calls_per_op.csv> <r2_func_breakdown.csv> [v1_callgrind_marginals_per_op.csv]');
  process.exit(2);
}
const [judgePath, margPath, evPath, callsPath, fnPath, v1Path] = args;

function parseCsv(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0);
  let headerIdx = 0;
  // identity first line ("# identity: ...") is metadata, header follows
  while (headerIdx < lines.length && lines[headerIdx].startsWith('#')) headerIdx++;
  if (headerIdx >= lines.length) throw new Error('CSV has no header row');
  const header = lines[headerIdx].split(',').map((h) => h.trim());
  const rows = [];
  for (const l of lines.slice(headerIdx + 1)) rows.push(splitCsvLine(l));
  return rows.map((cells) => {
    const row = {};
    header.forEach((h, i) => (row[h] = (cells[i] ?? '').trim()));
    return row;
  });
}
// RFC-4180-ish single-line CSV split (handles quoted fields with commas).
function splitCsvLine(line) {
  const out = [];
  let cur = '';
  let inQ = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQ) {
      if (ch === '"' && line[i + 1] === '"') { cur += '"'; i++; }
      else if (ch === '"') inQ = false;
      else cur += ch;
    } else if (ch === '"') inQ = true;
    else if (ch === ',') { out.push(cur); cur = ''; }
    else cur += ch;
  }
  out.push(cur);
  return out;
}
const num = (v) => {
  const x = Number(v);
  if (!Number.isFinite(x)) throw new Error(`non-finite number: ${JSON.stringify(v)}`);
  return x;
};
const assert = (cond, msg) => {
  if (!cond) throw new Error(`ASSERT FAILED: ${msg}`);
};

const judge = parseCsv(readFileSync(judgePath, 'utf8'));
const marg = parseCsv(readFileSync(margPath, 'utf8'));
const ev = parseCsv(readFileSync(evPath, 'utf8'));
const calls = parseCsv(readFileSync(callsPath, 'utf8'));
const fn = parseCsv(readFileSync(fnPath, 'utf8'));
const v1 = v1Path ? parseCsv(readFileSync(v1Path, 'utf8')) : null;

const KERNELS = ['pair16', 'pair64', 'larson', 'churn1024', 'churn256',
  'bulk64', 'churn64', 'churn16', 'bulk16k'];

// --- marginals: two arms (sefer, mi) per kernel, EstCycles recomputed -------
const perK = new Map(); // kernel -> { sefer, mi, ops }
{
  const seen = new Map();
  for (const r of marg) {
    const k = r.kernel;
    if (!perK.has(k)) {
      perK.set(k, { ops: num(r.ops_delta) });
      KERNELS.includes(k) || KERNELS.push(k);
    }
    const e = perK.get(k);
    assert(e[r.arm] === undefined, `duplicate arm ${r.arm} for kernel ${k} in marginals`);
    const c = {
      ir: num(r.ir_Ir),
      i1mr: num(r.ir_I1mr), d1mr: num(r.ir_D1mr), d1mw: num(r.ir_D1mw),
      ilmr: num(r.ir_ILmr), dlmr: num(r.ir_DLmr), dlmw: num(r.ir_DLmw),
      bcm: num(r.ir_Bcm),
    };
    // EstCycles is the primary calculation here (callgrind cost model).
    c.est = c.ir + 10 * (c.d1mr + c.d1mw + c.i1mr) + 100 * (c.dlmr + c.dlmw + c.ilmr) + 10 * c.bcm;
    e[r.arm] = {
      ...c,
      d1: c.d1mr + c.d1mw,
      ll: c.dlmr + c.dlmw + c.ilmr,
    };
    if (!seen.has(k)) seen.set(k, num(r.ops_delta));
    else assert(seen.get(k) === num(r.ops_delta), `ops_delta differs between arms for ${k}`);
  }
  assert(perK.size === 9, `expected 9 kernels in marginals, got ${perK.size}`);
  for (const k of KERNELS) {
    const e = perK.get(k);
    assert(e && e.sefer && e.mi, `kernel ${k} missing an arm in marginals`);
  }
}

// --- judge: one admitted row per kernel; time_ratio = exp(ab_median) --------
const judgeK = new Map();
for (const r of judge) {
  assert(!judgeK.has(r.kernel) || judgeK.get(r.kernel) === r, `duplicate kernel ${r.kernel} in judge CSV`);
  judgeK.set(r.kernel, r);
}
for (const k of KERNELS) {
  const r = judgeK.get(k);
  assert(r, `kernel ${k} missing from judge CSV`);
  assert(r.admitted === 'yes', `kernel ${k} not admitted by judge`);
  const tr = Math.exp(num(r.ab_median));
  assert(Math.abs(tr - num(r.ab_ratio)) < 1e-6, `ab_ratio column != exp(ab_median) for ${k}`);
}

// --- events: 9 kernels, ops_delta consistent; delta = per_op * ops ----------
const evK = new Map();
for (const r of ev) {
  assert(!evK.has(r.kernel), `duplicate kernel ${r.kernel} in events CSV`);
  evK.set(r.kernel, r);
}
assert(evK.size === 9, `expected 9 kernels in events CSV, got ${evK.size}`);
const EV_COLS = [
  'tcache_hits_total', 'large_cache_hits_total', 'tcache_and_large_cache_hits_total',
  'config_conflicts_total', 'heaps_claimed_high_water',
  'dbg_s1_refill_events', 'dbg_s1_mag_byte_writes',
];
for (const k of KERNELS) {
  const e = evK.get(k);
  assert(e, `kernel ${k} missing from events CSV`);
  assert(num(e.ops_delta) === perK.get(k).ops, `ops_delta mismatch (events vs marginals) for ${k}`);
  for (const c of EV_COLS) {
    const perOp = num(e[`per_op_${c}`]);
    const delta = num(e[`delta_${c}`]);
    const expect = perOp * num(e.ops_delta);
    // per_op columns are rounded to 6 decimals in the CSV: slop <= 0.5 + ops*5e-7
    const slop = 0.5 + 5e-7 * num(e.ops_delta);
    assert(Math.abs(delta - expect) <= slop,
      `events delta!=per_op*ops for ${k}/${c}: delta=${delta} expect=${expect} slop=${slop}`);
  }
}

// --- calls: calls_delta = calls80-calls40; calls_per_op*ops == calls_delta --
const callByK = new Map();
for (const k of KERNELS) callByK.set(k, []);
for (const r of calls) {
  assert(callByK.has(r.kernel), `unknown kernel ${r.kernel} in calls CSV`);
  callByK.get(r.kernel).push(r);
}
for (const k of KERNELS) {
  for (const r of callByK.get(k)) {
    const ops = num(r.ops_delta);
    assert(ops === perK.get(k).ops, `ops_delta mismatch (calls vs marginals) for ${k}`);
    const d = num(r.calls_delta);
    assert(Number.isInteger(d) && num(r.calls80) - num(r.calls40) === d,
      `calls_delta != calls80-calls40 for ${k} ${r.func}`);
    const perOp = num(r.calls_per_op);
    // calls_per_op printed with 6 decimals: slop <= 0.5 + ops*5e-7
    const slop = 0.5 + 5e-7 * ops;
    assert(Math.abs(perOp * ops - d) <= slop,
      `calls_per_op*ops != calls_delta for ${k} ${r.func}: ${perOp * ops} vs ${d} slop=${slop}`);
  }
}

// --- func breakdown: 9 kernels, sefer arm present ---------------------------
const fnByK = new Map();
for (const r of fn) {
  assert(KERNELS.includes(r.kernel), `unknown kernel ${r.kernel} in func breakdown`);
  assert(r.arm === 'sefer' || r.arm === 'mi', `unknown arm ${r.arm} in func breakdown`);
  if (!fnByK.has(`${r.kernel}|${r.arm}`)) fnByK.set(`${r.kernel}|${r.arm}`, []);
  fnByK.get(`${r.kernel}|${r.arm}`).push(r);
}
for (const k of KERNELS) {
  assert(fnByK.has(`${k}|sefer`), `func breakdown missing sefer rows for ${k}`);
  fnByK.get(`${k}|sefer`).sort((a, b) => num(b.self_ir_delta) - num(a.self_ir_delta));
}

// --- classification (thresholds fixed by owner BEFORE the runs; see header) -
const classify = (timeRatio, irRatio, estRatio) => {
  const lt = Math.log(timeRatio);
  assert(Math.abs(lt) >= 1e-9, `|ln(time_ratio)| < 1e-9 (judge A/A tie)`);
  assert(irRatio > 0, `ir_ratio <= 0`);
  assert(estRatio > 0, `est_ratio <= 0`);
  const dEst = Math.abs(Math.log(estRatio) - lt);
  const dIr = Math.abs(Math.log(irRatio));
  if (dEst <= 0.1 && dIr >= 0.7 * Math.abs(lt)) return 'instructions';
  if (dEst <= 0.1 && dIr < 0.7 * Math.abs(lt)) return 'memory/latency';
  return 'not_explained';
};

// --- short descriptions (interpretations are marked ОЦЕНКА in the report) ---
const DESC = {
  'sefer_a_alloc': 'probe shim: SHIM_CALLS fetch_add + SeferAlloc __rust_alloc face (ИЗМЕРЕНО)',
  'sefer_a_dealloc': 'probe shim: SHIM_CALLS fetch_add + __rust_dealloc face (ИЗМЕРЕНО)',
  'try_drain_freelist_batch': 'bulk-pop of segment free list into out-batch, per-block LIVE state-byte commit (ИЗМЕРЕНО)',
  'flush_class_internal': 'magazine flush: same-segment run grouping, push back on segment free list (ИЗМЕРЕНО)',
  'drain_large_sidecar_ingress_hot_bounded': 'large sidecar ingress drain (no dedicated bench-internals counter) (ОЦЕНКА)',
  'refill_magazine_slow': 'magazine refill slow path (calls= cross-checks dbg_s1_refill_events) (ИЗМЕРЕНО)',
  'try_carve_batch': 'bulk carve of segment free list batch (ИЗМЕРЕНО; small negative delta = annotation noise, см. ограничения)',
  'kernel_bulk': 'probe bulk loop (n allocs then n frees per pass) (ИЗМЕРЕНО)',
  'kernel_churn': 'probe ring-buffer churn loop (dealloc old, alloc new, N-byte write) (ИЗМЕРЕНО)',
  'kernel_pair': 'probe alloc+dealloc pair loop (ИЗМЕРЕНО)',
  'kernel_larson': 'probe larson loop (ИЗМЕРЕНО)',
  'main': 'harness/reporting loop, not allocator (ИЗМЕРЕНО)',
};
const descOf = (fnName) => {
  const key = fnName.replace(/^[:?]+/, '').trim();
  if (/\.(S|c):/.test(key)) return 'libc helper (memset/memcpy/malloc) inside the toggle window, not allocator (ОЦЕНКА)';
  for (const kk of Object.keys(DESC)) if (key.includes(kk)) return DESC[kk];
  return 'helper inside toggle window (ОЦЕНКА)';
};

// --- output -----------------------------------------------------------------
const f3 = (x) => x.toFixed(3);
const f2 = (x) => x.toFixed(2);
const clsRu = { 'instructions': 'инструкции', 'memory/latency': 'память', 'not_explained': 'не объяснено моделью' };

let out = '';
out += '# B4 gap attribution — sefer vs mimalloc (generated by scripts/b4_gap_attribution_table.mjs v2)\n\n';
out += 'Methodology v2: callgrind --toggle-collect=gp_kernel_probe::kernel_* (window inside the core kernel function only);\n';
out += 'time_ratio = exp(ab_median), judge J1 (Windows) = mi/sefer; Ir/EstCycles ratios = mi/sefer (WSL, marginal p80-p40 per op).\n';
out += 'EstCycles is the callgrind cost model (Ir + 10*(D1mr+D1mw+I1mr) + 100*(LLmr+LLmw+ILmr) + 10*Bcm), NOT hardware. ИЗМЕРЕНО unless noted.\n';
out += 'Classification thresholds (0.1 / 0.7) were fixed by the owner/orchestrator BEFORE the measurements; this script (v2) was written AFTER\n';
out += 'the first (full-process-window) iteration — iteration history in docs/perf/B4_GAP_ATTRIBUTION.md.\n\n';
out += '| kernel | time_ratio (mi/sefer) | Ir_ratio | EstCycles_ratio | Ir/op sefer | Ir/op mi | D1mr+D1mw/op | LLmr+LLmw/op | Bcm/op | events/op | class |\n';
out += '|---|---|---|---|---|---|---|---|---|---|---|\n';
for (const k of KERNELS) {
  const p = perK.get(k);
  const tr = Math.exp(num(judgeK.get(k).ab_median));
  const irR = p.mi.ir / p.sefer.ir;
  const estR = p.mi.est / p.sefer.est;
  const cls = classify(tr, irR, estR);
  const e = evK.get(k);
  const evParts = [];
  let anyEv = false;
  for (const c of EV_COLS) {
    const d = num(e[`delta_${c}`]);
    if (d !== 0) {
      anyEv = true;
      evParts.push(`${c}=${d} (${f3(d / num(e.ops_delta))}/op)`);
    }
  }
  const evCell = anyEv ? evParts.join('; ') : 'все нули (все delta_* = 0 по данным)';
  out += `| ${k} | ${f3(tr)} | ${f3(irR)} | ${f3(estR)} | ${f2(p.sefer.ir)} | ${f2(p.mi.ir)} | ${f3(p.sefer.d1)} / ${f3(p.mi.d1)} | ${f3(p.sefer.ll)} / ${f3(p.mi.ll)} | ${f2(p.sefer.bcm)} / ${f2(p.mi.bcm)} | ${evCell} | ${cls} (${clsRu[cls]}) |\n`;
}
out += '\nD1mr+D1mw/op, LLmr+LLmw/op (включая ILmr), Bcm/op: sefer / mi.\n';
out += 'events/op: печатается «все нули» ТОЛЬКО если все delta_* = 0 по данным; иначе реальные дельты и значения на op (ИЗМЕРЕНО).\n\n';

out += '## Функциональная разбивка: sefer top-5 по маржинальному self Ir/op (все 9 ядер)\n\n';
out += 'Доля считается скриптом: числитель self_ir_delta (toggle-окно), знаменатель — маржинальный Ir ядра (per-op sefer Ir × ops_delta); оба числа указаны.\n\n';
for (const k of KERNELS) {
  const p = perK.get(k);
  const ops = p.ops;
  const marginalIr = p.sefer.ir * ops;
  out += `### ${k} (маржинальный Ir = ${Math.round(marginalIr)}, ops_delta = ${ops})\n\n`;
  out += '| func | file:line | self Ir/op | self_ir_delta | % от маржинального Ir | описание |\n';
  out += '|---|---|---|---|---|---|\n';
  for (const r of fnByK.get(`${k}|sefer`).slice(0, 5)) {
    const dv = num(r.self_ir_delta);
    const perOp = dv / ops;
    const pct = dv / marginalIr * 100;
    out += `| ${r.func.replace(/^[:?]+/, '').trim()} | ${r.file_line} | ${perOp.toFixed(3)} | ${dv} | ${pct.toFixed(2)}% (${dv}/${Math.round(marginalIr)}) | ${descOf(r.func)} |\n`;
  }
  out += '\n';
}

// --- v1 vs v2 comparison (optional 6th input) -------------------------------
if (v1) {
  out += '## Сравнение с v1 (полно-процессное окно, отклонённая методика): Ir_ratio (mi/sefer)\n\n';
  out += '| kernel | v1 Ir_ratio | v2 Ir_ratio |\n|---|---|---|\n';
  const v1K = new Map();
  for (const r of v1) {
    if (!v1K.has(r.kernel)) v1K.set(r.kernel, {});
    v1K.get(r.kernel)[r.arm] = {
      ir: num(r.per_portion_delta_ir),
    };
  }
  for (const k of KERNELS) {
    const o = v1K.get(k);
    const v1r = o && o.sefer && o.mi ? o.mi.ir / o.sefer.ir : NaN;
    const p = perK.get(k);
    out += `| ${k} | ${Number.isFinite(v1r) ? f3(v1r) : 'n/a'} | ${f3(p.mi.ir / p.sefer.ir)} |\n`;
  }
  out += '\nv1 — полно-процессное окно (окно включало прогрев/отчёт пробы), методика отклонена ревью; v2 — toggle-окно внутри kernel_*.\n\n';
}

console.log(out);
try {
  writeFileSync('docs/perf/B4_GAP_ATTRIBUTION_table.generated.md', out);
} catch (err) {
  console.error(`note: could not write generated table file: ${err.message}`);
}
