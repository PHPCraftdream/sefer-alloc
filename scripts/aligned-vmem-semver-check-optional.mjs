#!/usr/bin/env node
// Optional semver-checks for aligned-vmem (item 65, task #1039).
//
// Runs `cargo semver-checks check-release --package aligned-vmem` ONLY if
// cargo-semver-checks is installed. If the tool is missing, prints a clear
// diagnostic and exits with code 0 (this is NOT an error swallow — the check
// is explicitly documented as optional when the tool is absent; see
// scripts/check-all.mjs's step comment for the rationale).
//
// This approach is used instead of `|| true` or `|| echo` because:
// - A missing tool is a CONFIGURATION absence, not a CODE failure.
// - An ACTUAL semver violation (tool present, but it reports a breakage) still
//   fails this script and therefore fails npm run check.
// - The distinction between "tool missing" and "tool present but broke" is
//   visible in the output, not silently collapsed to the same outcome.

import { spawn } from 'child_process';
import { REPO_ROOT } from './lib.mjs';

async function checkSemver() {
  console.log('[aligned-vmem-semver-check-optional] checking if cargo-semver-checks is installed...');

  // First, check if cargo-semver-checks is available
  const versionCheck = spawn('cargo', ['semver-checks', '--version'], {
    cwd: REPO_ROOT,
    stdio: 'pipe',
    shell: false,
  });

  const versionOutput = [];
  versionCheck.stdout.on('data', (data) => versionOutput.push(data));
  versionCheck.stderr.on('data', (data) => versionOutput.push(data));

  await new Promise((resolve) => versionCheck.on('close', resolve));
  const versionCode = versionCheck.exitCode ?? -1;

  if (versionCode !== 0) {
    console.log('[aligned-vmem-semver-check-optional] cargo-semver-checks not installed; skipping semver check (this is expected and NOT a failure)');
    console.log('[aligned-vmem-semver-check-optional] to enable this check, run: cargo install cargo-semver-checks');
    process.exit(0);
  }

  const versionString = Buffer.concat(versionOutput).toString().trim();
  console.log(`[aligned-vmem-semver-check-optional] found ${versionString}`);

  // Tool is present; run the actual check
  console.log('[aligned-vmem-semver-check-optional] running: cargo semver-checks check-release --package aligned-vmem');
  const semverCheck = spawn('cargo', ['semver-checks', 'check-release', '--package', 'aligned-vmem'], {
    cwd: REPO_ROOT,
    stdio: 'inherit',
    shell: false,
  });

  await new Promise((resolve) => semverCheck.on('close', resolve));
  const semverCode = semverCheck.exitCode ?? -1;

  if (semverCode === 0) {
    console.log('[aligned-vmem-semver-check-optional] semver check passed');
    process.exit(0);
  } else {
    console.log(`[aligned-vmem-semver-check-optional] semver check FAILED (exit ${semverCode})`);
    process.exit(semverCode);
  }
}

checkSemver().catch((err) => {
  console.error('[aligned-vmem-semver-check-optional] unexpected error:', err);
  process.exit(1);
});