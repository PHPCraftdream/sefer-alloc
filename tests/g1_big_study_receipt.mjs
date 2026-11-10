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
