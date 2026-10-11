// PRF03 executor and retained-evidence verifier; future arms are explicitly authorized CLI inputs.
import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, lstatSync, realpathSync, readdirSync, rmSync } from 'node:fs';
import { resolve, dirname, relative, isAbsolute, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';
import { deflateSync } from 'node:zlib';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const JUDGE = 'scripts/r19_prf03_large_route_retain_gate.mjs';
const PREREG = 'docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.md';
const FREEZE = PREREG.replace('.md', '.sha256');
const PRIMARY = ['production', 'bench-internals', 'internals'];
const PAIRED = [...PRIMARY, 'alloc-stats'];
const PAIRS = {
  R1: ['large_cache_prefill_only_4mib', 'large_cache_hit_only_4mib'],
  R2: ['large_cache_eviction_prefill_only_41x4mib', 'large_cache_eviction_hit_only_41x4mib'],
  R3: ['large_cache_payload_prefill_only_4mib', 'large_cache_payload_hit_only_4mib'],
};
const HARNESS = ['Cargo.toml', 'Cargo.lock', 'benches/perf_gate_iai.rs', 'benches/support/r19_prf03.rs',
  'examples/r19_prf03_paired_oracle.rs', 'examples/r19_prf03_native_mt.rs', JUDGE, PREREG, FREEZE,
  'scripts/r19_prf03_large_route_retain_run.mjs', 'scripts/iai.mjs', 'scripts/lib.mjs'];
const ROLES = { iai: ['perf_gate_iai', PRIMARY, 'bench'], primary: ['r19_prf03_paired_oracle', PRIMARY, 'example'],
  paired: ['r19_prf03_paired_oracle', PAIRED, 'example'], native: ['r19_prf03_native_mt', ['production'], 'example'],
  twin: ['r19_prf03_native_mt', PRIMARY, 'example'] };
const sha = b => createHash('sha256').update(b).digest('hex');
const json = x => JSON.stringify(x, null, 2) + '\n';
const now = () => new Date().toISOString();
const hex = x => assert.match(x, /^[a-f0-9]{64}$/);
const text = x => assert.ok(typeof x === 'string' && x.trim());
const int = x => assert.ok(Number.isSafeInteger(x) && x >= 0);
const feature = (got, expected) => assert.deepEqual([...got].sort(), [...expected].sort());
const quote = s => "'" + String(s).replaceAll("'", "'\\''") + "'";
const linuxPath = p => process.platform === 'win32' ? '/mnt/' + p[0].toLowerCase() + '/' + p.slice(3).replaceAll('\\', '/') : p;
function publicText(s) {
  return String(s).replace(/[A-Za-z]:[\\/][^\s"'<>|)]+/g, '<private>')
    .replace(/(^|[\s"'(=:,\[{])\/(?!\/)[^\s"'<>|)]+/gm, '$1<private>');
}
function inside(parent, child) {
  const r = relative(parent, child);
  assert.ok(r && !isAbsolute(r) && r !== '..' && !r.startsWith('..' + sep), 'path escapes worktree');
}
function local(parent, name) {
  assert.ok(typeof name === 'string' && !isAbsolute(name) && !name.split(/[\\/]/).includes('..'));
  const p = resolve(parent, name); inside(parent, p);
  let cursor = p;
  while (cursor !== parent) {
    if (existsSync(cursor)) assert.ok(!lstatSync(cursor).isSymbolicLink(), 'symlink rejected');
    cursor = dirname(cursor);
  }
  return p;
}
function put(dir, name, bytes, generated = false) {
  const p = local(dir, name); mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, generated ? publicText(bytes) : bytes, { flag: 'wx' });
  return { path: name, sha256: sha(readFileSync(p)) };
}
const putJson = (dir, name, value) => put(dir, name, json(value), true);
function get(dir, ref) {
  hex(ref.sha256); const bytes = readFileSync(local(dir, ref.path));
  assert.equal(sha(bytes), ref.sha256, 'retained byte/hash drift'); return bytes;
}
const getJson = (dir, ref) => JSON.parse(get(dir, ref));
function git(cwd, args) {
  assert.ok(['rev-parse', 'diff', 'ls-files', 'rev-list', 'merge-base', 'show'].includes(args[0]));
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd, encoding: null,
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' }, timeout: 60000, maxBuffer: 128 * 1024 * 1024 });
  assert.equal(r.status, 0, 'read-only git failed'); return r.stdout;
}
function ancestry(a, b, graph) {
  assert.match(a, /^[a-f0-9]{40}$/); assert.match(b, /^[a-f0-9]{40}$/);
  const parents = new Map(graph.trim().split('\n').filter(Boolean).map(l => {
    const [id, ...ps] = l.split(/\s+/); [id, ...ps].forEach(x => assert.match(x, /^[a-f0-9]{40}$/)); return [id, ps];
  }));
  const queue = [b], seen = new Set();
  while (queue.length) { const id = queue.pop(); if (id === a) return;
    if (!seen.has(id)) { seen.add(id); queue.push(...(parents.get(id) ?? [])); } }
  throw Error('A not ancestor of B');
}
function snapshot(cwd) {
  const baseHead = git(cwd, ['rev-parse', 'HEAD']).toString().trim();
  const tree = git(cwd, ['rev-parse', 'HEAD^{tree}']).toString().trim();
  const patch = git(cwd, ['diff', '--binary', 'HEAD', '--']);
  const tracked = git(cwd, ['ls-files', '-z']).toString().split('\0').filter(Boolean).sort();
  const untracked = git(cwd, ['ls-files', '--others', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean).sort();
  const ignored = git(cwd, ['ls-files', '--others', '--ignored', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean)
    .filter(p => /^(src|crates|benches|examples|\.cargo)\//.test(p) && !/(^|\/)target\//.test(p)
      || /^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?)$/.test(p)).sort();
  assert.equal(patch.length, 0, 'clean committed arm required: tracked patch present');
  assert.equal(untracked.length, 0, 'clean committed arm required: untracked inputs present');
  const paths = [...new Set([...tracked, ...untracked, ...ignored])].sort();
  const bytes = Object.fromEntries(paths.map(p => [p, readFileSync(local(cwd, p))]));
  const manifest = paths.map(p => [p, bytes[p].length, sha(bytes[p])]);
  const u = untracked.map(p => [p, bytes[p].length, sha(bytes[p])]);
  const source = { baseHead, tree, patchSha256: sha(patch), untrackedSha256: sha(JSON.stringify(u)),
    identitySha256: sha(JSON.stringify([baseHead, sha(patch), u])), capturedAt: now(),
    manifestSha256: sha(JSON.stringify(manifest)), ignoredInputs: ignored,
    harness: Object.fromEntries(HARNESS.map(p => { assert.ok(bytes[p], 'required harness absent'); return [p, sha(bytes[p])]; })) };
  return { source, patch, manifest, untracked: u, bytes };
}
function sourceStable(expected, actual) {
  for (const k of ['baseHead', 'tree', 'patchSha256', 'untrackedSha256', 'identitySha256', 'manifestSha256', 'ignoredInputs', 'harness'])
    assert.deepEqual(actual[k], expected[k], 'source drift');
}
function retainSource(dir, arm, s) {
  const blobs = {};
  for (const [p, b] of Object.entries(s.bytes)) {
    const name = 'source/' + sha(b) + '.bin';
    blobs[p] = { path: name, sha256: sha(b), byteLength: b.length };
    if (!existsSync(local(dir, name))) put(dir, name, b); // Exact sources are never publicText-filtered.
  }
  return { ...s.source, patch: put(dir, `${arm}.patch.bin`, s.patch),
    manifest: putJson(dir, `${arm}.manifest.json`, s.manifest),
    untracked: putJson(dir, `${arm}.untracked.json`, s.untracked), blobs };
}
function frozen(preregBytes, judgeBytes, companion) {
  const hashes = [{ path: PREREG, sha256: sha(preregBytes) }, { path: JUDGE, sha256: sha(judgeBytes) }];
  assert.equal(companion.toString(), hashes.map(x => `${x.sha256}  ${x.path}\n`).join(''), 'frozen hash drift'); return hashes;
}
function inventory(bytes) {
  const source = bytes.toString();
  const m = /library_benchmark_group!\(\s*name\s*=\s*perf_gate;\s*benchmarks\s*=([\s\S]*?)\);/.exec(source);
  assert.ok(m); const names = m[1].split(',').map(x => x.trim()).filter(Boolean);
  assert.equal(names.length, new Set(names).size);
  const bodies = name => [...source.matchAll(new RegExp('fn\\s+' + name + '\\s*\\([^)]*\\)\\s*\\{', 'g'))].map(match => {
    let depth = 1, end = match.index + match[0].length;
    for (; end < source.length && depth; end++) { if (source[end] === '{') depth++; if (source[end] === '}') depth--; }
    return source.slice(match.index, end);
  });
  const constants = Object.fromEntries([...source.matchAll(/const\s+(\w+):\s*usize\s*=\s*([^;]+);/g)].map(x => [x[1], x[2]]));
  const constant = name => {
    const expression = constants[name]?.replace(/\/\/.*$/gm, '').trim(); assert.ok(expression, 'work constant missing');
    if (/^\d+$/.test(expression)) return Number(expression);
    const m = /^(\w+)\s*([*/])\s*(\w+|\d+)$/.exec(expression); assert.ok(m, 'unknown work expression');
    const a = constant(m[1]), b = /^\d+$/.test(m[3]) ? Number(m[3]) : constant(m[3]);
    return m[2] === '*' ? a * b : a / b;
  };
  const declaredCount = (name, kind, body) => {
    if (kind === 'R1' || kind === 'R3') return { count: 9, unit: 'alloc/free pairs', expression: '8 prefix + 1 tail' };
    if (kind === 'R2') return { count: 82, unit: 'alloc/free pairs', expression: '2 * support::OBJECTS (41)' };
    if (name.startsWith('alloc_magazine_')) return { count: constant('PREFILL_CYCLES') * constant('MAGAZINE_FILL') + (/hit_only/.test(name) ? constant('MAGAZINE_FILL') : 0), unit: 'allocations (prefix frees included in body)', expression: 'PREFILL_CYCLES * MAGAZINE_FILL + hit-only MAGAZINE_FILL' };
    if (name.startsWith('large_cache_free_slot_search_')) return { count: constant('FREE_SLOT_SEARCH_DECOY_COUNT') + 1 + (/cycle_only/.test(name) ? constant('FREE_SLOT_SEARCH_CYCLES') : 0), unit: 'alloc/free pairs', expression: 'DECOY_COUNT + prime + cycle-only CYCLES' };
    if (name === 'realloc_grow') { assert.match(body, /doublings\s*=\s*16_u32/); return { count: 16, unit: 'realloc steps plus initial allocation/final free', expression: 'doublings' }; }
    if (name === 'large_alloc_free_cycle' || name === 'mimalloc_bootstrap_proxy') return { count: 1, unit: 'alloc/free pairs', expression: 'single cycle' };
    const loop = /0\.\.(CHURN_OPS(?:_2N)?)/.exec(body);
    if (loop) return { count: constant(loop[1]), unit: 'alloc/free pairs', expression: loop[1] };
    const batch = /\[\*mut u8; (COLD_BATCH(?:_2N|_4N)?)\]/.exec(body);
    assert.ok(batch, 'undeclared cell workload');
    const rounds = /recycle/.test(name) ? 2 : 1;
    return { count: constant(batch[1]) * rounds, unit: /alloc_only/.test(name) ? 'allocations' : 'alloc/free pairs', expression: `${batch[1]} * ${rounds}` };
  };
  const cells = [];
  for (const [index, name] of names.entries()) {
    const kind = Object.keys(PAIRS).find(k => PAIRS[k][1] === name) ?? (name.startsWith('mimalloc_') ? 'mimalloc'
      : /^(alloc_magazine_|cold_alloc_only_)/.test(name) || ['small_churn_16b', 'churn_256b'].includes(name) ? 'control'
        : name.startsWith('large_cache_free_slot_search_') || ['large_alloc_free_cycle', 'realloc_grow'].includes(name) ? 'affected' : null);
    if (!kind) continue;
    assert.match(name, /^\w+$/); const alternatives = bodies(name); assert.ok(alternatives.length);
    const body = alternatives.find(b => /claim_leaked_heap|SeferAlloc::new|MiMalloc/.test(b)) ?? alternatives[0];
    const claimPath = kind === 'mimalloc' ? 'mimalloc' : /claim_leaked_heap/.test(body) ? 'lease' : /SeferAlloc::new/.test(body) ? 'global' : null;
    assert.ok(claimPath, 'unknown heap claim path');
    cells.push({ name, kind, index, prefill: PAIRS[kind]?.[0] ?? null,
      prefillIndex: PAIRS[kind] ? names.indexOf(PAIRS[kind][0]) : null, claimPath,
      bodySha256: sha(body), declaredWorkCount: declaredCount(name, kind, body), declaredWork: { bodySha256: sha(body), constants } });
  }
  for (const k of Object.keys(PAIRS)) { assert.equal(cells.filter(c => c.kind === k).length, 1); assert.ok(names.includes(PAIRS[k][0])); }
  return cells;
}
export function makeTransport({ executable, prefix = [], path = linuxPath } = {}) {
  executable ??= process.platform === 'win32' ? 'wsl.exe' : 'bash';
  prefix = prefix.length ? prefix : process.platform === 'win32' ? ['bash', '-lc'] : ['-lc'];
  return {
    path,
    async execute(request) {
      const script = `cd ${quote(path(request.cwd))} && ${request.script}`;
      return new Promise((accept, reject) => {
        const child = spawn(executable, [...prefix, script], { cwd: request.cwd, shell: false,
          detached: process.platform !== 'win32', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
        let stdout = '', stderr = '', timedOut = false;
        const timer = setTimeout(() => {
          timedOut = true;
          if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
          else { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }
        }, request.timeoutMs ?? 600000);
        child.stdout.on('data', b => { stdout += b; }); child.stderr.on('data', b => { stderr += b; });
        child.on('error', e => { clearTimeout(timer); reject(e); });
        child.on('close', (code, signal) => { clearTimeout(timer); accept({ code, signal, timedOut, stdout, stderr }); });
      });
    },
  };
}
async function execute(ctx, request) {
  const id = request.id ?? randomUUID(); const startedAt = now();
  const result = await ctx.transport.execute({ ...request, id });
  const output = put(ctx.out, `logs/${id}.stdout.log`, result.stdout, true);
  const errors = put(ctx.out, `logs/${id}.stderr.log`, result.stderr, true);
  const receipt = { id, type: request.type, arm: request.arm, role: request.role, name: request.name ?? null,
    startedAt, endedAt: now(), code: result.code, signal: result.signal ?? null,
    timedOut: result.timedOut ?? false, stdout: output, stderr: errors };
  const ref = putJson(ctx.out, `logs/${id}.process.json`, receipt);
  assert.equal(result.code, request.expectedReject ? result.code : 0, 'child failed; exclusive diagnostic logs retained');
  assert.ok(!result.timedOut);
  if (request.expectedReject) {
    assert.notEqual(result.code, 0, 'negative workload unexpectedly accepted');
    assert.match(result.stderr, request.expectedReject, 'negative must fail its activation assertion');
  }
  return { ...result, receipt, ref };
}
async function environment(ctx, cwd, arm) {
  const script = 'set -e; rustc -Vv; cargo -V; uname -srm; python3 -c ' + quote('import json; rows=open("/proc/cpuinfo").read().splitlines(); print(json.dumps(sorted(set(l for l in rows if l.split(":")[0].strip() in ["vendor_id","model name","cpu family","model","stepping","siblings","cpu cores","flags"]))))')
    + '; valgrind --version; command -v iai-callgrind-runner; iai-callgrind-runner --version 2>&1 || true'
    // The effective (WSL/Linux) shell, not the launching Node process, decides compiler flags and iai/callgrind arguments.
    + '; if env | grep -Eq ' + quote('^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_TARGET|CARGO_BUILD_RUSTFLAGS|RUSTC|RUSTC_WORKSPACE_WRAPPER|CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER|CARGO_TARGET_[A-Z0-9_]*_RUSTFLAGS|CARGO_PROFILE_[A-Z0-9_]*|IAI_CALLGRIND_[A-Z0-9_]*)=')
    + '; then echo effective-environment-override-present >&2; exit 3; fi'
    // User-level Cargo configuration can inject rustflags/profiles; retain only its digest.
    + '; for f in "${CARGO_HOME:-$HOME/.cargo}/config.toml" "${CARGO_HOME:-$HOME/.cargo}/config"; do if [ -f "$f" ]; then echo "cargo-user-config $(sha256sum "$f" | cut -d" " -f1)"; fi; done; echo cargo-user-config-checked';
  const r = await execute(ctx, { cwd, script, type: 'environment', arm, timeoutMs: 60000 });
  assert.match(r.stdout, /rustc /); assert.match(r.stdout, /cargo /); assert.match(r.stdout, /0\.14\.2/); assert.match(r.stdout, /^cargo-user-config-checked$/m);
  const target = /^host: (.+)$/m.exec(r.stdout)?.[1]; text(target);
  return { rustc: /^rustc .*$/m.exec(r.stdout)?.[0], cargo: /^cargo .*$/m.exec(r.stdout)?.[0], target,
    os: publicText(r.stdout), cpu: publicText(r.stdout.split('model name').slice(1).join('model name')),
    allocator: 'installed SeferAlloc::new / HeapCore registry lease; mimalloc controls',
    flags: { RUSTFLAGS: process.env.RUSTFLAGS ?? '', CARGO_ENCODED_RUSTFLAGS: process.env.CARGO_ENCODED_RUSTFLAGS ?? '',
      CARGO_BUILD_TARGET: process.env.CARGO_BUILD_TARGET ?? '', profile: 'release' }, toolReceipt: r.ref };
}
async function build(ctx, arm, role) {
  const sourceArm = arm === 'C0' ? 'A' : arm, cwd = ctx.arms[sourceArm];
  sourceStable(ctx.snapshots[sourceArm].source, snapshot(cwd).source);
  const [name, features, kind] = ROLES[role]; const target = `target/gate-prf03-${arm}-${role}`;
  assert.ok(!existsSync(local(cwd, target)), 'build target exists; no overwrite');
  const script = `RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/${target}" cargo ${kind === 'bench' ? 'bench --no-run' : 'build --release'} --locked --message-format=json-render-diagnostics --${kind} ${quote(name)} --features ${quote(features.join(' '))}`;
  const r = await execute(ctx, { cwd, script, type: 'build', role, arm, timeoutMs: 1800000 });
  const artifacts = r.stdout.split(/\r?\n/).filter(l => l.startsWith('{')).map(l => JSON.parse(l))
    .filter(a => a.reason === 'compiler-artifact' && a.target.name === name && a.executable);
  assert.equal(artifacts.length, 1); const executable = artifacts[0].executable;
  const prefix = ctx.transport.path(cwd).replaceAll('\\', '/') + '/';
  assert.ok(executable.startsWith(prefix)); const binaryPath = executable.slice(prefix.length);
  assert.ok(binaryPath.startsWith(target + '/')); const bytes = readFileSync(local(cwd, binaryPath));
  sourceStable(ctx.snapshots[sourceArm].source, snapshot(cwd).source);
  const binary = put(ctx.out, `binaries/${arm}-${role}.bin`, bytes);
  return { arm, sourceArm, role, name, features, binaryPath, binary, binarySha256: binary.sha256,
    sourceIdentitySha256: ctx.snapshots[sourceArm].source.identitySha256, environmentSha256: ctx.envHash,
    buildId: r.receipt.id, capturedAt: now(), process: r.ref, locked: true, jobs: 3,
    rustcWrapper: '', cargoBuildRustcWrapper: '', profile: 'release' };
}
function binaryStable(ctx, b) {
  assert.equal(sha(readFileSync(local(ctx.arms[b.sourceArm], b.binaryPath))), b.binarySha256, 'built binary drift');
  get(ctx.out, b.binary);
}
function parseProfile(profile, log) {
  const pids = [...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m => Number(m[1])))];
  assert.equal(pids.length, 1, 'one retained valgrind PID required'); int(pids[0]); assert.ok(pids[0] > 0);
  const events = /^events: (.+)$/m.exec(profile)?.[1].split(/\s+/); assert.ok(events);
  const index = events.indexOf('Ir'); assert.ok(index >= 0);
  const totals = /^totals: (.+)$/m.exec(profile)?.[1].split(/\s+/).map(Number);
  const summary = /^summary: (.+)$/m.exec(profile)?.[1].split(/\s+/).map(Number);
  const counts = totals ?? summary; assert.ok(counts); int(counts[index]);
  assert.equal(Number(/^pid: (\d+)$/m.exec(profile)?.[1]), pids[0]);
  assert.match(profile, /__iai_callgrind_wrapper_mod/);
  return { pid: pids[0], instructions: counts[index] };
}
function walkFiles(dir) {
  const files = [];
  for (const name of readdirSync(dir)) {
    const p = local(dir, name), stat = lstatSync(p); assert.ok(!stat.isSymbolicLink());
    if (stat.isDirectory()) files.push(...walkFiles(p)); else if (stat.isFile()) files.push(p);
  }
  return files;
}
async function iaiArm(ctx, label, cells) {
  const arm = label === 'C0' ? 'C0' : label.startsWith('B') ? 'B' : 'A';
  const b = ctx.builds[`${arm}-iai`]; binaryStable(ctx, b);
  const id = randomUUID(), cwd = ctx.arms[b.sourceArm], home = `target/gate-prf03-${arm}-iai-observations-${id}`;
  assert.ok(!existsSync(local(cwd, home)));
  // Normal benchmark entry invokes installed iai 0.14.2; no cargo/rebuild or dispatch substitution.
  const script = ['./' + b.binaryPath, '--home', ctx.transport.path(local(cwd, home)), '--save-summary=json'].map(quote).join(' ');
  const r = await execute(ctx, { cwd, script, id, type: 'iai-arm', arm: label, role: 'iai', cells, timeoutMs: 1800000 });
  const metrics = new Map(); let current = null;
  for (const line of r.stdout.split(/\r?\n/)) {
    const header = /^(?:\w+::)+(\w+)\b/.exec(line); if (header) current = header[1];
    const count = /^\s*Instructions:\s*([\d,]+)/.exec(line);
    if (count && current) { assert.ok(!metrics.has(current)); metrics.set(current, Number(count[1].replaceAll(',', ''))); }
  }
  const files = walkFiles(local(cwd, home)); const observations = new Map();
  for (const name of [...new Set(cells.flatMap(c => [c.name, c.prefill].filter(Boolean)))]) {
    assert.ok(metrics.has(name), 'actual iai Instructions missing for inventory cell');
    const profiles = files.filter(p => /\.out(?:\.#\d+)?(?:\.\d+)?(?:-\d+)?$/.test(p) &&
      relative(local(cwd, home), dirname(p)).split(/[\\/]/).includes(name));
    assert.equal(profiles.length, 1, 'one current cell profile required');
    const profile = readFileSync(profiles[0], 'utf8');
    const profilePid = Number(/^pid: (\d+)$/m.exec(profile)?.[1]);
    const logs = files.filter(p => /\.log(?:\.#\d+)?$/.test(p) && readFileSync(p, 'utf8').includes(`==${profilePid}==`));
    assert.equal(logs.length, 1, 'one actual runner PID log required');
    const log = readFileSync(logs[0], 'utf8'), parsed = parseProfile(profile, log);
    assert.equal(parsed.instructions, metrics.get(name), 'actual iai/profile Instructions mismatch');
    const runId = `${id}-${name}`;
    const profileRef = put(ctx.out, `raw/${runId}.callgrind.log`, profile, true);
    const logRef = put(ctx.out, `raw/${runId}.valgrind.log`, log, true);
    const evidence = { id: runId, arm: label, role: 'iai', name, startedAt: r.receipt.startedAt, pid: parsed.pid,
      instructions: parsed.instructions, profile: profileRef, valgrind: logRef, process: r.ref,
      binarySha256: b.binarySha256, sourceIdentitySha256: b.sourceIdentitySha256, environmentSha256: ctx.envHash,
      features: b.features, actualIaiConsistency: { process: r.ref, name, instructions: metrics.get(name) } };
    observations.set(name, evidence);
  }
  // Retain unrelated group outputs as audit only; they never enter the frozen inventory.
  for (const [index, file] of files.entries()) put(ctx.out, `iai-audit/${id}/${index}.log`, readFileSync(file, 'utf8'), true);
  binaryStable(ctx, b); return observations;
}
function config(c, { mimalloc = false, nonRegime = false, nativeRaw = false } = {}) {
  assert.ok(c?.requested && c?.resolved);
  for (const k of ['cacheBudget', 'cacheSlots', 'maintenance', 'workCount']) assert.ok(k in c.requested && k in c.resolved);
  if (mimalloc) {
    for (const k of ['cacheBudget', 'cacheSlots', 'maintenance', 'workCount']) assert.equal(c.resolved[k], null);
  } else {
    assert.deepEqual(c.requested.cacheBudget, { policy: 'unbounded' }); assert.equal(c.requested.cacheSlots, 8);
    assert.equal(c.requested.maintenance, false); assert.equal(c.resolved.maintenance, false);
    if (!nativeRaw) { assert.deepEqual(c.resolved.cacheBudget, { policy: 'unbounded' }); assert.equal(c.resolved.cacheSlots, 8); }
    else { assert.equal(c.resolved.cacheBudget, null); assert.equal(c.resolved.cacheSlots, null); }
    if (nonRegime) { assert.equal(c.requested.workCount, null); assert.equal(c.resolved.workCount, null); }
    else { int(c.requested.workCount); assert.ok(c.requested.workCount > 0); assert.equal(c.resolved.workCount, c.requested.workCount); }
  }
  if (!nativeRaw) assert.deepEqual(c.requested, c.resolved);
  assert.equal(c.configConflictsDelta, 0); assert.equal(c.processBoundary, 'isolated-process');
  text(c.runId); int(c.pid); assert.ok(c.pid > 0);
}
function unavailable(row) {
  assert.ok(Object.hasOwn(row.oracle ?? {}, 'tailHitDelta'));
  assert.equal(row.oracle.tailHitDelta, null);
  assert.equal(row.routedScans, null); assert.equal(row.routedMissScans, null);
  assert.equal(row.routedScanSplit.prefill, null); assert.equal(row.routedScanSplit.tail, null);
  text(row.countersUnavailableReason ?? row.stats?.unavailableReason);
}
function oracleValid(row, role) {
  const probe = row.regime.startsWith('probe-');
  feature(row.features, role === 'paired' ? PAIRED : PRIMARY);
  config(row.config, { nonRegime: probe });
  text(row.requestedConfigDebug); assert.equal(row.requestedConfigDebugSha256, sha(row.requestedConfigDebug));
  assert.equal(row.processBoundary, 'isolated-process');
  assert.equal(row.pid, row.config.pid); assert.equal(row.runId, row.config.runId);
  if (probe) { assert.equal(row.claimPath, row.regime.slice(6)); return; }
  const s = row.structural; assert.ok(s);
  for (const k of ['slotsBefore', 'slotsAfterFree', 'cycleReservedDelta', 'cycleReleasedDelta', 'cachedCensus', 'payloadChanges']) int(s[k]);
  if (role === 'primary') unavailable(row);
  else {
    int(row.oracle.tailHitDelta); int(row.routedScans); int(row.routedMissScans);
    int(row.routedScanSplit.prefill); int(row.routedScanSplit.tail);
    assert.equal(row.routedScans, row.routedMissScans);
    assert.equal(row.routedScans, row.routedScanSplit.prefill + row.routedScanSplit.tail);
  }
  if (row.regime === 'R1') {
    if (row.negativeControl) {
      assert.ok(s.slotsAfterAlloc >= s.slotsBefore); assert.equal(s.allocReservedDelta, 1);
      if (role === 'paired') assert.equal(row.oracle.tailHitDelta, 0);
    } else {
      assert.deepEqual([s.slotsBefore, s.slotsAfterAlloc, s.slotsAfterFree], [1, 0, 1]);
      assert.deepEqual([s.allocReservedDelta, s.allocReleasedDelta, s.cycleReservedDelta, s.cycleReleasedDelta], [0, 0, 0, 0]);
      assert.equal(s.censusAfterAlloc, 1); assert.equal(s.cachedCensus, row.arm === 'A' ? 0 : 1);
      assert.equal(row.oracle.cachedCensus, s.cachedCensus);
      assert.equal(row.oracle.incarnationDelta, row.arm === 'A' ? 1 : 0);
      const r = s.routeConsistency;
      for (const k of ['samePointer', 'sameRoot', 'sameOwner', 'sameKind']) assert.equal(r[k], true);
      assert.equal(r.bracketAfter - r.bracketBefore - 1, row.oracle.incarnationDelta);
      assert.equal(r.tailIncarnation, row.arm === 'A' ? r.bracketBefore + 1 : r.prefixIncarnation);
      if (role === 'paired') assert.equal(row.oracle.tailHitDelta, 1);
    }
  } else if (row.regime === 'R2') {
    assert.deepEqual([s.slotsBefore, s.slotsAfterFree], [8, 8]);
    assert.deepEqual([s.cycleReservedDelta, s.cycleReleasedDelta], [33, 33]);
    assert.equal(row.description, 'eviction-dominated 33/41');
    if (role === 'paired') assert.equal(row.oracle.tailHitDelta, 8);
  } else if (row.regime === 'R3') {
    assert.equal(s.payloadChanges, 8); assert.equal(s.slotsAfterAlloc, s.slotsBefore - 1);
    assert.equal(s.slotsAfterFree, s.slotsBefore);
    assert.deepEqual([s.allocReservedDelta, s.allocReleasedDelta, s.cycleReservedDelta, s.cycleReleasedDelta], [0, 0, 0, 0]);
    if (role === 'paired') assert.equal(row.oracle.tailHitDelta, 1);
  } else throw Error('unknown oracle regime');
}
function structuralEqual(primary, paired) {
  assert.equal(primary.arm, paired.arm); assert.equal(primary.regime, paired.regime);
  assert.equal(primary.negativeControl, paired.negativeControl);
  assert.deepEqual(primary.structural, paired.structural);
  assert.deepEqual(primary.oracle.incarnationDelta, paired.oracle.incarnationDelta);
  assert.deepEqual(primary.oracle.cachedCensus, paired.oracle.cachedCensus);
  assert.deepEqual(primary.config.requested, paired.config.requested);
  assert.deepEqual(primary.config.resolved, paired.config.resolved);
  assert.equal(primary.requestedConfigDebugSha256, paired.requestedConfigDebugSha256);
}
function nativeValid(row, role, n, work) {
  feature(row.features, role === 'native' ? ['production'] : PRIMARY);
  assert.equal(row.role, role); assert.equal(row.n, n); assert.equal(row.latencySampleCount, work);
  assert.equal(row.heapsClaimedHighWaterDelta, n);
  assert.equal(row.readyDoneReservedDelta, 0); assert.equal(row.readyDoneReleasedDelta, 0);
  config(row.config, { nativeRaw: role === 'native' });
  assert.equal(row.config.requested.workCount, work); assert.equal(row.config.fallbackConfigConflictsDelta, 0);
  assert.equal(row.stats.large_cache_hits, null); assert.equal(row.stats.tcache_hits, null); text(row.stats.unavailableReason);
  assert.equal(row.workers.length, n);
  for (const [index, w] of row.workers.entries()) {
    assert.equal(w.worker, index); assert.equal(w.workCount, work / n);
    assert.equal(w.maintenance, false); assert.equal(w.maintenanceAfter, false);
    assert.equal(w.configConflictsDelta, 0); assert.equal(w.fallbackConfigConflictsDelta, 0);
    if (role === 'twin') { assert.equal(w.cacheSlots, 8); assert.deepEqual(w.cacheBudget, { policy: 'unbounded' }); }
    else { assert.equal(w.cacheSlots, null); assert.equal(w.cacheBudget, null); }
  }
  if (role === 'native') text(row.resolvedCacheUnavailableReason);
  assert.equal(row.requestedConfigDebugSha256, sha(row.requestedConfigDebug));
  for (const k of ['nsPerOp', 'p99Ns']) assert.ok(Number.isFinite(row[k]) && row[k] > 0);
}
async function observation(ctx, arm, role, args) {
  const b = ctx.builds[`${arm}-${role}`]; binaryStable(ctx, b);
  const rejects = args.includes('--negative-control') && ['R2', 'R3'].includes(args[0]);
  const rejectionPattern = args[0] === 'R2' ? /assertion `left == right` failed[\s\S]*left: \(0, 0\)[\s\S]*right: \(33, 33\)/ : /R3 payload did not change/;
  const id = randomUUID(); args = [...args, id];
  const negative = args.includes('--negative-control');
  if (negative) { args = args.filter(x => x !== '--negative-control'); args.push('--negative-control'); }
  const r = await execute(ctx, { cwd: ctx.arms[arm], script: ['./' + b.binaryPath, ...args].map(quote).join(' '),
    id, type: 'observation', role, arm, args, expectedReject: rejects ? rejectionPattern : null });
  if (rejects) {
    const rejectedPid = Number(/thread 'main' \((\d+)\)/.exec(r.stderr)?.[1]); int(rejectedPid); assert.ok(rejectedPid > 0, 'rejected oracle process PID absent');
    const envelope = { id, arm, role, regime: args[0], expectedReject: true, pid: rejectedPid, startedAt: r.receipt.startedAt,
      binarySha256: b.binarySha256, sourceIdentitySha256: b.sourceIdentitySha256, environmentSha256: ctx.envHash,
      process: r.ref, rejection: args[0] === 'R2' ? 'missing-tail-reserve-release' : 'unchanged-payload' };
    ctx.processes.push(envelope); binaryStable(ctx, b); return envelope;
  }
  const lines = r.stdout.trim().split(/\r?\n/); assert.equal(lines.length, 1, 'exactly one raw process object');
  const row = JSON.parse(lines[0]);
  const startedAt = new Date(Number(row.startedAtUnixMs)).toISOString();
  assert.ok(Date.parse(startedAt) >= Date.parse(r.receipt.startedAt) - 1000, 'invalid process start');
  row.requestedConfigDebugSha256 = sha(row.requestedConfigDebug);
  const raw = put(ctx.out, `raw/${id}.json`, lines[0] + '\n', true);
  const envelope = { id, arm, role, startedAt, binarySha256: b.binarySha256,
    sourceIdentitySha256: b.sourceIdentitySha256, environmentSha256: ctx.envHash, raw, process: r.ref,
    requestedConfigDebugSha256: row.requestedConfigDebugSha256 };
  const retained = putJson(ctx.out, `raw/${id}.envelope.json`, envelope);
  if (role === 'primary' || role === 'paired') oracleValid(row, role);
  else nativeValid(row, role, Number(args[0]), Number(args[1]));
  binaryStable(ctx, b);
  const entry = { ...envelope, envelope: retained, row }; ctx.processes.push(entry); return entry;
}
function provenance(entry, fields) {
  return { runId: entry.row.config.runId, pid: entry.row.config.pid, binarySha256: entry.binarySha256,
    rawSha256: entry.raw.sha256, receiptSha256: entry.raw.sha256, rawReceipt: entry.raw.path, copiedFields: fields };
}
function ownSource(s) {
  return Object.fromEntries(['baseHead', 'patchSha256', 'untrackedSha256', 'identitySha256', 'capturedAt'].map(k => [k, s[k]]));
}
async function pipeline(ctx) {
  assert.ok(!existsSync(ctx.out)); mkdirSync(ctx.out, { recursive: true });
  ctx.processes = []; ctx.snapshots = {}; ctx.builds = {}; ctx.rows = { iai: [], native: [] };
  for (const arm of ['A', 'B']) {
    ctx.arms[arm] = realpathSync(ctx.arms[arm]);
    ctx.snapshots[arm] = snapshot(ctx.arms[arm]);
  }
  assert.notEqual(ctx.arms.A, ctx.arms.B);
  const a = ctx.snapshots.A, b = ctx.snapshots.B;
  assert.notEqual(a.source.identitySha256, b.source.identitySha256);
  assert.deepEqual(a.source.harness, b.source.harness, 'harness mismatch');
  for (const p of HARNESS) assert.equal(a.source.harness[p], sha(readFileSync(local(ROOT, p))), 'worker harness differs');
  const graph = git(ctx.arms.B, ['rev-list', '--parents', b.source.baseHead]).toString();
  ancestry(a.source.baseHead, b.source.baseHead, graph);
  const hashes = frozen(a.bytes[PREREG], a.bytes[JUDGE], a.bytes[FREEZE]);
  assert.equal(sha(b.bytes[FREEZE]), sha(a.bytes[FREEZE]));
  const sources = { A: retainSource(ctx.out, 'A', a), B: retainSource(ctx.out, 'B', b) };
  const ancestryRef = put(ctx.out, 'ancestry.log', graph, true);
  const cells = inventory(a.bytes['benches/perf_gate_iai.rs']);
  assert.equal(cells.filter(c => c.kind === 'R2')[0].declaredWorkCount.count, 82);
  const supportBytes = a.bytes['benches/support/r19_prf03.rs'];
  assert.match(supportBytes.toString(), /OBJECTS:\s*usize\s*=\s*41\s*;/);
  const inv = putJson(ctx.out, 'inventory-receipt.json', { status: 'DRAFT', sourceSha256: a.source.harness['benches/perf_gate_iai.rs'],
    cells, note: 'source-derived bodies and constants retained; manual inventory completeness review required' });
  const environments = {};
  for (const arm of ['A', 'B']) environments[arm] = await environment(ctx, ctx.arms[arm], arm);
  const comparable = e => { const { toolReceipt, ...v } = e; return v; };
  assert.deepEqual(comparable(environments.A), comparable(environments.B), 'environment mismatch');
  const allocatorSourceHash = sha(JSON.stringify(a.manifest.filter(([p]) => p.startsWith('src/'))));
  const envRef = putJson(ctx.out, 'environment.json', { ...comparable(environments.A), allocatorSourceHash,
    armAllocatorSourceHashes: { A: allocatorSourceHash, B: sha(JSON.stringify(b.manifest.filter(([p]) => p.startsWith('src/')))) },
    capturedAt: now(), toolReceipts: Object.values(environments).map(e => e.toolReceipt) });
  ctx.envHash = envRef.sha256;
  for (const arm of ['A', 'B']) for (const role of Object.keys(ROLES)) ctx.builds[`${arm}-${role}`] = await build(ctx, arm, role);
  ctx.builds['C0-iai'] = await build(ctx, 'C0', 'iai');
  assert.notEqual(ctx.builds['A-iai'].binarySha256, ctx.builds['C0-iai'].binarySha256,
    'C0 identical bytes: INCOMPLETE; no metadata/behavior flag substitutions authorized');
  for (const arm of ['A', 'B']) sourceStable(ctx.snapshots[arm].source, snapshot(ctx.arms[arm]).source);
  for (const v of Object.values(ctx.builds)) binaryStable(ctx, v);
  ctx.allCapturedAt = now();
  putJson(ctx.out, 'all-identities-before-measurement.json', { capturedAt: ctx.allCapturedAt, builds: ctx.builds, environment: envRef, sources });
  const probes = {};
  for (const arm of ['A', 'B']) for (const claim of ['lease', 'global'])
    probes[`${arm}-${claim}`] = await observation(ctx, arm, 'primary', [`probe-${claim}`, arm]);
  const negativePairs = [];
  for (const arm of ['A', 'B']) {
    const primary = await observation(ctx, arm, 'primary', ['R1', arm, '--negative-control']);
    const paired = await observation(ctx, arm, 'paired', ['R1', arm, '--negative-control']);
    structuralEqual(primary.row, paired.row); negativePairs.push({ arm, regime: 'R1', primary: primary.id, paired: paired.id });
    for (const regime of ['R2', 'R3']) {
      const p = await observation(ctx, arm, 'primary', [regime, arm, '--negative-control']);
      const q = await observation(ctx, arm, 'paired', [regime, arm, '--negative-control']);
      negativePairs.push({ arm, regime, primary: p.id, paired: q.id });
    }
  }
  const iaiRows = [];
  for (const label of ['A1', 'B1', 'A2', 'B2', 'C0']) {
    const measured = await iaiArm(ctx, label, cells);
    for (const cell of cells) {
    const arm = label.startsWith('B') ? 'B' : 'A'; let primary, paired;
    if (PAIRS[cell.kind]) {
      primary = await observation(ctx, arm, 'primary', [cell.kind, arm]);
      paired = await observation(ctx, arm, 'paired', [cell.kind, arm]);
      structuralEqual(primary.row, paired.row);
    }
    const hit = measured.get(cell.name); ctx.processes.push(hit);
    const prefill = cell.prefill ? measured.get(cell.prefill) : null;
    if (prefill) ctx.processes.push(prefill);
    const probe = paired ?? (cell.kind === 'mimalloc' ? null : probes[`${arm}-${cell.claimPath}`]);
  const requested = cell.kind === 'mimalloc' ? { cacheBudget: null, cacheSlots: null, maintenance: null, workCount: null }
      : { ...probe.row.config.requested, workCount: paired ? probe.row.config.requested.workCount : null };
    const resolved = cell.kind === 'mimalloc' ? { ...requested } : { ...probe.row.config.resolved, workCount: requested.workCount };
    const c = { requested, resolved, configConflictsDelta: probe?.row.config.configConflictsDelta ?? 0,
      processBoundary: 'isolated-process', runId: hit.id, pid: hit.pid };
    if (probe) c.resolvedProvenance = provenance(probe, ['cacheBudget', 'cacheSlots', 'maintenance', 'configConflictsDelta']);
    const row = { name: cell.name, arm: label, environmentSha256: ctx.envHash, binarySha256: hit.binarySha256,
      source: ownSource(sources[arm]), startedAt: hit.startedAt, features: PRIMARY, oracleFeatures: PAIRED,
      receipt: hit.profile.path, config: c, hit: hit.instructions, prefill: prefill?.instructions ?? null,
      routedScans: paired?.row.routedScans ?? null, activation: paired ? true : null,
      oracle: paired?.row.oracle ?? null };
    const rowReceipt = putJson(ctx.out, `rows/${hit.id}.iai-row.json`, row);
    ctx.rows.iai.push(row); iaiRows.push({ row, rowReceipt, hit: hit.id, prefill: prefill?.id ?? null,
      primary: primary?.id ?? null, paired: paired?.id ?? null, probe: probe?.id ?? null });
  }
  }
  const nativePairs = [];
  for (const n of [1, 4, 8]) for (let sample = 0; sample < 9; sample++) {
    const labels = sample % 2 === 0 ? ['A', 'B', 'AA'] : ['B', 'AA', 'A'];
    for (const label of labels) {
      const arm = label === 'B' ? 'B' : 'A';
      const twin = await observation(ctx, arm, 'twin', [String(n), String(ctx.work)]);
      const native = await observation(ctx, arm, 'native', [String(n), String(ctx.work)]);
      assert.equal(twin.row.requestedConfigDebugSha256, native.row.requestedConfigDebugSha256);
      const c = structuredClone(native.row.config);
      c.resolved.cacheBudget = twin.row.config.resolved.cacheBudget; c.resolved.cacheSlots = twin.row.config.resolved.cacheSlots;
      c.resolvedProvenance = provenance(twin, ['cacheBudget', 'cacheSlots']); config(c);
      const row = { n, arm: label, sample, source: ownSource(sources[arm]), startedAt: native.startedAt,
        environmentSha256: ctx.envHash, binarySha256: native.binarySha256, features: ['production'], receipt: native.raw.path,
        config: c, requestedConfigDebugSha256: native.requestedConfigDebugSha256, nsPerOp: native.row.nsPerOp, p99Ns: native.row.p99Ns };
      const rowReceipt = putJson(ctx.out, `rows/${native.id}.native-row.json`, row);
      ctx.rows.native.push(row); nativePairs.push({ row, rowReceipt, twin: twin.id, native: native.id });
    }
  }
  for (const arm of ['A', 'B']) sourceStable(ctx.snapshots[arm].source, snapshot(ctx.arms[arm]).source);
  for (const v of Object.values(ctx.builds)) binaryStable(ctx, v);
  const reviewNotePath = 'docs/reviews/2026-10-11-prf03-rust-boundary-draft.md';
  const reviewNote = put(ctx.out, 'source/A-only-review-note.bin', readFileSync(local(ROOT, reviewNotePath)));
  const aSourceHash = sha(JSON.stringify(a.manifest.filter(([p]) => p.startsWith('src/'))));
  const review = putJson(ctx.out, 'feature-exact-DRAFT.json', { status: 'DRAFT', sourceIdentities: [sources.A.identitySha256, sources.B.identitySha256],
    builds: ctx.builds, negativePairs, iaiRows: iaiRows.filter(r => r.primary), nativePairs,
    AOnlyExitTableReceipt: { note: reviewNote, allocatorSourceSha256: aSourceHash },
    note: 'Not reviewed. Owner must attest cache exits and feature transfer, all binary/source/raw linkages and inventory.' });
  const input = { version: 1, hashes, environmentSha256: ctx.envHash, inventoryReceipt: inv.path,
    featureExactReceipt: review.path, inventory: cells.map(({ name, kind }) => ({ name, kind })), ...ctx.rows };
  const bundle = { version: 1, environment: envRef, sources, ancestry: ancestryRef, builds: ctx.builds,
    allCapturedAt: ctx.allCapturedAt, inventory: inv, featureExactDraft: review, processes: ctx.processes.map(({ row, ...p }) => p),
    negativePairs, iaiRows, nativePairs, work: ctx.work, workDefaulted: ctx.workDefaulted ?? false, review: null,
    input: putJson(ctx.out, 'input.json', input),
    limitations: ['Normal iai benchmark entry runs the whole group in a future explicitly authorized --run; only frozen inventory cells enter the judge. Unrelated outputs retained as audit.'] };
  putJson(ctx.out, 'bundle.json', bundle);
  return aggregate(ctx.out, { reviewFile: ctx.reviewFile });
}
export function aggregate(dir, { reviewFile, synthetic = false } = {}) {
  const bundle = JSON.parse(readFileSync(local(dir, 'bundle.json'))); assert.equal(bundle.version, 1);
  const input = getJson(dir, bundle.input), env = getJson(dir, bundle.environment);
  assert.equal(input.environmentSha256, bundle.environment.sha256);
  for (const k of ['rustc', 'cargo', 'target', 'os', 'cpu', 'allocator', 'flags', 'armAllocatorSourceHashes']) assert.ok(k in env);
  const sources = bundle.sources;
  for (const arm of ['A', 'B']) {
    const s = sources[arm], manifest = getJson(dir, s.manifest), u = getJson(dir, s.untracked);
    assert.equal(sha(JSON.stringify(manifest)), s.manifestSha256);
    assert.equal(sha(JSON.stringify(u)), s.untrackedSha256);
    assert.equal(get(dir, s.patch).length, 0, 'retained source is not clean');
    assert.deepEqual(u, [], 'retained untracked source rejected');
    assert.equal(sha(get(dir, s.patch)), s.patchSha256);
    assert.equal(s.identitySha256, sha(JSON.stringify([s.baseHead, s.patchSha256, u])));
    assert.equal(manifest.length, Object.keys(s.blobs).length);
    for (const [path, length, digest] of manifest) {
      const ref = s.blobs[path]; assert.ok(ref); assert.equal(ref.sha256, digest);
      assert.equal(get(dir, ref).length, length);
    }
    for (const [path, length, digest] of u) assert.deepEqual(manifest.find(m => m[0] === path), [path, length, digest]);
    for (const p of HARNESS) assert.equal(s.harness[p], s.blobs[p].sha256);
    const allocatorHash = sha(JSON.stringify(manifest.filter(([p]) => p.startsWith('src/'))));
    assert.equal(env.armAllocatorSourceHashes[arm], allocatorHash);
  }
  assert.notEqual(sources.A.identitySha256, sources.B.identitySha256);
  assert.deepEqual(sources.A.harness, sources.B.harness);
  ancestry(sources.A.baseHead, sources.B.baseHead, get(dir, bundle.ancestry).toString());
  const hb = p => get(dir, sources.A.blobs[p]);
  const hashes = frozen(hb(PREREG), hb(JUDGE), hb(FREEZE)); assert.deepEqual(input.hashes, hashes);
  assert.equal(sha(readFileSync(local(ROOT, JUDGE))), hashes[1].sha256);
  assert.equal(sha(readFileSync(local(ROOT, PREREG))), hashes[0].sha256);
  const cells = inventory(hb('benches/perf_gate_iai.rs'));
  const inv = getJson(dir, bundle.inventory); assert.deepEqual(inv.cells, cells);
  assert.deepEqual(input.inventory, cells.map(({ name, kind }) => ({ name, kind })));
  assert.equal(input.iai.length, cells.length * 5); assert.equal(input.native.length, 81);
  assert.equal(bundle.iaiRows.length, input.iai.length); assert.equal(bundle.nativePairs.length, 81);
  const capture = Date.parse(bundle.allCapturedAt); assert.ok(Number.isFinite(capture));
  assert.ok(Date.parse(env.capturedAt) <= capture);
  const required = ['C0-iai', ...['A', 'B'].flatMap(a => Object.keys(ROLES).map(r => `${a}-${r}`))];
  assert.deepEqual(Object.keys(bundle.builds).sort(), required.sort());
  for (const [key, b] of Object.entries(bundle.builds)) {
    // C0 is a second build of the A source; only B-prefixed builds may come from B.
    assert.equal(b.sourceArm, key.startsWith('B-') ? 'B' : 'A', 'build source arm mismatch');
    assert.equal(b.binarySha256, sha(get(dir, b.binary))); assert.equal(b.sourceIdentitySha256, sources[b.sourceArm].identitySha256);
    assert.equal(b.environmentSha256, input.environmentSha256); feature(b.features, ROLES[b.role][1]);
    assert.equal(b.locked, true); assert.equal(b.jobs, 3); assert.equal(b.rustcWrapper, ''); assert.equal(b.cargoBuildRustcWrapper, '');
    assert.equal(b.profile, 'release'); assert.ok(Date.parse(b.capturedAt) <= capture);
    assert.ok(Date.parse(sources[b.sourceArm].capturedAt) <= Date.parse(b.capturedAt));
    assert.ok(b.binaryPath.startsWith(`target/gate-prf03-${b.arm}-${b.role}/`));
    const process = getJson(dir, b.process); assert.equal(process.code, 0); assert.equal(process.type, 'build');
    get(dir, process.stdout); get(dir, process.stderr);
  }
  assert.notEqual(bundle.builds['C0-iai'].binarySha256, bundle.builds['A-iai'].binarySha256);
  assert.notEqual(bundle.builds['C0-iai'].buildId, bundle.builds['A-iai'].buildId);
  const ids = new Set(), pids = new Set(), entries = new Map();
  for (const p of bundle.processes) {
    assert.ok(!ids.has(p.id), 'duplicate process runId'); ids.add(p.id);
    const envelopeFields = structuredClone(p);
    assert.ok(Date.parse(p.startedAt) > capture, 'process precedes ALL identities');
    const b = bundle.builds[`${p.role === 'iai' ? p.arm === 'C0' ? 'C0' : p.arm.startsWith('B') ? 'B' : 'A' : p.arm}-${p.role}`];
    assert.ok(b); assert.equal(p.binarySha256, b.binarySha256); assert.equal(p.sourceIdentitySha256, b.sourceIdentitySha256);
    const sourceArm = p.role === 'iai' ? (p.arm.startsWith('B') ? 'B' : 'A') : p.arm;
    assert.equal(p.sourceIdentitySha256, sources[sourceArm].identitySha256, 'process source identity mismatch');
    assert.equal(p.environmentSha256, input.environmentSha256);
    const process = getJson(dir, p.process); assert.equal(process.timedOut, false);
    get(dir, process.stdout); const stderr = get(dir, process.stderr).toString();
    assert.ok(Date.parse(process.startedAt) > capture);
    assert.ok(Date.parse(p.startedAt) >= Date.parse(process.startedAt));
    assert.ok(Date.parse(p.startedAt) <= Date.parse(process.endedAt) + 1000);
    if (p.expectedReject) {
      assert.notEqual(process.code, 0); assert.equal(process.type, 'observation');
      assert.ok(['R2', 'R3'].includes(p.regime));
      assert.match(stderr, p.regime === 'R2' ? /assertion `left == right` failed[\s\S]*left: \(0, 0\)[\s\S]*right: \(33, 33\)/ : /R3 payload did not change/);
      assert.equal(p.pid, Number(/thread 'main' \((\d+)\)/.exec(stderr)?.[1])); int(p.pid); assert.ok(p.pid > 0);
      assert.ok(!pids.has(p.pid)); pids.add(p.pid);
      entries.set(p.id, p); continue;
    }
    assert.equal(process.code, 0);
    let pid;
    if (p.role === 'iai') {
      const parsed = parseProfile(get(dir, p.profile).toString(), get(dir, p.valgrind).toString());
      assert.equal(p.pid, parsed.pid); assert.equal(p.instructions, parsed.instructions); pid = parsed.pid;
      assert.ok(p.actualIaiConsistency && typeof p.actualIaiConsistency === 'object');
      assert.deepEqual(p.actualIaiConsistency.process, p.process);
      assert.equal(p.actualIaiConsistency.name, p.name); assert.equal(p.actualIaiConsistency.instructions, parsed.instructions);
      const actualOutput = get(dir, process.stdout).toString();
      const blocks = actualOutput.split(/(?=^\w+::)/m);
      const block = blocks.find(b => new RegExp('^(?:\\w+::)+' + p.name + '\\b').test(b)); assert.ok(block);
      assert.equal(Number(/^\s*Instructions:\s*([\d,]+)/m.exec(block)?.[1].replaceAll(',', '')), parsed.instructions);
    } else {
      const row = getJson(dir, p.raw); row.requestedConfigDebugSha256 = sha(row.requestedConfigDebug);
      assert.equal(p.requestedConfigDebugSha256, row.requestedConfigDebugSha256);
      assert.ok(row.config?.runId, 'raw config missing');
      assert.equal(row.config.runId, p.id); assert.equal(new Date(Number(row.startedAtUnixMs)).toISOString(), p.startedAt);
      if (p.role === 'primary' || p.role === 'paired') oracleValid(row, p.role);
      else nativeValid(row, p.role, row.n, bundle.work);
      assert.deepEqual(getJson(dir, p.envelope), Object.fromEntries(Object.entries(envelopeFields).filter(([k]) => k !== 'envelope')));
      p.row = row; pid = row.config.pid;
    }
    assert.ok(!pids.has(pid), 'duplicate OS pid'); pids.add(pid); entries.set(p.id, p);
  }
  const used = new Set(); const entry = (id, role) => {
    // One observation backs exactly one row: a shared oracle/hit process is not an independent observation.
    const p = entries.get(id); assert.ok(p, 'missing process'); assert.equal(p.role, role);
    assert.ok(!used.has(id), 'process claimed by two rows'); used.add(id); return p;
  };
  const allProbes = new Map();
  for (const p of bundle.processes.filter(p => p.row?.regime?.startsWith('probe-'))) {
    const key = `${p.arm}-${p.row.claimPath}`; assert.ok(!allProbes.has(key)); allProbes.set(key, p); used.add(p.id);
  }
  assert.deepEqual([...allProbes.keys()].sort(), ['A-global', 'A-lease', 'B-global', 'B-lease']);
  assert.equal(bundle.negativePairs.length, 6);
  for (const arm of ['A', 'B']) for (const regime of ['R1', 'R2', 'R3']) {
    const pair = bundle.negativePairs.find(pair => pair.arm === arm && pair.regime === regime); assert.ok(pair);
    const p = entry(pair.primary, 'primary'), q = entry(pair.paired, 'paired');
    assert.equal(p.arm, arm); assert.equal(q.arm, arm);
    if (regime === 'R1') { assert.equal(p.row.negativeControl, true); assert.equal(p.row.regime, 'R1'); structuralEqual(p.row, q.row); }
    else { assert.equal(p.expectedReject, true); assert.equal(q.expectedReject, true); assert.equal(p.regime, regime); assert.equal(q.regime, regime); }
  }
  const scansByCell = new Map();
  for (const [index, link] of bundle.iaiRows.entries()) {
    assert.deepEqual(getJson(dir, link.rowReceipt), link.row, 'retained iai row drift');
    assert.deepEqual(link.row, input.iai[index]); const row = link.row;
    const cell = cells.find(c => c.name === row.name); assert.ok(cell, 'wrong cell name');
    const arm = row.arm.startsWith('B') ? 'B' : 'A';
    assert.deepEqual(row.source, ownSource(sources[arm])); feature(row.features, PRIMARY); feature(row.oracleFeatures, PAIRED);
    assert.equal(row.environmentSha256, input.environmentSha256);
    const h = entry(link.hit, 'iai'); assert.equal(h.name, cell.name); assert.equal(h.arm, row.arm);
    assert.equal(h.instructions, row.hit); assert.equal(h.binarySha256, row.binarySha256);
    assert.equal(h.startedAt, row.startedAt); assert.equal(row.config.pid, h.pid); assert.equal(row.config.runId, h.id);
    assert.equal(row.receipt, h.profile.path);
    const regime = !!PAIRS[cell.kind]; let probe;
    if (regime) {
      const f = entry(link.prefill, 'iai'); assert.equal(f.name, cell.prefill); assert.equal(f.instructions, row.prefill);
      assert.equal(f.arm, row.arm); assert.equal(f.binarySha256, h.binarySha256);
      const p = entry(link.primary, 'primary'), q = entry(link.paired, 'paired');
      assert.equal(p.row.negativeControl, false); assert.equal(p.row.regime, cell.kind); assert.equal(p.arm, arm);
      structuralEqual(p.row, q.row); probe = q;
      assert.deepEqual(row.oracle, q.row.oracle); assert.equal(row.routedScans, q.row.routedScans); assert.equal(row.activation, true);
      if (!scansByCell.has(cell.name)) scansByCell.set(cell.name, row.routedScans);
      assert.equal(scansByCell.get(cell.name), row.routedScans);
    } else {
      assert.equal(link.prefill, null); assert.equal(link.primary, null); assert.equal(link.paired, null);
      assert.equal(row.prefill, null); assert.equal(row.routedScans, null); assert.equal(row.oracle, null);
      probe = cell.kind === 'mimalloc' ? null : allProbes.get(`${arm}-${cell.claimPath}`);
    }
    config(row.config, { mimalloc: cell.kind === 'mimalloc', nonRegime: !regime });
    if (probe) {
      assert.equal(link.probe, probe.id);
      assert.deepEqual(row.config.resolvedProvenance, provenance(probe, ['cacheBudget', 'cacheSlots', 'maintenance', 'configConflictsDelta']));
      const expected = { ...probe.row.config.resolved, workCount: regime ? probe.row.config.resolved.workCount : null };
      assert.deepEqual(row.config.resolved, expected);
    } else { assert.equal(link.probe, null); assert.ok(!row.config.resolvedProvenance); }
  }
  for (const cell of cells) for (const arm of ['A1', 'A2', 'B1', 'B2', 'C0'])
    assert.equal(input.iai.filter(r => r.name === cell.name && r.arm === arm).length, 1);
  for (const [index, pair] of bundle.nativePairs.entries()) {
    assert.deepEqual(getJson(dir, pair.rowReceipt), pair.row, 'retained native row drift');
    assert.deepEqual(pair.row, input.native[index]); const row = pair.row;
    const t = entry(pair.twin, 'twin'), n = entry(pair.native, 'native');
    assert.equal(bundle.processes.indexOf(n), bundle.processes.indexOf(t) + 1, 'twin/native not adjacent');
    assert.equal(t.arm, n.arm); assert.equal(t.row.n, row.n); assert.equal(n.row.n, row.n);
    assert.ok(Date.parse(t.startedAt) <= Date.parse(n.startedAt));
    assert.equal(t.requestedConfigDebugSha256, n.requestedConfigDebugSha256);
    const arm = row.arm === 'B' ? 'B' : 'A'; assert.equal(n.arm, arm); assert.deepEqual(row.source, ownSource(sources[arm]));
    assert.equal(row.receipt, n.raw.path); assert.equal(row.binarySha256, n.binarySha256); assert.equal(row.startedAt, n.startedAt);
    assert.equal(row.requestedConfigDebugSha256, n.requestedConfigDebugSha256);
    assert.equal(row.nsPerOp, n.row.nsPerOp); assert.equal(row.p99Ns, n.row.p99Ns);
    const expected = structuredClone(n.row.config);
    expected.resolved.cacheBudget = t.row.config.resolved.cacheBudget; expected.resolved.cacheSlots = t.row.config.resolved.cacheSlots;
    expected.resolvedProvenance = provenance(t, ['cacheBudget', 'cacheSlots']); assert.deepEqual(row.config, expected); config(row.config);
  }
  for (const n of [1, 4, 8]) for (const arm of ['A', 'AA', 'B']) assert.equal(input.native.filter(r => r.n === n && r.arm === arm).length, 9);
  assert.equal(used.size, entries.size, 'orphan or unvalidated process');
  const draftReview = getJson(dir, bundle.featureExactDraft);
  const aNote = draftReview.AOnlyExitTableReceipt;
  get(dir, aNote.note); assert.equal(aNote.allocatorSourceSha256, env.armAllocatorSourceHashes.A);
  const linkage = { aOnlyReviewNoteSha256: aNote.note.sha256, aAllocatorSourceSha256: aNote.allocatorSourceSha256,
    sourceIdentities: ['A', 'B'].map(a => sources[a].identitySha256),
    binaries: Object.fromEntries(Object.entries(bundle.builds).map(([k, b]) => [k, b.binarySha256])),
    rawHashes: bundle.processes.map(p => p.raw?.sha256 ?? p.profile?.sha256 ?? p.process.sha256), inventorySha256: bundle.inventory.sha256,
    draftSha256: bundle.featureExactDraft.sha256 };
  get(dir, bundle.featureExactDraft);
  const blockers = [];
  if (bundle.processes.some(p => p.role === 'iai' && !p.actualIaiConsistency))
    blockers.push('independent actual iai Instructions consistency absent; direct Callgrind wrapper counts alone are not an iai receipt');
  let review;
  if (reviewFile) {
    review = JSON.parse(readFileSync(reviewFile)); assert.equal(review.status, 'reviewed'); text(review.reviewer);
    const reviewName = `review-${sha(json(review))}.json`;
    if (!existsSync(local(dir, reviewName))) putJson(dir, reviewName, review);
    assert.deepEqual(review.linkage, linkage); text(review.cacheExitReview); text(review.nativeFeatureTransferReview);
    assert.equal(review.inventoryComplete, true);
  } else blockers.push('feature-exact and inventory receipts remain DRAFT; manual linked review required');
  if (blockers.length) return { verdict: 'INCOMPLETE', blockers, linkage, validatedProcesses: entries.size };
  const reviewReference = `review-${sha(json(review))}.json`;
  const judged = { ...input, featureExactReceipt: reviewReference, inventoryReceipt: reviewReference };
  // The unchanged judge takes a filename, not stdin. Use fresh content-addressed input.
  const inputName = `judge-input-${sha(json(judged))}.json`;
  if (!existsSync(local(dir, inputName))) putJson(dir, inputName, judged);
  const actual = spawnSync(process.execPath, [local(ROOT, JUDGE), '--input', local(dir, inputName)], { cwd: ROOT, encoding: 'utf8', timeout: 60000 });
  assert.ok(actual.status !== null); const outcome = JSON.parse(actual.stdout); assert.equal(actual.status, outcome.verdict === 'GO' ? 0 : 1);
  if (synthetic) assert.equal(outcome.verdict, 'GO');
  return { ...outcome, validatedProcesses: entries.size, synthetic };
}
function fixtureRepos(base) {
  mkdirSync(base, { recursive: true }); const arms = {}; let parent = null;
  const objects = new Map();
  const object = (type, bytes) => {
    const body = Buffer.concat([Buffer.from(`${type} ${bytes.length}\0`), bytes]);
    const id = createHash('sha1').update(body).digest('hex'); objects.set(id, deflateSync(body)); return id;
  };
  for (const arm of ['A', 'B']) {
    const cwd = resolve(base, arm); mkdirSync(cwd);
    const files = { '.gitignore': Buffer.from('target/\n'), 'src/fixture.rs': Buffer.from(`// synthetic ${arm}\n`) };
    for (const p of HARNESS) files[p] = readFileSync(local(ROOT, p));
    const entries = Object.keys(files).sort().map(path => {
      const destination = local(cwd, path); mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, files[path], { flag: 'wx' });
      return { path, oid: object('blob', files[path]), size: files[path].length };
    });
    function tree(prefix = '') {
      const children = new Map();
      for (const e of entries.filter(e => e.path.startsWith(prefix))) {
        const suffix = e.path.slice(prefix.length), name = suffix.split('/')[0];
        children.set(name, suffix.includes('/') ? { directory: true } : e);
      }
      const body = [...children].sort(([a, x], [b, y]) => (a + (x.directory ? '/' : '')).localeCompare(b + (y.directory ? '/' : ''), 'en'))
        .map(([name, child]) => Buffer.concat([Buffer.from(`${child.directory ? '40000' : '100644'} ${name}\0`),
          Buffer.from(child.directory ? tree(prefix + name + '/') : child.oid, 'hex')]));
      return object('tree', Buffer.concat(body));
    }
    const rootTree = tree();
    const commit = object('commit', Buffer.from(`tree ${rootTree}\n${parent ? `parent ${parent}\n` : ''}author Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\n${arm}\n`));
    const gd = resolve(cwd, '.git'); mkdirSync(resolve(gd, 'refs', 'heads'), { recursive: true });
    for (const [id, bytes] of objects) { mkdirSync(resolve(gd, 'objects', id.slice(0, 2)), { recursive: true });
      writeFileSync(resolve(gd, 'objects', id.slice(0, 2), id.slice(2)), bytes, { flag: 'wx' }); }
    writeFileSync(resolve(gd, 'HEAD'), 'ref: refs/heads/fixture\n', { flag: 'wx' });
    writeFileSync(resolve(gd, 'refs', 'heads', 'fixture'), commit + '\n', { flag: 'wx' });
    const header = Buffer.alloc(12); header.write('DIRC'); header.writeUInt32BE(2, 4); header.writeUInt32BE(entries.length, 8);
    const indexEntries = entries.map(e => {
      const name = Buffer.from(e.path); const size = Math.ceil((62 + name.length + 1) / 8) * 8;
      const bytes = Buffer.alloc(size); bytes.writeUInt32BE(0o100644, 24); bytes.writeUInt32BE(e.size, 36);
      Buffer.from(e.oid, 'hex').copy(bytes, 40); bytes.writeUInt16BE(Math.min(name.length, 0xfff), 60); name.copy(bytes, 62); return bytes;
    });
    const body = Buffer.concat([header, ...indexEntries]);
    writeFileSync(resolve(gd, 'index'), Buffer.concat([body, createHash('sha1').update(body).digest()]), { flag: 'wx' });
    arms[arm] = cwd; parent = commit;
  }
  const bGit = resolve(arms.B, '.git'), aGit = resolve(arms.A, '.git');
  // A's repository needs no B objects; B contains the complete ancestor graph.
  assert.ok(existsSync(aGit) && existsSync(bGit));
  ancestry(git(arms.A, ['rev-parse', 'HEAD']).toString().trim(), git(arms.B, ['rev-parse', 'HEAD']).toString().trim(), git(arms.B, ['rev-list', '--parents', 'HEAD']).toString());
  for (const cwd of Object.values(arms)) snapshot(cwd);
  return arms;
}
function fixtureTransport() {
  let pid = 10000;
  return { path: p => p.replaceAll('\\', '/'), async execute(r) {
    if (r.type === 'environment') return { code: 0, stdout: 'rustc fixture\nhost: x86_64-unknown-linux-gnu\ncargo fixture\nLinux fixture\nmodel name fixture cpu\niai-callgrind-runner 0.14.2\ncargo-user-config-checked\n', stderr: '' };
    if (r.type === 'build') {
      const [name] = ROLES[r.role]; const binaryPath = `target/gate-prf03-${r.arm}-${r.role}/release/${r.role === 'iai' ? 'deps' : 'examples'}/${name}`;
      const file = local(r.cwd, binaryPath); mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, `fake executable ${r.arm} ${r.role}\n`, { flag: 'wx' });
      return { code: 0, stdout: JSON.stringify({ reason: 'compiler-artifact', target: { name }, executable: this.path(file) }) + '\n', stderr: '' };
    }
    if (r.type === 'iai-arm') {
      const homeArg = /'--home'\s+'([^']+)'/.exec(r.script); assert.ok(homeArg);
      const home = homeArg[1]; mkdirSync(home, { recursive: true }); let stdout = '';
      for (const cell of r.cells) for (const [name, prefill] of [[cell.name, false], ...(cell.prefill ? [[cell.prefill, true]] : [])]) {
        const count = cell.kind === 'R1' && !prefill && r.arm.startsWith('B') ? 800 : 1000;
        const current = ++pid, dir = resolve(home, name); mkdirSync(dir);
        writeFileSync(resolve(dir, 'callgrind.out'), `pid: ${current}\ncmd: --iai-run perf_gate fixture 0 perf_gate_iai::perf_gate::${name}\nfn=fixture::__iai_callgrind_wrapper_mod::inner\nevents: Ir\nsummary: ${count}\ntotals: ${count}\n`, { flag: 'wx' });
        writeFileSync(resolve(dir, 'callgrind.log'), `==${current}== Callgrind fixture\n`, { flag: 'wx' });
        stdout += `perf_gate_iai::perf_gate::${name}\n Instructions: ${count}|N/A\n`;
      }
      return { code: 0, stdout, stderr: '' };
    }
    assert.equal(r.type, 'observation');
    if (r.expectedReject) return { code: 134, stdout: '', stderr: `thread 'main' (${++pid}) panicked\n` + (r.args[0] === 'R2'
      ? "assertion `left == right` failed\n left: (0, 0)\n right: (33, 33)\n" : 'R3 payload did not change\n') };
    const twin = r.role === 'twin', native = r.role === 'native', paired = r.role === 'paired';
    const debug = 'LargeCacheConfig { fixture: DEFAULT }';
    // The fake transport does not measure: fixed outputs are selftest-only.
    const start = Date.now();
    const c = { requested: { cacheBudget: { policy: 'unbounded' }, cacheSlots: 8, maintenance: false,
      workCount: native || twin ? Number(r.args[1]) : r.args[0].startsWith('probe-') ? null : r.args[0] === 'R2' ? 82 : 9 },
      resolved: null, configConflictsDelta: 0, processBoundary: 'isolated-process', runId: r.id, pid: ++pid };
    c.resolved = structuredClone(c.requested);
    if (native) { c.resolved.cacheBudget = null; c.resolved.cacheSlots = null; }
    let row;
    if (native || twin) {
      const n = Number(r.args[0]), work = Number(r.args[1]); c.fallbackConfigConflictsDelta = 0;
      row = { version: 1, role: r.role, features: native ? ['production'] : PRIMARY, n, startedAtUnixMs: start,
        requestedConfigDebug: debug, config: c, latencySampleCount: work, nsPerOp: 100, p99Ns: 100,
        heapsClaimedHighWaterDelta: n, readyDoneReservedDelta: 0, readyDoneReleasedDelta: 0,
        stats: { large_cache_hits: null, tcache_hits: null, unavailableReason: 'alloc-stats compiled out' },
        resolvedCacheUnavailableReason: native ? 'requires twin provenance' : null,
        workers: Array.from({ length: n }, (_, worker) => ({ worker, workCount: work / n,
          cacheSlots: native ? null : 8, cacheBudget: native ? null : { policy: 'unbounded' }, maintenance: false,
          maintenanceAfter: false, configConflictsDelta: 0, fallbackConfigConflictsDelta: 0 })) };
    } else {
      const regime = r.args[0], neg = r.args.includes('--negative-control'), cached = r.arm === 'A' ? 0 : 1;
      const s = { slotsBefore: regime === 'R2' ? 8 : 1, slotsAfterAlloc: regime === 'R2' ? null : neg ? 1 : 0,
        slotsAfterFree: regime === 'R2' ? 8 : neg ? 2 : 1,
        allocReservedDelta: neg ? 1 : regime === 'R2' ? null : 0, allocReleasedDelta: regime === 'R2' ? null : 0,
        cycleReservedDelta: neg ? 1 : regime === 'R2' ? 33 : 0, cycleReleasedDelta: regime === 'R2' ? 33 : 0,
        censusAfterAlloc: regime === 'R1' ? 1 : null, cachedCensus: neg ? cached * 2 : cached,
        payloadChanges: regime === 'R3' ? 8 : 0,
        routeConsistency: regime === 'R1' ? { samePointer: !neg, sameRoot: !neg, sameOwner: true, sameKind: true,
          bracketBefore: 10, bracketAfter: 12 - cached, tailIncarnation: cached ? 8 : 11, prefixIncarnation: 8 } : null };
      row = { version: 1, arm: r.arm, regime, features: paired ? PAIRED : PRIMARY, negativeControl: neg,
        startedAtUnixMs: start, requestedConfigDebug: debug, config: c, pid: c.pid, runId: r.id,
        processBoundary: 'isolated-process', claimPath: regime === 'probe-global' ? 'global' : 'lease',
        structural: s, oracle: { tailHitDelta: paired ? neg ? 0 : regime === 'R2' ? 8 : 1 : null,
          incarnationDelta: regime === 'R1' ? 1 - cached : null, cachedCensus: s.cachedCensus },
        routedScans: paired ? 0 : null, routedMissScans: paired ? 0 : null,
        routedScanSplit: { prefill: paired ? 0 : null, tail: paired ? 0 : null },
        countersUnavailableReason: paired ? null : 'alloc-stats compiled out', description: regime === 'R2' ? 'eviction-dominated 33/41' : regime };
    }
    return { code: 0, stdout: JSON.stringify(row) + '\n', stderr: '' };
  } };
}
export async function selfTest() {
  const base = resolve(ROOT, 'target', `gate-prf03-selftest-${randomUUID()}`);
  mkdirSync(dirname(base), { recursive: true }); const arms = fixtureRepos(base);
  const out = resolve(base, 'bundle'); let checks = 0;
  try {
    const result = await pipeline({ arms, out, transport: fixtureTransport(), work: 8 });
    assert.equal(result.verdict, 'INCOMPLETE'); assert.equal(result.blockers.length, 1); checks++;
    const dirtyPath = local(arms.A, 'src/fixture.rs'), dirtyBytes = readFileSync(dirtyPath);
    writeFileSync(dirtyPath, Buffer.concat([dirtyBytes, Buffer.from('// drift\n')]));
    assert.throws(() => snapshot(arms.A)); checks++; writeFileSync(dirtyPath, dirtyBytes);
    const untrackedPath = local(arms.A, 'untracked.rs'); writeFileSync(untrackedPath, 'untracked', { flag: 'wx' });
    assert.throws(() => snapshot(arms.A)); checks++; rmSync(untrackedPath);
    assert.deepEqual(snapshot(arms.A).untracked, []); checks++;
    const reviewPath = resolve(base, 'review.json');
    writeFileSync(reviewPath, json({ status: 'reviewed', reviewer: 'synthetic fixture, not owner acceptance', linkage: result.linkage,
      cacheExitReview: 'synthetic internal fixture only', nativeFeatureTransferReview: 'synthetic internal fixture only', inventoryComplete: true }), { flag: 'wx' });
    assert.equal(aggregate(out, { reviewFile: reviewPath, synthetic: true }).verdict, 'GO'); checks++;
    const bundlePath = local(out, 'bundle.json'), original = readFileSync(bundlePath);
    const reject = mutate => {
      const b = JSON.parse(original); mutate(b); writeFileSync(bundlePath, json(b));
      assert.throws(() => aggregate(out, { reviewFile: reviewPath, synthetic: true })); checks++;
      writeFileSync(bundlePath, original);
    };
    reject(b => b.processes.pop());
    reject(b => b.nativePairs.pop());
    reject(b => b.iaiRows[0].row.name = 'wrong_name');
    reject(b => b.builds['C0-iai'].binarySha256 = b.builds['A-iai'].binarySha256);
    reject(b => b.sources.A.identitySha256 = sha('drift'));
    reject(b => b.sources.B.baseHead = 'f'.repeat(40));
    reject(b => b.processes[0].binarySha256 = sha('binary drift'));
    reject(b => b.processes[0].startedAt = b.allCapturedAt);
    reject(b => { const pair = b.nativePairs[0]; [pair.twin, pair.native] = [pair.native, pair.twin]; });
    reject(b => b.negativePairs = []);
    const rejectWith = (mutate, expected) => {
      const b = JSON.parse(original); mutate(b); writeFileSync(bundlePath, json(b));
      assert.throws(() => aggregate(out, { reviewFile: reviewPath, synthetic: true }), expected); checks++;
      writeFileSync(bundlePath, original);
    };
    // One paired/primary oracle pair backing two rows of the same cell (A1 and A2) is not two observations.
    rejectWith(b => { const rows = b.iaiRows.filter(l => l.row.name === 'large_cache_hit_only_4mib');
      rows[1].primary = rows[0].primary; rows[1].paired = rows[0].paired; }, /claimed by two rows/);
    // A self-consistent C0 build/process set derived from the B source is not "another binary of the A source".
    rejectWith(b => { const c0 = b.builds['C0-iai']; c0.sourceArm = 'B'; c0.sourceIdentitySha256 = b.sources.B.identitySha256;
      b.processes.filter(p => p.role === 'iai' && p.arm === 'C0').forEach(p => { p.sourceIdentitySha256 = b.sources.B.identitySha256; }); }, /build source arm mismatch/);
    const mutateLinkedRaw = (pick, mutate, expected = /AssertionError/) => {
      const b = JSON.parse(original), p = b.processes.find(pick), path = local(out, p.raw.path), old = readFileSync(path);
      const envelopePath = local(out, p.envelope.path), oldEnvelope = readFileSync(envelopePath);
      const processPath = local(out, p.process.path), oldProcess = readFileSync(processPath);
      const process = JSON.parse(oldProcess), stdoutPath = local(out, process.stdout.path), oldStdout = readFileSync(stdoutPath);
      const row = JSON.parse(old); mutate(row); const bytes = JSON.stringify(row) + '\n';
      writeFileSync(path, bytes); writeFileSync(stdoutPath, bytes);
      p.raw.sha256 = sha(readFileSync(path)); process.stdout.sha256 = sha(readFileSync(stdoutPath));
      writeFileSync(processPath, json(process)); p.process.sha256 = sha(readFileSync(processPath));
      const envelope = JSON.parse(oldEnvelope); envelope.raw = p.raw; envelope.process = p.process;
      writeFileSync(envelopePath, json(envelope)); p.envelope.sha256 = sha(readFileSync(envelopePath));
      writeFileSync(bundlePath, json(b));
      assert.throws(() => aggregate(out, { reviewFile: reviewPath, synthetic: true }), expected); checks++;
      writeFileSync(path, old); writeFileSync(stdoutPath, oldStdout); writeFileSync(processPath, oldProcess);
      writeFileSync(envelopePath, oldEnvelope); writeFileSync(bundlePath, original);
    };
    mutateLinkedRaw(p => p.role === 'paired' && !getJson(out, p.raw).regime.startsWith('probe-'), r => delete r.config);
    mutateLinkedRaw(p => p.role === 'primary' && p.raw && getJson(out, p.raw).regime === 'R1' && !getJson(out, p.raw).negativeControl,
      r => r.structural.routeConsistency.bracketBefore++);
    mutateLinkedRaw(p => p.role === 'primary', r => r.oracle.tailHitDelta = 0);
    mutateLinkedRaw(p => p.role === 'twin', r => r.workers[0].cacheSlots = 0);
    mutateLinkedRaw(p => p.role === 'paired' && p.raw && getJson(out, p.raw).regime === 'R3', r => r.structural.payloadChanges = 7);
    mutateLinkedRaw(p => p.role === 'native', r => r.config.resolved.cacheBudget = { policy: 'unbounded' });
    const originalBundle = JSON.parse(original);
    const rawRow = pick => { const p = originalBundle.processes.find(pick); const r = getJson(out, p.raw); r.requestedConfigDebugSha256 = sha(r.requestedConfigDebug); return r; };
    const primaryRow = rawRow(p => p.role === 'primary' && p.raw && getJson(out, p.raw).regime === 'R1' && !getJson(out, p.raw).negativeControl);
    const pairedRow = rawRow(p => p.role === 'paired' && p.raw && getJson(out, p.raw).regime === 'R1' && !getJson(out, p.raw).negativeControl);
    const invalidPrimary = structuredClone(primaryRow); invalidPrimary.oracle.tailHitDelta = 0;
    assert.throws(() => oracleValid(invalidPrimary, 'primary'), /AssertionError/); checks++;
    const invalidConfig = structuredClone(pairedRow); delete invalidConfig.config;
    assert.throws(() => oracleValid(invalidConfig, 'paired'), /AssertionError/); checks++;
    const inconsistent = structuredClone(pairedRow); inconsistent.structural.routeConsistency.prefixIncarnation++;
    assert.throws(() => structuralEqual(primaryRow, inconsistent), /AssertionError/); checks++;
    const twinRow = rawRow(p => p.role === 'twin'); twinRow.workers[0].cacheSlots = 0;
    assert.throws(() => nativeValid(twinRow, 'twin', twinRow.n, 8), /AssertionError/); checks++;
    assert.deepEqual(parseProfile('pid: 1\nfn=x::__iai_callgrind_wrapper_mod::cell\nevents: Ir\nsummary: 1\ntotals: 2\n', '==1== x\n'), { pid: 1, instructions: 2 }); checks++;
    assert.deepEqual(parseProfile('pid: 1\nfn=x::__iai_callgrind_wrapper_mod::cell\nevents: Ir\nsummary: 1\n', '==1== x\n'), { pid: 1, instructions: 1 }); checks++;
    const built = JSON.parse(original).builds['A-iai'].binary;
    const bp = local(out, built.path), bb = readFileSync(bp); writeFileSync(bp, Buffer.concat([bb, Buffer.from('drift')]));
    assert.throws(() => aggregate(out, { reviewFile: reviewPath })); checks++; writeFileSync(bp, bb);
    assert.equal(publicText('D:' + '/x'), '<private>'); checks++;
    assert.throws(() => local(ROOT, '../escape')); checks++;
    // Exercise the pipeline's actual injectable transport on failure, not just bundle validators.
    const bad = fixtureTransport(), executeOriginal = bad.execute.bind(bad);
    bad.execute = async r => r.type === 'observation' ? { code: 7, stdout: '', stderr: 'synthetic process failure' } : executeOriginal(r);
    const failureArms = fixtureRepos(resolve(base, 'failure-fixtures'));
    await assert.rejects(pipeline({ arms: failureArms, out: resolve(base, 'failed-bundle'), transport: bad, work: 8 })); checks++;
    for (const mode of ['C0-identical', 'invalid-config', 'source-drift', 'ancestry-unrelated', 'wrong-negative-error']) {
      const scenarioArms = fixtureRepos(resolve(base, `scenario-${mode}`));
      const transport = fixtureTransport(), originalExecute = transport.execute.bind(transport);
      if (mode === 'ancestry-unrelated') {
        const gd = resolve(scenarioArms.B, '.git'), oldHead = git(scenarioArms.B, ['rev-parse', 'HEAD']).toString().trim();
        const commit = git(scenarioArms.B, ['show', '--format=%B', '--no-patch', 'HEAD']).toString();
        const treeId = git(scenarioArms.B, ['rev-parse', 'HEAD^{tree}']).toString().trim();
        const body = Buffer.from(`tree ${treeId}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nUnrelated ${commit}\n`);
        const object = Buffer.concat([Buffer.from(`commit ${body.length}\0`), body]); const oid = createHash('sha1').update(object).digest('hex');
        mkdirSync(resolve(gd, 'objects', oid.slice(0, 2)), { recursive: true });
        writeFileSync(resolve(gd, 'objects', oid.slice(0, 2), oid.slice(2)), deflateSync(object), { flag: 'wx' });
        writeFileSync(resolve(gd, 'refs/heads/fixture'), oid + '\n'); assert.notEqual(oid, oldHead);
      }
      transport.execute = async r => {
        const answer = await originalExecute(r);
        if (mode === 'C0-identical' && r.type === 'build' && r.arm === 'C0') {
          const path = JSON.parse(answer.stdout).executable;
          const aPath = local(scenarioArms.A, 'target/gate-prf03-A-iai/release/deps/perf_gate_iai');
          writeFileSync(path, readFileSync(aPath));
        }
        if (mode === 'source-drift' && r.type === 'build') writeFileSync(local(r.cwd, 'src/fixture.rs'), '// changed during build\n');
        if (mode === 'invalid-config' && r.type === 'observation' && !r.expectedReject) {
          const row = JSON.parse(answer.stdout); delete row.config; answer.stdout = JSON.stringify(row) + '\n';
        }
        if (mode === 'wrong-negative-error' && r.expectedReject) answer.stderr = `thread 'main' (999999) panicked\nusage: wrong CLI\n`;
        return answer;
      };
      const expected = mode === 'C0-identical' ? /C0 identical bytes/ : mode === 'source-drift' ? /clean committed arm/ : mode === 'ancestry-unrelated' ? /A not ancestor/ : mode === 'wrong-negative-error' ? /negative must fail/ : /AssertionError/;
      await assert.rejects(pipeline({ arms: scenarioArms, out: resolve(base, `failed-${mode}`), transport, work: 8 }), expected); checks++;
    }
    const cli = parseCli(['--run', 'bundle', '--arm-a', ROOT, '--arm-b', 'explicit-future-arm']);
    assert.equal(cli.work, 8192); assert.equal(cli.workDefaulted, true); assert.equal(cli.arms.A, ROOT); checks++;
    assert.equal(parseCli(['--run', '--out', 'bundle', '--arm-a', ROOT, '--arm-b', 'b', '--work', '8']).work, 8); checks++;
    assert.equal(parseCli(['--aggregate', 'bundle']).out, 'bundle'); checks++;
    assert.equal(parseCli(['--aggregate', '--out', 'bundle']).out, 'bundle'); checks++;
    for (const args of [['--run', 'x', '--out', 'y'], ['--aggregate', 'x', '--work', '8'], ['--aggregate', 'x', '--unknown', 'y'], ['--aggregate'], ['--run', 'x', '--arm-a', 'a', '--arm-b', 'b', '--work', '7']]) {
      assert.throws(() => parseCli(args), /AssertionError/); checks++;
    }
    for (const key of ['CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS', 'RUSTC', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'IAI_CALLGRIND_RUNNER', 'IAI_CALLGRIND_CALLGRIND_ARGS', 'IAI_CALLGRIND_OUTPUT_FORMAT', 'CARGO_PROFILE_BENCH_OPT_LEVEL']) {
      assert.throws(() => rejectEnvironmentOverrides({ [key]: 'override' }), /inherited compiler/); checks++;
    }
    console.log(`PASS: ${checks} end-to-end synthetic checks; pipeline -> retained bundle -> aggregate -> unchanged judge. No real measurement/GO.`);
    console.log('Mini Git repositories used real loose zlib objects written via fs and read-only git; no git write commands or project Git writes.');
  } finally { rmSync(base, { recursive: true, force: true }); }
}
function options(args) {
  const o = {};
  for (let i = 0; i < args.length; i += 2) {
    assert.ok(args[i].startsWith('--') && args[i + 1] && !args[i + 1].startsWith('--'));
    assert.ok(!Object.hasOwn(o, args[i])); o[args[i]] = args[i + 1];
  }
  return o;
}
function parseCli(args) {
  const [mode, ...rest] = args;
  if (mode === '--self-test') { assert.equal(rest.length, 0); return { mode }; }
  assert.ok(['--run', '--aggregate'].includes(mode), 'unknown mode');
  const positional = rest[0] && !rest[0].startsWith('--') ? rest.shift() : null;
  const o = options(rest);
  const allowed = mode === '--run' ? ['--out', '--arm-a', '--arm-b', '--work', '--review-receipt', '--transport-executable', '--transport-prefix-json']
    : ['--out', '--review-receipt'];
  for (const key of Object.keys(o)) assert.ok(allowed.includes(key), 'unknown or conflicting CLI option');
  assert.ok(!(positional && o['--out']), 'positional outdir conflicts with --out');
  const out = positional ?? o['--out']; text(out);
  const parsed = { mode, out, reviewFile: o['--review-receipt'], transportExecutable: o['--transport-executable'],
    transportPrefix: o['--transport-prefix-json'] ? JSON.parse(o['--transport-prefix-json']) : [] };
  assert.ok(Array.isArray(parsed.transportPrefix) && parsed.transportPrefix.every(x => typeof x === 'string'));
  if (mode === '--run') {
    text(o['--arm-a']); text(o['--arm-b']);
    parsed.arms = { A: resolve(o['--arm-a']), B: resolve(o['--arm-b']) };
    parsed.work = o['--work'] === undefined ? 8192 : Number(o['--work']);
    parsed.workDefaulted = o['--work'] === undefined;
    int(parsed.work); assert.ok(parsed.work > 0 && parsed.work % 8 === 0);
  }
  return parsed;
}
function rejectEnvironmentOverrides(env) {
  for (const [key, value] of Object.entries(env)) {
    if (!value) continue;
    assert.ok(!['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_RUSTFLAGS', 'RUSTC',
      'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'IAI_CALLGRIND_RUNNER'].includes(key)
      && !/^CARGO_TARGET_.*_RUSTFLAGS$/.test(key) && !/^CARGO_PROFILE_/.test(key) && !/^IAI_CALLGRIND_/.test(key),
    'inherited compiler/profile/runner override rejected');
  }
}
async function main(args) {
  const o = parseCli(args);
  if (o.mode === '--self-test') return selfTest();
  if (o.mode === '--aggregate') {
    const result = aggregate(resolve(o.out), { reviewFile: o.reviewFile && resolve(o.reviewFile) });
    console.log(json(result)); if (result.verdict !== 'GO') process.exitCode = 1;
  } else {
    rejectEnvironmentOverrides(process.env);
    const transport = makeTransport({ executable: o.transportExecutable, prefix: o.transportPrefix });
    const outcome = await pipeline({ arms: o.arms, out: resolve(o.out), work: o.work, workDefaulted: o.workDefaulted,
      transport, reviewFile: o.reviewFile && resolve(o.reviewFile) });
    console.log(json(outcome)); if (outcome.verdict !== 'GO') process.exitCode = 1;
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await main(process.argv.slice(2)); }
  catch (e) { if (process.env.PRF03_SELFTEST_TRACE && process.argv[2] === '--self-test') throw e;
    console.log(json({ verdict: 'INCOMPLETE', reason: publicText(e.message) })); process.exitCode = 1; }
}
