import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { resolve, relative, sep, isAbsolute } from 'node:path';
import { gzipSync } from 'node:zlib';
import { REPO_ROOT, winToWsl } from './lib.mjs';

const quote = value => `'${String(value).replaceAll("'", "'\\''")}'`;
function linux(program, args, environment = {}, unset = []) {
  if (process.platform !== 'win32') {
    const env = { ...process.env, ...environment };
    for (const key of unset) delete env[key];
    return command(program, args, env);
  }
  const assignments = Object.entries(environment).map(([key, value]) => `${key}=${quote(value)}`).join(' ');
  const removals = unset.map(key => `-u ${quote(key)}`).join(' ');
  return command('wsl', ['bash', '-lc', `cd ${quote(winToWsl(REPO_ROOT))} && env ${removals} ${assignments} ${[program, ...args].map(quote).join(' ')}`]);
}

const [mode, requestedDirectory, arm] = process.argv.slice(2);
assert(requestedDirectory, 'evidence directory required');
const directory = resolve(REPO_ROOT, requestedDirectory);
const evidenceRelative = relative(REPO_ROOT, directory);
assert(evidenceRelative && evidenceRelative !== '..' && !evidenceRelative.startsWith(`..${sep}`) && !isAbsolute(evidenceRelative), 'evidence must be a child of REPO_ROOT');
const beneathEvidence = path => {
  const inside = relative(directory, resolve(REPO_ROOT, path));
  return inside === '' || (!inside.startsWith(`..${sep}`) && inside !== '..' && !isAbsolute(inside));
};
const evidenceArgument = evidenceRelative.replaceAll('\\', '/');
const reproduce = (...args) => ['node', 'scripts/r18-perf-gates.mjs', ...args].map(quote).join(' ');
const collectionCommand = arm => `env RUSTC_WORKSPACE_WRAPPER='' ${reproduce('collect', evidenceArgument, arm)}`;
const generationCommand = reproduce('generate', evidenceArgument);
const SAMPLE_COUNT = 3;
const reportRelative = 'docs/perf/R35_SIDE_CAR_SCAN_PREFILTER_GATE.md';
const summaryRelative = 'docs/perf/R35_SIDE_CAR_SCAN_PREFILTER_GATE_summary.csv';
const rawLogRelative = (arm, oracle, sample) => `docs/perf/_raw_r35_sidecar_scan_test_oracle_final_${arm}_${oracle ? 'oracle' : 'timing'}_${sample}.log`;
const priorRawLogRelative = (arm, oracle, sample) => `docs/perf/_raw_r35_sidecar_scan_${arm}_${oracle ? 'oracle' : 'timing'}_${sample}.log`;
const rawLogPaths = [priorRawLogRelative, rawLogRelative].flatMap(logRelative =>
  ['base', 'candidate'].flatMap(arm => [true, false].flatMap(oracle =>
    Array.from({ length: SAMPLE_COUNT }, (_, index) => logRelative(arm, oracle, index + 1))
      .flatMap(path => [path, `${path}.gz`]))));
const excludedPaths = [reportRelative, summaryRelative, ...rawLogPaths];
const priorEvidenceRelatives = [
  'docs/perf/r35_sidecar_scan_evidence',
  'docs/perf/r35_sidecar_scan_evidence_final',
  'docs/perf/r35_sidecar_scan_evidence_cutover',
  'docs/perf/r35_sidecar_scan_evidence_production_guard',
  'docs/perf/r35_sidecar_scan_evidence_wrapper_pin',
];
const beneathPriorEvidence = path => priorEvidenceRelatives.some(root => {
  const inside = relative(resolve(REPO_ROOT, root), resolve(REPO_ROOT, path));
  return inside === '' || (!inside.startsWith(`..${sep}`) && inside !== '..' && !isAbsolute(inside));
});
const excluded = path => beneathEvidence(path) || beneathPriorEvidence(path) || excludedPaths.includes(path.replaceAll('\\', '/'));
const sourceSnapshot = () => {
  const base = command('git', ['rev-parse', 'HEAD']).trim();
  const exclusions = [evidenceRelative, ...priorEvidenceRelatives, ...excludedPaths].map(path => `:(exclude,literal)${path.replaceAll('\\', '/')}`);
  const patch = command('git', ['diff', '--binary', 'HEAD', '--', '.', ...exclusions]);
  const untracked = command('git', ['ls-files', '--others', '--exclude-standard', '-z'])
    .split('\0').filter(path => path && !excluded(path)).sort();
  assert(untracked.every(path => !excluded(path)), 'generated evidence entered source snapshot');
  const files = untracked.map(path => ({ path, hex: readFileSync(resolve(REPO_ROOT, path)).toString('hex') }));
  const sha256 = createHash('sha256').update(patch).update(JSON.stringify(files)).digest('hex');
  return { base, patch, files, sha256 };
};
const assertSnapshot = (before, message) => assert.deepEqual(sourceSnapshot(), before, message);
function saveRaw(path, log) {
  const bytes = Buffer.byteLength(log);
  assert(bytes <= 2 * 1024 * 1024, 'raw log exceeds CLAUDE storage ceiling; external storage/excerpt required before reporting');
  const saved = bytes < 200 * 1024 ? path : `${path}.gz`;
  writeFileSync(saved, bytes < 200 * 1024 ? log : gzipSync(log));
  return relative(REPO_ROOT, saved).replaceAll('\\', '/');
}
const names = ['r18_scan_growth', 'r18_scan_cycles', 'r18_scan_publication'];
const target = 'r18_sidecar_scan_iai';
const header = `${target}::r18_scan::`;
const features = 'production bench-internals internals';
const publicationWindow = 'IAI setup: owner allocates 34 blocks, one joined foreign producer GlobalAlloc-deallocates all 34; counted wrapper: two owner GlobalAlloc alloc/dealloc rounds with routed discovery and prepublished sidecar drain; producer creation/join/dealloc excluded';
const build_cfg = 'r18_sidecar_scan_bench';
const rustflags = `--cfg ${build_cfg}`;
const buildEnvironment = { RUSTC_WRAPPER: '', CARGO_BUILD_RUSTC_WRAPPER: '', RUSTC_WORKSPACE_WRAPPER: '', CARGO_TARGET_DIR: '/tmp/sefer-r18-scan', RUSTFLAGS: rustflags };
const benchArgs = ['bench', '--locked', '--bench', target, '--features', features];
function executableHash(executable) {
  const hash = linux('sha256sum', ['--', executable]).trim().split(/\s+/)[0];
  assert.match(hash, /^[a-f0-9]{64}$/);
  return hash;
}
function attestExecutable() {
  const log = linux('cargo', [...benchArgs, '--no-run', '--message-format=json'], buildEnvironment, ['CARGO_ENCODED_RUSTFLAGS']);
  const artifacts = log.split('\n').filter(line => line.startsWith('{')).map(JSON.parse)
    .filter(row => row.reason === 'compiler-artifact' && row.target.name === target && row.target.kind.includes('bench') && row.executable);
  assert.equal(artifacts.length, 1, 'expected exactly one dedicated bench executable');
  const executable = artifacts[0].executable;
  return { executable, executableSha256: executableHash(executable) };
}
function command(program, args, env = process.env) {
  const result = spawnSync(program, args, { encoding: 'utf8', env, cwd: REPO_ROOT });
  assert.equal(result.status, 0, `${program}: ${result.stdout}\n${result.stderr}`);
  return `${result.stdout ?? ''}${result.stderr ?? ''}`;
}
if (mode === 'collect') {
  assert(['linux', 'win32'].includes(process.platform), 'Linux or Windows/WSL required');
  assert(['base', 'candidate'].includes(arm));
  const out = resolve(directory, arm);
  assert(!existsSync(out), 'refuse overwriting an evidence directory');
  const before = sourceSnapshot();
  const { base, patch, files, sha256 } = before;
  const binary = attestExecutable();
  const otherIdentity = resolve(directory, arm === 'base' ? 'candidate' : 'base', 'identity.json');
  if (existsSync(otherIdentity)) {
    const other = JSON.parse(readFileSync(otherIdentity));
    assert.equal(other.build_cfg, build_cfg, 'both modes must use the dedicated build cfg');
    assert.equal(other.rustflags, rustflags, 'both modes must use the explicit Rust flags');
    assert.equal(other.rustc_workspace_wrapper, '', 'both modes must explicitly disable the workspace wrapper');
    assert.equal(binary.executableSha256, other.executableSha256, 'both modes must measure the same binary');
    assert.equal(sha256, other.sha256, 'both modes must measure the same source');
  }
  assertSnapshot(before, 'compilation changed source identity');
  mkdirSync(out, { recursive: true });
  writeFileSync(`${out}/source.patch`, patch);
  writeFileSync(`${out}/untracked.json`, JSON.stringify(files));
  const identity = {
    base,
    sha256, sampleCount: SAMPLE_COUNT, evidenceExcluded: evidenceRelative, priorEvidenceExcluded: priorEvidenceRelatives,
    reportsExcluded: [reportRelative, summaryRelative], rawLogsExcluded: rawLogPaths,
    features, build_cfg, rustflags, rustc_workspace_wrapper: buildEnvironment.RUSTC_WORKSPACE_WRAPPER,
    publicationWindow, scanMode: arm, layer: 'direct SeferAlloc GlobalAlloc alloc/dealloc -> HeapCore (not installed global allocator)',
    rustc: linux('rustc', ['--version', '--verbose']),
    cargo: linux('cargo', ['--version']),
    os: linux('uname', ['-srmo']).trim(),
    cpu: linux('bash', ['-lc', "sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo | sort -u"]).trim(),
    valgrind: linux('valgrind', ['--version']),
    ...binary, target,
    reproduction: collectionCommand(arm),
  };
  writeFileSync(`${out}/identity.json`, JSON.stringify(identity));
  assertSnapshot(before, 'writing identity/evidence changed source identity');
  for (const oracle of [true, false]) {
    for (let sample = 1; sample <= SAMPLE_COUNT; sample++) {
      const activation = `${out}/activation-${oracle}-${sample}.jsonl`;
      const linuxPath = path => process.platform === 'win32' ? winToWsl(path) : path;
      const env = { ...buildEnvironment, SEFER_R18_SCAN_ORACLE: oracle ? '1' : '0',
        SEFER_R18_SCAN_MODE: arm };
      if (oracle) env.SEFER_R18_SAMPLES = linuxPath(activation);
      // Normal entire group: no unsupported benchmark filters. Uncounted
      // children never inherit a sample path and perform no benchmark file IO.
      assert.equal(executableHash(binary.executable), binary.executableSha256, 'binary changed before measurement');
      const log = linux('cargo', benchArgs, env,
        ['CARGO_ENCODED_RUSTFLAGS', ...(oracle ? [] : ['SEFER_R18_SAMPLES'])]);
      assert.equal(executableHash(binary.executable), binary.executableSha256, 'binary changed during measurement');
      const rawLog = saveRaw(resolve(REPO_ROOT, rawLogRelative(arm, oracle, sample)), log);
      const activations = oracle ? readFileSync(activation, 'utf8').trim().split('\n').map(JSON.parse) : [];
      const rows = names.map(name => {
        const block = log.split(new RegExp(`(?=^${target}::)`, 'm')).find(text => text.startsWith(`${header}${name}`));
        assert(block, `missing ${name}`);
        const metric = label => {
          const match = new RegExp(`^\\s*${label}:\\s*([\\d,]+)`, 'm').exec(block);
          assert(match, `missing ${label} for ${name}`);
          return Number(match[1].replaceAll(',', ''));
        };
        const matches = activations.filter(row => row.name === name);
        if (oracle) {
          assert(matches.length > 0, `missing activation ${name}`);
          for (const row of matches) {
            assert(row.routed_scans > 0, `routed activation absent for ${arm}/${name}`);
            assert.deepEqual(row, matches[0], `activation differs between benchmark invocations for ${name}`);
          }
        }
        const activationRow = oracle ? matches[0] : {
          name, layer: 'SeferAlloc::GlobalAlloc->HeapCore', oracle: false,
          baseline: arm === 'base', publish: name === 'r18_scan_publication',
          rounds: name === 'r18_scan_cycles' ? 6 : 2,
          visited: null, empty_skipped: null, exchanges: null, nonempty_exchanges: null, routed_scans: null,
        };
        assert.equal(activationRow.baseline, arm === 'base');
        assert.equal(activationRow.oracle, oracle);
        assert.equal(activationRow.publish, name === 'r18_scan_publication');
        return { ...activationRow, layer: identity.layer, arm, sample, sha256: identity.sha256,
          build_cfg: identity.build_cfg, rustflags: identity.rustflags, rustc_workspace_wrapper: identity.rustc_workspace_wrapper,
          raw_log: rawLog, ir: metric('Instructions'), cycles: metric('Estimated Cycles') };
      });
      writeFileSync(`${out}/sample-${oracle}-${sample}.json`, JSON.stringify(rows));
    }
  }
  assertSnapshot(before, 'generated logs/data changed source identity');
} else if (mode === 'generate') {
  const before = sourceSnapshot();
  const rows = [];
  for (const current of ['base', 'candidate']) {
    const identity = JSON.parse(readFileSync(`${directory}/${current}/identity.json`));
    assert.equal(identity.sha256, before.sha256, 'saved measurement source differs from current source');
    assert.equal(identity.base, before.base);
    assert.equal(identity.evidenceExcluded, evidenceRelative);
    assert.deepEqual(identity.priorEvidenceExcluded, priorEvidenceRelatives);
    assert.deepEqual(identity.reportsExcluded, [reportRelative, summaryRelative]);
    assert.deepEqual(identity.rawLogsExcluded, rawLogPaths);
    assert.match(identity.sha256, /^[a-f0-9]{64}$/);
    assert.equal(identity.scanMode, current);
    const other = JSON.parse(readFileSync(`${directory}/${current === 'base' ? 'candidate' : 'base'}/identity.json`));
    assert.notEqual(identity.scanMode, other.scanMode);
    assert.equal(identity.sha256, other.sha256, 'same-binary source must match');
    assert.equal(identity.target, target);
    assert.equal(identity.features, features);
    assert.equal(identity.build_cfg, build_cfg);
    assert.equal(identity.rustflags, rustflags);
    assert.equal(identity.rustc_workspace_wrapper, '', 'workspace wrapper must be explicitly disabled');
    assert.equal(identity.publicationWindow, publicationWindow);
    assert.equal(identity.reproduction, collectionCommand(current));
    assert.match(identity.executableSha256, /^[a-f0-9]{64}$/);
    assert.equal(identity.executableSha256, other.executableSha256, 'same executable required for both modes');
    assert.equal(identity.sampleCount, SAMPLE_COUNT);
    assert.equal(identity.base, other.base);
    for (const key of ['features', 'build_cfg', 'rustflags', 'rustc_workspace_wrapper', 'rustc', 'cargo', 'os', 'cpu', 'valgrind']) assert.equal(identity[key], other[key]);
    for (const oracle of [true, false]) for (let sample = 1; sample <= SAMPLE_COUNT; sample++) {
      const data = JSON.parse(readFileSync(`${directory}/${current}/sample-${oracle}-${sample}.json`));
      assert.deepEqual(data.map(row => row.name).sort(), [...names].sort());
      for (const row of data) {
        assert.equal(row.sha256, identity.sha256);
        assert.equal(row.build_cfg, identity.build_cfg);
        assert.equal(row.rustflags, identity.rustflags);
        assert.equal(row.rustc_workspace_wrapper, identity.rustc_workspace_wrapper);
        assert.equal(row.layer, identity.layer);
        assert.equal(row.oracle, oracle);
        assert.equal(row.publish, row.name === 'r18_scan_publication');
        assert.equal(row.baseline, current === 'base');
        assert.equal(row.arm, current);
        assert.equal(row.sample, sample);
        assert(Number.isSafeInteger(row.ir) && row.ir > 0);
        assert(Number.isSafeInteger(row.cycles) && row.cycles > 0);
        if (oracle) {
          for (const key of ['visited', 'empty_skipped', 'exchanges', 'nonempty_exchanges', 'routed_scans']) {
            assert(Number.isSafeInteger(row[key]) && row[key] >= 0, `invalid ${key}`);
          }
          assert(row.routed_scans > 0, 'allocation-discovery routed scan absent');
          if (!row.baseline) assert.equal(row.exchanges, row.nonempty_exchanges);
          if (row.baseline) assert.equal(row.visited, row.exchanges);
          else assert.equal(row.visited, row.empty_skipped + row.exchanges);
          assert(row.visited > 0 && row.empty_skipped > 0, 'empty-word activation absent');
          if (row.publish) assert(row.nonempty_exchanges > 0, 'publication activation absent');
        } else {
          for (const key of ['visited', 'empty_skipped', 'exchanges', 'nonempty_exchanges', 'routed_scans']) assert.equal(row[key], null);
        }
        rows.push({ ...row, base_commit: identity.base, source_sha256: identity.sha256,
          features: identity.features, build_cfg: identity.build_cfg, rustflags: identity.rustflags,
          rustc_workspace_wrapper: identity.rustc_workspace_wrapper,
          publication_window: row.publish === true ? identity.publicationWindow : null, sample_count: identity.sampleCount,
          cpu: /^model name\s*:\s*(.+)$/m.exec(identity.cpu)?.[1] ?? identity.cpu.trim().split('\n')[0],
          os: identity.os.trim(), statistic: 'per-sample observation' });
      }
    }
  }
  const summaries = [];
  let table = '| Scenario | Arithmetic mean base Ir | Arithmetic mean candidate Ir | Candidate/base Ir | Arithmetic mean base cycles | Arithmetic mean candidate cycles | Candidate/base cycles |\n|---|---:|---:|---:|---:|---:|---:|\n';
  for (const name of names) {
    const mean = (current, metric) => {
      const samples = rows.filter(row => row.name === name && row.arm === current && !row.oracle);
      assert.equal(samples.length, SAMPLE_COUNT);
      assert.equal(new Set(samples.map(row => row.ir)).size, 1, 'Ir unstable: not hard-gate evidence');
      return samples.reduce((sum, row) => sum + row[metric], 0) / samples.length;
    };
    const base = mean('base', 'ir'), candidate = mean('candidate', 'ir');
    const ratio = candidate / base;
    assert(Math.abs(ratio * base - candidate) < 1e-8);
    const baseCycles = mean('base', 'cycles'), candidateCycles = mean('candidate', 'cycles');
    const cycleRatio = candidateCycles / baseCycles;
    assert(Math.abs(cycleRatio * baseCycles - candidateCycles) < 1e-8);
    summaries.push({ name, base_ir_mean: base, candidate_ir_mean: candidate,
      candidate_base_ir_ratio: ratio, base_cycles_mean: baseCycles,
      candidate_cycles_mean: candidateCycles, candidate_base_cycles_ratio: cycleRatio });
    table += `| ${name} | ${base} | ${candidate} | ${ratio} | ${baseCycles} | ${candidateCycles} | ${cycleRatio} |\n`;
  }
  writeFileSync(`${directory}/derived.json`, JSON.stringify(summaries));
  const csvRows = rows.map(row => ({ ...row, ...summaries.find(summary => summary.name === row.name) }));
  const columns = Object.keys(csvRows[0]);
  const csv = value => `"${String(value ?? '').replaceAll('"', '""')}"`;
  writeFileSync(resolve(REPO_ROOT, summaryRelative), [columns.join(','), ...csvRows.map(row => columns.map(key => csv(row[key])).join(','))].join('\n') + '\n');
  const identityTable = '| Arm | Base SHA | Source SHA256 | Features | Build cfg | Rustflags | Workspace wrapper | CPU | OS | Samples |\n|---|---|---|---|---|---|---|---|---|---:|\n' +
    ['base', 'candidate'].map(arm => {
      const row = rows.find(row => row.arm === arm);
      const cell = value => String(value).replaceAll('|', '\\|').replaceAll('\n', ' ');
      return `| ${arm} | ${row.base_commit} | ${row.source_sha256} | ${cell(row.features)} | ${cell(row.build_cfg)} | ${cell(row.rustflags)} | RUSTC_WORKSPACE_WRAPPER='${cell(row.rustc_workspace_wrapper)}' | ${cell(row.cpu)} | ${cell(row.os)} | ${row.sample_count} |`;
    }).join('\n') + '\n';
  const activationTable = '| Arm | Scenario | Sample | Routed scans | Visited | Empty | Exchanges | Nonempty exchanges | Ir | Estimated cycles |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n' +
    rows.filter(row => row.oracle).map(row => `| ${row.arm} | ${row.name} | ${row.sample} | ${row.routed_scans} | ${row.visited} | ${row.empty_skipped} | ${row.exchanges} | ${row.nonempty_exchanges} | ${row.ir} | ${row.cycles} |`).join('\n') + '\n';
  const logs = [...new Set(rows.map(row => row.raw_log))].map(path => `- ${path}`).join('\n');
  const reproduction = '\n## Reproduction (repository root)\n\n```sh\n' +
    ['base', 'candidate'].map(arm => JSON.parse(readFileSync(`${directory}/${arm}/identity.json`)).reproduction).join('\n') +
    '\n' + generationCommand + '\n```\n\nFeatures: `' + features + '`. Dedicated target: `' + target +
    '`. Build cfg: `' + build_cfg + '`. RUSTFLAGS: `' + rustflags + '`. ' +
    "The collector explicitly sets `RUSTC_WORKSPACE_WRAPPER=''` for both compilation and measurement, disabling the workspace wrapper (not unsetting it). " +
    'The collector unsets inherited CARGO_ENCODED_RUSTFLAGS for both compilation and measurement so the explicit RUSTFLAGS apply. ' +
    'On Windows, Windows Node launches Linux commands through WSL; Linux Node is not needed. ' +
    'For recollection, select a fresh evidence destination and use it consistently for both collect commands and generate; existing arm directories are never overwritten. ' +
    'Collection first uses cargo bench --no-run --message-format=json to locate the Linux executable, hashes it with sha256sum, ' +
    'requires the same executable SHA256 for both modes, and checks it before and after every measurement.\n\n' +
    ['base', 'candidate'].map(arm => {
      const identity = JSON.parse(readFileSync(`${directory}/${arm}/identity.json`));
      return `- ${arm} executable SHA256: ${identity.executableSha256}`;
    }).join('\n') + '\n';
  writeFileSync(resolve(REPO_ROOT, reportRelative), '# R35 sidecar scan prefilter measurement\n\nDirect SeferAlloc GlobalAlloc alloc/dealloc -> HeapCore; SeferAlloc is NOT installed as the process global allocator. Same-binary baseline-vs-candidate runtime modes, not two source revisions. Arithmetic means of uncounted runs; runtime-enabled oracle controls collected separately. Ratios are candidate mean / base mean. Primitive counts are absent (not zero) in uncounted samples. No verdict.\n\n' + identityTable + '\n## Publication counted boundary\n\n' + publicationWindow + '. Same setup in both runtime modes; concurrent correctness remains covered separately by native and Loom tests. Stable uncounted Ir is required for every scenario.\n\n' + table + '\n' + activationTable + reproduction + '\n## Saved raw logs\n\n' + logs + '\n');
  assertSnapshot(before, 'writing logs/JSON/Markdown/CSV changed source snapshot');
} else {
  throw new Error('usage: collect DIRECTORY base|candidate; generate DIRECTORY');
}
