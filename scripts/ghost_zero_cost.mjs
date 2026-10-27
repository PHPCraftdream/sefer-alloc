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

// Symbol noise that differs between two builds of the same code: legacy hashes
// (`17h<hash>E`), v0 crate disambiguators (`Cs<base62>_`) and ThinLTO promotion
// suffixes (`.llvm.<n>`).
const symbols = (t) => t.replace(/17h[0-9a-f]{16}E/g, '17hXE').replace(/Cs[0-9A-Za-z]+_/g, 'CsX_').replace(/\.llvm\.\d+/g, '');

// function name -> normalized body
export function normalize(objdump) {
  const fns = new Map();
  let name = null, body = [];
  const flush = () => { if (name !== null) fns.set(name, (fns.get(name) ?? '') + body.join('\n')); };
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
      .replace(/\s+/g, ' ').trim());
  }
  flush();
  return fns;
}

function disasm(bin) {
  return normalize(run('objdump', ['-d', '--no-show-raw-insn', '-M', 'intel', '-j', '.text', bin]));
}

function irTotal(bin, dir, tag) {
  const outFile = join(dir, `cg.${tag}.out`);
  const r = spawnSync('valgrind', ['--tool=callgrind', `--callgrind-out-file=${outFile}`, bin], { encoding: 'utf8' });
  if (r.error) return null;
  assert.equal(r.status, 0, `probe under callgrind failed (${tag}): ${r.stderr}`);
  const m = /^summary:\s*(\d+)/m.exec(readFileSync(outFile, 'utf8')) || /^totals:\s*(\d+)/m.exec(readFileSync(outFile, 'utf8'));
  assert.ok(m, `no Ir total in ${outFile}`);
  return Number(m[1]);
}

const sha = (s) => createHash('sha256').update(s).digest('hex');

// Normalization must erase build noise and nothing else: each pair below is
// [base, head, should-compare-equal].
const fn = (body) => `0000000000001000 <_RNvCsAbc123_5crate1f.llvm.42>:\n${body}\n`;
const NORMALIZE_CASES = [
  ['llvm suffix, rip displacement, call address', fn('    1000:\tlock xchg DWORD PTR [rip+0x84a48],esi # 84a48 <REG.llvm.111+0x10>\n    1008:\tcall 2a0 <g.llvm.5>'),
    fn('    1000:\tlock xchg DWORD PTR [rip+0x91a48],esi # 91a48 <REG.llvm.999+0x10>\n    1008:\tcall 3b0 <g.llvm.6>').replace('Abc123', 'Zz9'), true],
  ['immediate changed', fn('    1000:\tmov eax,0x10'), fn('    1000:\tmov eax,0x18'), false],
  ['struct offset changed', fn('    1000:\tmov rax,QWORD PTR [rdi+0x10]'), fn('    1000:\tmov rax,QWORD PTR [rdi+0x18]'), false],
  ['ordering changed (xchg -> mov)', fn('    1000:\txchg DWORD PTR [rdi],esi'), fn('    1000:\tmov DWORD PTR [rdi],esi'), false],
  ['offset within a symbol changed', fn('    1000:\tlea rax,[rip+0x10] # 5000 <REG+0x10>'), fn('    1000:\tlea rax,[rip+0x10] # 5000 <REG+0x18>'), false],
];
for (const [name, x, y, same] of NORMALIZE_CASES) {
  const a = [...normalize(x)].join('|'), b = [...normalize(y)].join('|');
  assert.equal(a === b, same, `normalize self-test "${name}": expected ${same ? 'equal' : 'different'}`);
}
console.log(`[ghost-zero-cost] normalize self-test OK (${NORMALIZE_CASES.length} cases)`);
if (argv.includes('--self-test')) process.exit(0);

const work = mkdtempSync(join(tmpdir(), 'ghost-zero-cost-'));
try {
  const baseSha = run('git', ['rev-parse', base], { cwd: REPO_ROOT }).trim();
  const headTree = run('git', ['write-tree'], { cwd: REPO_ROOT }).trim();
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
  if (!process.exitCode) console.log(`[ghost-zero-cost] OK: ${summary.identical} functions identical, Ir delta ${summary.ir_delta}`);
} finally {
  spawnSync('git', ['worktree', 'remove', '--force', join(work, 'base')], { cwd: REPO_ROOT });
  rmSync(work, { recursive: true, force: true });
}
