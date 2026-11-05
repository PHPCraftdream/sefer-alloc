#!/usr/bin/env node
// Test-impact map (docs/CI_TIERS.md, CI-T1 stage 2): for every integration-test target, the set of repository
// files its instrumented binary executes. Generated nightly by .github/workflows/test-impact-map.yml as the
// artifact test-impact-map.json; stage 3 reads it to select tests from a diff. This script only produces and
// validates the map; nothing here selects tests.
//
// Usage (collect runs cargo, so it is a CI-only command):
//   node scripts/test-impact-map.mjs collect --features "<f>" --shard <i>/<n> --out part.json
//   node scripts/test-impact-map.mjs merge --out map.json part-0.json part-1.json ...
//   node scripts/test-impact-map.mjs check map.json     completeness against cargo metadata + paths exist
//   node scripts/test-impact-map.mjs --selftest
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import assert from 'node:assert/strict';

export const MAP_VERSION = 1;
/** Reasons a test is listed as unmapped: stage 3 must always run such a test. */
export const UNMAPPED_REASONS = new Set(['requires-features', 'test-failed', 'timeout', 'no-coverage']);
const RUN_TIMEOUT_MS = 15 * 60 * 1000;

/** `shard` is `i/n`; returns the targets of shard i (round-robin over the name-sorted list). */
export function shardOf(targets, shard) {
  const m = /^(\d+)\/(\d+)$/.exec(shard);
  assert.ok(m, `--shard must be i/n, got ${shard}`);
  const [i, n] = [Number(m[1]), Number(m[2])];
  assert.ok(n >= 1 && i < n, `shard ${shard} out of range`);
  return [...targets].sort((a, b) => a.path.localeCompare(b.path)).filter((_, k) => k % n === i);
}

/** Test targets of the root package from `cargo metadata --no-deps` JSON: [{ name, path, requiredFeatures }]. */
export function testTargets(metadata, root) {
  const out = [];
  for (const pkg of metadata.packages) {
    for (const t of pkg.targets) {
      if (!t.kind.includes('test')) continue;
      out.push({ name: t.name, path: toRel(t.src_path, root), requiredFeatures: t['required-features'] ?? [] });
    }
  }
  return out;
}

/** Repository-relative forward-slash path, or null when `p` lies outside `root`. */
export function toRel(p, root) {
  const r = relative(resolve(root), resolve(p));
  if (r === '' || r.startsWith('..') || /^[A-Za-z]:/.test(r)) return null;
  return r.split(sep).join('/');
}

/** Executable of test target `name` from `cargo test --no-run --message-format=json` output lines, or null. */
export function executableOf(jsonLines, name) {
  let exe = null;
  for (const line of jsonLines.split('\n')) {
    if (!line.startsWith('{')) continue;
    let m;
    try { m = JSON.parse(line); } catch { continue; }
    if (m.reason === 'compiler-artifact' && m.executable && m.profile?.test && m.target?.name === name && m.target.kind.includes('test')) exe = m.executable;
  }
  return exe;
}

/** Files with at least one covered line from `llvm-cov export --summary-only` JSON, repo-relative and sorted. */
export function coveredFiles(exportJson, root, own) {
  const files = new Set();
  for (const d of exportJson.data ?? []) {
    for (const f of d.files ?? []) {
      if ((f.summary?.lines?.covered ?? 0) === 0) continue;
      const rel = toRel(f.filename, root);
      if (rel && rel !== own) files.add(rel);
    }
  }
  return [...files].sort();
}

/** Union of shard parts; the same test in two parts, or differing row identity, is an error. */
export function mergeParts(parts) {
  assert.ok(parts.length > 0, 'no parts to merge');
  const [first, ...rest] = parts;
  const out = { version: MAP_VERSION, sha: first.sha, row: first.row, tests: {}, unmapped: {} };
  for (const p of [first, ...rest]) {
    assert.equal(p.version, MAP_VERSION, 'part version');
    assert.equal(p.sha, first.sha, 'parts come from different commits');
    assert.deepEqual(p.row, first.row, 'parts come from different rows');
    for (const [k, v] of Object.entries(p.tests)) {
      assert.ok(!(k in out.tests) && !(k in out.unmapped), `test ${k} appears in two parts`);
      out.tests[k] = v;
    }
    for (const [k, v] of Object.entries(p.unmapped)) {
      assert.ok(!(k in out.tests) && !(k in out.unmapped), `test ${k} appears in two parts`);
      out.unmapped[k] = v;
    }
  }
  return out;
}

/** Problems of a map against the declared test targets (`[{path}]`); empty means valid. */
export function checkMap(map, targets) {
  const problems = [];
  if (map.version !== MAP_VERSION) problems.push(`version ${map.version} != ${MAP_VERSION}`);
  if (!/^[0-9a-f]{40}$/.test(map.sha ?? '')) problems.push('sha is not a 40-hex commit id');
  if (typeof map.row?.features !== 'string') problems.push('row.features missing');
  const declared = new Set(targets.map(t => t.path));
  for (const t of declared) {
    const a = t in map.tests, b = t in map.unmapped;
    if (!a && !b) problems.push(`${t}: neither mapped nor listed as unmapped`);
    if (a && b) problems.push(`${t}: both mapped and unmapped`);
  }
  for (const k of [...Object.keys(map.tests), ...Object.keys(map.unmapped)]) {
    if (!declared.has(k)) problems.push(`${k}: not a test target of this commit`);
  }
  for (const [k, files] of Object.entries(map.tests)) {
    if (!Array.isArray(files) || files.length === 0) problems.push(`${k}: mapped to no files`);
    else if (files.some((f, i) => typeof f !== 'string' || f.startsWith('/') || /^[A-Za-z]:/.test(f) || (i > 0 && files[i - 1] >= f))) problems.push(`${k}: files must be sorted unique repo-relative paths`);
  }
  for (const [k, why] of Object.entries(map.unmapped)) {
    if (!UNMAPPED_REASONS.has(why)) problems.push(`${k}: unknown unmapped reason "${why}"`);
  }
  return problems;
}

function run(cmd, args, opts = {}) {
  return spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 1 << 28, ...opts });
}

function llvmTool(name) {
  const sysroot = run('rustc', ['--print', 'sysroot']).stdout.trim();
  const host = /^host: (.+)$/m.exec(run('rustc', ['-vV']).stdout)[1];
  const p = join(sysroot, 'lib', 'rustlib', host, 'bin', process.platform === 'win32' ? `${name}.exe` : name);
  assert.ok(existsSync(p), `${name} not found (rustup component add llvm-tools-preview)`);
  return p;
}

function collect({ features, shard, out }) {
  const root = process.cwd();
  const meta = JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1']).stdout);
  const mine = shardOf(testTargets(meta, root), shard);
  const profdata = llvmTool('llvm-profdata');
  const cov = llvmTool('llvm-cov');
  const tmp = join(root, 'target', 'test-impact');
  rmSync(tmp, { recursive: true, force: true });
  mkdirSync(tmp, { recursive: true });
  const env = { ...process.env, RUSTFLAGS: '-C instrument-coverage', CARGO_INCREMENTAL: '0' };
  const sha = run('git', ['rev-parse', 'HEAD']).stdout.trim();
  const rustc = run('rustc', ['--version']).stdout.trim();
  const map = { version: MAP_VERSION, sha, row: { features, rustc }, tests: {}, unmapped: {} };
  const buildFailed = [];
  for (const t of mine) {
    const b = run('cargo', ['test', '--features', features, '--test', t.name, '--no-run', '--message-format=json'], { env });
    const exe = b.status === 0 ? executableOf(b.stdout, t.name) : null;
    if (!exe) {
      if (/requires the features/.test(b.stderr)) map.unmapped[t.path] = 'requires-features';
      else { buildFailed.push(t.path); console.error(`build failed: ${t.path}\n${b.stderr.slice(-2000)}`); }
      continue;
    }
    const raw = join(tmp, `${t.name}-%p-%m.profraw`);
    const r = run(exe, [], { cwd: root, env: { ...env, LLVM_PROFILE_FILE: raw, CARGO_MANIFEST_DIR: root }, timeout: RUN_TIMEOUT_MS, stdio: 'ignore' });
    const raws = readdirSync(tmp).filter(f => f.startsWith(`${t.name}-`) && f.endsWith('.profraw')).map(f => join(tmp, f));
    let files = [];
    if (raws.length > 0) {
      const merged = join(tmp, `${t.name}.profdata`);
      const mg = run(profdata, ['merge', '-sparse', ...raws, '-o', merged]);
      if (mg.status === 0) {
        const ex = run(cov, ['export', exe, `-instr-profile=${merged}`, '--summary-only', '--ignore-filename-regex=(/\\.cargo/|/rustc/|/\\.rustup/)']);
        if (ex.status === 0) files = coveredFiles(JSON.parse(ex.stdout), root, t.path);
      }
    }
    for (const f of [exe, ...raws, join(tmp, `${t.name}.profdata`)]) rmSync(f, { force: true });
    if (r.error?.code === 'ETIMEDOUT') map.unmapped[t.path] = 'timeout';
    else if (r.status !== 0) map.unmapped[t.path] = 'test-failed';
    else if (files.length === 0) map.unmapped[t.path] = 'no-coverage';
    else map.tests[t.path] = files;
    console.log(`${t.path}: ${t.path in map.tests ? `${files.length} files` : map.unmapped[t.path]}`);
  }
  writeFileSync(out, `${JSON.stringify(map, null, 1)}\n`);
  if (buildFailed.length > 0) {
    console.error(`${buildFailed.length} test target(s) failed to build in this row: ${buildFailed.join(', ')}`);
    process.exit(1);
  }
}

function selftest() {
  const root = resolve('/repo');
  const p = rel => join(root, rel);
  // shards partition the targets exactly
  const ts = Array.from({ length: 10 }, (_, i) => ({ name: `t${i}`, path: `tests/t${i}.rs` }));
  const shards = [0, 1, 2].map(i => shardOf(ts, `${i}/3`));
  assert.deepEqual(shards.flat().map(t => t.path).sort(), ts.map(t => t.path).sort());
  assert.equal(new Set(shards.flat().map(t => t.path)).size, 10);
  assert.throws(() => shardOf(ts, '3/3'));
  assert.throws(() => shardOf(ts, 'x'));
  // metadata -> targets
  const meta = { packages: [{ targets: [
    { name: 'a', kind: ['test'], src_path: p('tests/a.rs'), 'required-features': ['x'] },
    { name: 'lib', kind: ['lib'], src_path: p('src/lib.rs') },
    { name: 'b', kind: ['test'], src_path: p('tests/b/main.rs') },
  ] }] };
  assert.deepEqual(testTargets(meta, root), [
    { name: 'a', path: 'tests/a.rs', requiredFeatures: ['x'] },
    { name: 'b', path: 'tests/b/main.rs', requiredFeatures: [] },
  ]);
  assert.equal(toRel(resolve('/other/x.rs'), root), null);
  // cargo json -> executable
  const lines = [
    'Compiling noise',
    JSON.stringify({ reason: 'compiler-artifact', executable: '/t/a-1', profile: { test: true }, target: { name: 'a', kind: ['test'] } }),
    JSON.stringify({ reason: 'compiler-artifact', executable: '/t/lib-1', profile: { test: true }, target: { name: 'a', kind: ['lib'] } }),
    JSON.stringify({ reason: 'compiler-artifact', executable: null, profile: { test: false }, target: { name: 'b', kind: ['test'] } }),
  ].join('\n');
  assert.equal(executableOf(lines, 'a'), '/t/a-1');
  assert.equal(executableOf(lines, 'b'), null);
  // llvm-cov export -> covered files (own file and out-of-repo files dropped)
  const exp = { data: [{ files: [
    { filename: p('src/z.rs'), summary: { lines: { covered: 3 } } },
    { filename: p('src/a.rs'), summary: { lines: { covered: 1 } } },
    { filename: p('src/dead.rs'), summary: { lines: { covered: 0 } } },
    { filename: p('tests/a.rs'), summary: { lines: { covered: 9 } } },
    { filename: resolve('/other/dep.rs'), summary: { lines: { covered: 5 } } },
  ] }] };
  assert.deepEqual(coveredFiles(exp, root, 'tests/a.rs'), ['src/a.rs', 'src/z.rs']);
  // merge + check
  const sha = 'a'.repeat(40);
  const row = { features: 'production internals', rustc: 'rustc 1.0' };
  const targets = [{ path: 'tests/a.rs' }, { path: 'tests/b.rs' }, { path: 'tests/c.rs' }];
  const part0 = { version: 1, sha, row, tests: { 'tests/a.rs': ['src/a.rs'] }, unmapped: {} };
  const part1 = { version: 1, sha, row, tests: { 'tests/b.rs': ['src/a.rs', 'src/b.rs'] }, unmapped: { 'tests/c.rs': 'requires-features' } };
  const map = mergeParts([part0, part1]);
  assert.deepEqual(checkMap(map, targets), []);
  assert.throws(() => mergeParts([part0, part0]), /two parts/);
  assert.throws(() => mergeParts([part0, { ...part1, sha: 'b'.repeat(40) }]), /different commits/);
  assert.throws(() => mergeParts([part0, { ...part1, row: { features: 'x' } }]), /different rows/);
  const bad = (mut, re) => {
    const m = JSON.parse(JSON.stringify(map));
    const t = targets.map(x => ({ ...x }));
    mut(m, t);
    assert.ok(checkMap(m, t).some(s => re.test(s)), `expected a problem matching ${re}`);
  };
  bad(m => { delete m.unmapped['tests/c.rs']; }, /neither mapped nor listed/);
  bad(m => { m.unmapped['tests/a.rs'] = 'test-failed'; }, /both mapped and unmapped/);
  bad((m, t) => t.pop(), /not a test target/);
  bad(m => { m.tests['tests/a.rs'] = []; }, /mapped to no files/);
  bad(m => { m.tests['tests/b.rs'] = ['src/b.rs', 'src/a.rs']; }, /sorted unique/);
  bad(m => { m.tests['tests/b.rs'] = ['/abs/x.rs']; }, /sorted unique/);
  bad(m => { m.unmapped['tests/c.rs'] = 'because'; }, /unknown unmapped reason/);
  bad(m => { m.sha = 'zz'; }, /40-hex/);
  bad(m => { m.version = 2; }, /version/);
  console.log('test-impact-map selftest: ok');
}

function arg(args, name) {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
}

function main(args) {
  if (args[0] === '--selftest') return selftest();
  if (args[0] === 'collect') {
    const [features, shard, out] = ['--features', '--shard', '--out'].map(f => arg(args, f));
    assert.ok(features !== undefined && shard && out, 'collect needs --features, --shard, --out');
    return collect({ features, shard, out });
  }
  if (args[0] === 'merge') {
    const out = arg(args, '--out');
    assert.ok(out, 'merge needs --out');
    const files = args.slice(1).filter((a, i, all) => a !== '--out' && all[i - 1] !== '--out');
    writeFileSync(out, `${JSON.stringify(mergeParts(files.map(f => JSON.parse(readFileSync(f, 'utf8')))), null, 1)}\n`);
    return undefined;
  }
  if (args[0] === 'check') {
    const map = JSON.parse(readFileSync(args[1], 'utf8'));
    const root = process.cwd();
    const meta = JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1']).stdout);
    const problems = checkMap(map, testTargets(meta, root));
    for (const [k, files] of Object.entries(map.tests)) {
      for (const f of files) if (!existsSync(join(root, f))) problems.push(`${k}: mapped file ${f} does not exist at this commit`);
    }
    if (problems.length > 0) {
      for (const s of problems) console.error(s);
      process.exit(1);
    }
    console.log(`test-impact map ok: ${Object.keys(map.tests).length} mapped, ${Object.keys(map.unmapped).length} unmapped`);
    return undefined;
  }
  console.error('usage: test-impact-map.mjs collect|merge|check|--selftest');
  process.exit(2);
}

if (process.argv[1]?.replace(/\\/g, '/').endsWith('scripts/test-impact-map.mjs')) main(process.argv.slice(2));
