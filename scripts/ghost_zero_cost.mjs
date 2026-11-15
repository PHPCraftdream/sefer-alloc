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
//   node scripts/ghost_zero_cost.mjs --base origin/main [--allow-declared-runtime [--head SHA] [--classify-only]] [--require-ir] [--out FILE.json]
// `--head` = the PR head, not the merge ref. When the checkout is the PR merge
// ref, its first parent replaces BASE (`resolveBase`): the recorded
// `pull_request.base.sha` goes stale once the base branch moves. Any other
// checkout is red.
// `--allow-declared-runtime`: pass without building only when the entire range
// satisfies scripts/ghost_diff.mjs (`zeroCostSkip`). For a mixed range, derive
// the newest first-parent prefix admitted by that unchanged policy and require
// exact production text/Ir identity for the entire remaining suffix. Report
// both the original PR base and the derived prefix; never claim whole-PR
// runtime identity from a suffix proof. Dirty trees retain the full proof.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';
import { changedLines, resolveBase, zeroCostSkip } from './ghost_diff.mjs';
import { declaredRuntimePrefix } from './ghost_runtime_prefix.mjs';
import { runtimeHistory } from './ghost_runtime_history.mjs';
import { checkRuntimeHistoryCases } from '../tests/ghost_runtime_history_fixtures.mjs';
import { checkRuntimePrefixCases } from '../tests/ghost_runtime_prefix_fixtures.mjs';

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
// `roRanges`: [start, end) of the .rodata / .data.rel.ro sections — a target
// inside them is the constant pool whatever symbol objdump names it by
// (anonymous strings resolve to the nearest preceding symbol, e.g.
// `<__abi_tag+0x566e>`).
// `gotMap`: link-time virtual address -> `GOT:<symbol VALUE>` label from
// `objdump -R` dynamic relocations. `.got`/`.got.plt` slots carry no per-slot
// symbols, so objdump names their addresses by the nearest preceding symbol
// (`_DYNAMIC+0x..`) — the slot ORDER depends on dynamic-symbol hash order
// (`-C metadata`), so those comments diff when only the slots moved.
// For the ET_DYN probe the `objdump -R` OFFSET and the `-d` comment address are
// the same link-time virtual address (load base 0), which is what makes the
// parseInt(addr, 16) lookups correct.
// RELATIVE/IRELATIVE entries (VALUE `*ABS` / `*ABS+0x..`) carry a
// build-specific addend address, so their label is the constant `GOT:RELATIVE`
// and never leaks the addend. Last entry wins on duplicate offsets.
export function gotRelocs(dump) {
  const map = new Map();
  let header = false;
  for (const line of dump.split('\n')) {
    if (!header) { if (/^OFFSET\s+TYPE\s+VALUE$/.test(line)) header = true; continue; }
    const m = /^([0-9a-fA-F]+)\s+(\S+)\s+(\S.*?)\s*$/.exec(line);
    if (!m) continue;
    const value = m[3];
    map.set(parseInt(m[1], 16), /^\*ABS/.test(value) ? 'GOT:RELATIVE' : `GOT:${value}`);
  }
  return map;
}

export function normalize(objdump, rodata = new Set(), roRanges = [], gotMap = new Map()) {
  const inRo = (hex) => { const x = parseInt(hex, 16); return roRanges.some(([lo, hi]) => x >= lo && x < hi); };
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
      .replace(/# ([0-9a-f]+) <[^<>]+>/g, (all, addr) => {
        if (inRo(addr)) return '# RO';
        // GOT slots: mask the slot's POSITION, not the imported symbol's
        // identity — a relocated slot yields `# GOT:<import>` (or the constant
        // `# GOT:RELATIVE`), so slot order changes normalize away while a
        // renamed/changed import at a slot still diffs. Unmapped addresses fall
        // through unchanged (including the `# RO`/rodata behavior, which keeps
        // precedence over the GOT map).
        if (gotMap.has(parseInt(addr, 16))) return `# ${gotMap.get(parseInt(addr, 16))}`;
        return all;
      })
      .replace(/\[rip([+-])0x[0-9a-f]+\]/g, '[rip$1X]')
      .replace(/\b[0-9a-f]+ </g, 'ADDR <')
      .replace(/<([^<>+]+)\+0x[0-9a-f]+>/g, (all, sym) => (rodata.has(sym) ? `<${sym}+R>` : all))
      .replace(/\s+/g, ' ').trim());
  }
  flush();
  for (const [k, v] of fns) fns.set(k, v.sort().join('\n--\n'));
  return fns;
}

// Pure line diff (diagnostics only; never feeds the verdict): classic LCS on
// the line sequences, deterministic. Returns ops {t:'same'|'del'|'add', s}.
export function diffLines(aText, bText) {
  const a = aText.split('\n'), b = bText.split('\n');
  const n = a.length, m = b.length;
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const ops = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) { ops.push({ t: 'same', s: a[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { ops.push({ t: 'del', s: a[i] }); i++; }
    else { ops.push({ t: 'add', s: b[j] }); j++; }
  }
  while (i < n) ops.push({ t: 'del', s: a[i++] });
  while (j < m) ops.push({ t: 'add', s: b[j++] });
  return ops;
}

// Classifier for one removed/added line pair. Token rule: strip every
// `0x[hex]` immediate token (immediates are code, but two lines differing
// ONLY in hex constants are an immediate-level diff, not a structural one),
// and bare hex runs that appear after a `# ` comment marker (objdump
// absolute target addresses not already masked by the RO rules — they are
// addresses, not code). Remainders compared after whitespace collapse;
// anything else (mnemonic, operand shapes, register names, struct offsets)
// is 'structural'.
const stripHexTokens = (s) => s
  .replace(/0x[0-9a-fA-F]+/g, '')
  .replace(/#(?:(\s+))[0-9a-f]+\b/g, '#$1');
export function diffKind(delLine, addLine) {
  const c = (s) => stripHexTokens(s).replace(/\s+/g, ' ').trim();
  return c(delLine) === c(addLine) ? 'hex-immediate' : 'structural';
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

function rodataRanges(bin) {
  const out = [];
  for (const line of run('objdump', ['-h', bin]).split('\n')) {
    const m = /^\s*\d+\s+(\.rodata\S*|\.data\.rel\.ro\S*)\s+([0-9a-f]+)\s+([0-9a-f]+)\s/.exec(line);
    if (m) out.push([parseInt(m[3], 16), parseInt(m[3], 16) + parseInt(m[2], 16)]);
  }
  return out;
}

function disasm(bin) {
  return normalize(run('objdump', ['-d', '-C', '--no-show-raw-insn', '-M', 'intel', '-j', '.text', bin]), rodataSymbols(bin), rodataRanges(bin), gotRelocs(run('objdump', ['-R', '-C', bin])));
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
  ['anonymous constant named by an unrelated symbol moved', fn('    1000:\tlea rdi,[rip+0x10] # 596a <__abi_tag+0x566e>'), fn('    1000:\tlea rdi,[rip+0x10] # 5999 <__abi_tag+0x569d>'), true],
  ['same nearest symbol, target outside the constant pool moved', fn('    1000:\tlea rdi,[rip+0x10] # 9000 <__abi_tag+0x8d04>'), fn('    1000:\tlea rdi,[rip+0x10] # 9008 <__abi_tag+0x8d0c>'), false],
];
const RO_RANGES = [[0x5340, 0x8000]];
const relocs = (...rows) => ['DYNAMIC RELOCATION RECORDS', 'OFFSET           TYPE              VALUE', ...rows].join('\n');
for (const [name, x, y, same] of NORMALIZE_CASES) {
  const ro = new Set(['CONST']);
  const a = [...normalize(x, ro, RO_RANGES)].join('|'), b = [...normalize(y, ro, RO_RANGES)].join('|');
  assert.equal(a === b, same, `normalize self-test "${name}": expected ${same ? 'equal' : 'different'}`);
}
console.log(`[ghost-zero-cost] normalize self-test OK (${NORMALIZE_CASES.length} cases)`);

// GOT-mask self-tests (runs unconditionally, before `--self-test` exits). The
// two sides of each case carry DIFFERENT instruction addresses, so the only
// thing that can make them equal is the masking itself.
const gotCases = [
  ['GOT slot moved, same import', '    1000:\tcall QWORD PTR [rip+0x2f97] # 1a230 <_DYNAMIC+0x200>', '    1010:\tcall QWORD PTR [rip+0x2f97] # 1a270 <_DYNAMIC+0x240>', relocs('000000000001a230 R_X86_64_JUMP_SLO printf@GLIBC_2.2.5+0x0'), relocs('000000000001a270 R_X86_64_JUMP_SLO printf@GLIBC_2.2.5+0x0'), true],
  ['GOT slot kept, import renamed', '    1000:\tcall QWORD PTR [rip+0x2f97] # 1a230 <_DYNAMIC+0x200>', '    1010:\tcall QWORD PTR [rip+0x2f97] # 1a230 <_DYNAMIC+0x200>', relocs('000000000001a230 R_X86_64_JUMP_SLO printf@GLIBC_2.2.5+0x0'), relocs('000000000001a230 R_X86_64_JUMP_SLO puts@GLIBC_2.2.5+0x0'), false],
  ['RELATIVE entries at moved slots, addends differ', '    1000:\tcall QWORD PTR [rip+0x2f97] # 1a010 <_DYNAMIC+0x1e0>', '    1010:\tcall QWORD PTR [rip+0x2f97] # 1a050 <_DYNAMIC+0x220>', relocs('000000000001a010 R_X86_64_RELATIVE *ABS+0x0000000000012345'), relocs('000000000001a050 R_X86_64_RELATIVE *ABS+0x0000000000019999'), true],
];
const fnGot = (body) => fn(`${body}\n    1008:\tret`);
const gotNormalize = (body, map) => [...normalize(fnGot(body), new Set(), [], gotRelocs(map))].join('|');
for (const [name, x, y, mx, my, same] of gotCases) {
  assert.equal(gotNormalize(x, mx) === gotNormalize(y, my), same, `got-masking self-test "${name}": expected ${same ? 'equal' : 'different'}`);
}
// Unmapped address: old behavior preserved (the `ADDR <` rule still fires).
const unmappedA = gotNormalize('    1000:\tcall QWORD PTR [rip+0x2f97] # 1a230 <_DYNAMIC+0x200>', relocs());
const unmappedB = gotNormalize('    1010:\tcall QWORD PTR [rip+0x2f97] # 1a270 <_DYNAMIC+0x240>', relocs());
assert.notEqual(unmappedA, unmappedB, 'got-masking self-test "unmapped address": bodies differ');
assert.ok(unmappedA.includes('ADDR <') && unmappedB.includes('ADDR <'), 'got-masking self-test "unmapped address": ADDR < rule intact');
// `# RO` precedence: rodata masking wins even when a GOT entry exists there.
const roMap = relocs('000000000000596a R_X86_64_GLOB_DAT CONST+0x0');
const roGotA = [...normalize(fn('    1000:\tlea rdi,[rip+0x10] # 596a <__abi_tag+0x566e>'), new Set(['CONST']), RO_RANGES, gotRelocs(roMap))].join('|');
const roGotB = [...normalize(fn('    1010:\tlea rdi,[rip+0x10] # 596a <__abi_tag+0x569d>'), new Set(['CONST']), RO_RANGES, gotRelocs(roMap))].join('|');
assert.equal(roGotA, roGotB, 'got-masking self-test "rodata precedence": # RO wins');
assert.ok(roGotA.includes('# RO'), 'got-masking self-test "rodata precedence": masked as # RO');
console.log(`[ghost-zero-cost] got-masking self-test OK (${gotCases.length + 2} cases)`);

// diffLines/diffKind/diff-detail self-tests (diagnostics only; the verdict
// code paths are unchanged).
assert.equal(diffLines('a\nb\nc', 'a\nb\nc').filter((o) => o.t !== 'same').length, 0, 'diffLines self-test: identical inputs produce no del/add');
assert.deepEqual(diffLines('a\nb\nc', 'a\nx\nc').filter((o) => o.t !== 'same'), [{ t: 'del', s: 'b' }, { t: 'add', s: 'x' }], 'diffLines self-test: one substitution');
assert.equal(diffKind('mov eax,0x10', 'mov eax,0x18'), 'hex-immediate', 'diffKind self-test: immediate-only');
assert.equal(diffKind('xchg DWORD PTR [rdi],esi', 'mov DWORD PTR [rdi],esi'), 'structural', 'diffKind self-test: different mnemonic');
assert.equal(diffKind('movabs rax,0x1234567890abcdef', 'movabs rax,0xfeedfacefeedface'), 'hex-immediate', 'diffKind self-test: 64-bit immediate');
assert.equal(diffKind('mov rax,QWORD PTR [rdi+0x10]', 'mov rax,QWORD PTR [rdi+0x18]'), 'hex-immediate', 'diffKind self-test: struct-offset-only is still hex-token-level');
assert.equal(diffKind('call 1a2b <f>', 'call 1c40 <g>'), 'structural', 'diffKind self-test: different call targets differ in more than hex');
assert.equal(diffKind('lea rax,[rip+0x10] # 5000 <REG+0x10>', 'lea rax,[rip+0x10] # 5000 <REG+0x18>'), 'hex-immediate', 'diffKind self-test: comment-address token stripped');
assert.equal(diffKind('call 1a2b <f>', 'call 1a2b <g>'), 'structural', 'diffKind self-test: same address, different symbol is structural');
const bigA = Array.from({ length: 40 }, (_, i) => `  i${i}: base line ${i}`).join('\n');
const bigB = Array.from({ length: 40 }, (_, i) => `  i${i}: head line ${i}`).join('\n');
const bigOps = diffLines(bigA, bigB).filter((o) => o.t !== 'same');
const bigEmitCap = 30;
const bigEmitted = Math.min(bigOps.length, bigEmitCap) + (bigOps.length > bigEmitCap ? 1 : 0);
assert.ok(bigOps.length > 30, 'diffLines self-test: synthetic case exceeds the cap');
assert.ok(bigEmitted <= bigEmitCap + 1, 'diff-detail self-test: emitted lines stay within cap + marker');
assert.ok(bigOps.length > bigEmitCap, 'diff-detail self-test: truncation marker required');
console.log(`[ghost-zero-cost] diff-diagnostics self-test OK (diffLines + diffKind + cap)`);

const L = (...pairs) => new Map(pairs);
const ok = ['perf(runtime): G1-K1 — drain'];
const SKIP_CASES = [
  ['perf(runtime), production-only lines', ok, L(['src/a.rs', ['    let y = 1;']]), true],
  ['perf(opt-in) among docs/bench/test commits', ['docs: x', 'bench(perf): y', 'test(evidence): z', 'perf(opt-in): w'], L(['src/a.rs', ['x']]), true],
  ['fix(perf), debug_assert line', ['fix(perf): z'], L(['src/a.rs', ['    #[cfg(debug_assertions)]', '    debug_assert!(ok);']]), true],
  ['docs-only paths with a runtime subject', ok, L(['docs/x.md', ['loom ghost shadow']]), true],
  ['no runtime subject', ['test(verification): ghost-checks G6d'], L(['src/a.rs', ['x']]), false],
  ['bare perf scope is not a declaration', ['perf(docs): measure'], L(['src/a.rs', ['x']]), false],
  ['prefix must lead the subject', ['docs: mention perf(runtime): later'], L(['src/a.rs', ['x']]), false],
  ['runtime + fix(verification) commit', [...ok, 'fix(verification): Linux-safe mounts'], L(['src/a.rs', ['use super::AtomicU64;']]), false],
  ['runtime + test(ghost-lint) commit', [...ok, 'test(ghost-lint): rule'], L(['src/a.rs', ['x']]), false],
  ['runtime + scoped list containing loom', [...ok, 'test(alloc, loom): model'], L(['src/a.rs', ['x']]), false],
  ['runtime + non-conventional subject', [...ok, 'WIP stuff'], L(['src/a.rs', ['x']]), false],
  ['ghost macro line', ok, L(['src/a.rs', ['    crate::ghost::ghost_assert!(x, "r");']]), false],
  ['ghost macro with a space before the bang', ok, L(['src/a.rs', ['    ghost_event !(Pub, id);']]), false],
  ['cfg split across lines (token alone)', ok, L(['src/a.rs', ['#[cfg(any(', '    loom,', '))]']]), false],
  ['Loom in another case', ok, L(['crates/x/src/lib.rs', ['#[cfg(Loom)]']]), false],
  ['shadow model call', ok, L(['src/a.rs', ['    shadow::record(x);']]), false],
  ['file under src/ghost', ok, L(['src/ghost/macros.rs', ['// note']]), false],
  ['root Cargo.toml: other feature line', ok, L(['Cargo.toml', ['fastbin = []']]), false],
  ['root Cargo.toml: production line and a dependency line', ok, L(['Cargo.toml', ['production = ["a"]', 'serde = "1"']]), false],
  ['root Cargo.toml: production line only (declared composition change)', ok, L(['Cargo.toml', ['production = ["a", "b"]', 'production = ["a"]', '# comment']]), true],
  ['root Cargo.toml: production line carrying a ghost token', ok, L(['Cargo.toml', ['production = ["a", "loom"]']]), false],
  ['crate Cargo.toml production line', ok, L(['crates/x/Cargo.toml', ['production = ["a"]']]), false],
  ['production line without a runtime subject', ['docs: x'], L(['Cargo.toml', ['production = ["a"]']]), false],
  ['shadow_reject call is not a ghost token', ok, L(['src/a.rs', ['    crate::alloc_core::state_byte::StateByte::shadow_reject(b"x");']]), true],
  ['shadow_reject next to a shadow model call', ok, L(['src/a.rs', ['    shadow_reject(x); shadow::record(x);']]), false],
  ['crate Cargo.lock changed', ok, L(['crates/ghost-lint/Cargo.lock', ['x']]), false],
  ['build.rs changed', ok, L(['build.rs', ['x']]), false],
  ['.cargo config changed', ok, L(['.cargo/config.toml', ['rustflags']]), false],
  ['rust-toolchain changed', ok, L(['rust-toolchain.toml', ['x']]), false],
  ['unstaged change in the checkout', ok, L(['src/a.rs', ['x']]), false, [' M src/a.rs']],
  ['staged change in the checkout', ok, L(['src/a.rs', ['x']]), false, ['M  src/ghost/x.rs']],
  ['untracked file in the checkout', ok, L(['src/a.rs', ['x']]), false, ['?? src/new.rs']],
];
for (const [name, subj, changed, skip, dirty] of SKIP_CASES) assert.equal(zeroCostSkip(subj, changed, dirty) !== null, skip, `zeroCostSkip self-test "${name}"`);
const HUNK = 'diff --git a/src/a b/src/a\n--- a/src/a\n+++ b/src/a\n@@ -1,2 +1,2 @@\n-old\n+new\n@@ -9 +9 @@\n+--x\n';
assert.deepEqual(changedLines(HUNK), ['old', 'new', '--x'], 'changedLines self-test: headers skipped, hunk lines kept (including a line starting with --)');
const B = 'b'.repeat(40), B2 = 'c'.repeat(40), H = 'h'.repeat(40), M = 'm'.repeat(40), X = 'x'.repeat(40), Y = 'y'.repeat(40);
const END_CASES = [
  ['checkout is the head', { recorded: B, head: H, checkout: H, parents: [B], reaches: true }, B],
  ['head is itself a merge, checkout is the head', { recorded: B, head: H, checkout: H, parents: [X, Y], reaches: false }, B],
  ['PR merge ref onto the recorded base', { recorded: B, head: H, checkout: M, parents: [B, H], reaches: true }, B],
  ['PR merge ref onto a moved base branch', { recorded: B, head: H, checkout: M, parents: [B2, H], reaches: true }, B2],
  ['merge onto a commit the recorded base does not reach', { recorded: B, head: H, checkout: M, parents: [X, H], reaches: false }, null],
  ['merge with swapped parents', { recorded: B, head: H, checkout: M, parents: [H, B2], reaches: true }, null],
  ['second parent is not the head', { recorded: B, head: H, checkout: M, parents: [B2, X], reaches: true }, null],
  ['octopus merge', { recorded: B, head: H, checkout: M, parents: [B2, H, X], reaches: true }, null],
  ['one parent and the checkout is not the head', { recorded: B, head: H, checkout: M, parents: [B], reaches: true }, null],
];
for (const [name, e, want] of END_CASES) {
  const r = resolveBase(e);
  assert.equal(r.base ?? null, want, `resolveBase self-test "${name}"`);
  assert.equal(r.why === undefined, want !== null, `resolveBase self-test "${name}": a refusal carries a reason`);
}
const prefixChecks = checkRuntimePrefixCases();
console.log(`[ghost-zero-cost] landing-history self-test OK (${checkRuntimeHistoryCases()} cases)`);
console.log(`[ghost-zero-cost] scope self-test OK (${SKIP_CASES.length} + ${END_CASES.length} cases + changedLines; ${prefixChecks} runtime-prefix checks)`);
if (argv.includes('--self-test')) process.exit(0);

const g = (args) => run('git', args, { cwd: REPO_ROOT });
const resolve = (r) => g(['rev-parse', '--verify', `${r}^{commit}`]).trim();
const recorded = resolve(base);
const head = resolve(opt('--head', 'HEAD'));
const checkout = resolve('HEAD');
const parents = g(['rev-list', '--parents', '-n', '1', checkout]).trim().split(' ').slice(1);
const reaches = parents.length > 0 && spawnSync('git', ['merge-base', '--is-ancestor', recorded, parents[0]], { cwd: REPO_ROOT }).status === 0;
const ends = resolveBase({ recorded, head, checkout, parents, reaches });
if (ends.why) {
  console.error(`[ghost-zero-cost] RED: ${ends.why}`);
  process.exit(1);
}
const prBase = ends.base;
let baseSha = prBase;
const endpoints = { recorded_base: recorded, pr_base: prBase, base: baseSha, checkout, head };
if (baseSha !== recorded) console.log(`[ghost-zero-cost] base ${baseSha}: the merge ref's first parent (recorded base ${recorded})`);

if (argv.includes('--allow-declared-runtime')) {
  const subjects = g(['log', '--no-merges', '--format=%s', `${baseSha}..${head}`]).split('\n').filter(Boolean);
  const paths = g(['diff', '--name-only', '-z', '--no-renames', baseSha, checkout]).split('\0').filter(Boolean);
  const changed = new Map(paths.map((p) => [p, changedLines(g(['diff', '-U0', '--no-color', '--no-ext-diff', '--no-renames', baseSha, checkout, '--', p]))]));
  const dirty = g(['status', '--porcelain', '--untracked-files=all']).split('\n').filter(Boolean);
  const reason = zeroCostSkip(subjects, changed, dirty);
  if (dirty.length) console.log(`[ghost-zero-cost] exception not applicable: uncommitted changes in the checkout: ${dirty.slice(0, 5).join(', ')}`);
  if (reason) {
    console.log(`[ghost-zero-cost] SKIP: ${reason}`);
    if (out) writeFileSync(out, JSON.stringify({ ...endpoints, skipped: true, reason, subjects, paths }, null, 2) + '\n');
    process.exit(0);
  }
  // A runtime-changing prefix may already satisfy the original skip policy.
  // Derive it from history, never from measured output or a caller-picked ref.
  const sourceHistory = dirty.length ? { base: prBase, head } : runtimeHistory({ base: prBase, head }, g);
  if (sourceHistory.bridge) endpoints.history_bridge = sourceHistory.bridge;
  function* candidates() {
    const history = g(['rev-list', '--first-parent', '--no-merges', sourceHistory.head, `^${sourceHistory.base}`]).trim().split('\n').filter(Boolean);
    for (const sha of history) {
      if (sha === sourceHistory.head) continue;
      const prefixSubjects = g(['log', '--no-merges', '--format=%s', `${sourceHistory.base}..${sha}`]).split('\n').filter(Boolean);
      const prefixPaths = g(['diff', '--name-only', '-z', '--no-renames', sourceHistory.base, sha]).split('\0').filter(Boolean);
      const prefixChanged = new Map(prefixPaths.map((p) => [p, changedLines(g(['diff', '-U0', '--no-color', '--no-ext-diff', '--no-renames', sourceHistory.base, sha, '--', p]))]));
      yield { sha, subjects: prefixSubjects, changed: prefixChanged };
    }
  }
  const prefix = declaredRuntimePrefix(candidates(), { head: sourceHistory.head, dirty });
  if (prefix) {
    baseSha = prefix.head;
    endpoints.base = baseSha;
    endpoints.runtime_prefix = { base: sourceHistory.base, ...prefix };
    endpoints.proof_scope = 'entire suffix after the policy-admitted runtime prefix; not whole-PR runtime identity';
    console.log(`[ghost-zero-cost] runtime prefix ${sourceHistory.base}..${baseSha}: admitted by the unchanged declared-runtime policy`);
    console.log(`[ghost-zero-cost] exact production text/Ir proof REQUIRED for suffix ${baseSha}..${head}`);
  } else {
    console.log('[ghost-zero-cost] exception does not apply: the original full-range zero-cost proof is required');
  }
  // Scope decision only: never build or execute a probe in this mode.
  if (argv.includes('--classify-only')) {
    if (out) writeFileSync(out, JSON.stringify({ ...endpoints, skipped: false, proof_required: true }, null, 2) + '\n');
    process.exit(0);
  }
}

const work = mkdtempSync(join(tmpdir(), 'ghost-zero-cost-'));
try {
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
    ...endpoints, head_tree: headTree, example: EXAMPLE, features: 'production', rustflags: RUSTFLAGS,
    functions_base: a.size, functions_head: b.size,
    identical: [...names].length - differing.length - onlyBase.length - onlyHead.length,
    differing: differing.length, only_base: onlyBase.length, only_head: onlyHead.length,
    text_sha256_base: sha(textA), text_sha256_head: sha(textB),
    ir_base: irBase, ir_head: irHead, ir_delta: irBase === null ? null : irHead - irBase,
  };
  assert.equal(summary.identical + summary.differing + summary.only_base + summary.only_head, names.size, 'function accounting');
  console.log(JSON.stringify(summary, null, 2));
  for (const n of [...differing, ...onlyBase, ...onlyHead].slice(0, 20)) console.log(`DIFF ${n}`);
  // Diagnostics only: show the mechanism of the first few differing functions.
  const DETAIL_CAP = 30, DETAIL_FNS = 5;
  for (const n of differing.slice(0, DETAIL_FNS)) {
    console.log(`DIFFDETAIL ${n}`);
    const ops = diffLines(a.get(n), b.get(n));
    let emitted = 0, dels = 0, adds = 0, hexOnly = 0, structural = 0;
    for (const { t, s } of ops) {
      if (t === 'del') dels++; else if (t === 'add') adds++;
      else continue;
      if (emitted < DETAIL_CAP) { console.log(`${t === 'del' ? '-' : '+'} ${s}`); emitted++; }
    }
    if (dels + adds > emitted) console.log(`... truncated (${dels + adds - emitted} more diff lines)`);
    // Pair dels with adds one-to-one (sorted multisets, so counts track);
    // leftover of either side counts as structural.
    const pairs = Math.min(dels, adds);
    let di = 0, ai = 0;
    for (let p = 0; p < pairs; p++) {
      while (ops[di].t !== 'del') di++;
      while (ops[ai].t !== 'add') ai++;
      if (diffKind(ops[di].s, ops[ai].s) === 'hex-immediate') hexOnly++; else structural++;
      di++; ai++;
    }
    structural += Math.abs(dels - adds);
    console.log(`DIFFSTAT ${n}: differing pairs ${pairs}, hex-immediate-only ${hexOnly}, structural ${structural}`);
  }
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
