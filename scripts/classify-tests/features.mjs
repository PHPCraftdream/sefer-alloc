// Per-file classification of tests/*.rs sources.
//
// Purpose: derive, for each test target's Rust source, the merge-blocking
// facts the 2026-11-15 plan (§1) needs:
//   - file-level `#![cfg(...)]` / `#![cfg_attr(...)]` predicates and the
//     feature names / cfg keys they reference (only attrs BEFORE the first
//     item count as file-level)
//   - #[global_allocator] presence
//   - first-config-wins API usage (against a marker list DERIVED from src/,
//     see deriveFirstConfigWinMarkers)
//   - process-spawn usage, abort markers, should_panic, catch_unwind,
//     `fn main(` shape, line count
//
// Comments are stripped before matching so markers never fire inside a
// comment. TWO text zones (2026-11-15 bug 3 fix):
//   (a) commentsStripped = stripCommentsKeepStrings — comments removed,
//       string literals PRESERVED verbatim; used for file-level inner-attr
//       extraction + cfg facts/predicates so `feature = "alloc-core"` keeps
//       its name (string contents were previously erased, corrupting group
//       keys and cfg_features).
//   (b) fullyStripped = stripComments — comments AND string-literal contents
//       removed; used for marker matching (global_allocator, first-config-
//       wins, process spawn, abort, loom::/miri:: code refs, main_fn) so a
//       string CONTAINING `#[global_allocator]` or `thread::spawn` cannot
//       fire a marker.
// Known caveat (unchanged): raw strings r#"..."# are treated as code by the
// stripper (documented limitation; hash-raw strings are not matched here).
// Pure functions over (path, contents) so tests can point them at synthetic
// trees.

import { readdirSync } from 'node:fs';
import { join } from 'node:path';

// Remove block comments (slash-star ... star-slash) and // comments
// (string-literal aware).
function stripCommentsImpl(src, keepStrings) {
  let out = '';
  let i = 0;
  let state = 'code'; // code | line | block | str | chr
  let blockDepth = 0;
  while (i < src.length) {
    const ch = src[i];
    const next = src[i + 1];
    if (state === 'code') {
      if (ch === '/' && next === '/') { state = 'line'; i += 2; continue; }
      if (ch === '/' && next === '*') { state = 'block'; blockDepth = 1; i += 2; continue; }
      if (ch === '"' || ch === 'r') {
        // crude but sufficient: track normal strings; raw strings handled by
        // falling through as code (hash-raw strings are not matched here).
        if (ch === '"') { state = 'str'; out += ch; i++; continue; }
      }
      if (ch === "'") { state = 'chr'; out += ch; i++; continue; }
      out += ch; i++; continue;
    }
    if (state === 'line') {
      if (ch === '\n') { state = 'code'; out += ch; }
      i++; continue;
    }
    if (state === 'block') {
      if (ch === '/' && next === '*') { blockDepth++; i += 2; continue; }
      if (ch === '*' && next === '/') { blockDepth--; i += 2; if (blockDepth === 0) state = 'code'; continue; }
      if (ch === '\n') out += ch;
      i++; continue;
    }
    if (state === 'str') {
      if (ch === '\\') { if (keepStrings) { out += src.slice(i, i + 2); } i += 2; continue; }
      if (ch === '"') { state = 'code'; out += keepStrings ? '"' : '""'; i++; continue; }
      if (keepStrings) out += ch;
      i++; continue;
    }
    // char literal
    if (ch === '\\') { if (keepStrings) { out += src.slice(i, i + 2); } i += 2; continue; }
    if (ch === "'") { state = 'code'; out += "''"; i++; continue; }
    if (keepStrings) out += ch;
    i++;
  }
  return out;
}

/**
 * Comments removed; string-literal contents PRESERVED verbatim (zone a).
 * Doc comments (`//!`, `///`) are stripped like any line comment.
 */
export function stripCommentsKeepStrings(src) {
  return stripCommentsImpl(src, true);
}

/**
 * Comments AND string-literal contents removed (zone b). Kept exported for
 * compatibility with existing consumers/tests.
 */
export function stripComments(src) {
  return stripCommentsImpl(src, false);
}

/**
 * Inner attributes (`#![...]`) that appear BEFORE the first non-attribute
 * item. Comment/blank lines are skipped; multi-line attrs are joined while
 * parentheses stay unbalanced. Returns array of { raw, kind, body } where
 * body is the text inside the outermost parens and kind is 'cfg'|'cfg_attr'.
 */
export function fileLevelInnerAttrs(stripped) {
  const attrs = [];
  const lines = stripped.split(/\r?\n/);
  let collecting = null; // { buf } while parens unbalanced
  let pastHeader = false;
  for (const line0 of lines) {
    const line = line0.trim();
    if (collecting) {
      collecting.buf += ' ' + line;
      if (balanced(collecting.buf)) {
        attrs.push(finishAttr(collecting.buf));
        collecting = null;
      }
      continue;
    }
    if (line === '') continue;
    if (line.startsWith('#![cfg(') || line.startsWith('#![cfg_attr(')) {
      if (balanced(line)) attrs.push(finishAttr(line));
      else collecting = { buf: line };
      continue;
    }
    if (line.startsWith('#![') || line.startsWith('#[')) continue; // other attrs don't end the header
    pastHeader = true;
    break;
  }
  if (collecting && !pastHeader) attrs.push(finishAttr(collecting.buf));
  return attrs;

  function balanced(t) {
    let d = 0;
    for (const c of t) { if (c === '(') d++; else if (c === ')') d--; }
    return d <= 0;
  }
  function finishAttr(t) {
    const kind = t.startsWith('#![cfg_attr') ? 'cfg_attr' : 'cfg';
    const open = t.indexOf('(');
    const body = t.slice(open + 1, t.lastIndexOf(')')).trim();
    return { raw: t, kind, body };
  }
}

const CFG_KEY_WORDS = new Set(['miri', 'loom', 'kani', 'windows', 'unix', 'test',
  'target_os', 'target_arch', 'target_pointer_width', 'target_endian',
  'target_family', 'debug_assertions', 'panic', 'proc_macro', 'feature',
  'not', 'all', 'any', 'version', 'docsrs', 'asan', 'sefer_implant']);

/** Extract referenced feature names and cfg keys from predicate bodies. */
export function cfgFacts(bodies) {
  const features = new Set();
  const keys = new Set();
  const joined = bodies.join('\n');
  for (const m of joined.matchAll(/feature\s*=\s*"([^"]+)"/g)) features.add(m[1]);
  for (const m of joined.matchAll(/([A-Za-z_][A-Za-z0-9_]*)/g)) {
    const word = m[1];
    if (CFG_KEY_WORDS.has(word)) keys.add(word);
    else if (/^(target_|tin)'/.test(word)) keys.add(word);
  }
  return { features: [...features].sort(), keys: [...keys].sort() };
}

/**
 * Derive the first-config-wins marker list from src/: every candidate that
 * actually occurs in src/ is kept with its first src/ hit (file:line).
 * Pure over explicit input paths.
 */
export function deriveFirstConfigWinMarkers(srcDir, candidates, grepFirstHit) {
  const markers = [];
  for (const c of candidates) {
    const hit = grepFirstHit(srcDir, c);
    if (hit) markers.push({ marker: c, src_hit: hit });
  }
  return markers;
}

/** Default candidate list — validated against src/ before use.
 * Full first-config-wins API family, derived by grepping src/ (2026-11-15):
 * with_config            src/global/sefer_alloc/core.rs:277
 * claim_with_config      (kept as candidate; dropped if absent from src/)
 * claim_lease_with_config src/registry/heap_registry (see derive output)
 * current_for_alloc_with_config / bind_slow_tagged_with_config
 *                        src/global/tls_heap.rs:411 / :491
 * new_with_config        src/registry/heap_core/core.rs:316
 * heap_ptr_with_config   src/global/fallback.rs:188
 * config_conflicts_total src/registry/heap_registry/counters.rs:99
 * fallback_config_conflicts src/global/sefer_alloc/core.rs:372
 * thp_tuple_conflicts    src/global/alloc_stats.rs:193 (field)
 * thp_tuple / thp_tuple_cell / thp_runtime
 *                        src/alloc_core/config/policy/large_thp_runtime/
 * config_conflicts       src/global/alloc_stats.rs:183 (AllocStats field)
 *
 * DROPPED marker: `publish_pending`
 *   src/registry/segment_route/large_state.rs:25 defines the Large-reservation
 *   route sidecar publication CAS (LIVE(g)->PENDING(g)). It is NOT a
 *   first-config-wins API, so files calling it need not be unmergeable for
 *   config reasons. Removed from candidates and from MARKER_PROVENANCE.
 * Auto-dropped by deriveFirstConfigWinMarkers (no src/ hit): claim_with_config.
 */
export const FIRST_CONFIG_WIN_CANDIDATES = [
  'with_config',
  'live_config_matches',
  'claim_with_config',
  'claim_lease_with_config',
  'current_for_alloc_with_config',
  'bind_slow_tagged_with_config',
  'new_with_config',
  'heap_ptr_with_config',
  'config_conflicts_total',
  'config_conflicts',
  'fallback_config_conflicts',
  'thp_tuple_conflicts',
  'thp_tuple',
  'thp_runtime',
  'thp_tuple_cell',
];

/**
 * Provenance (definition-site) notes for each candidate, verified by grep
 * against src/ (2026-11-15). Keyed by marker; a candidate absent here but
 * present in src/ still derives (src_hit comes from the live grep).
 */
export const MARKER_PROVENANCE = {
  with_config: 'src/global/sefer_alloc/core.rs:277',
  live_config_matches: 'src/alloc_core/alloc_core/lifecycle.rs:254 + src/registry/heap_core/core.rs:388',
  claim_with_config: '(no src/ hit; auto-dropped by derive)',
  claim_lease_with_config: 'src/registry/heap_core/core.rs (derive output)',
  current_for_alloc_with_config: 'src/global/tls_heap.rs:411',
  bind_slow_tagged_with_config: 'src/global/tls_heap.rs:491',
  new_with_config: 'src/registry/heap_core/core.rs:316 (+ claim.rs)',
  heap_ptr_with_config: 'src/global/fallback.rs:188',
  config_conflicts_total: 'src/registry/heap_registry/counters.rs:99',
  config_conflicts: 'src/global/alloc_stats.rs:183 (AllocStats field)',
  fallback_config_conflicts: 'src/global/sefer_alloc/core.rs:372',
  thp_tuple_conflicts: 'src/global/sefer_alloc/diag.rs (alloc_stats.rs:193 field family)',
  thp_tuple: 'src/alloc_core/config/policy/large_thp_runtime/thp_tuple.rs',
  thp_runtime: 'src/alloc_core/config/policy/large_thp_runtime/mod.rs',
  thp_tuple_cell: 'src/alloc_core/config/policy/large_thp_runtime/thp_tuple_cell.rs',
};

export function markerRegex(marker) {
  return new RegExp(`\\b${marker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`);
}

/**
 * Classify one Rust test source. `markerList` is the derived list of marker
 * strings (see deriveFirstConfigWinMarkers). Pure over contents.
 */
export function classifySource(contents, markerList) {
  // Zone a: comments stripped, strings kept — attrs + cfg facts/predicates.
  const commentsStripped = stripCommentsKeepStrings(contents);
  // Zone b: comments AND string contents stripped — marker matching only.
  const stripped = stripComments(contents);
  const attrs = fileLevelInnerAttrs(commentsStripped);
  const cfgBodies = attrs.map((a) => a.body);
  const { features, keys } = cfgFacts(cfgBodies);
  const allCfgText = attrs.map((a) => a.raw).join('\n');

  const global_allocator = /#\[global_allocator\]/.test(stripped);
  const first_config_wins = markerList
    .filter((m) => markerRegex(m.marker).test(stripped))
    .map((m) => m.marker);
  // Process-spawn signals ONLY: bare `spawn(` would false-positive on
  // thread::spawn / tokio::spawn (2026-11-15 fix; baseline <= 32 files).
  const process_spawn = /\bstd::process::Command\b|\bprocess::Command\b|\bCommand::new\b|\bcurrent_exe\b|\bCARGO_BIN_EXE_/.test(stripped);
  const abort_marker = /\bstd::process::abort\b|\bprocess::abort\b|\blibc::abort\b/.test(stripped);
  const should_panic = /#\[should_panic/.test(stripped);
  const catch_unwind = /\bcatch_unwind\b/.test(stripped);
  const main_fn = /\bfn\s+main\s*\(/.test(stripped);
  // Strict loom_only: loom-gated by file-level cfg OR uses loom:: APIs.
  // loom_mentions is informational only (any bare `loom` word in code).
  const loom_only = /\bloom\b/.test(allCfgText) || /\bloom::/.test(stripped);
  const loom_mentions = /\bloom\b/.test(stripped);
  const miri_only = /\bmiri\b/.test(allCfgText);
  const kani_gated = /\bkani\b/.test(allCfgText);

  return {
    file_level_cfg: attrs.filter((a) => a.kind === 'cfg').map((a) => a.body).join(' && ') || null,
    file_level_cfg_attr: attrs.filter((a) => a.kind === 'cfg_attr').map((a) => a.body).join(' && ') || null,
    cfg_features: features,
    cfg_keys: keys,
    global_allocator,
    first_config_wins,
    process_spawn,
    abort_marker,
    should_panic,
    catch_unwind,
    loom_only,
    loom_mentions,
    miri_only,
    kani_gated,
    main_fn,
    lines: contents.split(/\r?\n/).length,
  };
}

/**
 * Which zone of the raw source does the FIRST match of `re` fall in?
 * Returns 'code' | 'comment' | 'string-literal' | null (no match). Used to
 * explain naive-grep-baseline differences in the reconciliation table.
 */
export function rawMatchZone(raw, re) {
  const m = raw.match(re);
  if (!m) return null;
  const idx = m.index;
  let i = 0;
  let state = 'code';
  let blockDepth = 0;
  while (i <= idx && i < raw.length) {
    const ch = raw[i];
    const next = raw[i + 1];
    if (state === 'code') {
      if (ch === '/' && next === '/') { state = 'line'; i += 2; continue; }
      if (ch === '/' && next === '*') { state = 'block'; blockDepth = 1; i += 2; continue; }
      if (ch === '"') { state = 'str'; i++; continue; }
      if (ch === "'") { state = 'chr'; i++; continue; }
      i++; continue;
    }
    if (state === 'line') {
      if (ch === '\n') { state = 'code'; i++; continue; }
      i++; continue;
    }
    if (state === 'block') {
      if (ch === '/' && next === '*') { blockDepth++; i += 2; continue; }
      if (ch === '*' && next === '/') { blockDepth--; i += 2; if (blockDepth === 0) state = 'code'; continue; }
      i++; continue;
    }
    if (state === 'str') {
      if (ch === '\\') { i += 2; continue; }
      if (ch === '"') { state = 'code'; i++; continue; }
      i++; continue;
    }
    // chr
    if (ch === '\\') { i += 2; continue; }
    if (ch === "'") { state = 'code'; i++; continue; }
    i++;
  }
  return state === 'line' || state === 'block' ? 'comment'
    : state === 'str' ? 'string-literal' : 'code';
}

// Enumerate all Rust files under srcDir (sorted, relative to srcDir).
export function listSrcRustFiles(srcDir) {
  const out = [];
  const walk = (dir, rel) => {
    for (const ent of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const childRel = rel === '' ? ent.name : `${rel}/${ent.name}`;
      if (ent.isDirectory()) walk(join(dir, ent.name), childRel);
      else if (ent.isFile() && ent.name.endsWith('.rs')) out.push(childRel);
    }
  };
  walk(srcDir, '');
  return out;
}
