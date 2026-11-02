#!/usr/bin/env node
// WG-1 fair-axis report (measurement only). Reads docs/perf/fairaxis/win_run<N>.jsonl.gz
// (docs/perf/fairaxis/run.mjs) and prints, per run and scenario, sefer / mimalloc for:
//   window  = the judge's ns_per_op x ops (median of per-repetition ratios, with range),
//   process = wall time from spawn to exit measured by the runner (same statistic),
//   user, kernel = CPU split read by the judge at the end of the run (ratio of the arm medians),
//   faults  = page faults at the end of the run (ratio of the arm medians),
//   RSS left after the run (median MiB, sefer / mimalloc).
// Cells whose mimalloc median is 0 for a CPU field print `n/a` (clock resolution about 15 ms).
// Usage: node scripts/fairaxis_report.mjs [--csv FILE]
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { gunzipSync } from 'node:zlib';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const f3 = (v) => (Number.isFinite(v) ? v.toFixed(3) : 'n/a');
const SC = ['growmix', 'regrow', 'wild', 'large', 'grow64', 'phase', 'grow1k'];
const csv = [];
for (const run of [1, 2]) {
  const recs = gunzipSync(readFileSync(join(REPO_ROOT, 'docs', 'perf', 'fairaxis', `win_run${run}.jsonl.gz`))).toString('utf8')
    .trim().split(/\r?\n/).map((l) => JSON.parse(l));
  console.log(`\n### Run ${run}\n`);
  console.log('| Scenario | reps | window | window range | process | process range | user (ratio of medians) | kernel (ratio of medians) | faults (ratio of medians) | RSS left, MiB sefer / mi |');
  console.log('|---|---:|---:|---|---:|---|---:|---:|---:|---|');
  for (const sc of SC) {
    const S = recs.filter((r) => r.scenario === sc && r.arm === 'sefer').sort((a, b) => a.rep - b.rep);
    const M = recs.filter((r) => r.scenario === sc && r.arm === 'mi').sort((a, b) => a.rep - b.rep);
    assert.equal(S.length, M.length, `${sc}: equal repetitions`);
    assert.equal(new Set([...S, ...M].map((r) => r.r.checksum)).size, 1, `${sc}: same op sequence`);
    const win = (r) => r.r.ns_per_op * r.r.ops / 1e6;
    const pair = (f) => S.map((s, i) => f(s) / f(M[i]));
    const w = pair(win), p = pair((r) => r.proc_ms);
    const rom = (f) => { const d = med(M.map(f)); return d > 0 ? med(S.map(f)) / d : NaN; };
    const row = { sc, n: S.length, w: med(w), wlo: Math.min(...w), whi: Math.max(...w), p: med(p), plo: Math.min(...p), phi: Math.max(...p),
      u: rom((r) => r.r.user_ms), k: rom((r) => r.r.kernel_ms), f: rom((r) => r.r.faults),
      rs: med(S.map((r) => r.r.end_rss)) / 1048576, rm: med(M.map((r) => r.r.end_rss)) / 1048576 };
    console.log(`| ${sc} | ${row.n} | ${f3(row.w)} | ${f3(row.wlo)}–${f3(row.whi)} | ${f3(row.p)} | ${f3(row.plo)}–${f3(row.phi)} | ${f3(row.u)} | ${f3(row.k)} | ${f3(row.f)} | ${row.rs.toFixed(0)} / ${row.rm.toFixed(0)} |`);
    csv.push([run, sc, row.n, f3(row.w), f3(row.wlo), f3(row.whi), f3(row.p), f3(row.plo), f3(row.phi), f3(row.u), f3(row.k), f3(row.f), row.rs.toFixed(0), row.rm.toFixed(0)].join(','));
  }
}
const i = process.argv.indexOf('--csv');
if (i >= 0) writeFileSync(process.argv[i + 1], ['run,scenario,reps,window,window_lo,window_hi,process,process_lo,process_hi,user,kernel,faults,rss_sefer_mib,rss_mi_mib', ...csv].join('\n') + '\n');
