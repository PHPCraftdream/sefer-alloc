// CI ONLY: these fixtures spawn jq to evaluate the actual receipt filter; do not execute locally.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

const action = readFileSync(new URL('../.github/actions/ci-j13b-judge-preparation/action.yml', import.meta.url), 'utf8');
const receiptLines = action.split(/\r?\n/).filter(line => line.includes('> "$receipt_dir/ci-build-host-observation.json"'));
assert.equal(receiptLines.length, 1, 'expected one actual CI build-host observation receipt command');
const receiptMatch = receiptLines[0].match(/jq -n .*? '(\{.*\})' > "\$receipt_dir\/ci-build-host-observation\.json"/);
assert.ok(receiptMatch, 'could not extract the receipt command jq filter');
const receiptFilter = receiptMatch[1];
const contextLines = action.split(/\r?\n/).filter(line => line.includes('> "$receipt_dir/build-context.json"'));
assert.equal(contextLines.length, 1, 'expected one actual build-context command');
const contextMatch = contextLines[0].match(/jq -n .*? '(\{.*\})' > "\$receipt_dir\/build-context\.json"/);
assert.ok(contextMatch, 'could not extract the actual build-context filter');

test('actual build-context filter preserves unset and explicit empty/nonempty compiler flags', () => {
  for (const flags of [null, '', '-C opt-level=3']) {
    const args = ['-c', '-n'];
    for (const [name, value] of Object.entries({ commit: 'fixture-commit', tree: 'fixture-tree', status: '', rustc: 'fixture-rustc', cargo: 'fixture-cargo', host: 'x86_64-unknown-linux-gnu', rustflags: flags ?? '', encoded_flags: flags ?? '' })) {
      args.push('--arg', name, value);
    }
    args.push('--argjson', 'rustflags_set', String(flags !== null), '--argjson', 'encoded_flags_set', String(flags !== null), contextMatch[1]);
    const result = spawnSync('jq', args, { encoding: 'utf8', timeout: 5000, maxBuffer: 64 * 1024 });
    assert.equal(result.error, undefined, result.error?.message);
    assert.equal(result.status, 0, result.stderr);
    const rows = result.stdout.trim().split(/\r?\n/);
    assert.equal(rows.length, 1);
    const context = JSON.parse(rows[0]);
    assert.equal(context.dirty, false);
    assert.equal(context.commit, 'fixture-commit');
    for (const key of ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']) {
      assert.deepEqual(context[key], { set: flags !== null, value: flags });
    }
  }
});

function observation(enabledRaw, meminfoRaw, enabledAvailable = true, meminfoAvailable = true) {
  return {
    base_page: { available: true, value: 4096 },
    enabled: { available: enabledAvailable, path: '/fixture/enabled', raw: enabledRaw },
    defrag: { available: true, path: '/fixture/defrag', raw: 'always [madvise] never\n' },
    hpage: { available: true, path: '/fixture/hpage', raw: '2097152\n' },
    meminfo: { available: meminfoAvailable, path: '/fixture/meminfo', raw: meminfoRaw },
    swaps: { available: true, path: '/fixture/swaps', raw: 'Filename Type Size Used Priority\n' },
    cpu: { available: true, path: '/fixture/cpu-pressure', raw: 'some avg10=0.00\n' },
    memory: { available: true, path: '/fixture/memory-pressure', raw: 'some avg10=0.00\n' },
    io: { available: true, path: '/fixture/io-pressure', raw: 'some avg10=0.00\n' },
    cpuinfo: { available: true, path: '/fixture/cpuinfo', raw: 'processor\t: 0\n' },
    uname: 'synthetic CI host',
  };
}

function evaluateReceipt(input) {
  const args = ['-c', '-n'];
  for (const name of ['base_page', 'enabled', 'defrag', 'hpage', 'meminfo', 'swaps', 'cpu', 'memory', 'io', 'cpuinfo']) {
    args.push('--argjson', name, JSON.stringify(input[name]));
  }
  args.push('--arg', 'uname', input.uname, receiptFilter);
  const result = spawnSync('jq', args, { encoding: 'utf8', timeout: 5000, maxBuffer: 64 * 1024 });
  assert.equal(result.error, undefined, `jq process failed to start: ${result.error?.message}`);
  assert.equal(result.status, 0, `jq exited ${result.status}: ${result.stderr}`);
  const rows = result.stdout.split(/\r?\n/).filter(line => line.length > 0);
  assert.equal(rows.length, 1, `expected one JSON row, got ${rows.length}: ${result.stdout}`);
  const value = JSON.parse(rows[0]);
  assert.equal(typeof value, 'object');
  assert.notEqual(value, null);
  assert.equal(Array.isArray(value), false);
  assert.equal(value.scope, 'CI build host observation only; not evidence about a later measurement host and not admission');
  assert.equal(value.uname, input.uname);
  assert.equal(value.base_page_size_bytes.value, 4096);
  assert.equal(value.transparent_hugepage.enabled.available, input.enabled.available);
  assert.equal(value.transparent_hugepage.enabled.raw, input.enabled.raw);
  assert.equal(value.transparent_hugepage.defrag.raw, input.defrag.raw);
  assert.equal(value.transparent_hugepage.hpage_pmd_size.raw, input.hpage.raw);
  assert.equal(value.meminfo.raw, input.meminfo.raw);
  assert.equal(value.pressure.cpu.raw, input.cpu.raw);
  assert.equal(value.cpuinfo.raw, input.cpuinfo.raw);
  return value;
}

test('readable sysfs and meminfo without selected markers produce one object with null parsed values', () => {
  const input = observation('no selected marker\n', 'no MemAvailable\n');
  const value = evaluateReceipt(input);
  assert.equal(value.transparent_hugepage.selected_mode, null);
  assert.equal(value.mem_available_kib, null);
});

test('unreadable sysfs and meminfo preserve null parsed values in one object', () => {
  const input = observation(null, null, false, false);
  const value = evaluateReceipt(input);
  assert.equal(value.transparent_hugepage.selected_mode, null);
  assert.equal(value.mem_available_kib, null);
});

test('valid selected mode and MemAvailable values remain typed and intact', () => {
  const input = observation('always [madvise] never\n', 'MemAvailable: 8388608 kB\n');
  const value = evaluateReceipt(input);
  assert.equal(value.transparent_hugepage.selected_mode, 'madvise');
  assert.equal(value.mem_available_kib, 8388608);
});

test('selected-mode no-match does not erase a valid memory observation', () => {
  const input = observation('no selected marker\n', 'MemAvailable: 8388608 kB\n');
  const value = evaluateReceipt(input);
  assert.equal(value.transparent_hugepage.selected_mode, null);
  assert.equal(value.mem_available_kib, 8388608);
});

test('memory no-match does not erase a valid selected-mode observation', () => {
  const input = observation('always [madvise] never\n', 'no MemAvailable\n');
  const value = evaluateReceipt(input);
  assert.equal(value.transparent_hugepage.selected_mode, 'madvise');
  assert.equal(value.mem_available_kib, null);
});

test('actual measurement preflight filters reject missing or wrong facts without fake admission', () => {
  const workflow = readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const hostLine = workflow.split('\n').find(line => line.includes('> "$evidence/measurement-host.json"'));
  const hostFilter = hostLine.match(/ '(\{.*\})' > "\$evidence\/measurement-host\.json"/)[1];
  const gateLine = workflow.split('\n').find(line => line.includes("jq -e '.os=="));
  const gate = gateLine.match(/jq -e '([^']+)'/)[1];
  const good = { before: '2026-11-11T00:00:00.000Z', os: 'Linux\n', arch: 'x86_64\n', pages: '4096\n', huge: '2097152\n', mode: 'always [madvise] never\n', mem: 'MemAvailable: 8388608 kB\n', cpu: 'synthetic CPU' };
  const cases = [{ patch: {}, pass: true }, { patch: { mode: 'no marker' } }, { patch: { mem: 'no memory fact' } }, { patch: { mem: 'MemAvailable: 8388607 kB\n' } }, { patch: { pages: '16384' } }, { patch: { huge: '4096' } }, { patch: { os: 'Windows' } }, { patch: { arch: 'aarch64' } }, { patch: { cpu: '' } }, { patch: { mode: '[always] madvise never' } }];
  for (const { patch, pass = false } of cases) {
    const args = ['-n'];
    for (const [key, value] of Object.entries({ ...good, ...patch })) args.push('--arg', key, value);
    const host = spawnSync('jq', [...args, hostFilter], { encoding: 'utf8', timeout: 5000 });
    assert.equal(host.status, 0, host.stderr);
    const parsed = JSON.parse(host.stdout);
    assert.equal(Object.hasOwn(parsed, 'active_swap'), false);
    assert.equal(Object.hasOwn(parsed, 'active_pressure'), false);
    const admitted = spawnSync('jq', ['-e', gate], { input: host.stdout, encoding: 'utf8', timeout: 5000 });
    assert.equal(admitted.status, pass ? 0 : 1, admitted.stderr);
  }
});

test('fixed cell templates retain every registered argument and explicit arm environment', () => {
  const cells = {
    'cycle-1MiB': ['cycle','sefer','1','1'], 'cycle-8MiB-minus-1': ['cycle','sefer','8','1'],
    'cycle-8MiB': ['cycle','sefer','8','1'], 'cycle-16MiB': ['cycle','sefer','16','1'],
    'cycle-128MiB': ['cycle','sefer','128','1'], 'cycle-1024MiB': ['cycle','sefer','1024','1'],
    'grow-1GiB': ['grow','sefer','1024','1'], 'many-128x16MiB': ['many','sefer','16','1','128'],
    'edge-128MiB': ['edge','sefer','128','1'], 'cache-16MiB': ['cycle','sefer','16','2','1'],
  };
  for (const [cell, args] of Object.entries(cells)) {
    const config = JSON.parse(readFileSync(new URL(`../docs/perf/j13_limited_cells/${cell}.json`, import.meta.url), 'utf8'));
    assert.deepEqual(Object.keys(config).sort(), ['arms','metric','result_format','timeout_ms']);
    assert.equal(config.result_format, 'json');
    assert.equal(config.metric, 'cold_ns');
    assert.equal(config.timeout_ms, 120000);
    assert.deepEqual(Object.keys(config.arms), ['A','B']);
    for (const [arm, mode] of [['A','off'], ['B','always']]) {
      assert.equal(config.arms[arm].command, '__CI_EXECUTABLE__');
      assert.deepEqual(config.arms[arm].args, args);
      assert.deepEqual(config.arms[arm].env, { G1_BIG_THP: mode, G1_BIG_CACHE: cell === 'cache-16MiB' ? 'on' : 'off', ...(cell === 'cycle-8MiB-minus-1' ? { G1_BIG_BYTES: '8388607' } : {}) });
    }
  }
});
