// C1 only. No git writes, no other worktree access. --run is explicit opt-in.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, relative } from 'node:path';
import os from 'node:os';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function command(exe, args, env = process.env) {
  const result = spawnSync(exe, args, { cwd: root, env, encoding: 'utf8', timeout: 1_800_000, maxBuffer: 16 * 1024 * 1024 });
  // Do not copy build stderr (which contains private absolute paths) to artifacts.
  assert.equal(result.status, 0, `${exe} ${args.join(' ')} failed: ${result.error?.message ?? result.stderr}`);
  return result.stdout;
}
function safeArtifact(text) {
  for (const privatePath of [root, root.replaceAll('\\', '/'), os.homedir(), os.homedir().replaceAll('\\', '/')]) {
    assert(!text.toLowerCase().includes(privatePath.toLowerCase()), 'private path in artifact');
  }
  return text;
}
function put(dir, name, text) {
  assert(!existsSync(resolve(dir, name)), 'refusing artifact overwrite');
  writeFileSync(resolve(dir, name), safeArtifact(text), { flag: 'wx' });
}
function snapshotPatch() {
  let patch = command('git', ['diff', '--binary', 'HEAD', '--', '.']);
  const untracked = command('git', ['ls-files', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean);
  const sources = untracked.filter(p => /\.(rs|mjs|toml)$/.test(p) || /^docs\/perf\/R19_PRF04_.*\.md$/.test(p));
  for (const path of sources.sort()) {
    assert(/^[\w./-]+$/.test(path), 'unsupported patch filename');
    const text = readFileSync(resolve(root, path), 'utf8');
    assert(!text.includes('\0'), 'binary untracked source not supported');
    const lines = text.split('\n');
    const newline = lines.at(-1) === '';
    if (newline) lines.pop();
    patch += `diff --git a/${path} b/${path}\nnew file mode 100644\n--- /dev/null\n+++ b/${path}\n@@ -0,0 +1,${lines.length} @@\n`;
    patch += lines.map(line => `+${line}\n`).join('');
    if (!newline) patch += '\\ No newline at end of file\n';
  }
  assert(sources.includes('examples/r19_prf04_thread_wave_gate.rs') || command('git', ['ls-files', 'examples/r19_prf04_thread_wave_gate.rs']).trim(), 'harness missing from source identity');
  return safeArtifact(patch);
}
export function validate(row) {
  assert.equal(row.schema, 1);
  assert.equal(row.k, 32);
  assert([1, 128].includes(row.w));
  assert.equal(typeof row.diagnostic, 'boolean');
  assert.equal(row.maintenance_running, false);
  assert.equal(row.requested_config, 'LargeCacheConfig::new');
  assert.equal(row.config_conflicts_before, 0);
  assert.equal(row.config_conflicts_after, 0);
  assert.equal(row.count_before, 1);
  const expected = 1 + row.w * 31;
  assert.equal(row.count_delta, expected);
  assert.equal(row.count_after - row.count_before, expected);
  assert.equal(row.count_after, expected + 1); // main LIVE heap is additional
  for (const field of ['rss_before', 'rss_idle', 'rss_peak_sampled', 'segments_reserved_delta']) {
    assert(Number.isSafeInteger(row[field]) && row[field] >= 0, field);
  }
  assert(row.rss_idle > 0);
  if (row.diagnostic) {
    assert.deepEqual(row.claim_sources, [row.w - 1, expected, 0, 0, 0]);
    assert.equal(row.claim_sources.reduce((a, b) => a + b, 0), row.w * 32);
    assert.equal(row.resolved_default_matches, true);
    assert.deepEqual(row.resolved_config_results, [row.w * 32, 0]);
    assert.equal(row.wall_ns, null);
    assert.equal(row.first_workload_alloc_ns, null);
    assert(Number.isSafeInteger(row.routed_scans) && row.routed_scans >= 0);
  } else {
    assert.equal(row.claim_sources, null);
    assert.equal(row.resolved_config_results, null);
    assert.equal(row.routed_scans, null); // increments absent in plain production
    assert.equal(row.first_workload_alloc_ns.length, row.w * 32);
    assert(row.first_workload_alloc_ns.every(n => Number.isSafeInteger(n) && n >= 0));
    assert(Number.isSafeInteger(row.wall_ns) && row.wall_ns > 0);
  }
}
const median = xs => { const s = [...xs].sort((a, b) => a - b); assert.equal(s.length, 5); return s[2]; };
const quantile = (xs, q) => { const s = [...xs].sort((a, b) => a - b); return s[Math.ceil(q * s.length) - 1]; };
export function aggregate(rows) {
  assert.equal(rows.length, 12, 'exactly ten primary and two diagnostic samples');
  rows.forEach(validate);
  const primary = rows.filter(r => !r.diagnostic);
  assert.equal(primary.length, 10);
  const cells = [1, 128].map(w => {
    const cell = primary.filter(r => r.w === w);
    const diagnostic = rows.filter(r => r.diagnostic && r.w === w);
    assert.equal(cell.length, 5);
    assert.equal(diagnostic.length, 1);
    assert.equal(new Set(cell.map(r => r.repetition)).size, 5);
    assert(cell.every(r => r.repetition >= 1 && r.repetition <= 5));
    const commitAvailable = cell.every(r => Number.isSafeInteger(r.commit_idle) && r.commit_idle > 0);
    const times = cell.flatMap(r => r.first_workload_alloc_ns);
    return { w, n: 5, count_delta: 1 + w * 31, count_raw: 2 + w * 31,
      commit_idle_median: commitAvailable ? median(cell.map(r => r.commit_idle)) : null,
      rss_idle_median: median(cell.map(r => r.rss_idle)),
      rss_delta_median: median(cell.map(r => r.rss_idle - r.rss_before)),
      wall_ns_median: median(cell.map(r => r.wall_ns)),
      first_workload_p50_ns: quantile(times, 0.5), first_workload_p99_ns: quantile(times, 0.99),
      diagnostic_routed_scans: diagnostic[0].routed_scans };
  });
  const delta = cells.every(c => c.commit_idle_median !== null)
    ? cells[1].commit_idle_median - cells[0].commit_idle_median : null;
  if (delta !== null) assert.equal(delta + cells[0].commit_idle_median, cells[1].commit_idle_median);
  const columns = Object.keys(cells[0]);
  const csv = `${columns.join(',')}\n${cells.map(c => columns.map(k => c[k] ?? 'unavailable').join(',')).join('\n')}\n`;
  const table = `| ${columns.join(' | ')} |\n| ${columns.map(() => '---').join(' | ')} |\n${cells.map(c => `| ${columns.map(k => c[k] ?? 'unavailable').join(' | ')} |`).join('\n')}\n`;
  return { csv, markdown: `# C1 baseline measurement tables\n\n${table}\nCommit idle median difference (W128 minus W1), bytes: ${delta ?? 'unavailable — no commit-charge decision on this OS'}.\n\nThreshold: 67108864 bytes. This table is not a measured verdict; the orchestrator reviews validity and decides the next phase. Diagnostic timings are excluded.\n` };
}
function derive(dir) {
  const identity = JSON.parse(readFileSync(resolve(dir, 'identity.json'), 'utf8'));
  assert.equal(hash(readFileSync(resolve(dir, 'source.patch'))), identity.patch_sha256);
  const rows = JSON.parse(readFileSync(resolve(dir, 'samples.json'), 'utf8'));
  for (const row of rows) {
    assert.equal(row.patch_sha256, identity.patch_sha256);
    assert.equal(row.binary_sha256, identity.binaries[row.diagnostic ? 'diagnostic' : 'production'].sha256);
  }
  const result = aggregate(rows);
  put(dir, 'R19_PRF04_THREAD_WAVE_summary.csv', result.csv);
  put(dir, 'R19_PRF04_THREAD_WAVE_TABLES.md', result.markdown);
}
function run(dir) {
  assert(!existsSync(dir), 'fresh artifact directory required');
  const outputRelative = relative(resolve(root, 'docs/perf'), dir).replaceAll('\\', '/');
  assert(/^R19_PRF04_[\w-]+$/.test(outputRelative), 'output must be a direct docs/perf/R19_PRF04_* directory');
  const targetParent = resolve(root, 'target');
  assert(statSync(targetParent).isDirectory(), 'verified target parent required');
  const binaryDir = resolve(targetParent, `r19_prf04_${outputRelative.slice('R19_PRF04_'.length)}`);
  assert(!existsSync(binaryDir), 'fresh binary directory required');
  const patch = snapshotPatch(); // before building or measuring, includes untracked sources
  const base = command('git', ['rev-parse', 'HEAD']).trim();
  mkdirSync(dir);
  mkdirSync(binaryDir);
  put(dir, 'source.patch', patch);
  const env = { ...process.env, RUSTC_WRAPPER: '', CARGO_BUILD_JOBS: '3', CARGO_TARGET_DIR: resolve(root, 'target') };
  assert(!env.RUSTFLAGS && !env.CARGO_ENCODED_RUSTFLAGS && !env.CARGO_BUILD_TARGET, 'unset flags/target overrides for reproducibility');
  const rustc = command('rustc', ['-vV']);
  const cargo = command('cargo', ['--version']);
  const target = rustc.match(/^host: (.+)$/m)?.[1];
  assert(target);
  const binaries = {};
  for (const [mode, features] of [['production', 'production'], ['diagnostic', 'production bench-internals alloc-stats internals']]) {
    command('cargo', ['build', '--locked', '--release', '--example', 'r19_prf04_thread_wave_gate', '--features', features], env);
    const exe = resolve(root, 'target/release/examples/r19_prf04_thread_wave_gate' + (process.platform === 'win32' ? '.exe' : ''));
    const bytes = readFileSync(exe);
    const name = `${mode}${process.platform === 'win32' ? '.exe' : ''}`;
    const binaryPath = resolve(binaryDir, name);
    writeFileSync(binaryPath, bytes, { flag: 'wx', mode: 0o755 });
    const binaryRelative = relative(root, binaryPath).replaceAll('\\', '/');
    assert(/^target\/r19_prf04_[\w-]+\/(production|diagnostic)(\.exe)?$/.test(binaryRelative));
    const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--format-version', '1', '--features', features], env));
    // No manifest/target/source absolute paths are copied from cargo metadata.
    binaries[mode] = { name: binaryRelative, sha256: hash(bytes), requested_features: features,
      resolved_features: metadata.resolve.nodes.map(n => ({ id: n.id.replace(/path\+file:\/\/[^#]+#/g, 'local#'), features: n.features })) };
  }
  assert.equal(hash(snapshotPatch()), hash(patch), 'source changed during build');
  const identity = { base_sha: base, patch_sha256: hash(patch), rustc, cargo, target, binaries,
    os: { platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0]?.model },
    profile: 'release', locked: true, default_features: true };
  put(dir, 'identity.json', JSON.stringify(identity, null, 2) + '\n'); // BEFORE any measurement
  const rows = [];
  for (let repetition = 1; repetition <= 5; repetition++) {
    for (const w of repetition % 2 ? [1, 128] : [128, 1]) {
      const row = JSON.parse(command(resolve(root, binaries.production.name), [String(w)], env));
      Object.assign(row, { repetition, patch_sha256: identity.patch_sha256, binary_sha256: binaries.production.sha256 });
      validate(row);
      rows.push(row);
      put(dir, `primary_${repetition}_w${w}.json`, JSON.stringify(row) + '\n');
    }
  }
  for (const w of [1, 128]) {
    const row = JSON.parse(command(resolve(root, binaries.diagnostic.name), [String(w)], env));
    Object.assign(row, { repetition: 1, patch_sha256: identity.patch_sha256, binary_sha256: binaries.diagnostic.sha256 });
    validate(row);
    rows.push(row);
    put(dir, `diagnostic_w${w}.json`, JSON.stringify(row) + '\n');
  }
  assert.equal(hash(snapshotPatch()), identity.patch_sha256, 'source changed during measurement');
  put(dir, 'samples.json', JSON.stringify(rows, null, 2) + '\n');
  derive(dir);
}
function selfTest() {
  // Synthetic fixtures only: rejection controls, never allocator measurement.
  const fixture = (w, diagnostic, repetition) => ({ schema: 1, k: 32, w, diagnostic, repetition,
    maintenance_running: false, requested_config: 'LargeCacheConfig::new',
    resolved_default_matches: diagnostic, config_conflicts_before: 0, config_conflicts_after: 0,
    count_before: 1, count_after: 2 + w * 31, count_delta: 1 + w * 31,
    segments_reserved_delta: w * 32, rss_before: 100, rss_idle: 200 + w, rss_peak_sampled: 300 + w,
    commit_idle: 1000 + w * 1000000, claim_sources: diagnostic ? [w - 1, 1 + w * 31, 0, 0, 0] : null,
    resolved_config_results: diagnostic ? [w * 32, 0] : null,
    routed_scans: diagnostic ? 7 : null, wall_ns: diagnostic ? null : w * 100,
    first_workload_alloc_ns: diagnostic ? null : Array(w * 32).fill(10) });
  const rows = [1, 128].flatMap(w => [...Array.from({ length: 5 }, (_, i) => fixture(w, false, i + 1)), fixture(w, true, 1)]);
  assert(aggregate(rows).markdown.includes('127000000'));
  for (const mutate of [r => r.count_delta--, r => r.config_conflicts_after++, r => r.maintenance_running = true,
    r => r.claim_sources[1]--, r => r.resolved_default_matches = false, r => r.wall_ns = 1]) {
    const bad = structuredClone(rows);
    mutate(bad.find(r => r.diagnostic));
    assert.throws(() => aggregate(bad));
  }
  assert.throws(() => aggregate(rows.slice(1)));
  console.log('C1 aggregation self-test PASS (synthetic only)');
}
const [mode, path] = process.argv.slice(2);
if (mode === '--self-test') selfTest();
else if (mode === '--run' && path) run(resolve(root, path));
else if (mode === '--aggregate' && path) derive(resolve(root, path));
else throw new Error('usage: --self-test | --run docs/perf/R19_PRF04_<id> | --aggregate <existing run>');
