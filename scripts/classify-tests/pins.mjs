// External pin-surface scanning for the Node test classification script.
//
// Purpose: determine, without cargo or network, which test targets are pinned
// by surfaces OUTSIDE tests/ — i.e. places where merging/renaming a target
// would break something else:
//   - ci_workflow  : .github/workflows/*.yml   (target names, e.g. `--test <n>`)
//   - ci_action    : .github/actions/*/action.yml
//   - script       : scripts/*.mjs
//   - doc          : README.md, ARCHITECTURE.md, docs/**/*.md (excluding
//                    docs/perf/_raw_*.log-class noise and files > 4 MiB, so the
//                    88 MiB docs/perf corpus does not dominate the scan; no pin
//                    class from the surface list is skipped)
//                    ALSO excluded: the classification suite's own output
//                    artifacts `docs/perf/TEST_TARGET_CLASSIFICATION_*.md` and
//                    `docs/test_target_classification_*.csv`. Reason: self-
//                    reference — the report lists group/pilot member names, so
//                    scanning its own output would pin those targets and make
//                    the classification non-idempotent (generating the report
//                    would change the next report). The report's echo is not a
//                    pre-existing pin; P0.4's pin-update list is the report
//                    itself anyway.
//   - doc_json     : docs/verification/*.json — the curated ghost-rule
//                    catalog (CI-checked; each rule cites its covering loom/
//                    miri/kani test by name). Only docs/verification/ is
//                    scanned for .json (flat glob); perf-run JSONs elsewhere
//                    under docs/ are scratch artifacts, not pin surfaces.
//                    Same 4 MiB cap as doc.
// A target is externally_pinned iff its NAME occurs (word-boundary) in any of
// those surfaces. Repo-level published COUNT claims (e.g. README "562
// integration test files", docs/CI_TIERS.md "369 of 484") are collected
// separately as publishedCounts — these are what ANY merge of targets
// invalidates regardless of individual names.
//
// j13 hash pins: scripts/j13*.mjs hash src/ files (createHash/sha256).
// HASH-PINNED FILES are derived ONLY from hash-table shapes — a path mapped
// directly to a 64-hex digest (`<path>: '<64hex>'`, optionally quoted or
// `[CONST]:` where `const CONST = '<path>'` is bound in the same script —
// this covers j13b_static_reference.mjs's `[FRESH]: 'aae5...'` /
// `[RUNTIME]: 'ebe3...'` entries). Prose mentions of paths (e.g. a GO/NO-GO
// sentence inside a markdown template in a hashing script) are NOT pins
// (2026-11-15 bug 4 fix).
//
// Derives: { perTarget, publishedCounts, j13HashPins (hashing scripts),
//            hashPinnedFiles: [{path, script, hex}],
//            testsFilesHashPinned (paths under tests/ — expected EMPTY),
//            srcOrOtherFilesHashPinned (everything else), j13Scripts,
//            surfaceFiles }.
// Asserts: deterministic ordering everywhere (surfaces sorted by file,
// hits sorted by file/kind, names sorted).
//
// All functions are pure over explicit inputs (paths + text), so tests can
// feed synthetic surfaces.

import { readdirSync, statSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const MAX_SURFACE_FILE_BYTES = 4 * 1024 * 1024;

/** Read a file's text, or null if unreadable/oversized (capped docs scan). */
function readCapped(abs) {
  try {
    if (statSync(abs).size > MAX_SURFACE_FILE_BYTES) return null;
  } catch { return null; }
  try { return readFileSync(abs, 'utf8'); } catch { return null; }
}

/** Enumerate pin surfaces under rootDir. Returns [{file, kind, text}] sorted by file. */
export function collectPinSurfaces(rootDir) {
  const out = [];
  const push = (absRel, kind) => {
    const abs = join(rootDir, absRel);
    const text = readCapped(abs);
    if (text !== null) out.push({ file: absRel, kind, text });
  };
  for (const wf of listDir(join(rootDir, '.github', 'workflows'), '.yml')) {
    push(`.github/workflows/${wf}`, 'ci_workflow');
  }
  for (const act of listDir(join(rootDir, '.github', 'actions'), null, true)) {
    const f = join(rootDir, '.github', 'actions', act, 'action.yml');
    const text = readCapped(f);
    if (text !== null) out.push({ file: `.github/actions/${act}/action.yml`, kind: 'ci_action', text });
  }
  for (const s of listDir(join(rootDir, 'scripts'), '.mjs')) {
    push(`scripts/${s}`, 'script');
  }
  push('README.md', 'doc');
  push('ARCHITECTURE.md', 'doc');
  walkDocs(join(rootDir, 'docs'), 'docs', out);
  for (const j of listDir(join(rootDir, 'docs', 'verification'), '.json')) {
    push(`docs/verification/${j}`, 'doc_json');
  }
  out.sort((a, b) => a.file.localeCompare(b.file) || a.kind.localeCompare(b.kind));
  return out;
}

function listDir(dir, ext, dirsOnly = false) {
  let ents;
  try { ents = readdirSync(dir, { withFileTypes: true }); } catch { return []; }
  return ents.filter((e) => {
    if (dirsOnly) return e.isDirectory();
    return e.isFile() && (!ext || e.name.endsWith(ext));
  }).map((e) => e.name).sort((a, b) => a.localeCompare(b));
}

function walkDocs(dir, rel, out) {
  let ents;
  try { ents = readdirSync(dir, { withFileTypes: true }); } catch { return; }
  for (const e of ents.sort((a, b) => a.name.localeCompare(b.name))) {
    const childRel = `${rel}/${e.name}`;
    if (e.isDirectory()) walkDocs(join(dir, e.name), childRel, out);
    else if (e.isFile() && e.name.endsWith('.md')) {
      // Skip raw perf logs / noise by class (they are .log, not .md, anyway —
      // kept here as an explicit guard so future _raw_*.md never slow the scan).
      if (/_raw_/.test(e.name)) continue;
      // Exclude the classification suite's own outputs (self-reference /
      // idempotence: see header comment). The report lists member names, so
      // scanning its own echo would pin those targets.
      if (childRel.startsWith('docs/perf/TEST_TARGET_CLASSIFICATION_') && childRel.endsWith('.md')) continue;
      if (/^docs\/test_target_classification_.*\.csv$/.test(childRel)) continue;
      const text = readCapped(join(dir, e.name));
      if (text !== null) out.push({ file: childRel, kind: 'doc', text });
    }
  }
}

/** Count occurrences of `needle` with word boundaries in text. */
function countWord(text, name) {
  const re = new RegExp(`\\b${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'g');
  return (text.match(re) || []).length;
}

/**
 * Scan surfaces for target names.
 * surfaces: [{file, kind, text}]; names: sorted string[].
 * Returns perTarget: Map name -> {count, hits:[{file,kind,count}]} (only
 * targets with count>0 present), sorted deterministically.
 */
export function scanPins(surfaces, names) {
  const perTarget = new Map();
  for (const s of surfaces) {
    for (const name of names) {
      const c = countWord(s.text, name);
      if (c === 0) continue;
      if (!perTarget.has(name)) perTarget.set(name, { count: 0, hits: [] });
      const rec = perTarget.get(name);
      rec.count += c;
      rec.hits.push({ file: s.file, kind: s.kind, count: c });
    }
  }
  for (const rec of perTarget.values()) {
    rec.hits.sort((a, b) => a.file.localeCompare(b.file) || a.kind.localeCompare(b.kind));
  }
  return perTarget;
}

/**
 * Collect repo-level published COUNT claims from doc surfaces.
 * Returns [{file, line, text, numbers:[...]}] sorted by file/line.
 * Patterns (deliberately narrow to avoid noise):
 *   - "<N> integration test files"
 *   - "(N files)" / "`tests/*.rs` (N files)" style parenthesised file counts
 *   - "<N> of <M> tests"
 */
export function scanPublishedCounts(surfaces) {
  const out = [];
  const patterns = [
    /(\d{2,})\s+integration\s+test\s+files/gi,
    /(\d{2,})\s+of\s+(\d{2,})\s+tests/gi,
    /\((\d{2,})\s+files\)/gi,
  ];
  for (const s of surfaces) {
    if (s.kind !== 'doc') continue;
    const lines = s.text.split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const numbers = [];
      for (const re of patterns) {
        re.lastIndex = 0;
        let m;
        while ((m = re.exec(line)) !== null) {
          for (let g = 1; g < m.length; g++) if (m[g] !== undefined) numbers.push(Number(m[g]));
        }
      }
      if (numbers.length > 0) {
        out.push({ file: s.file, line: i + 1, text: line.trim(), numbers });
      }
    }
  }
  out.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line);
  return out;
}

/**
 * Inspect scripts/*.mjs surfaces for j13 hash pins.
 * Hash-pinned FILES are derived only from hash-table shapes: a path (or a
 * const-bound path key) immediately mapped to a 64-hex digest. Prose path
 * mentions are never pins.
 * Returns { j13Scripts, j13HashPins, hashPinnedFiles: [{path, script, hex}],
 * testsFilesHashPinned, srcOrOtherFilesHashPinned }.
 */
export function scanJ13HashPins(surfaces) {
  const j13Scripts = [];
  const j13HashPins = [];
  const hashPinnedFiles = [];
  // path directly mapped to a 64-hex digest in a table entry
  const SHAPE_RE = /(['"]?)([A-Za-z0-9_./-]+\.(?:rs|mjs))\1\s*:\s*['"]([0-9a-f]{64})['"]/g;
  // const NAME = '<...path.(rs|mjs)>' bindings (for `[NAME]: 'hex'` entries)
  const BIND_RE = /(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*['"]([A-Za-z0-9_./-]+\.(?:rs|mjs))['"]/g;
  for (const s of surfaces) {
    if (s.kind !== 'script') continue;
    const base = s.file.split('/').pop();
    if (!/^j13/.test(base)) continue;
    j13Scripts.push(s.file);
    const hashes = /createHash\(|\bsha-?256\b|\bblake3\b/i.test(s.text);
    if (hashes) j13HashPins.push(s.file);
    if (!hashes) continue;
    // bindings: name -> path, only for [NAME]: 'hex' table entries
    const bound = new Map();
    for (const m of s.text.matchAll(BIND_RE)) bound.set(m[1], m[2]);
    let re = SHAPE_RE;
    if (bound.size > 0) {
      const names = [...bound.keys()].map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|');
      re = new RegExp(SHAPE_RE.source.replace('[A-Za-z0-9_./-]+\\.(?:rs|mjs)', `(?:[A-Za-z0-9_./-]+\\.(?:rs|mjs)|\\[(?:${names})\\])`), 'g');
    }
    for (const m of s.text.matchAll(re)) {
      let path = m[2];
      if (path.startsWith('[')) {
        path = bound.get(path.slice(1, -1));
        if (!path) continue;
      }
      const entry = { path, script: s.file, hex: m[3] };
      if (!hashPinnedFiles.some((e) => e.path === path && e.script === s.file && e.hex === m[3])) {
        hashPinnedFiles.push(entry);
      }
    }
  }
  hashPinnedFiles.sort((a, b) => a.path.localeCompare(b.path) ||
    a.script.localeCompare(b.script) || a.hex.localeCompare(b.hex));
  const testsFilesHashPinned = hashPinnedFiles.filter((e) => e.path.startsWith('tests/'));
  const srcOrOtherFilesHashPinned = hashPinnedFiles.filter((e) => !e.path.startsWith('tests/'));
  return {
    j13Scripts: j13Scripts.sort((a, b) => a.localeCompare(b)),
    j13HashPins: j13HashPins.sort((a, b) => a.localeCompare(b)),
    hashPinnedFiles,
    testsFilesHashPinned,
    srcOrOtherFilesHashPinned,
  };
}

/**
 * Full pin report for one classification run.
 * Returns { perTarget (Map), publishedCounts, j13 (scanJ13HashPins result),
 * surfaceFiles: [{file, kind}] }.
 */
export function scanAllPins(surfaces, targetNames) {
  const perTarget = scanPins(surfaces, targetNames);
  const publishedCounts = scanPublishedCounts(surfaces);
  const j13 = scanJ13HashPins(surfaces);
  const surfaceFiles = surfaces.map((s) => ({ file: s.file, kind: s.kind }));
  return { perTarget, publishedCounts, j13, surfaceFiles };
}
