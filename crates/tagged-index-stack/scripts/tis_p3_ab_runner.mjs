#!/usr/bin/env node
// tis_p3_ab_runner.mjs — measurement driver for the P3-1/P3-2 A/B study of
// `crates/tagged-index-stack`:
//   P3-1: ArrayLinks::load_next/store_next Acquire/Release vs Relaxed.
//   P3-2: strong compare_exchange vs compare_exchange_weak in the push/pop
//         head CAS loops (relevant on LL/SC ISAs like non-LSE AArch64).
//
// Modes:
//   --mode codegen   — materialize the three variants, `rustc --emit=asm`
//                      each DIRECTLY (no cargo), extract/normalize function
//                      blocks, run per-ISA oracles, emit logs/CSV/table.
//   --mode wallclock — materialize three scratch CARGO crates, `cargo build
//                      --release`, run the harness per (variant, sample),
//                      run wallclock oracles, emit logs/CSV/summary.
//   --mode summary  — read the committed per-leg CSVs + the aarch64 raw log
//                      header and emit the compact summary CSV companion for
//                      the gate report. No build, no measurement.
//   --mode build-check — materialize ONE scratch CARGO crate (the `base`
//                      variant only — the push/pop API surface this checks
//                      is identical across all three variants) from the
//                      CURRENT src/{lib,imp}.rs and run a plain `cargo
//                      build` against it. No timing, no docs/perf artifacts.
//                      Exists so an API break in `push`/`pop` (e.g. the
//                      `3e83b1c` unsafe-fn migration) fails regular per-PR
//                      CI instead of staying invisible until the next
//                      workflow_dispatch-only wallclock/codegen run — see
//                      docs/reviews/2026-09-02-180547-tagged-index-stack-review-Sol-codex-run-8.md
//                      P2-2.
//
// Node >= 20, zero npm dependencies, Windows-safe (no POSIX-only APIs).
// This script never modifies any tracked repository file: it writes only
// under target/ (scratch) and docs/perf/ (artifacts) — build-check mode
// writes only under target/ (no docs/perf output at all).

import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// ── Paths ───────────────────────────────────────────────────────────────────
const scriptPath = fileURLToPath(import.meta.url);
const scriptDir = path.dirname(scriptPath);
// repo root is three levels up from this script
// (<repoRoot>/crates/tagged-index-stack/scripts/).
const repoRoot = path.resolve(scriptDir, '..', '..', '..');
const srcDir = path.join(repoRoot, 'crates', 'tagged-index-stack', 'src');
const tmplDir = path.join(scriptDir, 'tis_p3_ab');
const docsPerfDir = path.join(repoRoot, 'docs', 'perf');

const VARIANTS = ['base', 'links_relaxed', 'cas_weak'];
const FUNCTION_KEYS = ['load_next', 'store_next', 'push_index_impl', 'pop_index_impl'];
// Label matchers: primary is the key itself; fallback catches the public
// push/pop entry points (`push_index_impl`/`pop_index_impl` are small enough
// that LLVM inlines them into the fn-pointer-forced `push`/`pop` reify shims
// whose v0-mangled labels carry `4push`/`3pop`). Extraction is the union of
// every block matching any matcher, in label order — deterministic across
// variants.
const LABEL_MATCHERS = {
  load_next: ['load_next'],
  store_next: ['store_next'],
  push_index_impl: ['push_index_impl', '4push'],
  pop_index_impl: ['pop_index_impl', '3pop'],
};

// ── Text-exact substitution anchors (must each occur EXACTLY ONCE) ─────────
const ANCHORS = {
  LINK_LOAD: {
    find: 'self.next[index as usize].load(Ordering::Acquire)',
    replace: 'self.next[index as usize].load(Ordering::Relaxed)',
  },
  LINK_STORE: {
    find: 'self.next[index as usize].store(next, Ordering::Release)',
    replace: 'self.next[index as usize].store(next, Ordering::Relaxed)',
  },
  PUSH_CAS: {
    find: 'match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {',
    replace:
      'match head_ref.head.compare_exchange_weak(head, new_head, Ordering::Release, Ordering::Relaxed) {',
  },
  POP_CAS: {
    find: 'match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {',
    replace:
      'match head_ref.head.compare_exchange_weak(head, new_head, Ordering::Acquire, Ordering::Acquire) {',
  },
};

const VARIANT_ANCHORS = {
  base: [],
  links_relaxed: ['LINK_LOAD', 'LINK_STORE'],
  cas_weak: ['PUSH_CAS', 'POP_CAS'],
};

// ── CLI ─────────────────────────────────────────────────────────────────────
function parseArgs(argv) {
  const args = { mode: null, target: null, outDir: null, threads: 4, windowMs: 1000, samples: 3, smoke: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const need = () => {
      if (i + 1 >= argv.length) fail(`missing value for ${a}`);
      return argv[++i];
    };
    switch (a) {
      case '--mode': args.mode = need(); break;
      case '--target': args.target = need(); break;
      case '--out-dir': args.outDir = need(); break;
      case '--threads': args.threads = Number(need()); break;
      case '--window-ms': args.windowMs = Number(need()); break;
      case '--samples': args.samples = Number(need()); break;
      case '--smoke': args.smoke = true; break;
      default: fail(`unknown argument: ${a}`);
    }
  }
  if (args.mode !== 'codegen' && args.mode !== 'wallclock' && args.mode !== 'summary' && args.mode !== 'build-check') {
    fail(`--mode must be "codegen", "wallclock", "summary" or "build-check" (got ${JSON.stringify(args.mode)})`);
  }
  // build-check runs `cargo build` natively (no cross target); summary reads
  // committed artifacts. Both skip the target-triple requirement.
  if (args.mode !== 'summary' && args.mode !== 'build-check' && (!args.target || !/^[A-Za-z0-9_.-]+$/.test(args.target))) {
    fail('--target must be a rust target triple');
  }
  return args;
}

function fail(msg) {
  console.error(`tis_p3_ab_runner: FATAL: ${msg}`);
  process.exit(1);
}

function assert(cond, msg) {
  if (!cond) fail(`ORACLE/ASSERT failed: ${msg}`);
}

const sha256hex = (s) => createHash('sha256').update(s, 'utf8').digest('hex');

function runCapture(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: opts.cwd ?? repoRoot, encoding: 'utf8', shell: false });
  if (r.status !== 0) {
    fail(`command failed (${r.status}): ${cmd} ${args.join(' ')}\nstderr:\n${r.stderr}`);
  }
  return r.stdout;
}

// ── Header data (captured BEFORE building anything) ─────────────────────────
function captureHeader(args) {
  // Step 1: immutable source identity, from the repo root.
  const identityRaw = runCapture(process.execPath, ['scripts/capture-measurement-identity.mjs', '--json']);
  let identity;
  try {
    identity = JSON.parse(identityRaw);
  } catch (e) {
    fail(`capture-measurement-identity.mjs did not emit JSON: ${e.message}\nraw: ${identityRaw}`);
  }
  // Step 2: toolchain + run parameters.
  const rustcVersion = runCapture('rustc', ['--version', '--verbose']).trim();
  return {
    identity,
    rustcVersion,
    target: args.target,
    mode: args.mode,
    anchors: Object.fromEntries(Object.entries(ANCHORS).map(([k, v]) => [k, { find: v.find, replace: v.replace }])),
    generatedAt: new Date().toISOString(),
    driver: 'crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs',
  };
}

function headerComment(header) {
  return [
    '// =====================================================================',
    '// RAW MEASUREMENT ARTIFACT — generated by tis_p3_ab_runner.mjs, do not edit.',
    `// generatedAt: ${header.generatedAt}`,
    `// driver:      ${header.driver}`,
    `// mode:        ${header.mode}`,
    `// target:      ${header.target}`,
    `// rustc:       ${header.rustcVersion.replace(/\n/g, ' | ')}`,
    `// identity:    ${JSON.stringify(header.identity)}`,
    '// substitution anchors (text-exact, each verified to occur exactly once):',
    ...Object.values(header.anchors).map((a) => `//   ${a.find}  ->  ${a.replace}`),
    '// =====================================================================',
    '',
  ].join('\n');
}

// ── Substitution engine ─────────────────────────────────────────────────────
function applyAnchors(impSrc, anchorNames) {
  let out = impSrc;
  for (const name of anchorNames) {
    const a = ANCHORS[name];
    const count = out.split(a.find).length - 1;
    assert(count === 1, `anchor ${name}: expected exactly 1 occurrence, found ${count}: ${a.find}`);
    out = out.replace(a.find, a.replace);
  }
  // Global sanity: every anchor in this variant's list must have been applied
  // exactly once (the replace above consumed the only occurrence).
  return out;
}

function verifyAllAnchorsOnce(impSrc) {
  for (const [name, a] of Object.entries(ANCHORS)) {
    const count = impSrc.split(a.find).length - 1;
    assert(count === 1, `anchor ${name}: expected exactly 1 occurrence in src/imp.rs, found ${count}`);
  }
}

function freshDir(dir) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
}

function scratchRoot(args) {
  return args.outDir
    ? path.resolve(repoRoot, args.outDir)
    : path.join(repoRoot, 'target', 'tis_p3_ab', args.target);
}

// ── Assembler parsing ───────────────────────────────────────────────────────
const LABEL_RE = /^[A-Za-z_$][A-Za-z0-9_$.]*:$/;

function parseBlocks(asmText) {
  const blocks = [];
  let cur = null;
  for (const line of asmText.split(/\r?\n/)) {
    if (LABEL_RE.test(line)) {
      if (cur) blocks.push(cur);
      cur = { label: line.slice(0, -1), lines: [] };
    } else if (cur) {
      cur.lines.push(line);
    }
  }
  if (cur) blocks.push(cur);
  return blocks;
}

// Cross-variant comparison requires crate-name-independent normalized text:
// v0-mangled symbols embed the scratch crate name (which differs per
// variant), and LLVM `.Lanon.*` label hashes differ per compilation. Scrub
// both; mnemonics and register/immediate operands are untouched.
function scrubSymbols(line) {
  return line
    .replace(/\d+tis_p3ab_(base|links_relaxed|cas_weak)/g, 'TISCRATE')
    .replace(/Cs[A-Za-z0-9]{8,16}_/g, 'CSHASH')
    .replace(/\.Lanon\.[0-9a-f]+/g, '.Lanon');
}

function normalizeBlock(lines) {
  const out = [];
  for (let raw of lines) {
    let line = raw.trim();
    if (line === '') continue;
    if (/^\s*\./.test(raw)) continue; // assembler directives
    const h = line.indexOf('#');
    if (h >= 0) line = line.slice(0, h);
    const s = line.indexOf('//');
    if (s >= 0) line = line.slice(0, s);
    line = scrubSymbols(line.trim());
    if (line === '') continue;
    out.push(line);
  }
  return out;
}

const MNEMONIC_FAMILIES = ['ldar', 'stlr', 'ldaxr', 'stlxr', 'ldapr', 'ldxr', 'stxr', 'ldr', 'str', 'mov'];

function countFamilies(normLines) {
  const counts = Object.fromEntries(MNEMONIC_FAMILIES.map((m) => [m, 0]));
  counts.lock = 0;
  counts.cmpxchg = 0;
  counts.cas = 0; // AArch64 LSE single-instruction CAS: first token matches /^cas[a-z]*$/
  counts.cas8 = 0; // AArch64 outlined-atomics calls: any token starting with __aarch64_cas8_
  for (const line of normLines) {
    const tokens = line.split(/[\s\t]+/);
    const first = tokens[0].toLowerCase();
    if (first === 'lock') {
      counts.lock += 1;
      // x86: `lock cmpxchgq ...` — the lock prefix carries the cmpxchg.
      if (tokens.length > 1 && tokens[1].toLowerCase().startsWith('cmpxchg')) counts.cmpxchg += 1;
      continue;
    }
    if (first.startsWith('cmpxchg')) counts.cmpxchg += 1;
    if (/^cas[a-z]*$/.test(first)) counts.cas += 1;
    if (tokens.some((t) => t.startsWith('__aarch64_cas8_'))) counts.cas8 += 1;
    if (Object.hasOwn(counts, first)) counts[first] += 1;
  }
  return counts;
}

function extractFunctions(asmText) {
  // A function key maps to the concatenation (in label order) of every block
  // whose label contains the key — deterministic across variants.
  const res = {};
  for (const key of FUNCTION_KEYS) {
    const blocks = parseBlocks(asmText).filter((b) => LABEL_MATCHERS[key].some((m) => b.label.includes(m)));
    const norm = blocks.flatMap((b) => normalizeBlock(b.lines));
    res[key] = {
      found: blocks.length > 0,
      blockCount: blocks.length,
      labels: blocks.map((b) => b.label),
      normalizedText: norm.join('\n'),
      sha256_16: sha256hex(norm.join('\n')).slice(0, 16),
      instrCount: norm.length,
      counts: countFamilies(norm),
    };
  }
  return res;
}

// ── Codegen mode ────────────────────────────────────────────────────────────
// Toolchain-observed lowering facts (rustc 1.97.0 / LLVM 22, aarch64-linux-gnu,
// verified by direct inspection of the emitted asm):
//   * DEFAULT feature set (baseline armv8-a): both compare_exchange and
//     compare_exchange_weak lower to OUTLINED atomic calls (`bl
//     __aarch64_cas8_acq` / `bl __aarch64_cas8_rel`); there are NO inline
//     ldaxr/stlxr instructions. After normalization, cas_weak's push/pop
//     blocks are byte-identical to base's — strong vs weak CAS is a full
//     codegen identity on this lowering.
//   * -C target-feature=+lse: each CAS lowers to a single casl/casa
//     instruction (2 casa + 2 casl across push+pop), zero __aarch64_cas8
//     calls, zero ldaxr/stlxr. cas_weak == base here too.
//   * Links ordering (P3-1): base has ldar/stlr for ArrayLinks accesses
//     (residual ldar in relaxed = pop_index_impl's own 64-bit Acquire HEAD
//     load (`head_ref.load(Ordering::Acquire)`), which must remain); links_relaxed drops link ldar to 0 / link stlr to 0.
// The cas_weak identity asserts below are DELIBERATE and load-bearing: they
// are the self-updating oracle. If a future toolchain reintroduces an inline
// LL/SC lowering where weak differs from strong, these asserts FAIL loudly
// and reopen the P3-2 question instead of silently hiding the change.
function modeCodegen(args, header) {
  const impSrc = fs.readFileSync(path.join(srcDir, 'imp.rs'), 'utf8');
  const libSrc = fs.readFileSync(path.join(srcDir, 'lib.rs'), 'utf8');
  verifyAllAnchorsOnce(impSrc);

  const root = scratchRoot(args);
  fs.mkdirSync(docsPerfDir, { recursive: true });

  const isX86 = args.target.startsWith('x86_64');
  const isAarch64 = args.target.startsWith('aarch64');
  // aarch64 gets a second feature-set axis; other targets compile default only.
  const featureSets = isAarch64 ? ['default', 'lse'] : ['default'];

  const logLines = [headerComment(header)];
  if (isAarch64) {
    logLines.push(
      '// TOOLCHAIN-OBSERVED LOWERING (verified on emitted asm, rustc 1.97.0 / LLVM 22):',
      '//   default feature set: CAS = outlined __aarch64_cas8_acq/rel calls (no inline ldaxr/stlxr);',
      '//   +lse: CAS = single casl/casa instructions (zero outlined calls, zero ldaxr/stlxr);',
      '//   strong compare_exchange == compare_exchange_weak after normalization on BOTH feature',
      '//   sets. The cas_weak sha-identity asserts below are DELIBERATE: if a toolchain change',
      '//   reintroduces an inline-LL/SC lowering where weak differs, they fail loudly and reopen',
      '//   the P3-2 question.',
      '// Byte-exact sha identity is asserted for cas_weak (P3-2) but NOT for links_relaxed',
      '// push/pop: removing a link acquire/release legitimately shifts register allocation.',
      '// For links_relaxed the oracle instead asserts the exact acquire/release instruction',
      '// DELTA formulas derived from the base run: pop ldar == base_pop_ldar - base_load_next_ldar',
      '// (residual >= 1 is the head Acquire load, positively proving head ordering untouched);',
      '// push stlr == 0 with base stlr >= 1; plain ldr/str >= 1 where the link ordering was',
      '// dropped; CAS counts unchanged vs base.',
      '',
    );
  }

  const csvRows = [['target', 'features', 'function', 'variant', 'sha256_16', 'instr_count', 'ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8', 'identical_to_base']];

  // Compile one feature set: variant -> { asmText, funcs, fallback }.
  function compileFeatureSet(fset) {
    const froot = fset === 'default' ? root : `${root}_${fset}`;
    freshDir(froot);
    const out = {};
    for (const variant of VARIANTS) {
      const vdir = path.join(froot, variant);
      freshDir(vdir);
      const imp = applyAnchors(impSrc, VARIANT_ANCHORS[variant]);
      const wrapperSrc = fs.readFileSync(path.join(tmplDir, 'codegen_wrapper.rs.tmpl'), 'utf8');
      fs.writeFileSync(path.join(vdir, 'lib.rs'), libSrc);
      fs.writeFileSync(path.join(vdir, 'imp.rs'), imp);
      fs.writeFileSync(path.join(vdir, 'force_codegen.rs'), wrapperSrc);

      const outFile = path.join(vdir, `${variant}.s`);
      let fallback = false;
      const baseArgs = [
        '--edition=2021', '--crate-type=lib', `--crate-name=tis_p3ab_${variant}`,
        '--emit=asm', '-C', 'opt-level=3', '-C', 'codegen-units=1',
        '-C', 'debug-assertions=off', '-C', 'symbol-mangling-version=v0',
        ...(fset === 'lse' ? ['-C', 'target-feature=+lse'] : []),
        '--target', args.target, '-o', outFile, path.join(vdir, 'force_codegen.rs'),
      ];
      let r = spawnSync('rustc', baseArgs, { cwd: vdir, encoding: 'utf8' });
      if (r.status !== 0) {
        if (r.stderr.includes('symbol-mangling-version')) {
          fallback = true;
          const retryArgs = baseArgs.filter((a, i) => !(baseArgs[i] === 'symbol-mangling-version=v0' || (a === '-C' && baseArgs[i + 1] === 'symbol-mangling-version=v0')));
          r = spawnSync('rustc', retryArgs, { cwd: vdir, encoding: 'utf8' });
        }
        if (r.status !== 0) {
          process.stderr.write(r.stderr ?? '');
          fail(`rustc failed for variant ${variant} (target ${args.target}, features ${fset}), exit ${r.status}`);
        }
      }
      out[variant] = {
        asmText: fs.readFileSync(outFile, 'utf8'),
        funcs: extractFunctions(fs.readFileSync(outFile, 'utf8')),
        fallback,
      };
    }
    return out;
  }

  function wholeFileTallies(variants) {
    return Object.fromEntries(VARIANTS.map((v) => [v, {
      ldar: (variants[v].asmText.match(/\bldar\b/g) ?? []).length,
      stlr: (variants[v].asmText.match(/\bstlr\b/g) ?? []).length,
    }]));
  }

  // Oracles for one aarch64 feature set. `variants` is that set's compile map.
  function runAarch64Oracles(fset, variants, wholeFile) {
    const tag = `aarch64[${fset}]`;
    function identical(variant, key) {
      return variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    }
    function printNorm(variant, key) {
      logLines.push(`--- normalized text: features=${fset} variant=${variant} function=${key} ---`);
      logLines.push(variants[variant].funcs[key].normalizedText || '(empty)');
      logLines.push('');
    }
    function shaFail(variant, key, what) {
      return `oracle failed (${tag}): ${what}: sha256(${key}, base)=${variants.base.funcs[key].sha256_16} != sha256(${key}, ${variant})=${variants[variant].funcs[key].sha256_16}`;
    }

    // (a) base CAS shape.
    for (const key of ['push_index_impl', 'pop_index_impl']) {
      const f = variants.base.funcs[key];
      if (!f.found) continue;
      if (fset === 'default') {
        // Outlined-atomics lowering: each block must contain >= 1 __aarch64_cas8_ call.
        if (f.counts.cas8 < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} __aarch64_cas8_ outlined-call count=${f.counts.cas8}, expected >= 1 (observed rustc 1.97.0 lowering is outlined atomics)`);
        }
      } else {
        // LSE lowering: single-instruction CAS, zero outlined calls.
        if (f.counts.cas < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} cas-family instruction count=${f.counts.cas}, expected >= 1 (+lse must lower CAS to casl/casa)`);
        }
        if (f.counts.cas8 !== 0) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} __aarch64_cas8 call count=${f.counts.cas8}, expected 0 under +lse`);
        }
      }
    }

    // (b) links_relaxed ordering (identical story on both feature sets).
    //
    // ORACLE LEVEL NOTE: byte-exact sha identity is the WRONG oracle level for
    // links_relaxed push/pop. Removing a link acquire/release instruction
    // legitimately shifts register allocation, so the correct ground truth is
    // the exact acquire/release instruction DELTA (derived from the base run,
    // never hardcoded):
    //   * POP: the link Acquire contribution (== base load_next's ldar count)
    //     is removed; the residual relaxed ldar count is the head's own Acquire
    //     load and MUST be >= 1 — this positively proves the head ordering was
    //     untouched. A plain relaxed link load (ldr) must appear.
    //   * PUSH: base must contain link Release stores (stlr >= 1); all of them
    //     disappear (stlr == 0) and a plain relaxed link store (str) appears.
    //   * LOAD_NEXT/STORE_NEXT standalone blocks: base has the Acquire/Release
    //     instruction (>= 1); relaxed has none and a plain ldr/str instead.
    // Byte identity REMAINS asserted for cas_weak (P3-2) only.
    for (const key of ['load_next', 'store_next']) {
      const mne = key === 'load_next' ? 'ldar' : 'stlr';
      const plain = key === 'load_next' ? 'ldr' : 'str';
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      if (bf.found && rf.found) {
        if (bf.counts[mne] < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} ${mne}=${bf.counts[mne]}, expected >= 1`);
        }
        if (rf.counts[mne] !== 0) {
          printNorm('links_relaxed', key);
          fail(`${tag} oracle failed: links_relaxed ${key} ${mne}=${rf.counts[mne]}, expected 0 (a relaxed link cell must carry no ldar/stlr at all)`);
        }
        if (rf.counts[plain] < 1) {
          printNorm('links_relaxed', key);
          fail(`${tag} oracle failed: links_relaxed ${key} ${plain}=${rf.counts[plain]}, expected >= 1`);
        }
        logLines.push(`${tag}: links oracle basis: FUNCTION BLOCKS (per-function counts) for ${key}`);
      } else {
        logLines.push(`${tag}: links oracle basis: WHOLE FILE fallback for ${key} (function block not extracted; likely fully inlined)`);
        if (wholeFile.base[mne] < 1) {
          fail(`${tag} oracle failed (whole-file fallback): base .s ${mne} count ${wholeFile.base[mne]}, expected >= 1`);
        }
        if (wholeFile.links_relaxed[mne] !== 0) {
          fail(`${tag} oracle failed (whole-file fallback): links_relaxed .s ${mne} count ${wholeFile.links_relaxed[mne]}, expected 0`);
        }
      }
    }
    // POP: link Acquire contribution removed exactly; head Acquire load remains.
    for (const key of ['pop_index_impl']) {
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      if (!bf.found || !rf.found) continue;
      const basePopLdar = bf.counts.ldar;
      const baseLoadNextLdar = variants.base.funcs.load_next.found
        ? variants.base.funcs.load_next.counts.ldar
        : 0;
      const expected = basePopLdar - baseLoadNextLdar;
      if (rf.counts.ldar !== expected) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop ldar=${rf.counts.ldar}, expected ${expected} (= base pop ldar ${basePopLdar} - base load_next ldar ${baseLoadNextLdar}; the link Acquire contribution removed exactly)`);
      }
      if (rf.counts.ldar < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop residual ldar=${rf.counts.ldar}, expected >= 1 — this residual IS pop_index_impl's own 64-bit Acquire HEAD load (imp.rs \`head_ref.load(Ordering::Acquire)\`), so the oracle positively proves the HEAD ordering was untouched`);
      }
      if (rf.counts.ldr < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop ldr=${rf.counts.ldr}, expected >= 1 (plain relaxed link load must be present)`);
      }
      const baseCas = fset === 'default' ? bf.counts.cas8 : bf.counts.cas;
      const relCas = fset === 'default' ? rf.counts.cas8 : rf.counts.cas;
      if (relCas !== baseCas) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop cas count=${relCas}, expected ${baseCas} (links substitution must not touch the CAS)`);
      }
    }
    // PUSH: all link Release stores removed; plain relaxed store appears.
    for (const key of ['push_index_impl']) {
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      if (!bf.found || !rf.found) continue;
      if (bf.counts.stlr < 1) {
        printNorm('base', key);
        fail(`${tag} links_relaxed oracle failed: base push stlr=${bf.counts.stlr}, expected >= 1 (link Release stores must exist in base)`);
      }
      if (rf.counts.stlr !== 0) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push stlr=${rf.counts.stlr}, expected 0 (link Release stores must be gone)`);
      }
      if (rf.counts.str < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push str=${rf.counts.str}, expected >= 1 (plain relaxed link store must be present)`);
      }
      const baseCas = fset === 'default' ? bf.counts.cas8 : bf.counts.cas;
      const relCas = fset === 'default' ? rf.counts.cas8 : rf.counts.cas;
      if (relCas !== baseCas) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push cas count=${relCas}, expected ${baseCas} (links substitution must not touch the CAS)`);
      }
    }

    // (c) cas_weak: DELIBERATE identity assert (self-updating oracle — see the
    // file-header note). If this fails, a toolchain change has made weak CAS
    // diverge from strong; that REOPENS P3-2 and must NOT be weakened.
    for (const key of ['push_index_impl', 'pop_index_impl']) {
      if (!variants.base.funcs[key].found || !variants.cas_weak.funcs[key].found) continue;
      if (!identical('cas_weak', key)) {
        printNorm('base', key);
        printNorm('cas_weak', key);
        logLines.push(`P3-2 REOPENED: weak CAS now diverges from strong on ${tag} (${key}).`);
        fail(shaFail('cas_weak', key, 'deliberate strong==weak codegen identity assert (self-updating oracle; a divergence reopens P3-2)'));
      }
    }
  }

  // Compile + run oracles per feature set.
  const allRuns = {}; // fset -> { variants, wholeFile, fallback }
  for (const fset of featureSets) {
    const variants = compileFeatureSet(fset);
    const wholeFile = wholeFileTallies(variants);
    if (isAarch64) {
      logLines.push(`================ FEATURE SET: ${fset} ================`);
      logLines.push('');
      runAarch64Oracles(fset, variants, wholeFile);
    } else {
      // x86_64 (and other) targets: all-sha-identity vs base (unchanged).
      for (const key of FUNCTION_KEYS) {
        for (const variant of ['links_relaxed', 'cas_weak']) {
          if (!variants.base.funcs[key].found || !variants[variant].funcs[key].found) continue;
          if (variants[variant].funcs[key].sha256_16 !== variants.base.funcs[key].sha256_16) {
            logLines.push(`--- normalized text: variant=${variant} function=${key} ---`);
            logLines.push(variants[variant].funcs[key].normalizedText || '(empty)');
            logLines.push('');
            fail(`x86_64 identity oracle failed for function ${key} variant ${variant}`);
          }
        }
      }
      for (const key of ['push_index_impl', 'pop_index_impl']) {
        if (!variants.base.funcs[key].found) continue;
        if (variants.base.funcs[key].counts.cmpxchg < 1) {
          fail(`x86_64 oracle failed: base ${key} has cmpxchg count ${variants.base.funcs[key].counts.cmpxchg}, expected >= 1 (CAS instruction absent)`);
        }
      }
    }
    allRuns[fset] = { variants, wholeFile, fallback: VARIANTS.filter((v) => variants[v].fallback) };
  }

  // ── Log body ──────────────────────────────────────────────────────────────
  logLines.push('');
  for (const fset of featureSets) {
    const { variants, wholeFile, fallback } = allRuns[fset];
    logLines.push(`===== features: ${fset} — variants =====`);
    logLines.push(`symbol-mangling-v0 fallback used: ${fallback.join(', ') || 'none'}`);
    logLines.push('');
    const identical = (variant, key) => variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    for (const variant of VARIANTS) {
      logLines.push(`===== variant: ${variant} =====`);
      for (const key of FUNCTION_KEYS) {
        const f = variants[variant].funcs[key];
        if (!f.found) {
          logLines.push(`function ${key}: NOT EXTRACTED (no block label contains the key; likely fully inlined)`);
          continue;
        }
        logLines.push(`function ${key}: blocks=${f.blockCount} labels=${JSON.stringify(f.labels)}`);
        logLines.push(`  sha256_16=${f.sha256_16} instr_count=${f.instrCount}`);
        logLines.push(`  families=${JSON.stringify(f.counts)}`);
        if (variant === 'base' || !identical(variant, key)) {
          logLines.push(`--- normalized text: features=${fset} variant=${variant} function=${key} ---`);
          logLines.push(f.normalizedText || '(empty)');
          logLines.push('');
        } else {
          logLines.push(`  (normalized text omitted: identical to base)`);
        }
      }
      logLines.push('');
    }
    logLines.push(`whole-file ldar/stlr tallies [${fset}] (fallback basis): ${JSON.stringify(wholeFile)}`);
    logLines.push('');
  }

  // ── Derived markdown table (with asserted arithmetic) ─────────────────────
  const md = [];
  md.push(`# TIS P3 A/B codegen table — target ${args.target}`);
  md.push('');
  md.push('delta% is instr_count relative to base for the same function (asserted arithmetic).');
  md.push('');
  md.push('| target | features | function | variant | sha256_16 | instr_count | ldar | stlr | ldaxr | stlxr | cmpxchg | cas | cas8 | delta% vs base |');
  md.push('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
  for (const fset of featureSets) {
    const { variants } = allRuns[fset];
    const identical = (variant, key) => variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    for (const key of FUNCTION_KEYS) {
      for (const variant of VARIANTS) {
        const f = variants[variant].funcs[key];
        if (!f.found) continue;
        let deltaPct = '—';
        if (variant !== 'base') {
          const b = variants.base.funcs[key].instrCount;
          if (b > 0) {
            const ratio = Math.round((f.instrCount / b) * 1000) / 1000;
            assert(Math.round((f.instrCount / b) * 1000) / 1000 === ratio, `ratio arithmetic mismatch for ${fset}/${key}/${variant}`);
            deltaPct = String(Math.round((ratio - 1) * 1000) / 10);
          }
        }
        md.push(`| ${args.target} | ${fset} | ${key} | ${variant} | ${f.sha256_16} | ${f.instrCount} | ${f.counts.ldar} | ${f.counts.stlr} | ${f.counts.ldaxr} | ${f.counts.stlxr} | ${f.counts.cmpxchg} | ${f.counts.cas} | ${f.counts.cas8} | ${deltaPct} |`);
        csvRows.push([args.target, fset, key, variant, f.sha256_16, f.instrCount, f.counts.ldar, f.counts.stlr, f.counts.ldaxr, f.counts.stlxr, f.counts.cmpxchg, f.counts.cas, f.counts.cas8, String(identical(variant, key))]);
      }
    }
  }
  const mdText = md.join('\n') + '\n';
  logLines.push(mdText);

  // ── Artifacts ─────────────────────────────────────────────────────────────
  fs.writeFileSync(path.join(docsPerfDir, `_raw_tis_p3_ab_${args.target}_codegen.log`), logLines.join('\n') + '\n');
  const asmAll = featureSets
    .map((fset) => VARIANTS.map((v) => `# ===== features: ${fset} variant: ${v} =====\n` + allRuns[fset].variants[v].asmText).join('\n'))
    .join('\n');
  fs.writeFileSync(path.join(docsPerfDir, `_raw_tis_p3_ab_${args.target}_codegen.s.all`), asmAll);
  fs.writeFileSync(
    path.join(docsPerfDir, `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_${args.target}.csv`),
    csvRows.map((r) => r.join(',')).join('\n') + '\n',
  );

  console.log(mdText);
  console.log(`codegen mode OK: target=${args.target} scratch=${root} artifacts in docs/perf/`);
}

// ── Wallclock mode ──────────────────────────────────────────────────────────
function modeWallclock(args, header) {
  const impSrc = fs.readFileSync(path.join(srcDir, 'imp.rs'), 'utf8');
  const libSrc = fs.readFileSync(path.join(srcDir, 'lib.rs'), 'utf8');
  const cargoTmpl = fs.readFileSync(path.join(tmplDir, 'scratch_Cargo.toml.tmpl'), 'utf8');
  const harnessTmpl = fs.readFileSync(path.join(tmplDir, 'harness_bin.rs'), 'utf8');
  verifyAllAnchorsOnce(impSrc);

  let { threads, windowMs, samples } = args;
  let smoke = args.smoke;
  if (smoke) {
    threads = 4;
    windowMs = 100;
    samples = 1;
  }
  assert(Number.isInteger(threads) && threads >= 1, `--threads must be a positive integer (got ${threads})`);
  assert(Number.isInteger(windowMs) && windowMs >= 50, `--window-ms must be an integer >= 50 (got ${windowMs})`);
  assert(Number.isInteger(samples) && samples >= 1, `--samples must be a positive integer (got ${samples})`);

  const root = scratchRoot(args);
  freshDir(root);
  fs.mkdirSync(docsPerfDir, { recursive: true });

  const logLines = [headerComment(header)];
  logLines.push(`run params: threads=${threads} window_ms=${windowMs} samples=${samples} smoke=${smoke}`);
  logLines.push('');

  // Materialize + build the three scratch cargo crates.
  const crates = {};
  for (const variant of VARIANTS) {
    const crateName = `tis_p3ab_${variant}`;
    const cdir = path.join(root, variant);
    freshDir(cdir);
    fs.writeFileSync(path.join(cdir, 'Cargo.toml'), cargoTmpl.replaceAll('{{CRATE_NAME}}', crateName));
    fs.mkdirSync(path.join(cdir, 'src', 'bin'), { recursive: true });
    const imp = applyAnchors(impSrc, VARIANT_ANCHORS[variant]);
    fs.writeFileSync(path.join(cdir, 'lib.rs'), libSrc);
    fs.writeFileSync(path.join(cdir, 'imp.rs'), imp);
    fs.writeFileSync(path.join(cdir, 'src', 'bin', 'harness.rs'), harnessTmpl.replaceAll('{{CRATE_NAME}}', crateName));

    // Pin the target dir INSIDE the scratch crate: a global CARGO_TARGET_DIR
    // (common on dev machines) would otherwise send all three variants'
    // artifacts to one shared directory — collisions and wrong exe paths.
    const build = spawnSync('cargo', ['build', '--release'], {
      cwd: cdir,
      encoding: 'utf8',
      env: { ...process.env, CARGO_TARGET_DIR: path.join(cdir, 'target') },
    });
    if (build.status !== 0) {
      process.stderr.write(build.stderr ?? '');
      fail(`cargo build --release failed for variant ${variant} (cwd ${cdir})`);
    }
    logLines.push(`built variant ${variant}: cargo build --release OK (cwd ${cdir})`);
    const exeName = `harness${process.platform === 'win32' ? '.exe' : ''}`;
    crates[variant] = {
      exe: path.join(cdir, 'target', 'release', exeName),
      pushRetries: 0,
      popRetries: 0,
      samples: [],
    };
  }
  logLines.push('');

  // Run the harness per variant per sample.
  for (const variant of VARIANTS) {
    for (let sample = 1; sample <= samples; sample++) {
      const env = {
        ...process.env,
        TIS_AB_THREADS: String(threads),
        TIS_AB_WINDOW_MS: String(windowMs),
        TIS_AB_SMOKE: smoke ? '1' : '0',
        TIS_AB_VARIANT: variant,
      };
      const r = spawnSync(crates[variant].exe, [], {
        env: { ...process.env, CARGO_TARGET_DIR: path.join(root, variant, 'target'), ...env },
        encoding: 'utf8',
      });
      if (r.error) fail(`harness spawn failed for variant=${variant}: ${r.error.message}`);
      if (r.status !== 0) {
        process.stderr.write(r.stderr ?? '');
        fail(`harness exited ${r.status} for variant=${variant} sample=${sample}`);
      }
      logLines.push(`--- variant=${variant} sample=${sample} harness stdout (verbatim) ---`);
      logLines.push(r.stdout);
      let rec = null;
      for (const line of r.stdout.split(/\r?\n/)) {
        try {
          const j = JSON.parse(line);
          if (j && typeof j === 'object' && 'ops_per_sec' in j) rec = j;
        } catch { /* skip non-JSON lines */ }
      }
      if (!rec) fail(`harness emitted no JSON line for variant=${variant} sample=${sample}`);
      // Re-derive the ratio the harness printed (asserted arithmetic).
      const derived = rec.ops_total / (rec.elapsed_ms / 1000);
      assert(
        Math.abs(derived - rec.ops_per_sec) < 0.02 * rec.ops_per_sec,
        `ops_per_sec mismatch for variant=${variant} sample=${sample}: reported ${rec.ops_per_sec}, derived ${derived}`,
      );
      assert(rec.ops_total > 0, `ops_total must be > 0 for variant=${variant} sample=${sample} (got ${rec.ops_total})`);
      assert(rec.elapsed_ms >= 0.5 * windowMs, `lateness guard: elapsed_ms ${rec.elapsed_ms} < 0.5*window_ms ${0.5 * windowMs} for variant=${variant} sample=${sample}`);
      crates[variant].samples.push({ sample, ...rec });
      crates[variant].pushRetries += rec.push_retries;
      crates[variant].popRetries += rec.pop_retries;
    }
    const retries = crates[variant].pushRetries + crates[variant].popRetries;
    assert(
      retries > 0,
      `contended-workload oracle failed for variant=${variant}: push_retries+pop_retries summed over samples = ${retries}, expected > 0 (the CAS-retry path was never exercised)`,
    );
  }

  // ── Summary (median; asserted ratios) ─────────────────────────────────────
  function median(arr) {
    const s = [...arr].sort((a, b) => a - b);
    const n = s.length;
    return n % 2 === 1 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
  }
  const med = Object.fromEntries(VARIANTS.map((v) => [v, median(crates[v].samples.map((s) => s.ops_per_sec))]));
  function ratioOf(v) {
    const r = Math.round((med[v] / med.base) * 1000) / 1000;
    assert(Math.round((med[v] / med.base) * 1000) / 1000 === r, `summary ratio arithmetic mismatch for ${v}/base`);
    return r;
  }

  const md = [];
  md.push(`# TIS P3 A/B wallclock summary — target ${args.target}`);
  md.push('');
  md.push(`threads=${threads} window_ms=${windowMs} samples=${samples} smoke=${smoke}`);
  md.push('');
  md.push('| target | variant | median_ops_per_sec | ratio vs base | push_retries | pop_retries |');
  md.push('|---|---|---|---|---|---|');
  for (const v of VARIANTS) {
    md.push(`| ${args.target} | ${v} | ${med[v].toFixed(2)} | ${v === 'base' ? '1.0' : ratioOf(v).toFixed(3)} | ${crates[v].pushRetries} | ${crates[v].popRetries} |`);
  }
  const mdText = md.join('\n') + '\n';
  logLines.push(mdText);

  // ── Artifacts ─────────────────────────────────────────────────────────────
  const csv = [['target', 'variant', 'threads', 'window_ms', 'sample', 'ops_total', 'elapsed_ms', 'ops_per_sec', 'push_retries', 'pop_retries']];
  for (const v of VARIANTS) {
    for (const s of crates[v].samples) {
      csv.push([args.target, v, s.threads, s.window_ms, s.sample, s.ops_total, s.elapsed_ms, s.ops_per_sec, s.push_retries, s.pop_retries]);
    }
  }
  for (const v of VARIANTS) {
    csv.push([args.target, v, 'SUMMARY', `median_ops_per_sec=${med[v].toFixed(2)}`, `ratio_vs_base=${v === 'base' ? '1.0' : ratioOf(v).toFixed(3)}`, `push_retries=${crates[v].pushRetries}`, `pop_retries=${crates[v].popRetries}`]);
  }
  fs.writeFileSync(path.join(docsPerfDir, `_raw_tis_p3_ab_${args.target}_wallclock.log`), logLines.join('\n') + '\n');
  fs.writeFileSync(
    path.join(docsPerfDir, `TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_${args.target}.csv`),
    csv.map((r) => r.join(',')).join('\n') + '\n',
  );

  console.log(mdText);
  console.log(`wallclock mode OK: target=${args.target} scratch=${root} artifacts in docs/perf/`);
}

// ── Build-check mode ────────────────────────────────────────────────────────
// Static regression gate, NOT a measurement: materializes the `base` variant
// scratch crate exactly like wallclock mode does (same template files, same
// substitution engine, zero anchors applied) and runs a plain `cargo build`
// against it. This is deliberately the cheapest possible reuse of the real
// materialization path — reusing it (rather than a hand-rolled shell check)
// is the point: a drift-catching gate that exercises different code than the
// real wallclock mode could itself go stale the same way the mode it guards
// did. Only the `base` variant is built: the three VARIANT_ANCHORS differ
// only in atomic Ordering/CAS-strength substitutions inside `imp.rs`, never
// in the harness template's own `push`/`pop` call sites, so building all
// three would be redundant compile cost for zero extra API-break coverage.
function modeBuildCheck() {
  const impSrc = fs.readFileSync(path.join(srcDir, 'imp.rs'), 'utf8');
  const libSrc = fs.readFileSync(path.join(srcDir, 'lib.rs'), 'utf8');
  const cargoTmpl = fs.readFileSync(path.join(tmplDir, 'scratch_Cargo.toml.tmpl'), 'utf8');
  const harnessTmpl = fs.readFileSync(path.join(tmplDir, 'harness_bin.rs'), 'utf8');
  verifyAllAnchorsOnce(impSrc);

  const crateName = 'tis_p3ab_build_check';
  const root = path.join(repoRoot, 'target', 'tis_p3_ab', 'build-check');
  freshDir(root);
  fs.writeFileSync(path.join(root, 'Cargo.toml'), cargoTmpl.replaceAll('{{CRATE_NAME}}', crateName));
  fs.mkdirSync(path.join(root, 'src', 'bin'), { recursive: true });
  fs.writeFileSync(path.join(root, 'lib.rs'), libSrc);
  fs.writeFileSync(path.join(root, 'imp.rs'), impSrc);
  fs.writeFileSync(path.join(root, 'src', 'bin', 'harness.rs'), harnessTmpl.replaceAll('{{CRATE_NAME}}', crateName));

  // Plain `cargo build` (dev profile): this gate only needs to prove the
  // template still compiles against the current push/pop API, not produce a
  // benchmarkable binary — no `--release` needed.
  const build = spawnSync('cargo', ['build'], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CARGO_TARGET_DIR: path.join(root, 'target') },
  });
  if (build.status !== 0) {
    process.stderr.write(build.stderr ?? '');
    fail(`cargo build failed for the wall-clock harness template (build-check mode, cwd ${root})`);
  }
  console.log(`build-check mode OK: scratch=${root}`);
}

// ── Summary mode ────────────────────────────────────────────────────────────
// Reads the committed per-leg CSVs and the aarch64 raw log header, emits the
// one compact machine-readable companion CSV for the gate report. Fails
// loudly if any referenced artifact is missing. Every emitted ratio is
// re-derived from the CSV's own sample rows and asserted against the ratio
// the leg itself recorded.
const CODEGEN_CSV_TARGETS = ['x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'];
const WALLCLOCK_CSV_TARGET = 'x86_64-pc-windows-msvc';

function readCsvOrDie(file) {
  const p = path.join(docsPerfDir, file);
  if (!fs.existsSync(p)) fail(`summary mode: required artifact missing: docs/perf/${file}`);
  const lines = fs.readFileSync(p, 'utf8').split(/\r?\n/).filter((l) => l !== '');
  if (lines.length < 2) fail(`summary mode: ${file} has no data rows`);
  const header = lines[0].split(',');
  return { file, header, rows: lines.slice(1).map((l) => {
    const cells = l.split(',');
    // SUMMARY rows in the wallclock CSV carry fewer cells than the header
    // (key=value summary cells); tolerate short rows, never long ones.
    assert(cells.length <= header.length, `${file}: row has ${cells.length} cells, header has ${header.length}`);
    return Object.fromEntries(header.map((h, i) => [h, cells[i] ?? '']));
  }) };
}

function modeSummary() {
  const summaryRows = [['kind', 'target', 'features', 'function_or_variant', 'variant', 'metric', 'value', 'unit']];
  const emit = (kind, target, features, fov, variant, metric, value, unit) =>
    summaryRows.push([kind, target, features, fov, variant, metric, String(value), unit]);

  // (d) identity: parsed from the aarch64 codegen log header (never hardcoded).
  const aarch64Log = path.join(docsPerfDir, '_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log');
  if (!fs.existsSync(aarch64Log)) fail('summary mode: required artifact missing: docs/perf/_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log');
  const idLine = fs.readFileSync(aarch64Log, 'utf8').split(/\r?\n/).find((l) => l.startsWith('// identity:'));
  assert(idLine, 'summary mode: no "// identity:" JSON line in the aarch64 codegen raw log header');
  const identity = JSON.parse(idLine.slice('// identity:'.length).trim());
  assert(typeof identity.headSha === 'string' && identity.headSha.length === 40, `identity headSha malformed: ${identity.headSha}`);
  assert(typeof identity.treeSha === 'string' && identity.treeSha.length === 40, `identity treeSha malformed: ${identity.treeSha}`);
  emit('identity', 'aarch64-unknown-linux-gnu', '', '', '', 'head_sha', identity.headSha, 'sha');
  emit('identity', 'aarch64-unknown-linux-gnu', '', '', '', 'tree_sha', identity.treeSha, 'sha');

  // (a)+(b) codegen legs.
  const familyCols = ['ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8'];
  const codegenCsvs = CODEGEN_CSV_TARGETS.map((target) => ({ target, csv: readCsvOrDie(`TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_${target}.csv`) }));
  const familyNonzero = new Set(); // families nonzero somewhere across all codegen CSVs
  for (const { csv } of codegenCsvs) {
    for (const fam of familyCols) {
      if (csv.rows.some((r) => Number(r[fam]) !== 0)) familyNonzero.add(fam);
    }
  }
  for (const { target, csv } of codegenCsvs) {
    const file = `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_${target}.csv`;
    const expectedHeader = ['target', 'features', 'function', 'variant', 'sha256_16', 'instr_count', 'ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8', 'identical_to_base'];
    assert(JSON.stringify(csv.header) === JSON.stringify(expectedHeader), `${file}: unexpected header ${csv.header.join(',')}`);
    for (const r of csv.rows) {
      assert(r.target === target, `${file}: row target ${r.target} != ${target}`);
      emit('codegen', target, r.features, r.function, r.variant, 'instr_count', r.instr_count, 'instructions');
      for (const fam of familyCols) {
        if (familyNonzero.has(fam) && Number(r[fam]) !== 0) {
          emit('codegen', target, r.features, r.function, r.variant, fam, r[fam], 'instructions');
        }
      }
    }
    // (b) per (target, features, function) identity facts.
    const groups = new Map();
    for (const r of csv.rows) {
      const k = `${r.features}|${r.function}`;
      if (!groups.has(k)) groups.set(k, {});
      groups.get(k)[r.variant] = r.identical_to_base === 'true' ? 1 : 0;
    }
    for (const [k, byVariant] of groups) {
      const [features, fn] = k.split('|');
      assert('cas_weak' in byVariant, `${file}: missing cas_weak row for ${k}`);
      emit('codegen_identity', target, features, fn, 'cas_weak', 'identical_to_base', byVariant.cas_weak, 'boolean');
      if (target.startsWith('x86_64')) {
        assert('links_relaxed' in byVariant, `${file}: missing links_relaxed row for ${k}`);
        emit('codegen_identity', target, features, fn, 'links_relaxed', 'identical_to_base', byVariant.links_relaxed, 'boolean');
      }
    }
  }

  // (c) wallclock smoke leg: medians re-derived from the sample rows, ratios
  // re-derived from the medians, both asserted against the leg's own SUMMARY.
  const wcFile = `TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_${WALLCLOCK_CSV_TARGET}.csv`;
  const wc = readCsvOrDie(wcFile);
  const wcHeader = ['target', 'variant', 'threads', 'window_ms', 'sample', 'ops_total', 'elapsed_ms', 'ops_per_sec', 'push_retries', 'pop_retries'];
  assert(JSON.stringify(wc.header) === JSON.stringify(wcHeader), `${wcFile}: unexpected header ${wc.header.join(',')}`);
  function median(arr) {
    const s = [...arr].sort((a, b) => a - b);
    const n = s.length;
    return n % 2 === 1 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
  }
  const summaryRowsWc = {};
  for (const r of wc.rows) {
    if (r.threads === 'SUMMARY') {
      summaryRowsWc[r.variant] = r;
      continue;
    }
    assert(r.target === WALLCLOCK_CSV_TARGET, `${wcFile}: row target ${r.target} != ${WALLCLOCK_CSV_TARGET}`);
    const derived = Number(r.ops_total) / (Number(r.elapsed_ms) / 1000);
    const reported = Number(r.ops_per_sec);
    assert(Math.abs(derived - reported) < 0.02 * reported, `${wcFile}: ops_per_sec mismatch for ${r.variant}: reported ${reported}, derived ${derived}`);
  }
  const meds = {};
  for (const v of VARIANTS) {
    const samples = wc.rows.filter((r) => r.threads !== 'SUMMARY' && r.variant === v);
    assert(samples.length >= 1, `${wcFile}: no sample rows for variant ${v}`);
    meds[v] = median(samples.map((s) => Number(s.ops_per_sec)));
    emit('wallclock', WALLCLOCK_CSV_TARGET, '', '', v, 'median_ops_per_sec', meds[v].toFixed(2), 'ops/s');
  }
  for (const v of VARIANTS) {
    const stated = Object.values(summaryRowsWc[v] ?? {}).find((c) => typeof c === 'string' && c.startsWith('ratio_vs_base='))?.split('=')[1];
    assert(stated !== undefined, `${wcFile}: no ratio_vs_base SUMMARY cell for variant ${v}`);
    const r = Math.round((meds[v] / meds.base) * 1000) / 1000;
    assert(Math.round((meds[v] / meds.base) * 1000) / 1000 === r, `summary ratio arithmetic mismatch for ${v}/base`);
    assert(Math.abs(r - Number(stated)) < 5e-4, `${wcFile}: ratio_vs_base for ${v}: leg says ${stated}, re-derived ${r}`);
    emit('wallclock', WALLCLOCK_CSV_TARGET, '', '', v, 'ratio_vs_base', r.toFixed(3), 'ratio');
  }

  const outPath = path.join(docsPerfDir, 'TIS_LINK_ORDERING_WEAK_CAS_GATE_summary.csv');
  fs.writeFileSync(outPath, summaryRows.map((r) => r.join(',')).join('\n') + '\n');
  console.log(`summary mode OK: ${summaryRows.length - 1} data rows -> ${path.relative(repoRoot, outPath)}`);
}

// ── Main ────────────────────────────────────────────────────────────────────
const args = parseArgs(process.argv.slice(2));
if (args.mode === 'summary') {
  modeSummary();
} else if (args.mode === 'build-check') {
  modeBuildCheck();
} else {
  const header = captureHeader(args);
  if (args.mode === 'codegen') modeCodegen(args, header);
  else modeWallclock(args, header);
}
