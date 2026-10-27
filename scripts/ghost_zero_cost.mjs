// Ghost-checks zero-cost proof (docs/GHOST_CHECKS_PLAN.md §6). Builds
// examples/ghost_zero_cost_probe.rs with `--features production` (release, the
// repository's alignment RUSTFLAGS) from a BASE revision and from the HEAD
// working tree, then requires:
//   1. the normalized disassembly of every function in `.text` to be identical
//      (addresses, immediates that are addresses, symbol hashes and `+offset`
//      suffixes removed; functions compared by name, so layout cannot hide a diff);
//   2. identical callgrind Ir totals for the probe (when valgrind is present;
//      `--require-ir` makes its absence red).
// Linux only (objdump, valgrind). The probe source is taken from HEAD for both
// sides so the harness is identical; only the allocator source differs.
//
// A range that declares an intended production change (a `perf(runtime)`,
// `perf(opt-in)` or `fix(perf)` subject, CLAUDE.md R30-12; such commits owe a
// gate report) still gets the full diff and Ir delta printed, but they do not
// gate: the strict identity applies to ghost-only ranges.
//
//   node scripts/ghost_zero_cost.mjs --base origin/main [--require-ir] [--out FILE.json]
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const EXAMPLE = 'ghost_zero_cost_probe';
const RUSTFLAGS = '-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5';
const EXAMPLE_TOML = `\n[[example]]\nname = "${EXAMPLE}"\nrequired-features = ["alloc-global"]\n`;

const argv = process.argv.slice(2);
const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
const base = opt('--base', 'origin/main');
const out = opt('--out', null);
const requireIr = argv.includes('--require-ir');

const run = (cmd, args, o = {}) => execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 1 << 30, ...o });

function build(tree, target) {
  run('cargo', ['build', '--release', '--locked', '-q', '--example', EXAMPLE, '--features', 'production'], {
    cwd: tree, stdio: ['ignore', 'inherit', 'inherit'],
    env: { ...process.env, CARGO_TARGET_DIR: target, RUSTFLAGS },
  });
  const bin = join(target, 'release', 'examples', EXAMPLE);
  assert.ok(existsSync(bin), `built binary missing: ${bin}`);
  return bin;
}

// Symbol noise that differs between two builds of the same code (objdump -C
// demangles): crate disambiguators (`[a1b2]`), legacy hashes (`::h<16 hex>`),
// ThinLTO promotion suffixes (`.llvm.<n>`) and module paths — a pure move of an
// item between modules renames its symbol without changing a byte of code, so
// only the last path segment is kept and same-named functions compare as a
// multiset of bodies.
export const symbols = (t) => t
  .replace(/\[[0-9a-f]+\]/g, '')
  .replace(/::h[0-9a-f]{16}\b/g, '')
  .replace(/\.llvm\.\d+/g, '')
  .replace(/(?:[A-Za-z_][A-Za-z0-9_]*::)+(?=[A-Za-z_<])/g, '');

// function name -> sorted bodies of every function with that (shortened) name
// `rodata`: names of read-only data symbols — an offset into one of them
// (`<CONST+0x62c>`) is a position in the constant pool (strings, panic
// locations), which moves when an unrelated string changes length; offsets into
// mutable statics stay, they select fields.
export function normalize(objdump, rodata = new Set()) {
  const fns = new Map();
  let name = null, body = [];
  const flush = () => { if (name !== null) { if (!fns.has(name)) fns.set(name, []); fns.get(name).push(body.join('\n')); } };
  for (const line of objdump.split('\n')) {
    const h = /^[0-9a-f]+ <(.+)>:$/.exec(line);
    if (h) { flush(); name = symbols(h[1]); body = []; continue; }
    if (name === null) continue;
    const m = /^\s*[0-9a-f]+:\s*(.*)$/.exec(line);
    if (!m) continue;
    // Only absolute addresses go: rip-relative displacements and printed
    // targets (`call 1a2b <f>`, `# 84a38 <SYM>`). Immediates, struct offsets
    // and `<sym+0x..>` offsets stay — they are code.
    body.push(symbols(m[1])
      .replace(/\[rip([+-])0x[0-9a-f]+\]/g, '[rip$1X]')
      .replace(/\b[0-9a-f]+ </g, 'ADDR <')
      .replace(/<([^<>+]+)\+0x[0-9a-f]+>/g, (all, sym) => (rodata.has(sym) ? `<${sym}+R>` : all))
      .replace(/\s+/g, ' ').trim());
  }
  flush();
  for (const [k, v] of fns) fns.set(k, v.sort().join('\n--\n'));
  return fns;
}

function rodataSymbols(bin) {
  const names = new Set();
  for (const line of run('objdump', ['-t', '-C', bin]).split('\n')) {
    // .rodata (strings, tables) and .data.rel.ro (panic locations, vtables):
    // constants after relocation.
    const m = /^[0-9a-f]+\s.{7}\s(\.rodata\S*|\.data\.rel\.ro\S*)\s+[0-9a-f]+\s+(?:\.hidden\s+)?(.+)$/.exec(line);
    if (m) names.add(symbols(m[2].trim()));
  }
  return names;
}

function disasm(bin) {
  return normalize(run('objdump', ['-d', '-C', '--no-show-raw-insn', '-M', 'intel', '-j', '.text', bin]), rodataSymbols(bin));
}

function irTotal(bin, dir, tag) {
  const outFile = join(dir, `cg.${tag}.out`);
  // Same path for both runs: the binary's own path is part of process startup
  // (argv, auxv) and moves libc's memchr/memcpy Ir by a few units.
  const probe = join(dir, 'probe');
  copyFileSync(bin, probe);
  const r = spawnSync('valgrind', ['--tool=callgrind', `--callgrind-out-file=${outFile}`, probe], { encoding: 'utf8' });
  if (r.error) return null;
  assert.equal(r.status, 0, `probe under callgrind failed (${tag}): ${r.stderr}`);
  const m = /^summary:\s*(\d+)/m.exec(readFileSync(outFile, 'utf8')) || /^totals:\s*(\d+)/m.exec(readFileSync(outFile, 'utf8'));
  assert.ok(m, `no Ir total in ${outFile}`);
  return Number(m[1]);
}

const sha = (s) => createHash('sha256').update(s).digest('hex');

// Normalization must erase build noise and nothing else: each pair below is
// [base, head, should-compare-equal].
const fn = (body, path = 'registry::directory::') => `0000000000001000 <sefer_alloc[a1b2c3]::${path}f.llvm.42>:\n${body}\n`;
const NORMALIZE_CASES = [
  ['llvm suffix, rip displacement, call address', fn('    1000:\tlock xchg DWORD PTR [rip+0x84a48],esi # 84a48 <REG.llvm.111+0x10>\n    1008:\tcall 2a0 <g.llvm.5>'),
    fn('    1000:\tlock xchg DWORD PTR [rip+0x91a48],esi # 91a48 <REG.llvm.999+0x10>\n    1008:\tcall 3b0 <g.llvm.6>', 'registry::directory::route_directory::').replace('a1b2c3', 'ffee99'), true],
  ['two same-named functions in either order', fn('    1000:\tret') + fn('    2000:\tnop'), fn('    3000:\tnop') + fn('    4000:\tret'), true],
  ['immediate changed', fn('    1000:\tmov eax,0x10'), fn('    1000:\tmov eax,0x18'), false],
  ['struct offset changed', fn('    1000:\tmov rax,QWORD PTR [rdi+0x10]'), fn('    1000:\tmov rax,QWORD PTR [rdi+0x18]'), false],
  ['ordering changed (xchg -> mov)', fn('    1000:\txchg DWORD PTR [rdi],esi'), fn('    1000:\tmov DWORD PTR [rdi],esi'), false],
  ['offset within a mutable static changed', fn('    1000:\tlea rax,[rip+0x10] # 5000 <REG+0x10>'), fn('    1000:\tlea rax,[rip+0x10] # 5000 <REG+0x18>'), false],
  ['offset into a read-only constant moved', fn('    1000:\tlea rdi,[rip+0x10] # 5000 <CONST+0x62c>'), fn('    1000:\tlea rdi,[rip+0x10] # 5000 <CONST+0x604>'), true],
];
for (const [name, x, y, same] of NORMALIZE_CASES) {
  const ro = new Set(['CONST']);
  const a = [...normalize(x, ro)].join('|'), b = [...normalize(y, ro)].join('|');
  assert.equal(a === b, same, `normalize self-test "${name}": expected ${same ? 'equal' : 'different'}`);
}
console.log(`[ghost-zero-cost] normalize self-test OK (${NORMALIZE_CASES.length} cases)`);

export const declaresRuntimeChange = (subject) => /^(perf\((runtime|opt-in)\)|fix\(perf\)):/.test(subject);
for (const [subject, want] of [['perf(runtime): magazine flush', true], ['perf(opt-in): x', true], ['fix(perf): layout', true],
  ['test(verification): ghost-checks G2', false], ['perf(docs): measure', false], ['bench(perf): gate', false]]) {
  assert.equal(declaresRuntimeChange(subject), want, `subject self-test "${subject}"`);
}
if (argv.includes('--self-test')) process.exit(0);

const work = mkdtempSync(join(tmpdir(), 'ghost-zero-cost-'));
try {
  const baseSha = run('git', ['rev-parse', base], { cwd: REPO_ROOT }).trim();
  const headTree = run('git', ['write-tree'], { cwd: REPO_ROOT }).trim();
  const intended = run('git', ['log', '--format=%h %s', `${baseSha}..HEAD`], { cwd: REPO_ROOT })
    .split('\n').filter((l) => declaresRuntimeChange(l.slice(l.indexOf(' ') + 1)));
  const baseDir = join(work, 'base');
  run('git', ['worktree', 'add', '--detach', baseDir, baseSha], { cwd: REPO_ROOT, stdio: 'ignore' });
  // Same harness on both sides: the probe from HEAD, registered in base's manifest if absent.
  copyFileSync(join(REPO_ROOT, 'examples', `${EXAMPLE}.rs`), join(baseDir, 'examples', `${EXAMPLE}.rs`));
  const baseToml = readFileSync(join(baseDir, 'Cargo.toml'), 'utf8');
  if (!baseToml.includes(`name = "${EXAMPLE}"`)) writeFileSync(join(baseDir, 'Cargo.toml'), baseToml + EXAMPLE_TOML);

  const binBase = build(baseDir, join(work, 'tgt-base'));
  const binHead = build(REPO_ROOT, join(work, 'tgt-head'));
  const a = disasm(binBase), b = disasm(binHead);
  const names = new Set([...a.keys(), ...b.keys()]);
  const differing = [], onlyBase = [], onlyHead = [];
  for (const n of names) {
    if (!b.has(n)) onlyBase.push(n);
    else if (!a.has(n)) onlyHead.push(n);
    else if (a.get(n) !== b.get(n)) differing.push(n);
  }
  const textA = [...a].sort().map(([n, x]) => `${n}\n${x}`).join('\n');
  const textB = [...b].sort().map(([n, x]) => `${n}\n${x}`).join('\n');
  const irBase = irTotal(binBase, work, 'base');
  const irHead = irBase === null ? null : irTotal(binHead, work, 'head');
  const summary = {
    base: baseSha, head_tree: headTree, example: EXAMPLE, features: 'production', rustflags: RUSTFLAGS,
    functions_base: a.size, functions_head: b.size,
    identical: [...names].length - differing.length - onlyBase.length - onlyHead.length,
    differing: differing.length, only_base: onlyBase.length, only_head: onlyHead.length,
    text_sha256_base: sha(textA), text_sha256_head: sha(textB),
    ir_base: irBase, ir_head: irHead, ir_delta: irBase === null ? null : irHead - irBase,
  };
  assert.equal(summary.identical + summary.differing + summary.only_base + summary.only_head, names.size, 'function accounting');
  console.log(JSON.stringify(summary, null, 2));
  for (const n of [...differing, ...onlyBase, ...onlyHead].slice(0, 20)) console.log(`DIFF ${n}`);
  if (out) writeFileSync(out, JSON.stringify(summary, null, 2) + '\n');
  const textSame = summary.differing === 0 && summary.only_base === 0 && summary.only_head === 0;
  assert.equal(textSame, summary.text_sha256_base === summary.text_sha256_head, 'per-function verdict agrees with the whole-text hash');
  if (!textSame) { console.error('[ghost-zero-cost] RED: production .text differs'); process.exitCode = 1; }
  if (irBase === null) {
    console.log('[ghost-zero-cost] valgrind not found: Ir not measured');
    if (requireIr) process.exitCode = 1;
  } else if (summary.ir_delta !== 0) { console.error(`[ghost-zero-cost] RED: Ir delta ${summary.ir_delta}`); process.exitCode = 1; }
  if (process.exitCode && intended.length && (irBase !== null || !requireIr)) {
    console.log(`[ghost-zero-cost] INTENDED production change, not gating (${intended.join('; ')}): ${summary.differing + summary.only_base + summary.only_head} functions differ, Ir delta ${summary.ir_delta}`);
    process.exitCode = 0;
  } else if (!process.exitCode) console.log(`[ghost-zero-cost] OK: ${summary.identical} functions identical, Ir delta ${summary.ir_delta}`);
} finally {
  spawnSync('git', ['worktree', 'remove', '--force', join(work, 'base')], { cwd: REPO_ROOT });
  rmSync(work, { recursive: true, force: true });
}
