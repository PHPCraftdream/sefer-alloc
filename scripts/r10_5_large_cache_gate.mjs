#!/usr/bin/env node
// R10-5 driver: builds both arms of the warm-Large-cache-hit wall-clock gate,
// runs a cache-hit PROOF gate (the methodological heart of this task), then
// invokes `scripts/paired-ab-runner.mjs` twice (one A/B/B/A session per
// allocation size: 1.5 MiB / 1.75 MiB).
//
// This is the single-command entry point for reproducing
// `docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`. It does NOT modify any source
// file, Cargo.toml feature bundle, or existing script — it only:
//   1. Builds two example binaries (same source, different Cargo features).
//   2. Runs the cache-hit proof gate: launches each arm once per size and
//      asserts the baseline's `large_cache_hits` is large (warm cache) while
//      the treatment's is exactly 0 (small path) — PROVING the comparison is
//      warm-vs-warm, not accidentally warm-vs-cold (the gap R9-4 §2.4 left).
//   3. Writes two config JSON files to `docs/perf/paired_ab_runs/`
//      (gitignored — generated artifacts).
//   4. Invokes `scripts/paired-ab-runner.mjs --config <size>.json --pairs N`
//      twice, once per size.
//
// WHY BOTH ARMS CARRY `alloc-stats`. The per-hit `large_cache_hits` counter is
// gated behind `alloc-stats` (default OFF, NOT in `production`) — without it
// the counter reads 0 even when hits occur, and the cache-hit proof (step 2)
// would be impossible. So BOTH arms are built with `alloc-stats` ADDED to
// their feature set (NOT written into `Cargo.toml`'s `production` list —
// passed only on these probe binaries' `--features` build lines, exactly the
// usage `alloc-stats` exists for). The per-hit increment is a single Relaxed
// load+store on the owning thread (no `lock xadd`; see
// `src/alloc_core/alloc_core_large.rs` lines 129-157) — negligible against
// the us-scale Large-path work, and asymmetric only in that the baseline arm
// pays it (the treatment arm's small path does not hit the Large cache).
//
// WHY ONE SIZE PER PROCESS LAUNCH (argv). Running both sizes in one process
// would let the 1.5 MiB phase's cached Large spans absorb the 1.75 MiB
// phase's allocs (a 4 MiB cached span satisfies both 1.5 and 1.75 MiB
// requests under the `LARGE_CACHE_SIZE_FACTOR = 2` ratio bound), muddying
// the warm-up logic. argv[1] = size in KiB keeps each size's cache state
// independent — one size per launch, two launches per A/B/B/A block.
//
// USAGE:
//   node scripts/r10_5_large_cache_gate.mjs                # full run: pairs=20 per size (default)
//   node scripts/r10_5_large_cache_gate.mjs --quick         # smoke: pairs=4 per size
//   node scripts/r10_5_large_cache_gate.mjs --pairs 10      # custom pair count
//   node scripts/r10_5_large_cache_gate.mjs --verify-only   # build + cache-hit proof gate only, no pairing
//   node scripts/r10_5_large_cache_gate.mjs --skip-build    # reuse already-built exes (for re-running stats)

import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = process.platform === 'win32';

// ── CLI args ──────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const quick = args.includes('--quick');
const pairsArg = args.find((a, i) => args[i - 1] === '--pairs');
const verifyOnly = args.includes('--verify-only');
const skipBuild = args.includes('--skip-build');

const PAIRS = pairsArg ? Number(pairsArg) : quick ? 4 : 20;

// The two density-1 wide classes (1.5 / 1.75 MiB). 1.25 MiB is SKIPPED — it
// has a genuine cache-comparison-agnostic 2x density win (R9-4 §2.3), so this
// warm-vs-warm comparison does not apply to it.
const SIZES_KIB = [1536, 1792];
const ARM_OFF = 'paired_ab_large_cache_off';
const ARM_ON = 'paired_ab_large_cache_on';

// ── Path resolution (mirrors paired-ab-runner.mjs's exePath logic) ────────
function exePath(exampleName) {
  const name = isWin ? `${exampleName}.exe` : exampleName;
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${name}`;
}

const EXE_OFF = exePath(ARM_OFF);
const EXE_ON = exePath(ARM_ON);

// ── Build step: two cargo invocations, different --features ──────────────
async function buildArms() {
  console.log('[r10-5] Building baseline arm (production + alloc-stats; 1.5/1.75 MiB -> Large, warm cache)...');
  const { code: codeOff } = await run(
    'cargo',
    ['build', '--release', '--example', ARM_OFF, '--features', 'production,alloc-stats'],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (codeOff !== 0) throw new Error(`cargo build (off) failed (exit ${codeOff})`);

  console.log('[r10-5] Building treatment arm (production + medium-classes-wide + alloc-stats; 1.5/1.75 MiB -> small path)...');
  const { code: codeOn } = await run(
    'cargo',
    ['build', '--release', '--example', ARM_ON, '--features', 'production,medium-classes-wide,alloc-stats'],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (codeOn !== 0) throw new Error(`cargo build (on) failed (exit ${codeOn})`);

  for (const [label, p] of [['off', EXE_OFF], ['on', EXE_ON]]) {
    if (!existsSync(p)) throw new Error(`expected exe not found: ${p} (${label})`);
  }
  console.log(`[r10-5] Both arms built:\n  off: ${EXE_OFF}\n  on : ${EXE_ON}`);
}

// ── Cache-hit PROOF gate (the methodological heart of this task) ──────────
// Launches each arm once per size, parses every RESULT line, and asserts:
//   - baseline (off): large_cache_hits > 0  (genuinely HIT the warm cache)
//   - treatment (on): large_cache_hits == 0 (genuinely on the small path)
// If the baseline reading were NOT large, the comparison would be warm-vs-cold
// again — the exact gap this gate exists to close. Aborts the whole run on
// failure so no timing number is trusted without the proof.
function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) r[m[1]] = /^-?\d+$/.test(m[2]) ? Number(m[2]) : m[2];
  }
  return r;
}

async function launchOnce(exe, sizeKib) {
  // Suppress the probe's own stdout (RESULT lines + any noise) from the tee —
  // we parse it ourselves below. `run` still captures `out`.
  const { code, out } = await run(exe, [String(sizeKib)], { cwd: REPO_ROOT, stdio: 'pipe' });
  if (code !== 0) throw new Error(`${exe} ${sizeKib} exited ${code} (raw:\n${out}\n)`);
  return parseResult(out);
}

async function verifyCacheHits() {
  console.log('\n[r10-5] === Cache-hit PROOF gate (warm-vs-warm verification) ===');
  console.log('[r10-5] Asserting baseline HITS the warm cache and treatment uses the small path...');
  let allOk = true;
  for (const sizeKib of SIZES_KIB) {
    const off = await launchOnce(EXE_OFF, sizeKib);
    const on = await launchOnce(EXE_ON, sizeKib);
    const offHits = off.large_cache_hits;
    const onHits = on.large_cache_hits;
    const offSegs = off.segments_reserved_total;
    const onSegs = on.segments_reserved_total;
    // The cache-hit proof: baseline must show real hits, treatment must show 0.
    // WS_LEN=6, ROUNDS=3000, WARMUP=3 => steady-state hits ~= 6 * (3000 + ~2) ~ 18000.
    const offWarm = offHits >= 6; // at least one warm-up-cycle's worth of hits
    const onCold = onHits === 0;
    const ok = offWarm && onCold;
    if (!ok) allOk = false;
    console.log(
      `  ${sizeKib} KiB: off large_cache_hits=${offHits} (segs_reserved=${offSegs}) | on large_cache_hits=${onHits} (segs_reserved=${onSegs})` +
        `  => ${ok ? 'PROVEN warm-vs-warm' : 'FAILED PROOF'} (off ${offWarm ? 'warm' : 'NOT warm'} / on ${onCold ? 'small-path' : 'NOT small-path'})`,
    );
  }
  if (!allOk) {
    throw new Error(
      'cache-hit PROOF gate FAILED — the baseline is NOT hitting the warm cache or the treatment is NOT on the small path. ' +
        'Do NOT trust any timing number until this is fixed. (Check: WS_LEN vs LARGE_CACHE_SLOTS; alloc-stats in the build feature set; medium-classes-wide wired for the on arm.)',
    );
  }
  console.log('[r10-5] PROOF gate PASSED — comparison is genuinely warm Large-cache vs small-path recycle.');
}

// ── Config JSON generation ────────────────────────────────────────────────
// One config per size. Both arms point at the pre-built exes with the size as
// argv[1]. The sanity gate requires segments_reserved_total > 0 in BOTH arms
// (both genuinely install SeferAlloc and exercise it).
function writeConfig(sizeKib) {
  const cfg = {
    metric: 'recycle_ns',
    arms: {
      A: { command: EXE_OFF, args: [String(sizeKib)] },
      B: { command: EXE_ON, args: [String(sizeKib)] },
    },
    sanity: {
      key: 'segments_reserved_total',
      nonzero_arms: ['A', 'B'],
    },
    features_note:
      'A=production,alloc-stats (Large, warm cache); B=production,medium-classes-wide,alloc-stats (small path)',
  };
  const dir = `${REPO_ROOT}/docs/perf/paired_ab_runs`;
  mkdirSync(dir, { recursive: true });
  const path = `${dir}/_r10_5_${sizeKib}.json`;
  writeFileSync(path, JSON.stringify(cfg, null, 2));
  return path;
}

// ── Invoke the runner once per size ───────────────────────────────────────
function runPhase(sizeKib, configPath) {
  const runnerArgs = ['scripts/paired-ab-runner.mjs', '--config', configPath];
  if (verifyOnly) {
    runnerArgs.push('--verify-only');
  } else {
    runnerArgs.push('--pairs', String(PAIRS));
  }
  const label = sizeKib === 1536 ? '1.5 MiB (1536 KiB)' : sizeKib === 1792 ? '1.75 MiB (1792 KiB)' : `${sizeKib} KiB`;
  console.log(`\n[r10-5] === Size: ${label} (${verifyOnly ? 'verify-only' : `${PAIRS} pairs`}) ===`);
  return new Promise((res, rej) => {
    const child = spawn('node', runnerArgs, { cwd: REPO_ROOT, stdio: 'inherit', shell: isWin });
    child.on('error', rej);
    child.on('close', (code) => (code === 0 ? res() : rej(new Error(`runner exited ${code} for size ${sizeKib}`))));
  });
}

async function main() {
  if (!skipBuild) {
    await buildArms();
  } else {
    console.log('[r10-5] --skip-build: reusing already-built exes.');
    for (const [label, p] of [['off', EXE_OFF], ['on', EXE_ON]]) {
      if (!existsSync(p)) throw new Error(`--skip-build but exe missing: ${p} (${label})`);
    }
  }

  // ALWAYS run the cache-hit proof gate first — every timing number below it
  // is only trustworthy because this gate proved the comparison is warm-vs-warm.
  await verifyCacheHits();

  if (verifyOnly) {
    console.log('\n[r10-5] --verify-only: cache-hit proof gate complete, skipping paired comparison.');
    return;
  }

  const provenance = [];
  for (const sizeKib of SIZES_KIB) {
    const cfgPath = writeConfig(sizeKib);
    await runPhase(sizeKib, cfgPath);
    provenance.push({ sizeKib, config: cfgPath });
  }

  console.log('\n[r10-5] === All sizes complete ===');
  console.log('[r10-5] Config files (generated, gitignored):');
  for (const { sizeKib, config } of provenance) {
    console.log(`  ${sizeKib} KiB: ${config}`);
  }
  console.log('[r10-5] Provenance JSONs (one per size) written to docs/perf/paired_ab_runs/.');
  console.log('[r10-5] Read the two === A vs B === blocks above for the paired t-test / sign-test');
  console.log('       verdict per size. See docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md for the full report.');
}

main().catch((e) => {
  console.error(`\n[r10-5] FAIL -- ${e.message}`);
  process.exit(1);
});
