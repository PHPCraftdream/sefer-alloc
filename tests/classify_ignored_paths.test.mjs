import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, cpSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../', import.meta.url));
const tracked = 'tests/src_file_size_cap.rs';
const fresh = 'tests/new.rs';
const rustTest = '#[test]\nfn fixture() {}\n';
const modes = ['csv', 'json', 'report', 'summary', 'identity'];
const env = { ...process.env, CLASSIFY_TESTS_ALLOW_DIRTY: '1', GIT_OPTIONAL_LOCKS: '0' };

function put(root, path, text) {
  const file = join(root, path);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, text);
}

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'classify-ignored-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  // Borrow the real index without creating or changing any Git metadata.
  const dotGit = join(repo, '.git');
  const gitDir = lstatSync(dotGit).isFile()
    ? resolve(repo, readFileSync(dotGit, 'utf8').trim().replace(/^gitdir: /, ''))
    : dotGit;
  put(root, '.git', `gitdir: ${gitDir.replaceAll('\\', '/')}\n`);
  mkdirSync(join(root, 'scripts'));
  copyFileSync(join(repo, 'scripts/classify-tests.mjs'), join(root, 'scripts/classify-tests.mjs'));
  cpSync(join(repo, 'scripts/classify-tests'), join(root, 'scripts/classify-tests'), { recursive: true });
  put(root, 'Cargo.toml', '[package]\nname = "ignored-fixture"\nversion = "0.1.0"\n');
  put(root, 'src/lib.rs', 'pub fn with_config() {}\n');
  put(root, tracked, rustTest);
  put(root, fresh, rustTest);
  put(root, '.gitignore', `/${tracked}\ntarget/\n/tests/ignored_flat.rs\n/tests/fixture_dir/main.rs\nignored-local/\n`);

  const index = spawnSync('git', ['ls-files', '--', tracked, fresh], { cwd: root, env, encoding: 'utf8' });
  assert.equal(index.status, 0, index.stderr);
  assert.equal(index.stdout.trim(), tracked, 'fixture must contain a tracked test and an untracked test');
  return root;
}

function classify(root, mode) {
  const result = spawnSync(process.execPath, [join(root, 'scripts/classify-tests.mjs'), `--${mode}`], {
    cwd: root, env, encoding: 'utf8',
  });
  assert.equal(result.status, 0, `--${mode}: ${result.error ?? result.stderr}`);
  return result.stdout;
}

function inventory(root) {
  return classify(root, 'csv').trimEnd().split(/\r?\n/).slice(1)
    .map((line) => { const [path, , kind] = line.split(','); return [path, kind]; });
}

test('ignored compile-fail output adds neither files nor ghost directories', async (t) => {
  const root = fixture(t);
  const expected = [[fresh, 'target'], [tracked, 'target']];
  assert.deepEqual(inventory(root), expected);
  const before = new Map(modes.map((mode) => [mode, classify(root, mode)]));
  put(root, 'tests/compile_fail/x/target/junk.rs', rustTest);

  await t.test('CSV inventory contains only the tracked and new tests', () => {
    assert.deepEqual(inventory(root), expected);
  });
  for (const mode of modes) {
    await t.test(`--${mode} is unchanged after adding junk`, () => {
      assert.ok(classify(root, mode) === before.get(mode), `ignored junk changed --${mode}`);
    });
  }
});

test('ignored flat tests and directory entry points are not auto targets', (t) => {
  const root = fixture(t);
  put(root, 'tests/ignored_flat.rs', rustTest);
  put(root, 'tests/fixture_dir/main.rs', 'fn main() {}\n');
  put(root, 'tests/fixture_dir/helper.rs', 'pub fn helper() {}\n');
  assert.deepEqual(inventory(root), [
    ['tests/fixture_dir', 'directory-fixture'],
    ['tests/fixture_dir/helper.rs', 'support-module'],
    [fresh, 'target'],
    [tracked, 'target'],
  ]);
});

test('ignored Rust sources cannot contribute root or registry markers', (t) => {
  const root = fixture(t);
  put(root, 'src/ignored-local/markers.rs', 'pub fn claim_with_config() {}\n');
  put(root, 'crates/core/sefer-registry/src/ignored-local/markers.rs', 'pub fn claim_lease_with_config() {}\n');
  const { markerList } = JSON.parse(classify(root, 'json'));
  assert.deepEqual(markerList.map(({ marker }) => marker), ['with_config']);
});

test('ignored docs do not contribute pin surfaces or pin a test', (t) => {
  const root = fixture(t);
  const before = JSON.parse(classify(root, 'json')).pins;
  put(root, 'docs/ignored-local/pins.md', 'Run `cargo test --test src_file_size_cap`.\n');
  const { pins } = JSON.parse(classify(root, 'json'));
  assert.deepEqual(pins, before);
});
