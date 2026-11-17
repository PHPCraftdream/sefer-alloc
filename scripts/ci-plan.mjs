#!/usr/bin/env node
// CI tier planner: maps the files a change touches to the job groups ci.yml must run.
// Pure git-diff + path rules, no third-party action. Output: one `group=true|false`
// line per group (to $GITHUB_OUTPUT when set) plus a human summary on stdout.
//
// Groups (see docs/CI_TIERS.md):
//   always  fmt, commit-prefix-lint, evidence-registry, deny, licenses, docs-consistency
//   fast    clippy, check-matrix, test-*, no_std, docs, fuzz-build
//   seam    unsafe-seam proofs: miri-*, loom-*, kani, tsan, asan
//   os      cross-OS / cross-arch / MSRV
//   crate_* the gates of one workspace crate
// A schedule, a manual dispatch, or a `CI: full` trailer in the message of any commit of the
// range select EVERYTHING. The PR label `full-ci` and the auto-full on edits of ci.yml or this
// planner are gone: only an explicit, written trigger runs the full set. An unrecognised path
// selects `fast` (conservative), never nothing.
//
// Usage:
//   node scripts/ci-plan.mjs --event pull_request --base <sha> --head <sha>
//   node scripts/ci-plan.mjs --event push --base <before> --head <sha>
//   node scripts/ci-plan.mjs --files-from <file> --event pull_request   (tests, replay)
//   node scripts/ci-plan.mjs --replay <n>      classify the last n first-parent merges of HEAD
//   node scripts/ci-plan.mjs --selftest
import { spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync, existsSync } from 'node:fs';
import assert from 'node:assert/strict';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';
import { affectedCrates, crateForPath, loadCrateGraph, rootAffected } from './ci/crate-graph.mjs';

export const CRATE_GROUPS = {
  'aligned-vmem': 'crate_aligned_vmem',
  'globalalloc-model': 'crate_globalalloc_model',
  'numa-shim': 'crate_numa_shim',
  'once-ptr-cell': 'crate_once_ptr_cell',
  'proc-memstat': 'crate_proc',
  'proc-probe': 'crate_proc',
  'sefer-concurrent': 'crate_sefer_concurrent',
  'sefer-ghost': 'crate_sefer_ghost',
  'sefer-platform': 'crate_sefer_platform',
  'sefer-policy': 'crate_sefer_policy',
  'sefer-region': 'crate_sefer_region',
  'size-classes': 'crate_size_classes',
  'tagged-index-stack': 'crate_tagged_index_stack',
};
export const GROUPS = ['always', 'fast', 'seam', 'os', ...new Set(Object.values(CRATE_GROUPS))];
// Crates whose code is platform-specific: a change also exercises the cross-OS jobs.
const OS_CRATES = new Set(['aligned-vmem', 'numa-shim', 'proc-memstat', 'proc-probe', 'sefer-platform']);

// Edits of the workflow and of this planner run the build/test group (and the planner self-test);
// the full set needs an explicit `CI: full` trailer, a manual dispatch or the schedule.
const FAST_FILES = new Set(['.github/workflows/ci.yml', 'scripts/ci-plan.mjs']);
const FULL_TRAILER = /^CI: *full *$/im;

/** True when a commit message carries the explicit full-run trailer `CI: full` on a line of its own. */
export function hasFullTrailer(message) {
  return FULL_TRAILER.test(message ?? '');
}
const FAST_SCRIPTS = new Set(['check-matrix.mjs', 'run-check-matrix.mjs', 'lib.mjs', 'check-all.mjs']);
const SEAM_SCRIPTS = new Set(['loom.mjs', 'miri.mjs', 'miri-matrix.mjs']);

/** Groups selected by one path. `seamTests` = test basenames named by the loom/miri manifests. */
export function classifyPath(path, seamTests = new Set(), cargoBenchOnly = false) {
  const p = path.replace(/\\/g, '/');
  if (FAST_FILES.has(p)) return new Set(['fast']);
  if (p.startsWith('.github/actions/') || p.startsWith('scripts/ci/') || p === 'scripts/verify-ci-decomposition.mjs') {
    const expanded = effectiveWorkflow(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
    const jobs = parseJobs(expanded.text);
    // A deleted action still selects its job after the body moves back inline.
    const owners = expanded.dependencies.get(p) ?? new Set(jobs
      .filter(job => p === `.github/actions/ci-${job.id}/action.yml`).map(job => job.id));
    if (owners.size) return new Set(jobs.filter(job => owners.has(job.id))
      .map(job => GROUPS.includes(job.group) ? job.group : 'fast'));
    if (p.startsWith('scripts/')) return new Set(GROUPS.filter(group => group !== 'full'));
    return new Set(['fast']);
  }
  let m = /^crates\/([^/]+)\/([^/]+)\//.exec(p);
  if (m) {
    const g = new Set(['fast', 'seam']); // the root crate depends on every workspace crate
    if (CRATE_GROUPS[m[2]]) g.add(CRATE_GROUPS[m[2]]);
    if (OS_CRATES.has(m[2])) g.add('os');
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
  // Actual-source Miri imports these payload helpers.
  if (p === 'examples/_shared/j13b_owned_block.rs' || p === 'examples/_shared/j13b_window_kernel.rs' || p === 'examples/_shared/j13b_payload_actors.rs' || p === 'examples/_shared/j13b_owner_training.rs') return new Set(['fast', 'seam']);
  if (p === 'examples/_shared/j13b_os_probe.rs' || p === 'examples/_shared/j13b_adaptive_windows.rs') return new Set(['fast', 'seam', 'os']);
  if (/^(benches|examples|fuzz)\//.test(p)) return new Set(['fast']);
  // Cargo-free script packages (module-graph lint, classify-tests) consumed by
  // always-group jobs: no cargo group exercises them, and their top-level
  // entry points already classify 'always'.
  if (/^scripts\/(module-graph|classify-tests)\//.test(p)) return new Set(['always']);
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
export function plan({ files, event, trailerFull = false, seamTests = new Set(), cargoBenchOnly = false, crateGraph = null, crateGroups = CRATE_GROUPS }) {
  const out = Object.fromEntries([...GROUPS, 'full'].map(g => [g, false]));
  out.always = true;
  const forceFull = event === 'schedule' || event === 'workflow_dispatch' || trailerFull;
  // Groups wanted ONLY by crate-file rules. With a graph, `fast` always stays
  // on for a crate change: the root test jobs inside it are narrowed by
  // select-tests and enforce is off, so fast runs everything anyway. `seam`/`os`
  // are the real heavy root-crate groups: they are suppressed only when the
  // root is unaffected by both Cargo edges and reference edges (rootAffected).
  // Without a graph the old conservative behavior (crate files always select
  // fast/seam) is kept.
  const crateOnly = { fast: false, seam: false, os: false, groups: new Set() };
  const changedCrates = new Set();
  for (const f of files) {
    const p = f.replace(/\\/g, '/');
    const crate = crateForPath(p, crateGraph);
    if (crateGraph && crate) {
      changedCrates.add(crate);
      if (OS_CRATES.has(crate)) crateOnly.os = true;
      crateOnly.fast = true;
      crateOnly.seam = true;
      const g = crateGroups[crate];
      if (g) crateOnly.groups.add(g);
    } else {
      // null graph, or a crates/** path the graph cannot resolve: old rules.
      for (const g of classifyPath(f, seamTests, cargoBenchOnly)) out[g] = true;
    }
  }
  if (!crateGraph) {
    // Regression guard for the pre-graph behavior: nothing else to do, all
    // crate-originated fast/seam/os above already went straight into `out`.
  } else {
    const affected = affectedCrates(changedCrates, crateGraph);
    for (const name of affected) {
      const g = crateGroups[name];
      if (g) out[g] = true;
    }
    // Root crate affected (directly or via a dependent chain, Cargo or
    // reference edges) => its heavy seam/os surface must run.
    if (rootAffected(affected, crateGraph)) {
      out.seam ||= crateOnly.seam;
      out.os ||= crateOnly.os;
    }
    // fast applies regardless: root-side selection is handled by select-tests.
    out.fast ||= crateOnly.fast;
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

const TABLE_BEGIN = '<!-- ci-tiers:begin (generated by `node scripts/ci-plan.mjs --jobs-table`) -->';
const TABLE_END = '<!-- ci-tiers:end -->';
const GROUP_ORDER = ['always', 'fast', 'seam', 'os', ...new Set(Object.values(CRATE_GROUPS)), 'weekly', 'dispatch'];

/** Every ci.yml job as `{ id, name, group }`, the group being the one that gates it, from the workflow text. */
export function parseJobs(ciText) {
  const lines = ciText.split(/\r?\n/);
  const start = lines.findIndex(l => /^jobs:\s*$/.test(l));
  const jobs = [];
  let cur = null;
  for (let i = start + 1; i < lines.length; i++) {
    const m = /^  ([A-Za-z0-9_-]+):\s*$/.exec(lines[i]);
    if (m) { cur = { id: m[1], name: '', group: 'always' }; jobs.push(cur); continue; }
    if (!cur) continue;
    const n = /^    name:\s*(.*)$/.exec(lines[i]);
    if (n && !cur.name) cur.name = n[1].replace(/^['"]|['"]$/g, '');
    const f = /^    if:\s*(.*)$/.exec(lines[i]);
    if (f) {
      const g = /needs\.changes\.outputs\.([a-z_]+) == 'true'/.exec(f[1]);
      if (g) cur.group = g[1];
      else if (/github\.event\.schedule/.test(f[1])) cur.group = 'weekly';
      else if (/workflow_dispatch/.test(f[1])) cur.group = 'dispatch';
    }
  }
  return jobs;
}

/** Markdown table of every ci.yml job and the group that gates it, derived from the workflow text. */
export function jobsTable(ciText) {
  const jobs = parseJobs(ciText);
  const WHEN = {
    always: 'every change',
    fast: '`fast`: source, tests, benches, examples, fuzz, build scripts',
    seam: '`seam`: source/crates/Cargo, loom/miri tests and manifests',
    os: '`os`: Cargo/toolchain surface, platform code, platform crates',
    weekly: 'weekly schedule or manual dispatch only',
    dispatch: 'manual dispatch only',
  };
  const rows = jobs.sort((a, b) => GROUP_ORDER.indexOf(a.group) - GROUP_ORDER.indexOf(b.group) || a.id.localeCompare(b.id))
    .map(j => `| \`${j.id}\` | ${(j.name || j.id).replace(/\|/g, '\\|')} | ${WHEN[j.group] ?? `\`${j.group}\`: changes under its crate`} |`);
  return ['| Job | Name | Runs when |', '|---|---|---|', ...rows].join('\n');
}

/** True when the generated table between the markers of `docText` matches `ciText`. */
export function docTableMatches(docText, ciText) {
  if (docText.indexOf(TABLE_END) !== docText.lastIndexOf(TABLE_END)) return false; // duplicate end marker
  const a = docText.indexOf(TABLE_BEGIN), b = docText.indexOf(TABLE_END);
  if (a < 0 || b < a) return false;
  return docText.slice(a + TABLE_BEGIN.length, b).trim() === jobsTable(ciText).trim();
}

function selftest() {
  const seam = new Set(['class_leaves_publication_miri', 'j13b_adaptive_windows_native']);
  const P = (files, event = 'pull_request', trailerFull = false) => plan({ files, event, trailerFull, seamTests: seam });
  const on = r => Object.keys(r).filter(k => r[k]).sort().join(',');
  // Fixture graph mirroring the workspace shape: root -> mid -> leaf chain, a
  // standalone leaf, and a dev-dep edge. Names kept abstract on purpose.
  const fx = () => {
    const g = {
      packages: new Map([
        ['sefer-alloc', { dir: '.', deps: new Set(['mid']), dependents: new Set() }],
        ['mid', { dir: 'crates/x/mid', deps: new Set(['leaf', 'devdep']), dependents: new Set(['sefer-alloc']) }],
        ['leaf', { dir: 'crates/x/leaf', deps: new Set(), dependents: new Set(['mid']) }],
        ['standalone', { dir: 'crates/x/standalone', deps: new Set(), dependents: new Set() }],
        ['devdep', { dir: 'crates/x/devdep', deps: new Set(), dependents: new Set(['mid']) }],
      ]),
      root: 'sefer-alloc',
    };
    return g;
  };
  const CRATE_GROUPS_FIX = { mid: 'crate_mid', leaf: 'crate_leaf', standalone: 'crate_standalone', devdep: 'crate_devdep' };
  const PG = (files, graph, event = 'pull_request', groups = CRATE_GROUPS_FIX) =>
    plan({ files, event, seamTests: seam, crateGraph: graph, crateGroups: groups });
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
  eq(on(P(['examples/_shared/j13b_owned_block.rs'])), 'always,fast,seam', 'unsafe judge owner source');
  eq(on(P(['examples/_shared/j13b_window_kernel.rs'])), 'always,fast,seam', 'actual-source window operation model');
  eq(on(P(['examples/_shared/j13b_payload_actors.rs'])), 'always,fast,seam', 'actual-source persistent payload actors');
  eq(on(P(['examples/_shared/j13b_owner_training.rs'])), 'always,fast,seam', 'actual-source training model-rejection guard');
  eq(on(P(['examples/_shared/j13b_run_stats.rs'])), 'always,fast', 'window statistic record only');
  eq(on(P(['examples/_shared/j13b_os_probe.rs'])), 'always,fast,os,seam', 'native OS producer and model rejection');
  eq(on(P(['examples/_shared/j13b_os_sample.rs'])), 'always,fast', 'OS quantity record only');
  eq(on(P(['examples/_shared/j13b_adaptive_windows.rs'])), 'always,fast,os,seam', 'adaptive native OS windows and model rejection');
  eq(on(P(['tests/j13b_adaptive_windows_native.rs'])), 'always,fast,os,seam', 'adaptive window model-rejection fixture');
  eq(on(P(['examples/_shared/j13b_workload.rs'])), 'always,fast', 'safe judge plan/kernel source');
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
  eq(on(P(['crates/containers/sefer-concurrent/src/lib.rs'])), 'always,crate_sefer_concurrent,fast,seam', 'container crate');
  eq(on(P(['crates/containers/sefer-region/src/lib.rs'])), 'always,crate_sefer_region,fast,seam', 'crate');
  eq(on(P(['crates/primitives/sefer-ghost/src/lib.rs'])), 'always,crate_sefer_ghost,fast,seam', 'crate');
  eq(on(P(['crates/core/sefer-policy/src/lib.rs'])), 'always,crate_sefer_policy,fast,seam', 'crate');
  eq(on(P(['crates/platform/aligned-vmem/src/lib.rs'])), 'always,crate_aligned_vmem,fast,os,seam', 'platform crate');
  eq(on(P(['crates/platform/sefer-platform/src/os.rs'])), 'always,crate_sefer_platform,fast,os,seam', 'sefer-platform crate');
  eq(on(P(['crates/platform/proc-probe/src/lib.rs', 'crates/platform/proc-memstat/src/lib.rs'])), 'always,crate_proc,fast,os,seam', 'proc');
  eq(on(P(['crates/brand-new/src/lib.rs'])), 'always,fast,seam', 'unknown crate is conservative');
  // scripts: matrix/manifest scripts feed fast/seam; other scripts are `always`.
  eq(on(P(['scripts/check-matrix.mjs'])), 'always,fast', 'check-matrix');
  eq(on(P(['scripts/loom.mjs'])), 'always,fast,seam', 'loom script');
  eq(on(P(['scripts/verify-structure-limits.mjs'])), 'always', 'verify script');
  eq(on(P(['scripts/module-graph/graph.mjs'])), 'always', 'module-graph package');
  eq(on(P(['scripts/classify-tests/discover.mjs'])), 'always', 'classify-tests package');
  // unknown top-level path: fast (never nothing).
  eq(on(P(['weird/new/file.bin'])), 'always,fast', 'unknown path');
  // Schedule, dispatch and an explicit `CI: full` trailer select everything; ci.yml and the planner
  // edits no longer do (they run the build/test group), and neither does a PR label.
  const all = [...GROUPS, 'full'].sort().join(',');
  eq(on(P(['.github/workflows/ci.yml'])), 'always,fast', 'ci.yml => fast, not full');
  eq(on(P(['scripts/ci-plan.mjs'])), 'always,fast', 'planner => fast, not full');
  eq(on(P(['docs/a.md'], 'schedule')), all, 'schedule => full');
  eq(on(P(['docs/a.md'], 'workflow_dispatch')), all, 'dispatch => full');
  eq(on(P(['docs/a.md'], 'pull_request', true)), all, 'trailer => full');
  eq(on(P(['docs/a.md'], 'push', true)), all, 'trailer on push => full, seam and os included');
  eq(on(P(['src/lib.rs'], 'push', false)), 'always,fast', 'push without trailer drops seam/os');
  eq(on(P([])), 'always', 'no files: always only');
  eq(hasFullTrailer('perf: x\n\nbody\n\nCI: full\n\nCo-Authored-By: A <a@b>'), true, 'trailer in a message');
  eq(hasFullTrailer('subject\n\nCI: full'), true, 'trailer at the end, no newline');
  eq(hasFullTrailer('subject\r\n\r\nci: Full \r\n'), true, 'case and CRLF tolerant');
  eq(hasFullTrailer('CI: full'), true, 'trailer on the first line');
  eq(hasFullTrailer('subject\n\nthe CI: full run was green'), false, 'prose mention is not a trailer');
  eq(hasFullTrailer('subject\n\nCI: fullish'), false, 'other value');
  eq(hasFullTrailer('subject\n\nCI: fast'), false, 'other value 2');
  eq(hasFullTrailer(''), false, 'empty message');
  // other workflows under .github do not select ci.yml groups.
  eq(on(P(['.github/workflows/r20-gates.yml'])), 'always', 'other workflow');
  // push to main: heavy groups are left to the nightly run, the rest follows the diff.
  const push = P(['src/registry/heap_registry/claim.rs', 'crates/platform/numa-shim/src/lib.rs'], 'push');
  eq(on(push), 'always,crate_numa_shim,fast', 'push drops seam/os');
  for (const g of heavy) eq(push[g], false, `push ${g}`);
  eq(on(P(['docs/a.md'], 'push')), 'always', 'push docs');
  // an ordinary PR never silently drops a group a path asked for.
  const mixed = P(['docs/a.md', 'tests/x.rs', 'src/lib.rs', 'crates/platform/numa-shim/src/lib.rs']);
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
  // The generated jobs table follows the workflow's own gating.
  const fixture = ['jobs:', '  a:', '    name: A', '  b:', '    needs: changes', "    if: needs.changes.outputs.seam == 'true'",
    '    name: B', '  c:', "    if: ${{ github.event_name == 'workflow_dispatch' || github.event.schedule == '0 6 * * 1' }}", '    name: C'].join('\n');
  const t = jobsTable(fixture);
  eq(/\| `a` \| A \| every change \|/.test(t), true, 'table always');
  eq(/\| `b` \| B \| `seam`/.test(t), true, 'table seam');
  eq(/\| `c` \| C \| weekly schedule/.test(t), true, 'table weekly');
  eq(docTableMatches(`x\n${TABLE_BEGIN}\n${t}\n${TABLE_END}\n`, fixture), true, 'doc matches');
  eq(docTableMatches(`x\n${TABLE_BEGIN}\n${t}\nextra\n${TABLE_END}\n`, fixture), false, 'doc drift detected');
  eq(docTableMatches('no markers', fixture), false, 'doc without markers');
  eq(docTableMatches(`x\n${TABLE_BEGIN}\n${t}\n${TABLE_END}\n${TABLE_END}\n`, fixture), false, 'duplicate end marker rejected');
  // Windows-style separators classify like POSIX ones.
  eq(on(P(['src\\registry\\x.rs'])), 'always,fast,seam', 'backslashes');
  // --- crate-graph narrowing (fixture graph injected via plan({crateGraph})) ---
  // Every "don't run" assertion below has a counterfactual: the same files with
  // crateGraph=null keep fast/seam (see the crates/** assertions above), so a
  // regression that silently re-enables the blanket selection goes red here.
  const G = fx();
  // (a) standalone leaf, root does not depend on it: fast stays on (root test
  // jobs inside it are narrowed by select-tests), seam/os are suppressed.
  eq(on(plan({ files: ['crates/x/standalone/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_standalone,fast', 'standalone crate keeps fast, drops seam/os');
  // (b) crate the root depends on: root affected => fast/seam stay on.  eq(on(plan({ files: ['crates/x/mid/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_mid,fast,seam', 'root-dependent crate keeps fast/seam');
  // (c) leaf changed, only mid depends on it, root depends on mid: both groups + fast/seam.
  eq(on(plan({ files: ['crates/x/leaf/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_leaf,crate_mid,fast,seam', 'dependent chain narrows upward');
  // (d) dev-dependency edge: devdep changed pulls mid's group.
  eq(on(plan({ files: ['crates/x/devdep/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_devdep,crate_mid,fast,seam', 'dev-dep edge triggers dependent');
  // (e) unknown crate dir with a graph: old conservative fast+seam.
  eq(on(plan({ files: ['crates/brand-new/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,fast,seam', 'unknown crate dir with graph');
  // (f) regression guard: null graph + crates path = old behavior exactly.
  eq(on(P(['crates/containers/sefer-region/src/lib.rs'])), 'always,crate_sefer_region,fast,seam', 'null graph old behavior');
  // (g) mixed change: crate file + src file => fast/seam regardless of root.
  eq(on(plan({ files: ['crates/x/standalone/src/lib.rs', 'src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_standalone,fast,seam', 'mixed change keeps fast/seam');
  // (h) root Cargo.lock still the full fast+seam+os surface with a graph present.
  eq(on(plan({ files: ['Cargo.lock'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,fast,os,seam', 'Cargo.lock with graph');
  // (i) push event still drops seam/os after narrowing.
  eq(on(plan({ files: ['crates/x/mid/src/lib.rs'], event: 'push', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_mid,fast', 'push drops seam/os after narrowing');
  // crate manifest (Cargo.toml inside the crate dir) affects that crate like its src.
  eq(on(plan({ files: ['crates/x/standalone/Cargo.toml'], event: 'pull_request', seamTests: seam, crateGraph: G, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_standalone,fast', 'crate manifest keeps fast');
  // counterfactual for (a): without narrowing the same file WOULD select fast+seam.
  eq(on(plan({ files: ['crates/x/standalone/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: null, crateGroups: CRATE_GROUPS_FIX })), 'always,fast,seam', 'counterfactual: no graph => fast/seam');
  // (j) HOLE-2 regression guard: a reference edge (#[path]/include!) from the
  // root alone keeps seam/os on even with no declared Cargo path. The fixture
  // root mirrors the post-mergeRefEdges state (ref edge merged into deps).
  const GR = fx();
  GR.packages.set('sefer-alloc', { dir: '.', deps: new Set(['mid', 'standalone']), dependents: new Set(), refDeps: new Set(['standalone']) });
  GR.packages.get('standalone').dependents.add('sefer-alloc');
  eq(on(plan({ files: ['crates/x/standalone/src/lib.rs'], event: 'pull_request', seamTests: seam, crateGraph: GR, crateGroups: CRATE_GROUPS_FIX })), 'always,crate_standalone,fast,seam', 'reference edge alone keeps seam/os on');
  console.log(JSON.stringify({ version: 1, status: 'PASS', scope: 'ci-plan selftest', checks: n }));
}

function main() {
  if (process.argv.includes('--selftest')) return selftest();
  if (process.argv.includes('--jobs-table')) {
    console.log(`${TABLE_BEGIN}\n${jobsTable(readFileSync('.github/workflows/ci.yml', 'utf8'))}\n${TABLE_END}`);
    return;
  }
  if (process.argv.includes('--check-doc')) {
    const doc = arg('--check-doc') ?? 'docs/CI_TIERS.md';
    const ok = docTableMatches(readFileSync(doc, 'utf8'), readFileSync('.github/workflows/ci.yml', 'utf8'));
    console.log(ok ? `ci-plan: ${doc} jobs table matches ci.yml` : `ci-plan: ${doc} jobs table is stale; regenerate with: node scripts/ci-plan.mjs --jobs-table`);
    process.exitCode = ok ? 0 : 1;
    return;
  }
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
  if ((arg('--labels') ?? '').split(',').includes('full-ci')) {
    console.log('ci-plan: the full-ci label no longer selects the full set; put `CI: full` on its own line in a commit message or dispatch the workflow');
  }
  let files;
  if (arg('--files-from')) files = readFileSync(arg('--files-from'), 'utf8').split(/\r?\n/).filter(Boolean);
  else files = filesFromGit(arg('--base'), arg('--head') ?? 'HEAD', event);
  let result;
  if (files === null && event !== 'schedule' && event !== 'workflow_dispatch') {
    // no usable base (new branch, force push, unreachable base): run everything, never less.
    result = plan({ files: [], event, trailerFull: true });
  } else {
    let benchOnly = false;
    if (files && files.includes('Cargo.toml') && !zero(arg('--base'))) {
      try { benchOnly = cargoTomlBenchOnly(git(['diff', '-U0', arg('--base'), arg('--head') ?? 'HEAD', '--', 'Cargo.toml'])); } catch { benchOnly = false; }
    }
    let trailerFull = false;
    if (!zero(arg('--base'))) {
      try { trailerFull = hasFullTrailer(git(['log', '--format=%B', `${arg('--base')}..${arg('--head') ?? 'HEAD'}`])); } catch { trailerFull = false; }
    }
    if (trailerFull) console.log('ci-plan: `CI: full` trailer found in the commit range; running the full set');
    // Graph load failure (cargo unavailable etc.) returns null: old behavior.
    const crateGraph = loadCrateGraph(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
    result = plan({ files: files ?? [], event, trailerFull, seamTests: seamTestNames(), cargoBenchOnly: benchOnly, crateGraph });
    if (crateGraph) {
      const changed = new Set();
      for (const f of files ?? []) {
        const c = crateForPath(f, crateGraph);
        if (c) changed.add(c);
      }
      if (changed.size) console.log(`ci-plan: affected crates: ${[...affectedCrates(changed, crateGraph)].sort().join(', ')}`);
    }
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

if (import.meta.url === `file://${process.argv[1]?.replace(/\\/g, '/')}` ||
    process.argv[1]?.endsWith('ci-plan.mjs')) main();
