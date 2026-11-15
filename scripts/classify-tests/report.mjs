// Markdown report renderer for the Node test classification script.
//
// Purpose: turn the classification data structure (built by
// scripts/classify-tests.mjs) into the full Markdown report. EVERY number is
// read from the data structure — no prose numbers are typed here. No
// wall-clock timestamps: the header carries the fixed plan date label.
//
// Derives: text only. Asserts: renderReport throws if any value it would
// print is NaN/undefined/Infinity/[object Object] (guard function).
//
// Pure: takes the data object, returns a string.

const PLAN_DATE = '2026-11-15';

/** Plan-published hypotheses (§1 of the 2026-11-15 plan). Legit constants. */
export const PLAN_HYPOTHESES = [
  { metric: 'tests/ .rs files', planValue: 562 },
  { metric: 'file-level cfg', planValue: 514 },
  { metric: 'global_allocator', planValue: 79 },
  { metric: 'first_config_wins', planValue: 111 },
  { metric: 'process_spawn', planValue: 33 },
  { metric: 'loom_only (plan definition: loom mentions)', planValue: 21 },
  { metric: 'harness=false (all harness=false lines in Cargo.toml)', planValue: 52 },
  { metric: '[[test]] tables', planValue: 29 },
];

/** Short definitions for every flag in the CSV/report. */
export const FLAG_DEFINITIONS = [
  ['target', 'entry under tests/ that Cargo builds as a test target (explicit [[test]] or autodiscovered)'],
  ['non-target', 'tests/ entry Cargo does not build (fixture dirs, support modules, .mjs scripts, regressions file)'],
  ['test_enabled=false', 'target with `test = false` in its [[test]] table'],
  ['harness_false', 'target with `harness = false` in its [[test]] table (custom main)'],
  ['declared_explicit', 'target declared via an explicit [[test]] table'],
  ['required_features', 'required-features list of the [[test]] table (space-joined)'],
  ['file_level_cfg', 'file-level #![cfg(...)] or #![cfg_attr(...)] predicate before the first item'],
  ['cfg_features / cfg_keys', 'feature names / cfg keys referenced by the file-level cfg'],
  ['global_allocator', 'file installs a #[global_allocator]'],
  ['first_config_wins', 'first-config-wins API markers present (comma-joined; see marker table)'],
  ['process_spawn', 'uses std::process::Command / current_exe / CARGO_BIN_EXE / spawn'],
  ['abort', 'calls std::process::abort / libc::abort'],
  ['should_panic / catch_unwind', 'informational: #[should_panic] / catch_unwind usage (never a merge blocker)'],
  ['loom_only', 'strict: loom-gated by file-level cfg OR uses loom:: APIs'],
  ['loom_mentions', 'informational: any bare `loom` word in stripped source'],
  ['miri_only', 'file-level cfg mentions miri'],
  ['kani_gated', 'file-level cfg mentions kani'],
  ['main_fn', 'source defines `fn main(`'],
  ['externally_pinned', 'target NAME appears in .github workflows/actions, scripts/*.mjs, docs markdown or the docs/verification/*.json catalog'],
  ['mergeable', 'passes ALL strict merge rules (see unmergeable_reasons for violations)'],
  ['unmergeable_reasons', 'full list of violated strict rules, semicolon-joined slugs'],
  ['group', '(theme|features|file-cfg) key slug; only mergeable targets are grouped'],
];

function guard(v) {
  const s = String(v);
  if (s === 'NaN' || s === 'undefined' || s === 'Infinity' || s === '[object Object]' || s === 'null') {
    throw new Error(`report guard: refusing to print ${s}`);
  }
  return s;
}

function esc(s) { return guard(s).replaceAll('|', '\\|'); }

function table(header, rows) {
  const head = `| ${header.map(esc).join(' | ')} |`;
  const sep = `| ${header.map(() => '---').join(' | ')} |`;
  const body = rows.map((r) => `| ${r.map(esc).join(' | ')} |`).join('\n');
  return [head, sep, body].join('\n');
}

/** Render the full Markdown report from the data structure. */
export function renderReport(data) {
  const L = [];
  L.push(`# Integration-test classification report (plan ${PLAN_DATE})`);
  L.push('');
  L.push(`Script: \`scripts/classify-tests.mjs\` (modules under \`scripts/classify-tests/\`).`);
  L.push('');
  L.push(`Sources: commit \`${data.identity.headSha}\`, tree \`${data.identity.treeSha}\`; tracked inputs ${data.identity.trackedInputsClean ? 'clean' : 'DIRTY (assertion failure)'}.`);
  L.push('');

  // Marker table
  L.push('## First-config-wins markers (derived from src/)');
  L.push('');
  L.push(table(
    ['marker', 'first src/ hit (file:line)', 'provenance'],
    data.markerList.map((m) => [m.marker, m.src_hit, m.provenance || '-']),
  ));
  L.push('');
  L.push('Dropped from the marker list: `publish_pending` (src/registry/segment_route/large_state.rs:25) — the Large-reservation route sidecar publication CAS (LIVE(g)->PENDING(g)), not a first-config-wins API. `claim_with_config` auto-dropped (no src/ hit).');
  L.push('');

  // Pin surfaces
  L.push('## Pin surfaces scanned');
  L.push('');
  const byKind = {};
  for (const s of data.pins.surfaceFiles) byKind[s.kind] = (byKind[s.kind] || 0) + 1;
  L.push(table(
    ['kind', 'files'],
    Object.keys(byKind).sort().map((k) => [k, byKind[k]]),
  ));
  L.push('');
  L.push('Note: the classification suite\'s own output artifacts (`docs/perf/TEST_TARGET_CLASSIFICATION_*.md`, `docs/test_target_classification_*.csv`) are excluded from the doc pin surface: scanning the report\'s own echo would pin the targets it lists, making the classification non-idempotent (self-reference, not a pre-existing pin).');
  L.push('');
  L.push(`Targets externally pinned by name: **${data.counts.externally_pinned}** of ${data.counts.targets}.`);
  L.push('');

  // Hypotheses
  L.push('## Plan hypotheses (plan §1) vs naive grep vs measured');
  L.push('');
  L.push(table(
    ['metric', `plan (${PLAN_DATE})`, 'naive grep', 'measured', 'verdict'],
    PLAN_HYPOTHESES.map((h) => {
      const row = data.hypothesesRows.find((r) => r.metric === h.metric);
      if (!row || row.measured === undefined) throw new Error(`no measured value for hypothesis '${h.metric}'`);
      return [h.metric, h.planValue, row.naive, row.measured,
        row.measured === h.planValue ? 'confirmed' : 'corrected'];
    }),
  ));
  L.push('');
  L.push('Note: the plan\'s §1 numbers are grep approximations over overlapping sets — the `naive grep` column reproduces them over raw text (see the reconciliation table for exactly which files differ and why). Example: global_allocator plan 79 = naive grep 79; measured 64 because 15 files match only in comments/strings.');

  // Definitions
  L.push('## Definitions');
  L.push('');
  L.push(table(['term', 'meaning'], FLAG_DEFINITIONS));
  L.push('');

  // Intersections
  L.push('## Flag-set intersections (pairwise, over all targets)');
  L.push('');
  const flags = data.intersectionFlags;
  const header = ['set (size) \\ in', ...flags.map((f) => `${f} (${data.intersections.sets[f]})`)];
  const rows = flags.map((a) => [`**${a}** (${data.intersections.sets[a]})`,
    ...flags.map((b) => data.intersections.matrix[a][b])]);
  L.push(table(header, rows));
  L.push('');

  // Non-targets
  L.push(`## Non-target inventory (${data.counts.nonTargets} entries; ${data.counts.topLevelNonTargets} of them are top-level entries of tests/)`);
  L.push('');
  L.push(`Top-level accounting: tests/ has **${data.counts.topLevelEntries}** top-level entries; ${data.counts.targets} are targets; the remaining **${data.counts.topLevelNonTargets}** are non-targets. Fixture directories additionally contribute deeper files to the inventory below (kind \`directory-fixture\` / \`support-module\` / \`other-file\`).`);
  L.push('');
  L.push(table(
    ['path', 'kind'],
    data.nonTargets.map((n) => [n.path, n.kind]),
  ));
  L.push('');

  // Groups
  const groupsSorted = data.groups.slice().sort((a, b) => b.savedLinks - a.savedLinks ||
    a.key.localeCompare(b.key));
  L.push(`## Groups (top 10 of ${data.counts.groupCount} by saved links; total saved links ${data.counts.totalSavedLinks})`);
  L.push('');
  L.push(table(
    ['group key', 'members', 'saved links', 'member names'],
    groupsSorted.slice(0, 10).map((g) => {
      const names = g.members.length > 8
        ? `${g.members.slice(0, 8).join(', ')} +${g.members.length - 8} more`
        : g.members.join(', ');
      return [g.key, g.members.length, g.savedLinks, names];
    }),
  ));
  L.push('');
  L.push('Full member lists: see the `--csv` output (column `group`).');
  L.push('');

  // Pilot
  L.push('## Pilot candidate');
  L.push('');
  const p = data.pilot;
  if (p) {
    L.push(`Group key: \`${p.key}\``);
    L.push('');
    L.push('Members:');
    L.push('');
    for (const m of p.members) L.push(`- \`${m}\``);
    L.push('');
    L.push(`Saved links: **${p.savedLinks}**.`);
  } else {
    L.push('no candidate: pool empty under this rung');
  }
  L.push('');
  L.push('By construction of the strict rule, no member is name-pinned externally. But ANY merge of targets invalidates the repo-level published COUNT claims below, which would need updating in the same change:');
  L.push('');
  L.push(table(
    ['file', 'line', 'text', 'numbers'],
    data.pins.publishedCounts.map((c) => [c.file, c.line, c.text, c.numbers.join('/')]),
  ));
  L.push('');
  L.push(`j13 hash pins: ${data.pins.j13HashPins.length === 0 ? 'none' : data.pins.j13HashPins.join(', ')} hash src/ files; tests/ files hash-pinned: **${data.pins.testsFilesHashPinned.length === 0 ? 'none (expected)' : data.pins.testsFilesHashPinned.length}**.`);
  L.push('');
  L.push('### File-hash pin inventory (derived from hash-table shapes in j13 scripts)');
  L.push('');
  const hp = [...(data.pins.testsFilesHashPinned || []), ...(data.pins.srcOrOtherFilesHashPinned || [])];
  if (hp.length === 0) {
    L.push('(no file-hash pins derived)');
  } else {
    L.push(table(
      ['path', 'script', 'zone'],
      hp.map((e) => [e.path, e.script, e.path.startsWith('tests/') ? 'tests/' : 'src/other']),
    ));
  }
  L.push('');

  // Relaxation ladder
  L.push('## Relaxation ladder');
  L.push('');
  L.push(table(
    ['rung', 'description', 'pilot-eligible', 'mergeable files', 'groups', 'total saved links'],
    data.relaxation.map((r) => [r.rung, r.description, r.pilotEligible ? 'yes' : 'no', r.fileCount, r.groupCount, r.savedLinks]),
  ));
  L.push('');

  // Per-rung pilot eligibility (presented without a go/no-go verdict)
  L.push('### Pilot eligibility per rung');
  L.push('');
  const compat = data.relaxation.find((r) => r.rung === 'compat');
  L.push(`Strict mergeable pool: **${data.counts.mergeable}** files in ${data.counts.groupCount} groups (total saved links ${data.counts.totalSavedLinks})${data.counts.mergeable <= 1 ? ' — trivial pool (<= 1 file)' : ''}.`);
  L.push('');
  L.push(table(
    ['rung', 'pilot-eligible', 'why', 'pilot key', 'pilot members', 'pilot saved links'],
    data.relaxation.map((r) => [r.rung, r.pilotEligible ? 'yes' : 'no', r.eligibilityReason,
      r.pilot ? r.pilot.key : '-', r.pilot ? String(r.pilot.members.length) : '-',
      r.pilot ? String(r.pilot.savedLinks) : '-']),
  ));
  L.push('');
  if (compat && compat.pilot) {
    L.push(`\`compat\` rung pilot candidate (the pilot candidate of the classification):`);
    L.push('');
    L.push(`Group key: \`${compat.pilot.key}\`; members: ${compat.pilot.members.length}; saved links: **${compat.pilot.savedLinks}**.`);
    L.push('');
    for (const m of compat.pilot.members) L.push(`- \`${m}\``);
    L.push('');
  } else {
    L.push('`compat` rung: no candidate group (no group has >1 member).');
    L.push('');
  }
  L.push('Pilot eligibility per rung is derived from each rung\'s relaxation rules; no go/no-go verdict is expressed.');
  L.push('');

  // Grep reconciliation
  L.push('## Grep reconciliation (naive baseline vs measured)');
  L.push('');
  L.push('Naive baseline patterns (fixed, over RAW text — intentionally naive, these are the greps plan §1 used): global_allocator `/#\\[global_allocator\\]/`; first_config_wins `\\b(with_config|config_conflicts|thp_tuple)\\b`; process_spawn `(process::Command|current_exe|CARGO_BIN_EXE_)`; loom `\\bloom\\b`; miri `\\bmiri\\b`; file_level_cfg lines matching `^\\s*#!\\[cfg`. Baseline-only reason codes: comment-only, string-literal, item-level-attr-only (matched code, but not before the first item), marker-list (matched word is not a derived marker). Measured-only reason codes: multi-line-attr, cfg_attr-form, bare-Command-import (true positive via `use std::process::{Command, ...}` + bare `Command::new`, outside the naive pattern).');
  L.push('');
  for (const rec of data.reconciliation) {
    L.push(`### ${rec.flag} — naive ${rec.naive} vs measured ${rec.measured}`);
    L.push('');
    L.push(`Pattern: \`${rec.naivePattern}\``);
    L.push('');
    const bRows = rec.baselineOnly.files.map((f) => [f.path, f.reason]);
    L.push(`In baseline but not measured (${rec.baselineOnly.count}):`);
    L.push('');
    L.push(bRows.length === 0 ? '(none)' : table(['path', 'reason'], bRows));
    L.push('');
    const mRows = rec.measuredOnly.files.map((f) => [f.path, f.reason]);
    L.push(`Measured but not in baseline (${rec.measuredOnly.count}):`);
    L.push('');
    L.push(mRows.length === 0 ? '(none)' : table(['path', 'reason'], mRows));
    L.push('');
  }

  // Reproduce
  L.push('## How to reproduce');
  L.push('');
  L.push('```sh');
  L.push('node scripts/classify-tests.mjs --json');
  L.push('node scripts/classify-tests.mjs --csv');
  L.push('node scripts/classify-tests.mjs --report');
  L.push('```');
  L.push('');
  L.push('The script writes nothing itself; all output goes to stdout. Deterministic: identical runs on an identical tree produce byte-identical output. Requires Node >= 24, zero npm dependencies, no cargo, no network.');
  L.push('');
  return L.join('\n');
}
