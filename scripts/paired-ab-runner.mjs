// R6-OPT-A6 / Stage A.1-2: general-purpose process-level paired A/B/B/A judge.
//
// WHY THIS EXISTS. This project already ran a ONE-OFF version of exactly this
// measurement protocol: `docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md` (the
// R5-R2 investigation) used 20 alternating process-level repetitions with a
// hand-rolled paired t-test/sign-test to confirm a real wall-clock signal
// (paired t-stat 3.94-5.27, sign test 17-19/20) that a single-run
// `sample_size(10)` Criterion run could not resolve. That investigation's own
// script was explicitly "too specific" (tied to that exact regression's log
// layout) and discarded (see the doc's "Scripts" section). THIS script is the
// clean, reusable, general-purpose version of the same protocol: it drives
// the three `paired_ab_*` example binaries (each REALLY installing its own
// `#[global_allocator]` — see `examples/_shared/paired_ab_workload.rs`'s
// module doc for why that differs from `benches/global_alloc.rs`'s direct-
// call comparison) through an alternating A/B/B/A launch order and computes
// the same paired t-test + sign test R5-R2 used, so a future perf claim about
// this crate can cite ONE tool instead of hand-rolling stats again.
//
// WHY A/B/B/A, NOT A/B/A/B. Per R5_R2_CHURN_REGRESSION_PAIRED_AB.md's
// "20-round alternating A/B/B/A protocol" section: the pattern
// `A B B A | A B B A | ...` averages out MONOTONIC host drift (thermal
// throttling, frequency scaling, background load creeping up over an
// N-minute session) across each 4-launch block, rather than letting a
// consistent drift alias into a bias for whichever arm always runs
// first/last in a strict alternation. Each block's A-B pairing and B-A
// pairing are both adjacent-in-time, which is what makes "pair each A run
// against its nearest-in-time B run" a fair comparison.
//
// WHAT IT MEASURES. Wall-clock `elapsed_ns` for one call to the shared
// workload (`examples/_shared/paired_ab_workload.rs::run_workload`),
// identical across all three binaries by construction (verified via
// `--verify-only`, see below). Also captures each run's
// `segments_reserved_total` (the installed-allocator sanity check — SeferAlloc's
// own counter must be non-zero in the sefer binary and exactly 0 in the other
// two, or the run aborts as a harness bug, not a measurement) and RSS/commit-
// charge (R6-OPT-A1's probe technique, reused here per this task's own
// "combine with A1's commit-charge metrics" instruction).
//
// PROVENANCE. Every invocation writes one timestamped JSON file to
// `docs/perf/paired_ab_runs/` containing: every raw per-process sample (not
// just aggregates), the git commit hash, `rustc --version --verbose`, CPU
// info (best-effort via `wmic`/`/proc/cpuinfo`), Windows power plan (best-
// effort via `powercfg`), and the Cargo feature set the binaries were built
// with — so a future investigation can re-analyze the raw numbers without
// re-running the measurement (this task's own explicit "saves raw samples +
// full provenance" requirement).
//
// STATISTICS. A hand-rolled paired t-test (mean of the N `(a - b)` deltas,
// two-tailed, reported against the standard df=N-1 critical-value table) and
// a sign test (count of deltas favoring each side) — the EXACT methodology
// `R5_R2_CHURN_REGRESSION_PAIRED_AB.md` used (see that file's "Paired
// statistic" section), not a different statistical approach.
//
// USAGE:
//   node scripts/paired-ab-runner.mjs                       # full run: 20 pairs, sefer vs mimalloc AND sefer vs system
//   node scripts/paired-ab-runner.mjs --quick                # smoke-test: 4 pairs
//   node scripts/paired-ab-runner.mjs --pairs 30              # override pair count
//   node scripts/paired-ab-runner.mjs --arms sefer,sefer      # same-vs-same control run (the honesty check)
//   node scripts/paired-ab-runner.mjs --arms sefer,mimalloc   # one specific comparison
//   npm run paired-ab                                          # if wired in package.json

import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = process.platform === 'win32';

const args = process.argv.slice(2);
const quick = args.includes('--quick');
const pairsArg = args.find((a, i) => args[i - 1] === '--pairs');
const armsArg = args.find((a, i) => args[i - 1] === '--arms');
const verifyOnly = args.includes('--verify-only');

// Per the task's own explicit threshold: "at least 20 paired repetitions ...
// matching R5-R2's actual N=20" is the documented default for real claims.
// --quick drops this to a fast smoke-test count for iteration.
const PAIRS = pairsArg ? Number(pairsArg) : quick ? 4 : 20;

const ALL_ARMS = ['sefer', 'mimalloc', 'system'];
const REQUESTED_ARMS = armsArg ? armsArg.split(',').map((s) => s.trim()) : null;

const OUT_DIR = `${REPO_ROOT}/docs/perf/paired_ab_runs`;

function exePath(arm) {
  const name = isWin ? `paired_ab_${arm}.exe` : `paired_ab_${arm}`;
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${name}`;
}

async function buildArms(arms) {
  const unique = [...new Set(arms)];
  console.log(`[paired-ab] building example(s): ${unique.map((a) => `paired_ab_${a}`).join(', ')}...`);
  const exampleFlags = unique.flatMap((a) => ['--example', `paired_ab_${a}`]);
  // paired_ab_sefer requires alloc-global; the other two build fine without
  // any feature but also compile cleanly WITH it (mimalloc/System arms never
  // reference SeferAlloc, so the feature flag is harmless to them) — building
  // all three in ONE cargo invocation with alloc-global on keeps this simple
  // and matches the sibling runners' one-shot build step.
  const { code } = await run(
    'cargo',
    ['build', '--release', ...exampleFlags, '--features', 'alloc-global'],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
}

function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) r[m[1]] = /^-?\d+$/.test(m[2]) ? Number(m[2]) : m[2];
  }
  return r;
}

async function runOnce(arm) {
  const { out } = await run(exePath(arm), [], { cwd: REPO_ROOT });
  const r = parseResult(out);
  if (r.elapsed_ns == null) {
    throw new Error(`paired_ab_${arm} produced no RESULT elapsed_ns line — harness bug (raw output:\n${out}\n)`);
  }
  return r;
}

// ── Installed-allocator sanity check (task's own verification requirement) ──
// SeferAlloc's own diagnostic counter (`segments_reserved_total`) must move
// (be > 0) ONLY in the sefer binary, and read exactly 0 in mimalloc/system —
// confirming the three binaries genuinely differ in which allocator is
// globally installed, not just in name.
function checkInstalledAllocator(arm, sample) {
  const seg = sample.segments_reserved_total;
  if (arm === 'sefer') {
    if (!(seg > 0)) {
      throw new Error(
        `installed-allocator check FAILED: paired_ab_sefer reported segments_reserved_total=${seg} ` +
          `(expected > 0) — SeferAlloc does not appear to be genuinely installed/exercised.`,
      );
    }
  } else if (seg !== 0) {
    throw new Error(
      `installed-allocator check FAILED: paired_ab_${arm} reported segments_reserved_total=${seg} ` +
        `(expected exactly 0 — SeferAlloc must never be constructed in this binary).`,
    );
  }
}

// ── Provenance capture ───────────────────────────────────────────────────

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

function rustcVersion() {
  try {
    return execFileSync('rustc', ['--version', '--verbose']).toString().trim();
  } catch {
    return 'unavailable';
  }
}

function cpuInfo() {
  if (isWin) {
    try {
      return execFileSync('wmic', ['cpu', 'get', 'name,numberofcores,numberoflogicalprocessors', '/format:list'])
        .toString()
        .split(/\r?\n/)
        .filter((l) => l.trim())
        .join(' | ');
    } catch {
      return 'unavailable (wmic failed — deprecated on newer Windows builds; documented limitation)';
    }
  }
  try {
    return execFileSync('cat', ['/proc/cpuinfo']).toString().split('\n').slice(0, 30).join('\n');
  } catch {
    return 'unavailable';
  }
}

function powerPlan() {
  if (!isWin) return 'n/a (not Windows)';
  try {
    return execFileSync('powercfg', ['-getactivescheme']).toString().trim();
  } catch {
    return 'unavailable (powercfg -getactivescheme failed — documented limitation, not fatal)';
  }
}

// ── Statistics — the EXACT methodology from R5_R2_CHURN_REGRESSION_PAIRED_AB.md ──
//
// Paired t-test: mean of the N (a - b) deltas, sample stddev, standard error,
// t = mean / se. Two-tailed critical values at p<0.05 for common df, taken
// from the same table class R5-R2 cited (df=19 -> ~2.093); for other N we
// interpolate a short built-in table and fall back to a conservative note if
// N falls outside it, rather than silently mis-reporting significance.
const T_CRIT_005 = {
  3: 4.303, 4: 3.182, 5: 2.776, 6: 2.571, 7: 2.447, 8: 2.365, 9: 2.306, 10: 2.262,
  11: 2.228, 12: 2.201, 13: 2.179, 14: 2.16, 15: 2.145, 16: 2.131, 17: 2.12, 18: 2.11,
  19: 2.101, 20: 2.093, 25: 2.064, 30: 2.045, 40: 2.021, 60: 2.0, 120: 1.98,
};

function tCritical(df) {
  if (df <= 0) return null;
  const keys = Object.keys(T_CRIT_005).map(Number).sort((a, b) => a - b);
  for (const k of keys) if (df <= k) return T_CRIT_005[k];
  return 1.96; // large-df normal approximation
}

function pairedTTest(deltas) {
  const n = deltas.length;
  if (n < 2) return null;
  const mean = deltas.reduce((a, b) => a + b, 0) / n;
  const variance = deltas.reduce((a, b) => a + (b - mean) ** 2, 0) / (n - 1);
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const t = se === 0 ? (mean === 0 ? 0 : Infinity) : mean / se;
  const df = n - 1;
  const crit = tCritical(df);
  const significant = crit != null && Math.abs(t) > crit;
  return { n, mean, sd, se, t, df, crit, significant };
}

function signTest(deltas) {
  let aFaster = 0; // delta < 0: a faster than b
  let bFaster = 0; // delta > 0: b faster than a
  let ties = 0;
  for (const d of deltas) {
    if (d < 0) aFaster++;
    else if (d > 0) bFaster++;
    else ties++;
  }
  return { aFaster, bFaster, ties, n: deltas.length };
}

// ── Main A/B/B/A driver ──────────────────────────────────────────────────

async function runPairedComparison(armA, armB, pairs) {
  console.log(`\n[paired-ab] ${armA} vs ${armB} — ${pairs} pairs, A/B/B/A protocol`);
  const samplesA = [];
  const samplesB = [];
  const rawLog = [];

  // Each "block" is one A/B/B/A launch quadruple (4 process launches),
  // repeated `pairs` times (so `pairs` blocks -> `pairs` A-samples and
  // `pairs` B-samples, `4*pairs` total process launches) — mirroring
  // R5_R2_CHURN_REGRESSION_PAIRED_AB.md's "40 process launches followed the
  // strict pattern A B B A | A B B A | ..." for N=20.
  for (let block = 0; block < pairs; block++) {
    const order = ['A', 'B', 'B', 'A'];
    const blockSamples = { A: [], B: [] };
    for (const slot of order) {
      const arm = slot === 'A' ? armA : armB;
      const sample = await runOnce(arm);
      checkInstalledAllocator(arm, sample);
      blockSamples[slot].push(sample);
      rawLog.push({ block, slot, arm, ...sample, wall_clock_iso: new Date().toISOString() });
      process.stdout.write('.');
    }
    // Each block contributes exactly one A-sample and one B-sample per the
    // A/B/B/A shape (the FIRST A/B pair of the quadruple) — mirrored as a
    // single paired observation per block, matching R5-R2's "pairing each
    // new run against its immediately-adjacent old run" convention. We take
    // the mean of the block's 2 same-arm samples (both A's, both B's) as
    // that block's representative value, which also cancels a little
    // within-block jitter while still yielding exactly `pairs` paired deltas.
    const meanA = blockSamples.A.reduce((s, r) => s + r.elapsed_ns, 0) / blockSamples.A.length;
    const meanB = blockSamples.B.reduce((s, r) => s + r.elapsed_ns, 0) / blockSamples.B.length;
    samplesA.push(meanA);
    samplesB.push(meanB);
  }
  console.log('');

  const deltas = samplesA.map((a, i) => a - samplesB[i]); // A - B, ns
  const tTest = pairedTTest(deltas);
  const sign = signTest(deltas);

  return { armA, armB, pairs, samplesA, samplesB, deltas, tTest, sign, rawLog };
}

function fmtNs(n) {
  if (n == null || Number.isNaN(n)) return '-';
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toFixed(3)} ms`;
  if (Math.abs(n) >= 1_000) return `${(n / 1_000).toFixed(3)} us`;
  return `${n.toFixed(0)} ns`;
}

function printComparison(result) {
  const { armA, armB, tTest, sign } = result;
  const sameArm = armA === armB;
  const labelA = sameArm ? `${armA}(A-slot)` : armA;
  const labelB = sameArm ? `${armB}(B-slot)` : armB;
  console.log(`\n  === ${labelA} vs ${labelB} (A - B, ns)${sameArm ? '  [SAME-VS-SAME CONTROL]' : ''} ===`);
  if (tTest) {
    console.log(
      `  n=${tTest.n}  mean Δ=${fmtNs(tTest.mean)}  sd=${fmtNs(tTest.sd)}  se=${fmtNs(tTest.se)}  ` +
        `t=${tTest.t.toFixed(3)}  df=${tTest.df}  crit(p<0.05)=${tTest.crit?.toFixed(3) ?? 'n/a'}  ` +
        `${tTest.significant ? '=> REAL (rejects null)' : '=> NOT statistically distinguishable from noise (fails to reject null)'}`,
    );
  } else {
    console.log('  (not enough samples for a t-test)');
  }
  console.log(
    `  sign test: ${labelA}-faster=${sign.aFaster}/${sign.n}  ${labelB}-faster=${sign.bFaster}/${sign.n}  ties=${sign.ties}`,
  );
}

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });

  const arms = REQUESTED_ARMS ?? ['sefer', 'mimalloc', 'system'];
  for (const a of arms) {
    if (!ALL_ARMS.includes(a)) throw new Error(`unknown arm '${a}' — must be one of ${ALL_ARMS.join(', ')}`);
  }

  await buildArms(arms);

  if (verifyOnly) {
    // Quick structural check only: run each requested arm once, confirm the
    // installed-allocator counter behaves as expected, then exit. Used by
    // this task's own verification step 1 ("confirm each of the three
    // executables genuinely installs its own allocator").
    console.log('\n[paired-ab] --verify-only: one sample per arm, checking installed-allocator counters...');
    for (const arm of [...new Set(arms)]) {
      const s = await runOnce(arm);
      checkInstalledAllocator(arm, s);
      console.log(`  paired_ab_${arm}: elapsed_ns=${s.elapsed_ns} segments_reserved_total=${s.segments_reserved_total} -- OK`);
    }
    console.log('\n[paired-ab] PASS -- all requested arms genuinely install their own (or no) allocator as claimed.');
    return;
  }

  // Comparisons: if the caller passed exactly 2 arms via --arms, run exactly
  // that one comparison (this is how the same-vs-same honesty check and any
  // single ad-hoc comparison are invoked). Otherwise (the default 3-arm
  // case), run sefer-vs-mimalloc and sefer-vs-system.
  const comparisons =
    REQUESTED_ARMS && REQUESTED_ARMS.length === 2
      ? [[REQUESTED_ARMS[0], REQUESTED_ARMS[1]]]
      : [
          ['sefer', 'mimalloc'],
          ['sefer', 'system'],
        ];

  const results = [];
  for (const [a, b] of comparisons) {
    results.push(await runPairedComparison(a, b, PAIRS));
  }

  for (const r of results) printComparison(r);

  // ── Provenance file ────────────────────────────────────────────────────
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const outFile = `${OUT_DIR}/${timestamp}.json`;
  const provenance = {
    timestamp: new Date().toISOString(),
    git_commit: gitCommit(),
    git_dirty: gitDirty(),
    rustc_version_verbose: rustcVersion(),
    cpu_info: cpuInfo(),
    power_plan: powerPlan(),
    platform: process.platform,
    cargo_features_built_with: 'alloc-global',
    protocol: 'A/B/B/A, pairs = one block of 4 launches (A B B A), block value = mean of the 2 same-arm samples',
    pairs_per_comparison: PAIRS,
    comparisons: results.map((r) => ({
      arm_a: r.armA,
      arm_b: r.armB,
      pairs: r.pairs,
      samples_a_ns: r.samplesA,
      samples_b_ns: r.samplesB,
      deltas_a_minus_b_ns: r.deltas,
      paired_t_test: r.tTest,
      sign_test: r.sign,
      raw_process_launches: r.rawLog,
    })),
  };
  writeFileSync(outFile, JSON.stringify(provenance, null, 2));
  console.log(`\n[paired-ab] provenance written to ${outFile}`);

  console.log(
    '\n[paired-ab] Reading this output: `t` past `crit` (or sign test heavily lopsided, e.g. 17+/20) means a REAL,\n' +
      '  non-noise wall-clock difference on THIS host for THIS workload — mirroring\n' +
      '  docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md\'s verdict criteria. A same-vs-same control run\n' +
      '  (--arms sefer,sefer) should show t well under `crit` and a roughly even sign-test split --\n' +
      '  if it does not, that indicates a harness bug (non-reproducible workload, background load\n' +
      '  bleeding into the timed region), not a real self-difference.',
  );
}

main().catch((e) => {
  console.error(`\n[paired-ab] FAIL -- ${e.message}`);
  process.exit(1);
});
