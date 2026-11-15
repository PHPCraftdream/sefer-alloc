#!/usr/bin/env node
// Cargo-free Node classification of the sefer-alloc integration tests.
//
// Purpose: derive, WITHOUT cargo/network, the full target/non-target
// inventory of tests/, per-file classification flags, external pin surfaces,
// mergeability (strict rules), merge groups, the pilot candidate and the
// relaxation ladder — as three deterministic stdout outputs.
//
// Usage:
//   node scripts/classify-tests.mjs --json     # full JSON data structure
//   node scripts/classify-tests.mjs --csv      # RFC-4180 CSV, one row per tests/ entry
//   node scripts/classify-tests.mjs --report   # full Markdown report
//   (exactly one flag required; no flag or several -> usage, exit 2)
//
// Derives (from repo data only):
//   - targets/non-targets via Cargo autotests rules (scripts/classify-tests/)
//   - per-file flags via comment-stripped classification
//   - first-config-wins markers from src/ provenance
//   - external pins (workflows, actions, scripts, docs) + published counts
//   - mergeability slugs, groups, pilot, relaxation ladder, intersections
// Asserts (hard errors):
//   - git identity commands succeed; tracked files under tests/, Cargo.toml,
//     src/ are clean per `git status --porcelain` (untracked allowed)
//   - targets + non-targets cover every tests/ file exactly once
//   - every explicit [[test]] table resolves to an existing file
//   - group members == mergeable set; sum(saved_links) == mergeable - groups
//   - every pairwise intersection <= min(|A|,|B|)
//   - hypotheses 'measured' values come from the counts object by construction
// Determinism: sorted everywhere, no wall-clock timestamps; the report header
// carries the fixed plan date label '2026-11-15'. The script writes nothing.

import { spawnSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { parseToml, packageValue } from './classify-tests/toml.mjs';
import { discoverTargets } from './classify-tests/discover.mjs';
import {
  classifySource, deriveFirstConfigWinMarkers,
  FIRST_CONFIG_WIN_CANDIDATES, MARKER_PROVENANCE, listSrcRustFiles, markerRegex,
  rawMatchZone,
} from './classify-tests/features.mjs';
import { collectPinSurfaces, scanAllPins } from './classify-tests/pins.mjs';
import {
  mergeability, themePrefix, groupKey, buildGroups, pilotCandidate,
  relaxationLadder, assertGroupInvariants, intersections,
} from './classify-tests/groups.mjs';
import { renderReport, PLAN_HYPOTHESES } from './classify-tests/report.mjs';

const PLAN_DATE = '2026-11-15';
const USAGE = `usage: node scripts/classify-tests.mjs --json | --csv | --report
(plan label ${PLAN_DATE}; exactly one output flag)`;

// ---------------------------------------------------------------- CLI plumbing
const args = process.argv.slice(2).filter((a) => a.startsWith('--'));
const modes = args.filter((a) => ['--json', '--csv', '--report'].includes(a));
if (args.length !== modes.length || modes.length !== 1) {
  process.stderr.write(`${USAGE}\n`);
  process.exit(2);
}
const mode = modes[0];

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// ---------------------------------------------------------------- identity
function git(argsv) {
  const r = spawnSync('git', argsv, { cwd: repoRoot, encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`git ${argsv.join(' ')} failed: ${r.stderr}`);
  return r.stdout.trim();
}

const headSha = git(['rev-parse', 'HEAD']);
const treeSha = git(['rev-parse', 'HEAD^{tree}']);
const porcelain = git(['status', '--porcelain', '--untracked-files=normal']);

// Tracked files under tests/, Cargo.toml, src/ must be clean; untracked new
// files are allowed (this worktree legitimately adds classification scripts).
const dirty = [];
for (const line of porcelain.split(/\r?\n/)) {
  if (!line) continue;
  const status = line.slice(0, 2);
  let path = line.slice(3);
  const arrow = path.indexOf(' -> ');
  if (arrow !== -1) path = path.slice(arrow + 4);
  if (status === '??') continue; // untracked allowed
  if (path === 'Cargo.toml' || path.startsWith('tests/') || path.startsWith('src/')) {
    dirty.push(`${status} ${path}`);
  }
}
if (dirty.length > 0) {
  throw new Error(`tracked classification inputs are dirty:\n${dirty.join('\n')}`);
}
const trackedInputsClean = true;

// ---------------------------------------------------------------- discovery
const manifest = parseToml(readFileSync(join(repoRoot, 'Cargo.toml'), 'utf8'));
const autotests = packageValue(manifest, 'autotests', true);
const { targets, nonTargets, explicitCount } = discoverTargets(repoRoot, manifest);

// Coverage assertion: every file under tests/ appears exactly once across
// targets + nonTargets (both hold paths relative to repoRoot, posix-style).
const listedPaths = new Map();
for (const t of targets) listedPaths.set(t.path, 'target');
for (const n of nonTargets) {
  if (listedPaths.has(n.path)) throw new Error(`entry listed twice: ${n.path}`);
  listedPaths.set(n.path, n.kind);
}
const allFiles = [];
const walkFiles = (dir, rel) => {
  for (const ent of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const childRel = rel === '' ? ent.name : `${rel}/${ent.name}`;
    if (ent.isDirectory()) walkFiles(join(dir, ent.name), childRel);
    else if (ent.isFile()) allFiles.push(childRel);
  }
};
walkFiles(join(repoRoot, 'tests'), '');
for (const f of allFiles) {
  const p = `tests/${f}`;
  if (!listedPaths.has(p)) throw new Error(`tests/ file missing from inventory: ${p}`);
}
if (listedPaths.size !== targets.length + nonTargets.length) {
  throw new Error('inventory paths inconsistent');
}

// Explicit tables resolve to existing files.
for (const t of targets) {
  if (!t.declared_explicit) continue;
  try { statSync(join(repoRoot, t.path)); } catch {
    throw new Error(`explicit [[test]] table resolves to missing file: ${t.path}`);
  }
}

// ---------------------------------------------------------------- markers
const srcDir = join(repoRoot, 'src');
const srcFiles = listSrcRustFiles(srcDir);
const grepFirstHit = (dir, marker) => {
  void dir;
  const re = markerRegex(marker);
  for (const f of srcFiles) {
    const text = readFileSync(join(srcDir, f), 'utf8');
    const m = text.match(re);
    if (m) {
      const before = text.slice(0, m.index);
      const line = before.split(/\r?\n/).length;
      return { file: `src/${f}`, line };
    }
  }
  return null;
};
const markerRecords = deriveFirstConfigWinMarkers(srcDir, FIRST_CONFIG_WIN_CANDIDATES, grepFirstHit);
const markerList = markerRecords.map((m) => ({
  marker: m.marker,
  src_hit: `${m.src_hit.file}:${m.src_hit.line}`,
  provenance: MARKER_PROVENANCE[m.marker] || 'derived from src/ grep',
}));

// ---------------------------------------------------------------- classify
const pins = scanAllPins(collectPinSurfaces(repoRoot), targets.map((t) => t.name));

const rows = [];
for (const t of targets) {
  const contents = readFileSync(join(repoRoot, t.path), 'utf8');
  const facts = classifySource(contents, markerRecords);
  const externalPins = pins.perTarget.get(t.name) || { count: 0, hits: [] };
  const m = mergeability(t, facts, externalPins);
  rows.push({ target: t, facts, externalPins, raw: contents, ...m });
}
rows.sort((a, b) => a.target.path.localeCompare(b.target.path));

// ---------------------------------------------------------------- groups
const mergeableRows = rows.filter((r) => r.mergeable);
const mergeableNames = mergeableRows.map((r) => r.target.name).sort((a, b) => a.localeCompare(b));
const groupsMap = buildGroups(mergeableRows, groupKey);
assertGroupInvariants(mergeableNames, groupsMap);
const pilot = pilotCandidate(groupsMap);
const groupsSorted = [...groupsMap.values()].sort((a, b) => b.savedLinks - a.savedLinks ||
  a.key.localeCompare(b.key));
const relaxation = relaxationLadder(rows);

// ---------------------------------------------------------------- naive baselines
// Fixed, deliberately naive grep baselines over RAW target text (documented
// in the report caption). The plan's §1 numbers were greps like these.
const NAIVE_BASELINES = {
  global_allocator: { re: /#\[global_allocator\]/, pattern: '/\\#\\[global_allocator\\]/ on raw text' },
  first_config_wins: { re: /\b(with_config|config_conflicts|thp_tuple)\b/, pattern: '\\b(with_config|config_conflicts|thp_tuple)\\b on raw text' },
  process_spawn: { re: /(process::Command|current_exe|CARGO_BIN_EXE_)/, pattern: '(process::Command|current_exe|CARGO_BIN_EXE_) on raw text' },
  loom_tree: { re: /\bloom\b/, pattern: '\\bloom\\b on raw text' },
  miri_tree: { re: /\bmiri\b/, pattern: '\\bmiri\\b on raw text' },
  file_level_cfg: { re: /^\s*#!\[cfg/, pattern: 'lines matching ^\\s*#!\\[cfg on raw text (line-anchored)' },
};
const MEASURED_FLAG = {
  global_allocator: (f) => f.global_allocator === true,
  first_config_wins: (f) => f.first_config_wins.length > 0,
  process_spawn: (f) => f.process_spawn === true,
  loom_tree: (f) => f.loom_only === true,
  miri_tree: (f) => f.miri_only === true,
  file_level_cfg: (f) => f.file_level_cfg !== null || f.file_level_cfg_attr !== null,
};

function baselineOnlyReason(flag, raw, zone) {
  if (zone === 'comment') return 'comment-only';
  if (zone === 'string-literal') return 'string-literal';
  // matched in code but the measured flag is false: the word sits in an
  // ITEM-level attr (e.g. #[cfg(miri)] on one item) or is not a derived
  // first-config-wins marker
  if (flag === 'first_config_wins') return 'marker-list';
  return 'item-level-attr-only';
}
function measuredOnlyReason(flag, raw) {
  if (flag === 'file_level_cfg') {
    return /^\s*#!\[cfg_attr/m.test(raw) ? 'cfg_attr-form' : 'multi-line-attr';
  }
  if (flag === 'process_spawn') {
    // true positive via `use std::process::{Command, ...}` + bare Command::new
    return /\bCommand::new\b/.test(raw) ? 'bare-Command-import' : 'cfg_attr-form';
  }
  if (flag === 'first_config_wins') {
    // derived marker matched (e.g. claim_lease_with_config) that the naive
    // word-boundary pattern cannot see
    return 'marker-list';
  }
  return 'cfg_attr-form';
}

const reconciliation = Object.entries(NAIVE_BASELINES).map(([flag, { re, pattern }]) => {
  const naiveRe = flag === 'file_level_cfg' ? new RegExp(re.source, 'm') : re;
  const baselineOnly = [];
  const measuredOnly = [];
  let naive = 0;
  let measured = 0;
  for (const r of rows) {
    const naiveHit = naiveRe.test(r.raw);
    const measuredHit = MEASURED_FLAG[flag](r.facts);
    if (naiveHit) naive++;
    if (measuredHit) measured++;
    if (naiveHit && !measuredHit) {
      baselineOnly.push({ path: r.target.path,
        reason: baselineOnlyReason(flag, r.raw, rawMatchZone(r.raw, naiveRe)) });
    } else if (measuredHit && !naiveHit) {
      measuredOnly.push({ path: r.target.path, reason: measuredOnlyReason(flag, r.raw) });
    }
  }
  baselineOnly.sort((a, b) => a.path.localeCompare(b.path));
  measuredOnly.sort((a, b) => a.path.localeCompare(b.path));
  return {
    flag, naivePattern: pattern, naive, measured,
    baselineOnly: { count: baselineOnly.length, files: baselineOnly },
    measuredOnly: { count: measuredOnly.length, files: measuredOnly },
  };
});

// Naive counts keyed like the hypotheses metrics (plan §1 = grep approximations).
const cargoRaw = readFileSync(join(repoRoot, 'Cargo.toml'), 'utf8');
const naiveCounts = {
  'tests/ .rs files': targets.length,
  'file-level cfg': reconciliation.find((r) => r.flag === 'file_level_cfg').naive,
  global_allocator: reconciliation.find((r) => r.flag === 'global_allocator').naive,
  first_config_wins: reconciliation.find((r) => r.flag === 'first_config_wins').naive,
  process_spawn: reconciliation.find((r) => r.flag === 'process_spawn').naive,
  'loom_only (plan definition: loom mentions)': reconciliation.find((r) => r.flag === 'loom_tree').naive,
  'harness=false (all harness=false lines in Cargo.toml)':
    cargoRaw.split(/\r?\n/).filter((l) => l.includes('harness = false')).length,
  '[[test]] tables': cargoRaw.split(/\r?\n/).filter((l) => /^\[\[test\]\]/.test(l)).length,
};

// ---------------------------------------------------------------- counts
const flagCount = (pred) => rows.filter((r) => pred(r.facts, r.target)).length;
const nonTargetsByKind = {};
for (const n of nonTargets) nonTargetsByKind[n.kind] = (nonTargetsByKind[n.kind] || 0) + 1;
const topLevelNonTargets = nonTargets.filter((n) => !n.path.slice('tests/'.length).includes('/')).length;

const counts = {
  targets: targets.length,
  targetsEnabled: targets.filter((t) => t.test_enabled).length,
  targetsTestFalse: targets.filter((t) => !t.test_enabled).length,
  declaredExplicit: explicitCount,
  harnessFalse: targets.filter((t) => !t.harness).length,
  nonTargets: nonTargets.length,
  topLevelEntries: targets.length + topLevelNonTargets,
  topLevelNonTargets,
  totalEntries: targets.length + nonTargets.length,
  autotests: autotests === true,
  global_allocator: flagCount((f) => f.global_allocator),
  first_config_wins: flagCount((f) => f.first_config_wins.length > 0),
  process_spawn: flagCount((f) => f.process_spawn),
  abort: flagCount((f) => f.abort_marker),
  should_panic: flagCount((f) => f.should_panic),
  catch_unwind: flagCount((f) => f.catch_unwind),
  loom_only: flagCount((f) => f.loom_only),
  loom_mentions: flagCount((f) => f.loom_mentions),
  miri_only: flagCount((f) => f.miri_only),
  kani_gated: flagCount((f) => f.kani_gated),
  main_fn: flagCount((f) => f.main_fn),
  file_level_cfg: flagCount((f) => f.file_level_cfg !== null || f.file_level_cfg_attr !== null),
  file_cfg_only: flagCount((f) => f.file_level_cfg !== null),
  cfg_attr: flagCount((f) => f.file_level_cfg_attr !== null),
  required_features: targets.filter((t) => t.required_features.length > 0).length,
  externally_pinned: rows.filter((r) => r.externalPins.count > 0).length,
  mergeable: mergeableRows.length,
  groupCount: groupsMap.size,
  totalSavedLinks: [...groupsMap.values()].reduce((s, g) => s + g.savedLinks, 0),
};

// Hypotheses table built FROM the counts object (derived, not typed).
const HYPOTHESIS_COUNTS_KEYS = {
  'tests/ .rs files': 'targets',
  'file-level cfg': 'file_level_cfg',
  'global_allocator': 'global_allocator',
  'first_config_wins': 'first_config_wins',
  'process_spawn': 'process_spawn',
  'loom_only (plan definition: loom mentions)': 'loom_only',
  'harness=false (all harness=false lines in Cargo.toml)': 'harnessFalse',
  '[[test]] tables': 'declaredExplicit',
};
const hypotheses = {
  plan: PLAN_HYPOTHESES.map((h) => {
    const key = HYPOTHESIS_COUNTS_KEYS[h.metric];
    if (!key || counts[key] === undefined) {
      throw new Error(`hypothesis '${h.metric}' has no counts key`);
    }
  return { metric: h.metric, planValue: h.planValue, naive: naiveCounts[h.metric],
    measured: counts[key],
    verdict: counts[key] === h.planValue ? 'confirmed' : 'corrected' };
  }),
  notes: [
    'plan sets overlap and are grep approximations without comment stripping; the naive column reproduces those greps over raw text (see reconciliation table)',
    'harness=false measured counts test targets only; the plan\'s 52 counts every `harness = false` line in Cargo.toml including [[bench]]/[[example]] tables',
    'loom column: strict loom_only measured; informational loom_mentions is in counts; naive loom counts every raw `loom` word (file-level cfg + loom:: use)',
    'plan 79 global_allocator vs naive 79 vs measured 64: the 15 baseline-only files are comment/string-only mentions (see reconciliation)',
  ],
};

// ---------------------------------------------------------------- intersections
const INTERSECTION_FLAGS = ['global_allocator', 'first_config_wins', 'process_spawn',
  'loom_only', 'miri_only', 'kani_gated', 'harness_false', 'test_disabled',
  'required_features', 'file_cfg', 'externally_pinned'];
const intersectionRows = rows.map((r) => ({
  name: r.target.name,
  flags: {
    global_allocator: r.facts.global_allocator,
    first_config_wins: r.facts.first_config_wins,
    process_spawn: r.facts.process_spawn,
    loom_only: r.facts.loom_only,
    miri_only: r.facts.miri_only,
    kani_gated: r.facts.kani_gated,
    harness_false: !r.target.harness,
    test_disabled: !r.target.test_enabled,
    required_features: r.target.required_features.length > 0,
    file_cfg: r.facts.file_level_cfg !== null || r.facts.file_level_cfg_attr !== null,
    externally_pinned: r.externalPins.count > 0,
  },
}));
const inter = intersections(intersectionRows, INTERSECTION_FLAGS);

// ---------------------------------------------------------------- JSON output
const groupOfTarget = new Map();
for (const g of groupsMap.values()) {
  for (const n of g.members) groupOfTarget.set(n, g.key);
}

if (mode === '--json') {
  const out = {
    planDate: PLAN_DATE,
    identity: { headSha, treeSha, trackedInputsClean },
    counts,
    markerList,
    hypotheses,
    reconciliation,
    groups: groupsSorted,
    pilot: pilot && {
      key: pilot.key, members: pilot.members, savedLinks: pilot.savedLinks,
      publishedCounts: pins.publishedCounts,
      j13HashPins: pins.j13.j13HashPins,
      testsFilesHashPinned: pins.j13.testsFilesHashPinned,
      srcOrOtherFilesHashPinned: pins.j13.srcOrOtherFilesHashPinned,
    },
    relaxation,
    pins: {
      publishedCounts: pins.publishedCounts,
      j13Scripts: pins.j13.j13Scripts,
      j13HashPins: pins.j13.j13HashPins,
      hashPinnedFiles: pins.j13.hashPinnedFiles,
      testsFilesHashPinned: pins.j13.testsFilesHashPinned,
      srcOrOtherFilesHashPinned: pins.j13.srcOrOtherFilesHashPinned,
      pinnedTargetCount: counts.externally_pinned,
      surfaceFiles: pins.surfaceFiles,
    },
    intersectionFlags: INTERSECTION_FLAGS,
    intersections: { sets: inter.sets, matrix: inter.matrix },
    nonTargets,
  };
  process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
} else if (mode === '--csv') {
// ---------------------------------------------------------------- CSV output
const yesno = (b) => (b ? 'yes' : 'no');
function csvField(v) {
  const s = String(v ?? '');
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
}

const CSV_HEADER = ['path', 'name', 'kind', 'test_enabled', 'harness_false', 'declared_explicit',
  'required_features', 'file_level_cfg', 'cfg_features', 'cfg_keys', 'global_allocator',
  'first_config_wins', 'process_spawn', 'abort', 'should_panic', 'catch_unwind',
  'loom_only', 'miri_only', 'kani_gated', 'main_fn', 'lines', 'external_pin_count',
  'external_pin_surfaces', 'mergeable', 'unmergeable_reasons', 'group'];

const csvLines = [CSV_HEADER.join(',')];
const pushRow = (fields) => csvLines.push(fields.map(csvField).join(','));

for (const r of rows) {
  const t = r.target;
  const f = r.facts;
  const surfaces = r.externalPins.hits.map((h) => `${h.file}:${h.kind}`);
  pushRow([
    t.path, t.name, 'target',
    yesno(t.test_enabled), yesno(!t.harness), yesno(t.declared_explicit),
    t.required_features.join(' '),
    f.file_level_cfg || '', f.cfg_features.join(' '), f.cfg_keys.join(' '),
    yesno(f.global_allocator), f.first_config_wins.join(','), yesno(f.process_spawn),
    yesno(f.abort_marker), yesno(f.should_panic), yesno(f.catch_unwind),
    yesno(f.loom_only), yesno(f.miri_only), yesno(f.kani_gated), yesno(f.main_fn),
    f.lines, r.externalPins.count, surfaces.join(';'),
    yesno(r.mergeable), r.reasons.join(';'), groupOfTarget.get(t.name) || '',
  ]);
}
for (const n of nonTargets) {
  pushRow([n.path, n.path.split('/').pop(), n.kind,
    '', '', '', '', '', '', '', 'no', '', 'no', 'no', 'no', 'no', 'no', 'no', 'no', 'no',
    '', 0, '', 'no', '', '']);
}
process.stdout.write(`${csvLines.join('\r\n')}\r\n`);
} else {

// ---------------------------------------------------------------- report output
const reportData = {
  identity: { headSha, treeSha, trackedInputsClean },
  counts,
  markerList,
  reconciliation,
  pins: {
    surfaceFiles: pins.surfaceFiles,
    publishedCounts: pins.publishedCounts,
    j13HashPins: pins.j13.j13HashPins,
    testsFilesHashPinned: pins.j13.testsFilesHashPinned,
    srcOrOtherFilesHashPinned: pins.j13.srcOrOtherFilesHashPinned,
  },
  hypotheses: Object.fromEntries(hypotheses.plan.map((h) => [h.metric, h.measured])),
  hypothesesRows: hypotheses.plan,
  hypothesesNotes: hypotheses.notes,
  intersectionFlags: INTERSECTION_FLAGS,
  intersections: inter,
  nonTargets,
  groups: groupsSorted,
  pilot: pilot && {
    key: pilot.key, members: pilot.members, savedLinks: pilot.savedLinks,
  },
  relaxation,
};
process.stdout.write(renderReport(reportData));
}
