// Temp-repository CLI fixtures for classifier inventory/merge regressions.
// Every generated artifact and merge-file input stays entirely in os.tmpdir.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

export const REPORT_PATH = 'docs/perf/TEST_TARGET_CLASSIFICATION_20261115.md';
export const CSV_PATH = 'docs/test_target_classification_20261115.csv';

export function createInventoryTrees(repoRoot) {
  const scratch = mkdtempSync(join(tmpdir(), 'classify-inventory-merge-'));
  const gitResult = spawnSync('git', ['rev-parse', '--git-dir'], { cwd: repoRoot, encoding: 'utf8' });
  assert.equal(gitResult.status, 0, gitResult.stderr);
  const pointer = `gitdir: ${resolve(repoRoot, gitResult.stdout.trim()).replaceAll('\\', '/') }\n`;
  const names = ['base', 'left', 'right', 'union'];
  const trees = Object.fromEntries(names.map((name) => [name, join(scratch, name)]));
  const nodes = {
    base: ['alpha_middle'],
    left: ['alpha_delta', 'alpha_golf', 'alpha_middle'],
    right: ['alpha_middle', 'alpha_zeta'],
    union: ['alpha_delta', 'alpha_golf', 'alpha_middle', 'alpha_zeta'],
  };
  for (const name of names) {
    const root = trees[name];
    mkdirSync(join(root, 'scripts'), { recursive: true });
    mkdirSync(join(root, 'tests'), { recursive: true });
    mkdirSync(join(root, 'src'), { recursive: true });
    mkdirSync(join(root, 'docs', 'perf'), { recursive: true });
    mkdirSync(join(root, 'docs'), { recursive: true });
    writeFileSync(join(root, '.git'), pointer);
    writeFileSync(join(root, 'Cargo.toml'), '[package]\nname = "inventory-fixture"\nversion = "0.1.0"\n');
    writeFileSync(join(root, 'README.md'), 'Fixture references alpha_middle once.\n');
    writeFileSync(join(root, 'src', 'marker.rs'), 'pub fn fixture() {}\n');
    for (const node of nodes[name]) {
      writeFileSync(join(root, 'tests', `${node}.rs`), '#[test]\nfn test_node() {}\n');
    }
    copyFileSync(join(repoRoot, 'scripts', 'classify-tests.mjs'), join(root, 'scripts', 'classify-tests.mjs'));
    cpSync(join(repoRoot, 'scripts', 'classify-tests'), join(root, 'scripts', 'classify-tests'), { recursive: true });
  }
  return { scratch, trees, cleanup: () => rmSync(scratch, { recursive: true, force: true }) };
}

export function runClassifier(tree, flag) {
  return spawnSync(process.execPath, [join(tree, 'scripts', 'classify-tests.mjs'), flag], {
    cwd: tree, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
    env: { ...process.env, CLASSIFY_TESTS_ALLOW_DIRTY: '1', GIT_OPTIONAL_LOCKS: '0' },
  });
}

export function generateTree(tree) {
  const output = {};
  for (const [key, flag] of [['report', '--report'], ['csv', '--csv'], ['summary', '--summary']]) {
    output[key] = runClassifier(tree, flag);
    assert.equal(output[key].status, 0, `${flag} failed: ${output[key].stderr}`);
  }
  writeFileSync(join(tree, REPORT_PATH), output.report.stdout);
  writeFileSync(join(tree, CSV_PATH), output.csv.stdout);
  return output;
}

export function mergeInputs(scratch, label, versions) {
  const dir = join(scratch, 'merge-inputs', label);
  mkdirSync(dir, { recursive: true });
  return Object.fromEntries(Object.entries(versions).map(([name, contents]) => {
    const path = join(dir, `${name}.txt`);
    writeFileSync(path, contents);
    return [name, path];
  }));
}

export function mergeFile(left, base, right) {
  return spawnSync('git', ['merge-file', '-p', left, base, right], {
    encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
  });
}

export function csvValue(csv, targetPath, column) {
  const [header, ...rows] = csv.trimEnd().split(/\r?\n/);
  const columns = header.split(',');
  const row = rows.find((line) => line.startsWith(`${targetPath},`));
  assert.ok(row, `missing CSV row for ${targetPath}`);
  return row.split(',')[columns.indexOf(column)];
}
