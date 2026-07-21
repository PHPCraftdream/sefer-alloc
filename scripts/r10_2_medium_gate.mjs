#!/usr/bin/env node
// R10-2 driver: builds both arms of the medium-classes wall-clock gate, writes
// three per-phase --config JSONs, and invokes `scripts/paired-ab-runner.mjs`
// three times (one A/B/B/A session per phase: alloc / free / realloc).
//
// This is the single-command entry point for reproducing
// `docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md`. It does NOT modify any
// source file, Cargo.toml feature bundle, or existing script — it only:
//   1. Builds two example binaries (same source, different Cargo features).
//   2. Writes three small config JSON files to `docs/perf/paired_ab_runs/`
//      (gitignored — generated artifacts, like the provenance JSONs).
//   3. Invokes `scripts/paired-ab-runner.mjs --config <phase>.json --pairs N`
//      three times, once per phase metric.
//
// WHY THREE RUNS (not one). The runner pairs ONE `metric` (one RESULT key)
// across A/B/B/A launches per invocation. This judge has THREE independently-
// attributable phase metrics (alloc_ns / free_ns / realloc_ns). Running the
// runner three times — once per phase — means each phase gets its own fresh
// A/B/B/A session with its own paired t-test / sign-test, computed by the
// EXISTING runner code (no stats logic is reimplemented here). The cost is
// 3× the process launches (3 × pairs × 4); each launch is sub-second, so the
// total session is well under a minute even at the default pairs=20.
//
// WHY THE ARMS ARE PRE-BUILT (no `build` step in the config). The runner's
// config schema has a single optional `build` command shared by both arms.
// But THIS task needs two DIFFERENT builds of conceptually the same probe
// (one without `medium-classes`, one with) — two separate `cargo build`
// invocations with different `--features` flags, producing two differently-
// named executables. So this driver builds both arms up front (step 1) and
// the config JSONs point at the resulting exe paths with NO `build` step —
// the runner just launches them.
//
// USAGE:
//   node scripts/r10_2_medium_gate.mjs                # full run: pairs=20 per phase (default)
//   node scripts/r10_2_medium_gate.mjs --quick         # smoke: pairs=4 per phase
//   node scripts/r10_2_medium_gate.mjs --pairs 10      # custom pair count
//   node scripts/r10_2_medium_gate.mjs --verify-only   # build + one sample per arm, no pairing
//   node scripts/r10_2_medium_gate.mjs --skip-build    # reuse already-built exes (for re-running stats)

import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = process.platform === 'win32';

// ── CLI args ──────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const quick = args.includes('--quick');
const pairsArg = args.find((a, i) => args[i - 1] === '--pairs');
const verifyOnly = args.includes('--verify-only');
const skipBuild = args.includes('--skip-build');

const PAIRS = pairsArg ? Number(pairsArg) : quick ? 4 : 20;

const PHASES = ['alloc', 'free', 'realloc'];
const ARM_OFF = 'paired_ab_medium_off';
const ARM_ON = 'paired_ab_medium_on';

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
  console.log('[r10-2] Building baseline arm (production, no medium-classes)...');
  const { code: codeOff } = await run(
    'cargo',
    ['build', '--release', '--example', ARM_OFF, '--features', 'production'],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (codeOff !== 0) throw new Error(`cargo build (off) failed (exit ${codeOff})`);

  console.log('[r10-2] Building treatment arm (production + medium-classes)...');
  const { code: codeOn } = await run(
    'cargo',
    ['build', '--release', '--example', ARM_ON, '--features', 'production,medium-classes'],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (codeOn !== 0) throw new Error(`cargo build (on) failed (exit ${codeOn})`);

  // Verify both exes exist.
  for (const [label, p] of [['off', EXE_OFF], ['on', EXE_ON]]) {
    if (!existsSync(p)) throw new Error(`expected exe not found: ${p} (${label})`);
  }
  console.log(`[r10-2] Both arms built:\n  off: ${EXE_OFF}\n  on : ${EXE_ON}`);
}

// ── Config JSON generation ────────────────────────────────────────────────
// Each phase gets its own config with `metric` set to that phase's RESULT key.
// Both arms point at the pre-built exes. The sanity gate requires
// segments_reserved_total > 0 in BOTH arms (both genuinely install SeferAlloc
// — unlike the built-in sefer-vs-mimalloc gate where only the sefer arm is
// nonzero).
function writeConfig(phase) {
  const cfg = {
    metric: `${phase}_ns`,
    arms: {
      A: { command: EXE_OFF, args: [] },
      B: { command: EXE_ON, args: [] },
    },
    sanity: {
      key: 'segments_reserved_total',
      nonzero_arms: ['A', 'B'],
    },
    features_note: 'A=production (medium-classes OFF), B=production,medium-classes (ON)',
  };
  const dir = `${REPO_ROOT}/docs/perf/paired_ab_runs`;
  mkdirSync(dir, { recursive: true });
  const path = `${dir}/_r10_2_${phase}.json`;
  writeFileSync(path, JSON.stringify(cfg, null, 2));
  return path;
}

// ── Invoke the runner once per phase ──────────────────────────────────────
function runPhase(phase, configPath) {
  const runnerArgs = ['scripts/paired-ab-runner.mjs', '--config', configPath];
  if (verifyOnly) {
    runnerArgs.push('--verify-only');
  } else {
    runnerArgs.push('--pairs', String(PAIRS));
  }
  console.log(`\n[r10-2] === Phase: ${phase} (${verifyOnly ? 'verify-only' : `${PAIRS} pairs`}) ===`);
  return new Promise((res, rej) => {
    const child = spawn('node', runnerArgs, { cwd: REPO_ROOT, stdio: 'inherit', shell: isWin });
    child.on('error', rej);
    child.on('close', (code) => (code === 0 ? res() : rej(new Error(`runner exited ${code} for phase ${phase}`))));
  });
}

async function main() {
  if (!skipBuild) {
    await buildArms();
  } else {
    console.log('[r10-2] --skip-build: reusing already-built exes.');
    for (const [label, p] of [['off', EXE_OFF], ['on', EXE_ON]]) {
      if (!existsSync(p)) throw new Error(`--skip-build but exe missing: ${p} (${label})`);
    }
  }

  const provenance = [];
  for (const phase of PHASES) {
    const cfgPath = writeConfig(phase);
    await runPhase(phase, cfgPath);
    provenance.push({ phase, config: cfgPath });
  }

  console.log('\n[r10-2] === All phases complete ===');
  console.log('[r10-2] Config files (generated, gitignored):');
  for (const { phase, config } of provenance) {
    console.log(`  ${phase}: ${config}`);
  }
  console.log('[r10-2] Provenance JSONs (one per phase) written to docs/perf/paired_ab_runs/');
  console.log('[r10-2] Read the three === A vs B === blocks above for the paired t-test / sign-test');
  console.log('       verdict per phase. See docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md for the full report.');
}

main().catch((e) => {
  console.error(`\n[r10-2] FAIL -- ${e.message}`);
  process.exit(1);
});
