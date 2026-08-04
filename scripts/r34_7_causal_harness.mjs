// R34-7 (task #526): causal subprocess comparative harness for SeferAlloc vs
// mimalloc vs System.
//
// WHY THIS EXISTS. `benches/global_alloc.rs` compares the three allocators by
// calling each one's `GlobalAlloc` impl directly in ONE process — none is
// ever installed as `#[global_allocator]`, and only SeferAlloc's state is
// reset between groups (`dbg_trim_current_thread`). The R32/R33 review
// (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md` §"P1")
// proved this is non-causal: in one run, the control arms (mimalloc/System,
// whose code never changed) "regressed" by +59%/+71% — MORE than SeferAlloc's
// own +53% — proving the entire run-over-run signal was host-state drift, not
// code. This script is the causal MVP replacement: it builds three worker
// binaries (each ACTUALLY installing its own `#[global_allocator]`), launches
// a FRESH subprocess per (allocator, repetition) with a recorded alternation
// order, and reports median/min/max per allocator with identity context.
//
// WHAT IT MEASURES. `ns_per_op` = nanoseconds per free+alloc pair in the
// churn-write workload (`examples/_shared/r34_7_causal_workload.rs`, the exact
// same pattern `benches/global_alloc.rs::churn_step_write` uses). Each worker
// binary runs the workload `--iterations` rounds (each = 1024 free+alloc
// pairs) and prints `RESULT ns_per_op=<f64>`.
//
// ALTERNATION ORDER. Each repetition cycles through the three allocators in a
// ROTATING starting position (rep 0: sefer→mimalloc→system; rep 1:
// mimalloc→system→sefer; rep 2: system→sefer→mimalloc; rep 3: repeat). This
// breaks "always first/last" bias from monotonic host drift. The ACTUAL launch
// order is recorded in the output (not just the intention).
//
// IDENTITY CONTEXT. Before measurement: git HEAD SHA, dirty-tree status,
// `rustc --version --verbose` (includes host triple), and CPU model — printed
// to stdout and also available in the JSON provenance file. This is the MVP
// identity capture (not the full identity-bound baseline mismatch guard the
// complete specification calls for — see the "Remaining work" section in the
// task report).
//
// MVP SCOPE. One workload (churn-write), one size (default 64 B), simple
// cyclic alternation (not Latin square), median/min/max (not paired t-test).
// The full 7-part specification (Latin square, real container workloads,
// formal identity-bound baseline, decision-gate profile, pair/call/
// transaction/batch unit separation) is explicitly out of scope for this task
// and listed as remaining work for follow-up tasks R34-23/R34-25/R34-26.
//
// USAGE:
//   node scripts/r34_7_causal_harness.mjs                     # full run: 10 reps × 3 allocators, size=64, iter=20
//   node scripts/r34_7_causal_harness.mjs --quick              # smoke: 3 reps × 3 allocators
//   node scripts/r34_7_causal_harness.mjs --size 256           # override block size
//   node scripts/r34_7_causal_harness.mjs --iterations 50      # override timed rounds per process
//   node scripts/r34_7_causal_harness.mjs --reps 20            # override repetition count

import { execFileSync, spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import { cpus, platform } from 'node:os';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = platform === 'win32';

// ── CLI args ──────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const quick = args.includes('--quick');
const sizeArg = args.find((a, i) => args[i - 1] === '--size');
const iterArg = args.find((a, i) => args[i - 1] === '--iterations');
const repsArg = args.find((a, i) => args[i - 1] === '--reps');

const SIZE = sizeArg ? Number(sizeArg) : 64;
const ITERATIONS = iterArg ? Number(iterArg) : 20;
const REPS = repsArg ? Number(repsArg) : quick ? 3 : 10;

const ALLOCATORS = ['sefer', 'mimalloc', 'system'];

const OUT_DIR = `${REPO_ROOT}/docs/perf/r34_7_runs`;

// ── Binary path resolution ────────────────────────────────────────────────
function exePath(name) {
  const exe = isWin ? `${name}.exe` : name;
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${exe}`;
}

// ── Provenance helpers (same pattern as paired-ab-runner.mjs) ─────────────
function gitCommit() {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO_ROOT }).toString().trim();
  } catch {
    return 'unknown';
  }
}

function gitDirty() {
  try {
    const out = execFileSync('git', ['status', '--porcelain'], { cwd: REPO_ROOT }).toString();
    return out.trim().length > 0;
  } catch {
    return null;
  }
}

function gitBranch() {
  try {
    return execFileSync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], { cwd: REPO_ROOT })
      .toString()
      .trim();
  } catch {
    return 'unknown';
  }
}

function rustcVersion() {
  try {
    return execFileSync('rustc', ['--version', '--verbose']).toString().trim();
  } catch {
    return 'unavailable';
  }
}

function cpuModel() {
  // Use Node's built-in os.cpus() — simpler and more portable than wmic/
  // /proc/cpuinfo, and the task explicitly suggests os.cpus()[0].model.
  const c = cpus();
  return c.length > 0 ? `${c[0].model} (${c.length} cores)` : 'unknown';
}

// ── RESULT parsing (same regex as paired-ab-runner.mjs) ───────────────────
function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) r[m[1]] = /^-?\d+(\.\d+)?$/.test(m[2]) ? Number(m[2]) : m[2];
  }
  return r;
}

// ── Build ────────────────────────────────────────────────────────────────
async function buildWorkers() {
  console.log('[r34-7] building 3 worker binaries (release, features=production)...');
  const exampleFlags = ALLOCATORS.map((a) => ['--example', `r34_7_causal_worker_${a}`]).flat();
  const { code } = await run(
    'cargo',
    ['build', '--release', ...exampleFlags, '--features', 'production'],
    { cwd: REPO_ROOT },
  );
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
  console.log('[r34-7] build OK');
}

// ── Run one worker subprocess ────────────────────────────────────────────
//
// Uses a QUIET spawn (no stdout tee, unlike lib.mjs's `run`) so the RESULT
// lines from each worker don't flood the orchestrator's console output. The
// orchestrator prints its own structured per-rep summary instead.
function runQuiet(cmd, cmdArgs) {
  return new Promise((res, rej) => {
    const child = spawn(cmd, cmdArgs, { cwd: REPO_ROOT });
    let out = '';
    child.stdout.on('data', (buf) => {
      out += buf.toString();
    });
    child.stderr.on('data', (buf) => {
      out += buf.toString();
    });
    child.on('error', rej);
    child.on('close', (code) => res({ code: code ?? 1, out }));
  });
}

async function runOnce(allocator, size, iterations) {
  const cmd = exePath(`r34_7_causal_worker_${allocator}`);
  const cmdArgs = ['--size', String(size), '--iterations', String(iterations)];
  const { code, out } = await runQuiet(cmd, cmdArgs);
  if (code !== 0) {
    throw new Error(`worker '${allocator}' exited ${code} (raw output:\n${out}\n)`);
  }
  const r = parseResult(out);
  if (r.ns_per_op == null) {
    throw new Error(`worker '${allocator}' produced no RESULT ns_per_op line (raw output:\n${out}\n)`);
  }
  return r;
}

// ── Sanity gate: installed-allocator check ───────────────────────────────
function checkSanity(allocator, sample) {
  const val = sample.segments_reserved_total;
  if (allocator === 'sefer') {
    if (!(val > 0)) {
      throw new Error(
        `installed-allocator check FAILED: sefer worker reported segments_reserved_total=${val} (expected > 0)`,
      );
    }
  } else if (val !== 0) {
    throw new Error(
      `installed-allocator check FAILED: ${allocator} worker reported segments_reserved_total=${val} (expected 0)`,
    );
  }
}

// ── Statistics ───────────────────────────────────────────────────────────
function median(arr) {
  const sorted = [...arr].sort((a, b) => a - b);
  const n = sorted.length;
  if (n % 2 === 1) return sorted[Math.floor(n / 2)];
  return (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
}

function mean(arr) {
  return arr.reduce((a, b) => a + b, 0) / arr.length;
}

function stdev(arr) {
  if (arr.length < 2) return 0;
  const m = mean(arr);
  return Math.sqrt(arr.reduce((s, v) => s + (v - m) ** 2, 0) / (arr.length - 1));
}

function fmtNs(ns) {
  if (ns == null || Number.isNaN(ns)) return '-';
  if (ns < 1) return `${ns.toFixed(2)} ns`;
  if (ns < 1000) return `${ns.toFixed(1)} ns`;
  return `${(ns / 1000).toFixed(2)} us`;
}

function fmtPct(p) {
  return `${p.toFixed(1)}%`;
}

// ── Alternation: cyclic rotation of the 3 allocators per rep ─────────────
//
// Rep 0 starts with sefer, rep 1 starts with mimalloc, rep 2 starts with
// system, then repeats. This means no allocator is always first or always
// last — breaking the "monotonic host drift aliases into a consistent bias"
// failure mode the old single-process bench suffered from.
function orderForRep(rep) {
  const start = rep % ALLOCATORS.length;
  return ALLOCATORS.map((_, i) => ALLOCATORS[(start + i) % ALLOCATORS.length]);
}

// ── Main ─────────────────────────────────────────────────────────────────
async function main() {
  mkdirSync(OUT_DIR, { recursive: true });

  // ── Identity context ──────────────────────────────────────────────────
  const identity = {
    git_commit: gitCommit(),
    git_branch: gitBranch(),
    git_dirty: gitDirty(),
    rustc_version_verbose: rustcVersion(),
    cpu_model: cpuModel(),
    platform: process.platform,
    block_size: SIZE,
  };

  console.log('╔══════════════════════════════════════════════════════════════╗');
  console.log('║  R34-7 Causal Comparative Harness — subprocess-isolated A/B/C ║');
  console.log('╚══════════════════════════════════════════════════════════════╝');
  console.log();
  console.log('── Identity context ──────────────────────────────────────────────');
  console.log(`  git HEAD:     ${identity.git_commit}`);
  console.log(`  git branch:   ${identity.git_branch}`);
  console.log(`  git dirty:    ${identity.git_dirty}`);
  console.log(`  rustc:        ${identity.rustc_version_verbose.split('\n')[0]}`);
  const hostLine = identity.rustc_version_verbose
    .split('\n')
    .find((l) => l.startsWith('host:'));
  console.log(`  host triple:  ${hostLine ? hostLine.replace('host: ', '') : 'unknown'}`);
  console.log(`  CPU:          ${identity.cpu_model}`);
  console.log(`  platform:     ${identity.platform}`);
  console.log();
  console.log('── Measurement parameters ────────────────────────────────────────');
  console.log(`  workload:     churn-write (free+alloc+write16B per op)`);
  console.log(`  block size:   ${SIZE} B`);
  console.log(`  iterations:   ${ITERATIONS} rounds × 1024 ops/round = ${ITERATIONS * 1024} ops per process`);
  console.log(`  repetitions:  ${REPS}`);
  console.log(`  total launches: ${REPS * 3} fresh subprocesses`);
  console.log();

  // ── Build ─────────────────────────────────────────────────────────────
  await buildWorkers();

  // ── Warmup run (verify each binary produces a sane number) ───────────
  console.log('── Sanity check: one launch per allocator ────────────────────────');
  for (const alloc of ALLOCATORS) {
    const s = await runOnce(alloc, SIZE, 1);
    checkSanity(alloc, s);
    const sanityVal = s.segments_reserved_total;
    console.log(
      `  ${alloc.padEnd(8)}: ns_per_op=${fmtNs(s.ns_per_op)}  segments_reserved_total=${sanityVal}  ✓`,
    );
  }
  console.log();

  // ── Measurement: fresh subprocess per (allocator, rep) ───────────────
  console.log('── Measurement (fresh subprocess per launch) ─────────────────────');
  const samples = { sefer: [], mimalloc: [], system: [] };
  const rawLog = [];
  const actualOrder = [];

  for (let rep = 0; rep < REPS; rep++) {
    const order = orderForRep(rep);
    const repSamples = [];
    for (const alloc of order) {
      actualOrder.push(alloc);
      const s = await runOnce(alloc, SIZE, ITERATIONS);
      checkSanity(alloc, s);
      samples[alloc].push(s.ns_per_op);
      repSamples.push({ alloc, ns_per_op: s.ns_per_op });
      rawLog.push({
        rep,
        alloc,
        ns_per_op: s.ns_per_op,
        segments_reserved_total: s.segments_reserved_total,
        wall_clock_iso: new Date().toISOString(),
      });
      process.stdout.write('.');
    }
    // Print this rep's per-allocator values inline.
    const parts = repSamples.map((r) => `${r.alloc}=${fmtNs(r.ns_per_op)}`).join('  ');
    process.stdout.write(`  [rep ${rep + 1}/${REPS}] ${parts}\n`);
  }
  console.log();

  // ── Actual launch order ───────────────────────────────────────────────
  console.log('── Actual launch order (recorded, not just intended) ─────────────');
  console.log(`  ${actualOrder.join(' → ')}`);
  console.log();

  // ── Statistics table ─────────────────────────────────────────────────
  console.log('── Results: ns per free+alloc pair ────────────────────────────────');
  console.log();
  console.log('| Allocator | median |    min |    max |   mean |  stdev |   CV% | samples |');
  console.log('|-----------|-------:|-------:|-------:|-------:|-------:|------:|--------:|');
  const stats = {};
  for (const alloc of ALLOCATORS) {
    const s = samples[alloc];
    const med = median(s);
    const mn = Math.min(...s);
    const mx = Math.max(...s);
    const m = mean(s);
    const sd = stdev(s);
    const cv = m > 0 ? (sd / m) * 100 : 0;
    stats[alloc] = { median: med, min: mn, max: mx, mean: m, stdev: sd, cv, samples: s };
    console.log(
      `| ${alloc.padEnd(9)} | ${fmtNs(med).padStart(6)} | ${fmtNs(mn).padStart(6)} | ${fmtNs(mx).padStart(6)} | ${fmtNs(m).padStart(6)} | ${fmtNs(sd).padStart(6)} | ${fmtPct(cv).padStart(5)} | ${String(s.length).padStart(7)} |`,
    );
  }
  console.log();

  // ── Ratios ────────────────────────────────────────────────────────────
  console.log('── Comparative ratios (median-based) ─────────────────────────────');
  const seferMed = stats.sefer.median;
  const miMed = stats.mimalloc.median;
  const sysMed = stats.system.median;

  function ratioLine(label, a, b) {
    if (b == null || b === 0) return `  ${label}: n/a`;
    const r = a / b;
    if (r <= 1) return `  ${label}: ${(1 / r).toFixed(2)}× faster`;
    return `  ${label}: ${r.toFixed(2)}× slower`;
  }

  console.log(ratioLine('SeferAlloc vs mimalloc', seferMed, miMed));
  console.log(ratioLine('SeferAlloc vs System  ', seferMed, sysMed));
  console.log(ratioLine('mimalloc  vs System   ', miMed, sysMed));
  console.log();

  // ── Causality evidence: CV% comparison ────────────────────────────────
  console.log('── Causality evidence ────────────────────────────────────────────');
  console.log(
    '  The old single-process bench (benches/global_alloc.rs) showed control-arm',
  );
  console.log(
    '  "regressions" of +50-90% (mimalloc/System code unchanged) — pure host drift.',
  );
  console.log(
    '  This harness measures each allocator in a FRESH subprocess, so cross-arm',
  );
  console.log('  state leakage is impossible by construction.');
  console.log();
  console.log('  Coefficient of variation (CV% = stdev/mean × 100) per allocator:');
  for (const alloc of ALLOCATORS) {
    const verdict = stats[alloc].cv < 5 ? 'STABLE' : stats[alloc].cv < 10 ? 'moderate' : 'NOISY';
    console.log(`    ${alloc.padEnd(8)}: ${fmtPct(stats[alloc].cv).padStart(5)} CV  [${verdict}]`);
  }
  console.log();
  console.log(
    '  A CV% well under the old bench\'s +50% false-regression range confirms the',
  );
  console.log(
    '  subprocess-isolated signal is causal (code-attributable), not host-drift.',
  );
  console.log();

  // ── Provenance JSON ───────────────────────────────────────────────────
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const outFile = `${OUT_DIR}/${timestamp}.json`;
  const provenance = {
    timestamp: new Date().toISOString(),
    task: 'R34-7',
    identity,
    parameters: {
      workload: 'churn-write',
      block_size: SIZE,
      iterations: ITERATIONS,
      reps: REPS,
      ops_per_process: ITERATIONS * 1024,
    },
    actual_launch_order: actualOrder,
    stats,
    raw_samples: rawLog,
  };
  writeFileSync(outFile, JSON.stringify(provenance, null, 2));
  console.log(`[r34-7] provenance written to ${outFile}`);
}

main().catch((e) => {
  console.error(`\n[r34-7] FAIL -- ${e.message}`);
  process.exit(1);
});
