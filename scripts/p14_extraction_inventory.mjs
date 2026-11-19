#!/usr/bin/env node
// P1.4 extraction inventory (cargo-free, derived data only — not typed, not
// part of any typed contract). Scans the P1.4 candidate files for the
// `sefer-platform` extraction (docs/plans/2026-11-15-workspace-restructure-plan.md
// step P1.4) and prints:
//   1. file -> decision (extract / stay + reason) -> external `crate::` deps
//      that point OUTSIDE the extracted set.
//   2. per-file symbol inventory with line numbers and attributes
//      (cfg / inline / cold / allow(unsafe_code)).
//   3. reverse edges: root files referencing symbols that would move.
//   4. unsafe / ghost / concurrent usage per file.
//   5. tests/ files referencing the extracted surface, with
//      docs/test_target_classification_20261115.csv cross-check.
// Asserts pin the key relationships the P1.4 decision rests on.
//
// Usage: node scripts/p14_extraction_inventory.mjs   (exit 1 on any assert)
import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dirname, '..');
// P2.2 moved `src/alloc_core/**` wholesale into `crates/core/sefer-alloc-core`, and derived data
// (classification CSV, layers.json, ghost-rules.json) has since moved on. This P1.4 inventory is the
// recorded P1.4 acceptance derivation, so ALL of its repo reads are pinned to the P1.4 acceptance commit
// — the script stays reproducible instead of silently breaking on later moves.
const P1_4_SHA = 'b8d86949';
const read = (p) =>
  execFileSync('git', ['show', `${P1_4_SHA}:${p}`], { cwd: ROOT, encoding: 'utf8', maxBuffer: 1 << 26 }).replace(/\r\n/g, '\n');
const lines = (p) => read(p).split(/\r?\n/);

// ── derived decisions (documented reasons from the P1.4 code read) ──
const FILES = [
  { path: 'src/alloc_core/platform/fatal.rs', decision: 'EXTRACT', reason: 'leaf: zero crate:: deps; #[cold] terminal handler; whole file moves' },
  { path: 'src/alloc_core/platform/os.rs', decision: 'SPLIT', reason: 'backup half (Segment/SEGMENT/PAGE/counters/release/decommit/depot) moves; directory-sidecar trio stays: depends on alloc_core::segment_directory::SegmentDirectory + alloc_core::sidecar::* + segment_header::NO_NODE_RAW' },
  { path: 'src/alloc_core/platform/node.rs', decision: 'STAY', reason: 'root-blockers: continuation_offset (hardened) reads alloc_core::os::SEGMENT, continuation_offset_or_fatal + continuation_fatal read alloc_core::config::continuation_bounds (P1.3 not landed); rest is raw-pointer allocator-metadata membrane whose pub-widening is forbidden by the plan visibility rule; extractable remnant is too small' },
  { path: 'src/alloc_core/platform/numa.rs', decision: 'STAY', reason: 'NOT a leaf w.r.t. the extracted set: depends on alloc_core::fatal, alloc_core::os::{SEGMENT, SEGMENTS_*_TOTAL, release_segment} and alloc_core::node::Node (script output section 1). Joining the extracted crate would drag node.rs/os.rs residue with it; defer to the owner decision on whether numa.rs joins sefer-platform together with the full os split' },
  { path: 'src/alloc_core/platform/sidecar.rs', decision: 'STAY', reason: 'generic primitive, but consumed by os.rs residue + large_cache_extended; deps segment-adjacent (SegmentDirectory layout fixups at call sites); not clearly platform-only' },
  { path: 'src/alloc_core/platform/sidecar_stats.rs', decision: 'STAY', reason: 'leaf counters over aligned-vmem, but they are the acceptance oracle for sidecar.rs (which stays) and feed AllocStats/dbg hooks in root' },
  { path: 'src/alloc_core/platform/size_classes.rs', decision: 'STAY', reason: 'thin shim over the size-classes crate, but Params::huge_threshold = alloc_core::os::SEGMENT — a policy coupling into os; not leaf-w.r.t. platform' },
  { path: 'src/global/process_exit.rs', decision: 'EXTRACT', reason: 'zero crate:: deps (no ghost bridge needed even though P1.1 is not landed); FFI + one atomic; consumed via global::mod re-export' },
];

// symbols that move out of os.rs (the extracted half)
// os.rs residue that stays in the root (SegmentDirectory/sidecar deps)
const OS_STAY = ['reserve_directory_sidecar', 'read_directory_class_words', 'read_directory_node_bucket'];
// os.rs items that move too (they only reference moved symbols + aligned_vmem)
const OS_MOVE_CONSTRUCTORS = ['reserve_biased', 'reserve', 'reserve_exact', 'reserve_capacity_exact', 'reserve_lazy', 'reserve_small_lazy', 'reserve_lazy_for_measurement', 'as_ptr', 'reservation', 'reservation_len', 'advise_huge_pages', 'try_disable_transparent_huge_pages', 'try_advise_huge_pages_range'];
const OS_MOVE = ['SEGMENTS_RESERVED_TOTAL', 'SEGMENTS_RELEASED_TOTAL', 'SEGMENTS_RESERVE_FAILED_TOTAL', 'SEGMENT', 'PAGE', 'MAX_REALISTIC_PAGE_SIZE', 'segment_base_of', 'segment_base_of_ptr', 'Segment', 'release_segment', 'try_release_segment', 'segments_reserved_total', 'segments_released_total', 'decommit_pages', 'commit_pages', 'commit_pages_for_measurement', 'recommit_pages', 'reserve_depot_overflow', 'commit_depot_overflow', 'decommit_depot_overflow'].concat(OS_MOVE_CONSTRUCTORS);

// ── scanners ──
const symRe = /^\s*(?:#\[[^\]]*\]\s*)*pub(?:\((?:crate|super)\))? (?:const|static|fn|unsafe fn|struct|enum|type|impl)?/;

function inventory(path) {
  const ls = lines(path);
  const syms = [];
  ls.forEach((l, i) => {
    const m = /(?:const|static|fn|struct|type)\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(l);
    const isPub = /pub(\((crate|super|in [^)]+)\))? (const|static|unsafe )?fn|pub (const|static|struct|fn|unsafe fn)/.test(l);
    if (!isPub || !m) return;
    // walk backwards for attributes on preceding lines
    const attrs = [];
    for (let j = i - 1; j >= 0 && /^\s*#\[|^\s*#\[/ .test(ls[j]); j--) attrs.unshift(ls[j].trim());
    let j = i - 1;
    while (j >= 0 && (ls[j].trim().startsWith('#[') || ls[j].trim().startsWith('#!['))) { attrs.unshift(ls[j].trim()); j--; }
    syms.push({ name: m[1], line: i + 1, attrs });
  });
  return syms;
}

function crateDeps(path) {
  const ls = lines(path);
  const out = new Map(); // path -> [line]
  ls.forEach((l, i) => {
    if (l.trim().startsWith('//')) return; // skip pure comment lines
    for (const m of l.matchAll(/crate::([A-Za-z0-9_:]+)/g)) {
      const p = m[1].split('::').slice(0, 3).join('::');
      if (!out.has(p)) out.set(p, []);
      out.get(p).push(i + 1);
    }
  });
  return out;
}

const EXTRACTED_MODULES = new Set(['alloc_core::platform::fatal', 'global::process_exit', 'alloc_core::platform::os']);
const isSelfDep = (file, dep) =>
  (file.includes('platform') && dep.startsWith('alloc_core::platform')) ||
  (file.startsWith('src/global/process_exit') && dep.startsWith('global::process_exit'));

console.log('== 1. decision table ==');
for (const f of FILES) {
  const deps = crateDeps(f.path);
  const ext = [...deps].filter(([d]) => !isSelfDep(f.path, d));
  console.log(`${f.path}`);
  console.log(`  decision: ${f.decision} — ${f.reason}`);
  console.log(`  external crate:: deps: ${ext.length ? ext.map(([d, ls]) => `${d} (lines ${ls.join(',')})`).join('; ') : 'NONE (leaf)'}`);
}

console.log('\n== 2. symbol inventory (pub items with attributes) ==');
for (const f of FILES) {
  console.log(`-- ${f.path}`);
  for (const s of inventory(f.path)) {
    const notable = s.attrs.filter((a) => /cfg|inline|cold|allow|must_use|doc\(hidden\)/.test(a));
    const bucket = OS_STAY.includes(s.name) ? 'STAY(os residue)' : OS_MOVE.includes(s.name) ? 'MOVE' : '';
    console.log(`  ${s.line}  ${s.name} ${bucket ? '[' + bucket + ']' : ''} ${notable.join(' ')}`);
  }
}

console.log('\n== 3. in-file cross-symbol dependencies (os.rs residue -> moved / moved -> residue) ==');
const os = lines('src/alloc_core/platform/os.rs');
os.forEach((l, i) => {
  for (const st of OS_STAY) if (new RegExp(`\\b${st}\\b`).test(l)) {
    for (const mv of OS_MOVE) if (new RegExp(`\\b${mv}\\b`).test(l)) console.log(`  os.rs:${i + 1} STAY(${st}) references MOVE(${mv})`);
  }
});

console.log('\n== 4. unsafe / ghost / concurrent usage ==');
for (const f of FILES) {
  const t = read(f.path);
  console.log(`  ${f.path}: allow(unsafe_code)=${t.includes('#![allow(unsafe_code)]')}, unsafe blocks=${(t.match(/unsafe \{/g) || []).length}+fn, ghost=${/ghost/.test(t) ? /ghost_assert|crate::ghost/.test(t) ? 'CODE' : 'doc-only' : 'none'}, concurrent=${t.includes('crate::concurrent') ? 'code' : t.includes('concurrent') ? 'doc/comment-only' : 'none'}`);
}

console.log('\n== 5. tests/ referencing extracted surface ==');
const { execSync } = await import('node:child_process');
const cls = read('docs/test_target_classification_20261115.csv');
const testFiles = execSync(`git ls-tree -r --name-only ${P1_4_SHA} tests/`, { cwd: ROOT, encoding: 'utf8' }).split(/\r?\n/).filter((p) => p.endsWith('.rs'));
for (const tf of testFiles) {
  const t = read(tf);
  const hits = [];
  if (/alloc_core::os|platform::os|release_segment|decommit_pages|SEGMENTS_/.test(t)) hits.push('os');
  if (/process_exit/.test(t)) hits.push('process_exit');
  if (/platform::fatal|alloc_core::fatal/.test(t)) hits.push('fatal');
  if (hits.length) {
    const row = cls.split(/\r?\n/).find((l) => l.startsWith(`${tf},`) || l.startsWith(`${tf.replace(/\//g, '\\')},`));
    const ga = /#\[global_allocator\]/.test(t);
    const fcw = /with_config|config_conflicts|THP|first-config/.test(t);
    const pathInclude = /#\[path\s*=/.test(t);
    console.log(`  ${tf}: [${hits}] global_allocator=${ga} first-config-wins=${fcw} #[path]-includes-source=${pathInclude} classified=${row ? 'yes' : 'NO-ROW'}`);
  }
}

// ── asserts on key relationships ──
const assert = (await import('node:assert/strict')).default;
// fatal.rs is a leaf: no crate:: references at all.
assert(crateDeps('src/alloc_core/platform/fatal.rs').size === 0, 'fatal.rs must be a leaf');
// process_exit.rs has no crate:: deps (no ghost bridge needed).
assert(crateDeps('src/global/process_exit.rs').size === 0, 'process_exit.rs must have zero crate:: deps');
// os.rs residue deps exist and are exactly the segment-directory trio.
const osDepMods = new Set([...crateDeps('src/alloc_core/platform/os.rs').keys()].filter((d) => !d.startsWith('alloc_core::platform')).map((d) => d.split('::').slice(0, 2).join('::')));
assert(deepEq([...osDepMods], ['alloc_core::segment_directory', 'alloc_core::sidecar', 'alloc_core::segment_header']), 'os.rs root deps must be exactly segment_directory + sidecar + segment_header, got ' + [...osDepMods]);
// node.rs root blockers confirmed.
const nodeDeps = [...crateDeps('src/alloc_core/platform/node.rs').keys()].filter((d) => !d.startsWith('alloc_core::platform'));
assert(nodeDeps.some((d) => d.startsWith('alloc_core::config')) && nodeDeps.some((d) => d.startsWith('alloc_core::os')), 'node.rs must depend on alloc_core::config (continuation_bounds) and alloc_core::os (SEGMENT), got ' + nodeDeps);
// layers.json declares the platform boundary and the process_exit back-edge claim.
const layers = JSON.parse(read('scripts/module-graph/layers.json'));
assert(layers.boundaries.some((b) => b.crate === 'sefer-platform' && b.prefixes[0] === 'alloc_core.platform'), 'layers.json must declare sefer-platform boundary');
assert(layers.expected_back_edges.some((e) => e.id === 'claim-2-segment-route-to-global-exit'), 'claim-2 (segment_route -> global exit state) must stay until P2.0b relocates it');
// ghost-rules cache/process-exit pins the file path that would move.
const grules = JSON.parse(read('docs/verification/ghost-rules.json'));
const gr = JSON.stringify(grules);
// Accepts the pre-move path or the post-move crate path (the script runs on both trees).
assert(gr.includes('src/global/process_exit.rs') || gr.includes('crates/platform/sefer-platform/src/process_exit.rs'), 'ghost-rules.json must reference process_exit.rs (src/global/ before the move, crates/platform/sefer-platform/src/ after) (cache/process-exit rule)');
// con01 test #[path]-includes the source file — moving breaks it.
const con01 = read('tests/con01_exit_detector_probe.rs');
assert(con01.includes('#[path = "../src/global/process_exit.rs"]') || con01.includes('#[path = "../crates/platform/sefer-platform/src/process_exit.rs"]'), 'con01 test path-includes process_exit.rs (old or crate path)');
// structure limits baseline pins the platform folder row.
assert(read('docs/structure_limits_baseline.csv').includes('src/alloc_core/platform'), 'structure_limits_baseline.csv pins src/alloc_core/platform');
console.log('\nselftest: all asserts passed');
function deepEq(a, b) { return a.length === b.length && [...a].sort().join('|') === [...b].sort().join('|'); }
// existsSync silences unused-import lint
if (!existsSync(join(ROOT, 'Cargo.toml'))) process.exit(1);
