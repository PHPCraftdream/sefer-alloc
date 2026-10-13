#!/usr/bin/env node
// CI tier planner: maps the files a change touches to the job groups ci.yml must run.
// Pure git-diff + path rules, no third-party action. Output: one `group=true|false`
// line per group (to $GITHUB_OUTPUT when set) plus a human summary on stdout.
//
// Groups (see docs/CI_TIERS.md):
//   always  fmt, commit-prefix-lint, evidence-registry, deny, docs-consistency
//   fast    clippy, check-matrix, test-*, no_std, docs, fuzz-build
//   seam    unsafe-seam proofs: miri-*, loom-*, kani, tsan, asan
//   os      cross-OS / cross-arch / MSRV
//   crate_* the gates of one workspace crate
// A schedule or manual dispatch, the `full-ci` PR label, and any change to this
// planner or to ci.yml select EVERYTHING. An unrecognised path selects `fast`
// (conservative), never nothing.
//
// Usage:
//   node scripts/ci-plan.mjs --event pull_request --base <sha> --head <sha> [--labels a,b]
//   node scripts/ci-plan.mjs --event push --base <before> --head <sha>
//   node scripts/ci-plan.mjs --files-from <file> --event pull_request   (tests, replay)
//   node scripts/ci-plan.mjs --replay <n>      classify the last n first-parent merges of HEAD
//   node scripts/ci-plan.mjs --selftest
import { spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync, existsSync } from 'node:fs';
import assert from 'node:assert/strict';
import { basename } from 'node:path';

export const CRATE_GROUPS = {
  'aligned-vmem': 'crate_aligned_vmem',
  'globalalloc-model': 'crate_globalalloc_model',
  'numa-shim': 'crate_numa_shim',
  'once-ptr-cell': 'crate_once_ptr_cell',
  'proc-memstat': 'crate_proc',
  'proc-probe': 'crate_proc',
  'sefer-region': 'crate_sefer_region',
  'size-classes': 'crate_size_classes',
  'tagged-index-stack': 'crate_tagged_index_stack',
};
export const GROUPS = ['always', 'fast', 'seam', 'os', ...new Set(Object.values(CRATE_GROUPS))];
// Crates whose code is platform-specific: a change also exercises the cross-OS jobs.
const OS_CRATES = new Set(['aligned-vmem', 'numa-shim', 'proc-memstat', 'proc-probe']);

const FULL_FILES = new Set(['.github/workflows/ci.yml', 'scripts/ci-plan.mjs']);
const FAST_SCRIPTS = new Set(['check-matrix.mjs', 'run-check-matrix.mjs', 'lib.mjs', 'check-all.mjs']);
const SEAM_SCRIPTS = new Set(['loom.mjs', 'miri.mjs', 'miri-matrix.mjs']);

/** Groups selected by one path. `seamTests` = test basenames named by the loom/miri manifests. */
export function classifyPath(path, seamTests = new Set(), cargoBenchOnly = false) {
  const p = path.replace(/\\/g, '/');
  if (FULL_FILES.has(p)) return new Set(['full']);
  let m = /^crates\/([^/]+)\//.exec(p);
  if (m) {
    const g = new Set(['fast', 'seam']); // the root crate depends on every workspace crate
    if (CRATE_GROUPS[m[1]]) g.add(CRATE_GROUPS[m[1]]);
    if (OS_CRATES.has(m[1])) g.add('os');
    return g;
  }
  if (/^src\/alloc_core\/platform\//.test(p)) return new Set(['fast', 'seam', 'os']);
  if (/^src\//.test(p)) return new Set(['fast', 'seam']);
  // A root Cargo.toml edit that only adds/removes [[bench]]/[[example]]/[[test]] targets changes no
  // dependency, feature or MSRV surface: it only needs the build/test groups.
  if (p === 'Cargo.toml' && cargoBenchOnly) return new Set(['fast']);
  if (/^(Cargo\.toml|Cargo\.lock|build\.rs|rust-toolchain(\.toml)?|\.cargo\/.*)$/.test(p)) {
    return new Set(['fast', 'seam', 'os']); // dependency / toolchain / MSRV surface
  }
  if (/^tests\//.test(p)) {
    const g = new Set(['fast']);
    const name = basename(p).replace(/\.rs$/, '');
    if (/(loom|miri|kani)/.test(name) || seamTests.has(name)) g.add('seam');
    if (/(windows|macos|numa|platform|_os_)/.test(name)) g.add('os');
    return g;
  }
  if (/^(benches|examples|fuzz)\//.test(p)) return new Set(['fast']);
  m = /^scripts\/([^/]+)$/.exec(p);
  if (m) {
    if (SEAM_SCRIPTS.has(m[1])) return new Set(['fast', 'seam']);
    if (FAST_SCRIPTS.has(m[1])) return new Set(['fast']);
    return new Set(['always']);
  }
  if (/^(docs\/|\.rush\/|\.github\/)/.test(p) || /\.(md|txt|csv|sha256|json)$/.test(p) ||
      /^(deny\.toml|\.gitignore|\.gitattributes|LICENSE.*|package\.json|package-lock\.json)$/.test(p)) {
    return new Set(['always']);
  }
  return new Set(['fast']); // unknown: conservative
}

/** @returns {Record<string, boolean>} one boolean per group plus `full`. */
export function plan({ files, event, labels = [], seamTests = new Set(), cargoBenchOnly = false }) {
  const out = Object.fromEntries([...GROUPS, 'full'].map(g => [g, false]));
  out.always = true;
  const forceFull = event === 'schedule' || event === 'workflow_dispatch' || labels.includes('full-ci');
  for (const f of files) {
    for (const g of classifyPath(f, seamTests, cargoBenchOnly)) out[g] = true;
  }
  if (out.full || forceFull) {
    for (const g of Object.keys(out)) out[g] = true;
    return out;
  }
  if (event === 'push') {
    // A merge to main re-runs what its PR already ran; the heavy seam/os groups are
    // left to the nightly full run (docs/CI_TIERS.md).
    out.seam = false;
    out.os = false;
  }
  return out;
}

/** True when a `git diff -U0` of Cargo.toml only touches [[bench]]/[[example]]/[[test]] target tables. */
export function cargoTomlBenchOnly(diffText) {
  let seen = false, inTarget = false;
  for (const line of diffText.split(/\r?\n/)) {
    if (/^(diff |index |--- |\+\+\+ |@@)/.test(line) || line === '') continue;
    if (!/^[+-]/.test(line)) continue;
    const body = line.slice(1);
    if (/^\s*(#.*)?$/.test(body)) continue; // blank lines and comments are neutral
    seen = true;
    if (/^\[\[(bench|example|test)\]\]\s*$/.test(body)) { inTarget = true; continue; }
    if (/^\[/.test(body)) return false; // any other table header
    if (!/^\s*(name|path|harness|test|bench|doc|required-features)\s*=/.test(body)) return false;
    if (!inTarget) return false; // key lines must follow a target header
  }
  return seen;
}

function git(args) {
  const r = spawnSync('git', args, { encoding: 'utf8', maxBuffer: 1 << 26 });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout;
}
const zero = s => !s || /^0+$/.test(s);

function seamTestNames() {
  const names = new Set();
  for (const f of ['scripts/loom.mjs', 'scripts/miri-matrix.mjs', 'scripts/miri.mjs']) {
    if (!existsSync(f)) continue;
    for (const m of readFileSync(f, 'utf8').matchAll(/\b([a-z][a-z0-9_]+)\b/g)) names.add(m[1]);
  }
  return names;
}

function arg(name) {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function filesFromGit(base, head, event) {
  if (zero(base)) return null; // new branch / forced push: no usable base => caller treats as unknown
  try {
    return git(['diff', '--name-only', `${base}`, `${head}`]).split('\n').filter(Boolean);
  } catch (e) {
    console.error(`ci-plan: ${e.message}; falling back to a full plan`);
    return null;
  }
}

function summary(result, files, event) {
  const on = Object.entries(result).filter(([, v]) => v).map(([k]) => k);
  return `ci-plan: event=${event} files=${files ? files.length : '?'} groups=${on.join(',')}`;
}

function selftest() {
  const seam = new Set(['class_leaves_publication_miri']);
  const P = (files, event = 'pull_request', labels = []) => plan({ files, event, labels, seamTests: seam });
  const on = r => Object.keys(r).filter(k => r[k]).sort().join(',');
  const heavy = ['seam', 'os'];
  let n = 0;
  const eq = (got, want, msg) => { assert.equal(got, want, msg); n++; };

  // docs / scripts / reviews only: just `always`.
  eq(on(P(['docs/reviews/x.md', 'README.md', 'scripts/r20_prf01_verify_artifacts.mjs'])), 'always', 'docs-only');
  eq(on(P(['docs/perf/R20_PRF04_CLAIM_PATH_RUN01/summary.csv', 'docs/perf/x.sha256'])), 'always', 'evidence data only');
  eq(on(P(['.rush/stdin/a.prompt'])), 'always', 'rush prompts');
  // tests / benches / examples without seam names: fast.
  eq(on(P(['tests/foo_bar.rs'])), 'always,fast', 'ordinary test');
  eq(on(P(['benches/r20_prf01_controls_iai.rs', 'examples/e.rs'])), 'always,fast', 'bench+example');
  // test that appears in the loom/miri manifests or is named loom/miri: seam too.
  eq(on(P(['tests/class_leaves_publication_miri.rs'])), 'always,fast,seam', 'miri test');
  eq(on(P(['tests/loom_r20_prf01_summary_transition.rs'])), 'always,fast,seam', 'loom test');
  eq(on(P(['tests/numa_seam.rs'])), 'always,fast,os', 'numa test');
  // root src: fast + seam; platform dir adds os.
  eq(on(P(['src/registry/heap_registry/claim.rs'])), 'always,fast,seam', 'src');
  eq(on(P(['src/alloc_core/platform/x.rs'])), 'always,fast,os,seam', 'platform src');
  // dependency / toolchain surface: fast+seam+os (MSRV, cross-OS).
  eq(on(P(['Cargo.lock'])), 'always,fast,os,seam', 'lock');
  // workspace crates: own gates + root dependents (+os for platform crates).
  eq(on(P(['crates/sefer-region/src/lib.rs'])), 'always,crate_sefer_region,fast,seam', 'crate');
  eq(on(P(['crates/aligned-vmem/src/lib.rs'])), 'always,crate_aligned_vmem,fast,os,seam', 'platform crate');
  eq(on(P(['crates/proc-probe/src/lib.rs', 'crates/proc-memstat/src/lib.rs'])), 'always,crate_proc,fast,os,seam', 'proc');
  eq(on(P(['crates/brand-new/src/lib.rs'])), 'always,fast,seam', 'unknown crate is conservative');
  // scripts: matrix/manifest scripts feed fast/seam; other scripts are `always`.
  eq(on(P(['scripts/check-matrix.mjs'])), 'always,fast', 'check-matrix');
  eq(on(P(['scripts/loom.mjs'])), 'always,fast,seam', 'loom script');
  eq(on(P(['scripts/verify-structure-limits.mjs'])), 'always', 'verify script');
  // unknown top-level path: fast (never nothing).
  eq(on(P(['weird/new/file.bin'])), 'always,fast', 'unknown path');
  // ci.yml / the planner itself, schedule, dispatch, label: everything.
  const all = [...GROUPS, 'full'].sort().join(',');
  eq(on(P(['.github/workflows/ci.yml'])), all, 'ci.yml => full');
  eq(on(P(['scripts/ci-plan.mjs'])), all, 'planner => full');
  eq(on(P(['docs/a.md'], 'schedule')), all, 'schedule => full');
  eq(on(P(['docs/a.md'], 'workflow_dispatch')), all, 'dispatch => full');
  eq(on(P(['docs/a.md'], 'pull_request', ['full-ci'])), all, 'label => full');
  eq(on(P([], 'pull_request', ['other'])), 'always', 'no files: always only');
  // other workflows under .github do not select ci.yml groups.
  eq(on(P(['.github/workflows/r20-gates.yml'])), 'always', 'other workflow');
  // push to main: heavy groups are left to the nightly run, the rest follows the diff.
  const push = P(['src/registry/heap_registry/claim.rs', 'crates/numa-shim/src/lib.rs'], 'push');
  eq(on(push), 'always,crate_numa_shim,fast', 'push drops seam/os');
  for (const g of heavy) eq(push[g], false, `push ${g}`);
  eq(on(P(['docs/a.md'], 'push')), 'always', 'push docs');
  // an ordinary PR never silently drops a group a path asked for.
  const mixed = P(['docs/a.md', 'tests/x.rs', 'src/lib.rs', 'crates/numa-shim/src/lib.rs']);
  for (const g of ['always', 'fast', 'seam', 'os', 'crate_numa_shim']) eq(mixed[g], true, `mixed ${g}`);
  // Cargo.toml: only [[bench]]/[[example]]/[[test]] edits are fast-only; anything else keeps the full surface.
  const benchAdd = '@@ -1 +1,5 @@\n+\n+[[bench]]\n+name = "x"\n+harness = false\n+required-features = ["a"]\n';
  eq(cargoTomlBenchOnly(benchAdd), true, 'bench-only diff');
  eq(cargoTomlBenchOnly('@@ -1 +1 @@\n-serde = "1"\n+serde = "2"\n'), false, 'dependency diff');
  eq(cargoTomlBenchOnly('@@ -1 +1 @@\n+[features]\n+a = []\n'), false, 'feature table diff');
  eq(cargoTomlBenchOnly('@@ -1 +1 @@\n+name = "x"\n'), false, 'key without a target header');
  eq(cargoTomlBenchOnly(''), false, 'empty diff');
  eq(cargoTomlBenchOnly('@@ -1 +1 @@\n+# note\n+[[bench]]\n+name = "x"\n'), true, 'comment + bench target');
  eq(cargoTomlBenchOnly('@@ -1 +1 @@\n+# only a comment\n'), false, 'comment only is not a target change');
  eq(on(plan({ files: ['Cargo.toml'], event: 'pull_request', cargoBenchOnly: true })), 'always,fast', 'Cargo.toml bench-only');
  eq(on(plan({ files: ['Cargo.toml'], event: 'pull_request', cargoBenchOnly: false })), 'always,fast,os,seam', 'Cargo.toml general');
  // Windows-style separators classify like POSIX ones.
  eq(on(P(['src\\registry\\x.rs'])), 'always,fast,seam', 'backslashes');
  console.log(JSON.stringify({ version: 1, status: 'PASS', scope: 'ci-plan selftest', checks: n }));
}

function main() {
  if (process.argv.includes('--selftest')) return selftest();
  if (process.argv.includes('--replay')) {
    const count = Number(arg('--replay') ?? 10);
    const merges = git(['log', '--first-parent', '--merges', `-${count}`, '--format=%H %s']).trim().split('\n');
    const seamTests = seamTestNames();
    for (const line of merges) {
      const [sha, ...subject] = line.split(' ');
      const files = git(['diff', '--name-only', `${sha}^1`, sha]).split('\n').filter(Boolean);
      let benchOnly = false;
      if (files.includes('Cargo.toml')) benchOnly = cargoTomlBenchOnly(git(['diff', '-U0', `${sha}^1`, sha, '--', 'Cargo.toml']));
      const r = plan({ files, event: 'pull_request', seamTests, cargoBenchOnly: benchOnly });
      console.log(`${sha.slice(0, 8)} ${String(files.length).padStart(3)} files  ${Object.keys(r).filter(k => r[k]).join(',').padEnd(60)} ${subject.join(' ').slice(0, 60)}`);
    }
    return;
  }
  const event = arg('--event') ?? process.env.GITHUB_EVENT_NAME ?? 'pull_request';
  const labels = (arg('--labels') ?? '').split(',').map(s => s.trim()).filter(Boolean);
  let files;
  if (arg('--files-from')) files = readFileSync(arg('--files-from'), 'utf8').split(/\r?\n/).filter(Boolean);
  else files = filesFromGit(arg('--base'), arg('--head') ?? 'HEAD', event);
  let result;
  if (files === null && event !== 'schedule' && event !== 'workflow_dispatch') {
    // no usable base (new branch, force push, unreachable base): run everything, never less.
    result = plan({ files: ['.github/workflows/ci.yml'], event });
  } else {
    let benchOnly = false;
    if (files && files.includes('Cargo.toml') && !zero(arg('--base'))) {
      try { benchOnly = cargoTomlBenchOnly(git(['diff', '-U0', arg('--base'), arg('--head') ?? 'HEAD', '--', 'Cargo.toml'])); } catch { benchOnly = false; }
    }
    result = plan({ files: files ?? [], event, labels, seamTests: seamTestNames(), cargoBenchOnly: benchOnly });
  }
  console.log(summary(result, files, event));
  const outPath = process.env.GITHUB_OUTPUT;
  if (outPath) appendFileSync(outPath, Object.entries(result).map(([k, v]) => `${k}=${v}\n`).join(''));
  else console.log(JSON.stringify(result));
  if (process.env.GITHUB_STEP_SUMMARY) {
    const on = Object.keys(result).filter(k => result[k]);
    appendFileSync(process.env.GITHUB_STEP_SUMMARY,
      `### CI plan\n\n- event: \`${event}\`, changed files: ${files ? files.length : 'unknown (full plan)'}\n- groups on: ${on.map(g => `\`${g}\``).join(', ')}\n`);
  }
}

if (import.meta.url === `file://${process.argv[1].replace(/\\/g, '/')}` ||
    process.argv[1]?.endsWith('ci-plan.mjs')) main();
