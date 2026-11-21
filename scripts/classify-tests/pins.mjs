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

import { statSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const MAX_SURFACE_FILE_BYTES = 4 * 1024 * 1024;

/** Read a file's text, or null if unreadable/oversized (capped docs scan). */
function readCapped(abs) {
  try {
    if (statSync(abs).size > MAX_SURFACE_FILE_BYTES) return null;
  } catch { return null; }
  try { return readFileSync(abs, 'utf8'); } catch { return null; }
}

/** Collect pin surfaces from Git-enumerated repo-relative POSIX paths, sorted by file. */
export function collectPinSurfaces(rootDir, files) {
  const out = [];
  for (const file of files) {
    let kind;
    if (/^\.github\/workflows\/[^/]*\.yml$/.test(file)) kind = 'ci_workflow';
    else if (/^\.github\/actions\/[^/]+\/action\.yml$/.test(file)) kind = 'ci_action';
    else if (/^scripts\/[^/]*\.mjs$/.test(file)) kind = 'script';
    else if (/^docs\/verification\/[^/]*\.json$/.test(file)) kind = 'doc_json';
    else if (file === 'README.md' || file === 'ARCHITECTURE.md') kind = 'doc';
    else if (file.startsWith('docs/') && file.endsWith('.md')) {
      if (file.slice(file.lastIndexOf('/') + 1).includes('_raw_')) continue;
      // Skip the generated report to avoid pinning targets named in our own output.
      if (file.startsWith('docs/perf/TEST_TARGET_CLASSIFICATION_')) continue;
      kind = 'doc';
    } else continue;

    const text = readCapped(join(rootDir, file));
    if (text !== null) out.push({ file, kind, text });
  }
  out.sort((a, b) => a.file.localeCompare(b.file) || a.kind.localeCompare(b.kind));
  return out;
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
