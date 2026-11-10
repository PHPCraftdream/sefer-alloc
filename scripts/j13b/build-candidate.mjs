// CI-only current-candidate construction, not a workload or a complete arm manifest.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, realpathSync, renameSync, writeFileSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hashExecutable } from '../j13b_artifacts.mjs';
import { compilerArtifact } from './compiler-artifact.mjs';
import { trackedSource } from './tracked-source.mjs';

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));

export function buildCandidate(directory) {
  assert(process.platform === 'linux' && process.env.CI === 'true' && process.env.GITHUB_ACTIONS === 'true',
    'candidate builder requires reviewed native Linux GitHub CI');
  for (const [key, value] of Object.entries(process.env)) {
    if (value && /^(RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_(TARGET|RUSTC|RUSTC_WRAPPER|RUSTFLAGS)|CARGO_PROFILE_.*|CARGO_TARGET_.*_RUSTFLAGS)$/.test(key)) {
      throw Error(`uncontrolled compiler/build override: ${key}`);
    }
  }
  assert.equal(realpathSync(root), root, 'source root must not traverse symlinks');
  const output = resolve(root, directory);
  assert(/^\.rush\/j13b-builder-[A-Za-z0-9_-]+$/.test(relative(root, output)), 'invalid isolated builder directory');
  const parent = resolve(root, '.rush');
  if (existsSync(parent)) assert(lstatSync(parent).isDirectory() && realpathSync(parent) === parent, 'unsafe output parent');
  else mkdirSync(parent);
  assert(!existsSync(output), 'builder output must not already exist');
  mkdirSync(output);
  const run = (program, args, environment = process.env) => execFileSync('/usr/bin/timeout',
    ['--kill-after=10s', '600s', program, ...args], {
      cwd: root, env: { ...environment, LC_ALL: 'C' }, maxBuffer: 16 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
    });
  const text = bytes => new TextDecoder('utf-8', { fatal: true }).decode(bytes).trim();
  const commit = text(run('git', ['rev-parse', 'HEAD']));
  const source = trackedSource(output, commit);
  const rustc = text(run('rustup', ['which', 'rustc']));
  const cargo = text(run('rustup', ['which', 'cargo']));
  assert(isAbsolute(rustc) && isAbsolute(cargo), 'toolchain executables must be absolute');
  const compilerHash = hashExecutable(rustc);
  const cargoHash = hashExecutable(cargo);
  const version = text(run(rustc, ['-vV']));
  assert(version.length > 0 && Buffer.byteLength(version) <= 16384, 'compiler identity bound');
  const target = /^host: (x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)$/m.exec(version)?.[1];
  assert(target, 'unverified compiler host target');
  assert.equal(target.split('-')[0], process.arch === 'x64' ? 'x86_64' : process.arch === 'arm64' ? 'aarch64' : '',
    'compiler host must match native Node architecture');
  const env = { ...process.env, RUSTC: rustc, RUSTFLAGS: '', CARGO_ENCODED_RUSTFLAGS: '',
    CARGO_TARGET_DIR: resolve(output, 'target') };
  delete env.SEFER_J13B_BUILD;
  const metadata = JSON.parse(text(run(cargo, ['metadata', '--locked', '--no-deps', '--format-version', '1'], env)));
  const packages = metadata.packages?.filter(pkg => pkg.manifest_path === resolve(root, 'Cargo.toml') && pkg.name === 'sefer-alloc');
  assert(packages?.length === 1 && typeof packages[0].id === 'string', 'exact root package identity required');
  const expected = { packageId: packages[0].id,
    sourcePath: resolve(root, 'examples/g1_j13b_judge.rs'),
    executablePath: resolve(output, 'target', target, 'release/examples/g1_j13b_judge') };
  const args = ['build', '--locked', '--release', '--target', target,
    '--features', 'production internals bench-internals', '--example', 'g1_j13b_judge', '--message-format=json'];
  const firstBytes = run(cargo, args, env);
  const first = compilerArtifact(firstBytes, expected);
  writeFileSync(resolve(output, 'unsealed-cargo.jsonl'), firstBytes, { flag: 'wx' });
  const seal = { source_commit: commit, source_sha256: source.source_sha256,
    rustc: version, features: first.features.join(' '), variant: 'candidate',
    original_source_sha256: null, overlay_sha256: null };
  assert(Buffer.byteLength(JSON.stringify(seal)) <= 64 * 1024, 'compiled seal bound');
  const finalBytes = run(cargo, args, { ...env, SEFER_J13B_BUILD: JSON.stringify(seal) });
  const final = compilerArtifact(finalBytes, expected);
  writeFileSync(resolve(output, 'sealed-cargo.jsonl'), finalBytes, { flag: 'wx' });
  assert.deepEqual(final, first, 'source/target/features/profile changed across sealed build');
  assert.equal(text(run(rustc, ['-vV'])), version, 'compiler version changed');
  assert.equal(hashExecutable(rustc), compilerHash, 'compiler executable changed');
  assert.equal(hashExecutable(cargo), cargoHash, 'Cargo executable changed');
  assert.deepEqual(trackedSource(output, commit), source, 'tracked source changed during build');
  const artifact = { path: relative(output, final.executable), ...seal, binary_sha256: hashExecutable(final.executable) };
  const record = { schema: 'j13b.candidate-build.v1', artifact,
    provenance: { scope: 'reviewed trusted CI builder/compiler/host; no hostile-host attestation or payload admission',
      source_recipe: source.recipe, source_inventory_sha256: source.inventory_sha256,
      source_total_bytes: source.total_bytes, source_files: source.files,
      compiler_executable_sha256: compilerHash, cargo_executable_sha256: cargoHash,
      compiler_verbose: version, target, features: final.features,
      rustflags: '', cargo_encoded_rustflags: '', release: true, compiled_seal: seal,
      source_rechecked: true, toolchain_rechecked: true } };
  const temporary = resolve(output, 'candidate-build.pending');
  const destination = resolve(output, 'candidate-build.json');
  writeFileSync(temporary, `${JSON.stringify(record, null, 2)}\n`, { flag: 'wx' });
  assert(!existsSync(destination), 'candidate record must not overwrite another result');
  renameSync(temporary, destination);
  return Object.freeze({ record, directory: output, recordPath: destination });
}
