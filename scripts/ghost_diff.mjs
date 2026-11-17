// Scope of scripts/ghost_zero_cost.mjs (`--allow-declared-runtime`), pure; its
// self-test lives there. Plan §6 binds the zero-cost proof to ghost and
// verification changes; a range that declares a runtime change by its R30-12
// prefix changes production .text by design. The exception is fail-closed: any
// doubt keeps the proof required.
//
// Skip only when ALL hold over the PR's own commits (merges excluded):
//   1. some subject declares a runtime change: perf(runtime) / perf(opt-in) / fix(perf);
//   2. every subject has a conventional prefix and none is scoped to
//      verification / ghost / loom / miri / kani;
//   3. no changed path is a toolchain or build-graph input (Cargo.toml,
//      Cargo.lock, build.rs, .cargo/, rust-toolchain*) or lies under src/ghost/
//      or crates/primitives/sefer-ghost/src/;
//      the one exception is the root Cargo.toml when every changed line is the
//      `production = [...]` composition line or a comment — a declared
//      perf(runtime) change of the shipping feature set (NL-9) — and none of
//      those lines carries a ghost token (rule 4);
//   4. no changed line of a build-input file (src/, crates/) contains ghost,
//      shadow, sefer_ghost, loom, miri or kani in any case — this also catches a
//      cfg written across lines, since the token sits on its own line. The
//      production helper `StateByte::shadow_reject` (a debug-build abort for a
//      rejected free) is not a ghost token and is removed from the line first.
// A ghost macro called through an alias changes nothing in production: the
// macros compile to nothing outside cfg(any(loom, miri, kani, sefer_ghost)) and
// their definitions are under src/ghost/ and crates/primitives/sefer-ghost/src/ (rule 3).

import { parseBaselineText } from './module-graph/layering.mjs';

const RUNTIME = /^(?:perf\((?:runtime|opt-in)\)|fix\(perf\))!?:/;
const CONVENTIONAL = /^[a-z]+(?:\([^()]*\))?!?: \S/;
const VERIFICATION_SCOPE = /^[a-z]+\((?:[^()]*[,/ ])?(?:verification|ghost[^()]*|loom|miri|kani)(?:[,/ ][^()]*)?\)/;
const BUILD_GRAPH = /^(?:(?:[^/]+\/)*Cargo\.(?:toml|lock)|(?:[^/]+\/)*build\.rs|\.cargo\/.*|rust-toolchain(?:\.toml)?)$/;
const GHOST_TOKEN = /ghost|shadow|\b(?:loom|miri|kani)\b/i;
const SHADOW_REJECT = /\bshadow_reject\b/g;
const hasGhostToken = (l) => GHOST_TOKEN.test(l.replace(SHADOW_REJECT, ''));
const PRODUCTION_LINE = /^production = \[[^\]]*\]\s*$/;
const COMMENT_LINE = /^\s*#/;
const productionCompositionOnly = (lines) => lines.every((l) => (PRODUCTION_LINE.test(l) || COMMENT_LINE.test(l)) && !hasGhostToken(l));
export const isBuildInput = (path) => path.startsWith('src/') || path.startsWith('crates/');

// `changed`: Map path -> array of changed line texts (without the +/- sign);
// `dirty`: `git status --porcelain` lines — any uncommitted change keeps the proof.
export function zeroCostSkip(subjects, changed, dirty = []) {
  if (dirty.length) return null;
  if (!subjects.some((s) => RUNTIME.test(s))) return null;
  for (const s of subjects) {
    if (!CONVENTIONAL.test(s)) return null;
    if (VERIFICATION_SCOPE.test(s)) return null;
  }
  for (const [path, lines] of changed) {
    if (path === 'Cargo.toml' && productionCompositionOnly(lines)) continue;
    if (BUILD_GRAPH.test(path) || path.startsWith('src/ghost/') || path.startsWith('crates/primitives/sefer-ghost/src/')) return null;
    if (isBuildInput(path) && lines.some(hasGhostToken)) return null;
  }
  return 'the range declares a runtime change (perf(runtime) / perf(opt-in) / fix(perf)), has no verification-scoped commit and changes no ghost code or build-graph input';
}

// Changed lines of a `git diff -U0` for ONE path (headers before the first hunk skipped).
export function changedLines(diff) {
  const out = [];
  let inHunk = false;
  for (const line of diff.split('\n')) {
    if (line.startsWith('@@')) { inHunk = true; continue; }
    if (!inHunk) continue;
    if (line.startsWith('+') || line.startsWith('-')) out.push(line.slice(1));
  }
  return out;
}

// The base both the classification and the proof use. The checkout must be
// the head itself (base = the recorded one) or the PR merge ref [p0, head],
// whose first parent is the base branch tip GitHub merged onto; the recorded
// `pull_request.base.sha` is not refreshed when the base branch moves, so it
// must only reach p0 (`reaches`: `git merge-base --is-ancestor recorded p0`,
// false on any error). -> { base } or { why }.
export function resolveBase({ recorded, head, checkout, parents, reaches }) {
  if (checkout === head) return { base: recorded };
  if (parents.length === 2 && parents[1] === head && reaches) return { base: parents[0] };
  return { why: `checkout ${checkout} is neither the head ${head} nor its merge onto a descendant of ${recorded}` };
}

const EXTRACTION_SUBJECT = /^build\(workspace\): extract\s+\S/;
const EXTRACTION_TRAILER = /^Extraction: sefer-[a-z0-9-]+$/m;
const EXTRACTION_FORBIDDEN = /(?:^|\/)(?:build\.rs|\.cargo(?:\/.*)?|rust-toolchain.*)$/;
const EXTRACTION_CRATE_PATH = /^crates\/.+/;

// The extraction exemption judges ghost CODE, not prose. Line comments and
// their tails (`//`, `///`, `//!`) are stripped from a changed line before
// scanning, and only the production ghost-code forms count: the
// `ghost_assert!` / `ghost_event!` macros, a `cfg(sefer_ghost)` or documented
// `cfg(any(loom, miri, kani, sefer_ghost))` gate, a changed path under the
// ghost definitions (`src/ghost/`, `crates/primitives/sefer-ghost/src/`), or
// a non-string changed line (fail closed). Renamed files contribute only
// their rename-aware edit hunks; a whole-file `--no-renames` view of the same
// rename stays stricter.
const GHOST_SOURCE_PATH = /^src\/ghost\/|^crates\/primitives\/sefer-ghost\/src\//;
const RUST_SOURCE_PATH = /\.(?:rs|rs\.tmpl)$/;
const GHOST_CODE_TOKEN = /ghost_assert!|ghost_event!|cfg\s*\(\s*sefer_ghost\s*\)|cfg\s*\(\s*any\s*\(\s*loom\s*,\s*miri\s*,\s*kani\s*,\s*sefer_ghost\s*\)\s*\)/;

// Code view of one changed Rust line: double-quoted string literals are kept,
// everything from an unquoted `//` on is comment prose and dropped.
export function rustCodeLine(line) {
  let code = '';
  for (let index = 0; index < line.length; index++) {
    const char = line[index];
    if (char === '"') {
      code += char;
      index++;
      while (index < line.length) {
        if (line[index] === '\\') {
          code += line[index] + (line[index + 1] ?? '');
          index += 2;
          continue;
        }
        code += line[index];
        if (line[index] === '"') break;
        index++;
      }
      continue;
    }
    if (char === '/' && line[index + 1] === '/') break;
    code += char;
  }
  return code;
}

const ghostCodeLeak = (line) => typeof line !== 'string' || GHOST_CODE_TOKEN.test(rustCodeLine(line));

// `hunksByDestination` maps rename destinations to their rename-aware edit
// hunk lines; each destination must belong to a rename whose source leaves
// `changed`, destinations absent from `changed` are ignored, and malformed
// input returns null (fail closed).
export function extractionChangedLines(changed, renames, hunksByDestination) {
  if (!(changed instanceof Map) || !Array.isArray(renames) || !(hunksByDestination instanceof Map)) return null;
  const extraction = new Map(changed);
  for (const [to, hunks] of hunksByDestination) {
    const rename = renames.find((candidate) => candidate && candidate.to === to);
    if (!rename || typeof rename.from !== 'string' || typeof to !== 'string' || !Array.isArray(hunks)) return null;
    extraction.delete(rename.from);
    if (extraction.has(to)) extraction.set(to, hunks);
  }
  return extraction;
}

// `evidence` is derived from the base..head range by ghost_zero_cost.mjs;
// member/dependency entries are root Cargo.toml additions and renames are
// git's R/C name-status records. Missing or malformed evidence is never an
// extraction exemption.
function withoutTomlComment(line) {
  let quoted = false;
  let escaped = false;
  for (let index = 0; index < line.length; index++) {
    const char = line[index];
    if (char === '"' && !escaped) quoted = !quoted;
    if (char === '#' && !quoted) return line.slice(0, index);
    escaped = char === '\\' && !escaped;
    if (char !== '\\') escaped = false;
  }
  return line;
}

function manifestGraph(manifest) {
  const lines = manifest.split(/\r?\n/).map(withoutTomlComment);
  const members = new Set();
  const pathDeps = new Map();
  let section = '';
  let membersBuffer = null;
  let depName = null;
  let depValue = '';
  const saveDependency = () => {
    if (depName !== null) {
      const path = /\bpath\s*=\s*"([^"]+)"/.exec(depValue)?.[1];
      if (path) pathDeps.set(depName, path);
    }
    depName = null;
    depValue = '';
  };

  for (const line of lines) {
    const header = /^\s*\[([^\]]+)\]\s*(?:#.*)?$/.exec(line);
    if (header) {
      saveDependency();
      section = header[1];
      continue;
    }
    if (section === 'workspace') {
      if (membersBuffer !== null) {
        membersBuffer += ` ${line}`;
        if (line.includes(']')) {
          for (const match of membersBuffer.matchAll(/"([^"]+)"/g)) members.add(match[1]);
          membersBuffer = null;
        }
      } else {
        const memberArray = /^\s*members\s*=\s*\[([^\]]*)\]/.exec(line);
        if (memberArray) {
          for (const match of memberArray[1].matchAll(/"([^"]+)"/g)) members.add(match[1]);
        } else if (/^\s*members\s*=\s*\[/.test(line)) {
          membersBuffer = line;
        }
      }
      continue;
    }
    if (section === 'dependencies' || /^target\..*\.dependencies$/.test(section)) {
      if (depName !== null) {
        depValue += ` ${line.trim()}`;
        if (/[}\]]/.test(line)) saveDependency();
        continue;
      }
      const entry = /^\s*([A-Za-z0-9_-]+)\s*=\s*(.*)$/.exec(line);
      if (entry) {
        depName = entry[1];
        depValue = entry[2];
        if (!/[{[]/.test(depValue) || /[}\]]/.test(depValue)) saveDependency();
      }
    }
  }
  saveDependency();
  return { members, pathDeps };
}

const addedDiffLines = (diff) => diff.split(/\r?\n/)
  .filter((line) => line.startsWith('+') && !line.startsWith('+++'))
  .map((line) => line.slice(1));

// Root-manifest graph evidence is sourced from added diff hunk lines and
// cross-checked against the base and head manifests. This prevents comment or
// formatting-only edits from becoming graph evidence.
export function cargoBuildGraphAdditions(diff, baseManifest, headManifest) {
  if (typeof diff !== 'string' || typeof baseManifest !== 'string' || typeof headManifest !== 'string') return null;
  const base = manifestGraph(baseManifest);
  const head = manifestGraph(headManifest);
  const additions = addedDiffLines(diff).map(withoutTomlComment);
  const addedLineMentions = (value) => additions.some((line) => line.includes(`"${value}"`));
  const membersAdded = [...head.members].filter((member) => !base.members.has(member) && addedLineMentions(member));
  const pathDepsAdded = [...head.pathDeps].filter(([name, path]) =>
    head.members.has(path) && addedLineMentions(path)
      && (!base.pathDeps.has(name) || base.pathDeps.get(name) !== path))
    .map(([, path]) => path);
  return { membersAdded, pathDepsAdded };
}

export function extractionPolicy(messages, changed, dirty, evidence) {
  if (!Array.isArray(dirty) || dirty.length) return null;
  if (!evidence || !Array.isArray(evidence.membersAdded) || !Array.isArray(evidence.pathDepsAdded) || !Array.isArray(evidence.renames)) return null;
  if (![...evidence.membersAdded, ...evidence.pathDepsAdded].every((path) => typeof path === 'string' && EXTRACTION_CRATE_PATH.test(path))) return null;
  const validRenames = evidence.renames.filter((rename) => rename && typeof rename.from === 'string' && typeof rename.to === 'string'
    && rename.from.startsWith('src/') && rename.to.startsWith('crates/'));
  if (!validRenames.length || (!evidence.membersAdded.length && !evidence.pathDepsAdded.length)) return null;

  const commits = Array.isArray(messages) ? messages : [messages];
  if (!commits.some((message) => {
    if (typeof message !== 'string') return false;
    const subject = message.split(/\r?\n/, 1)[0];
    return EXTRACTION_SUBJECT.test(subject) || EXTRACTION_TRAILER.test(message);
  })) return null;

  if (!(changed instanceof Map)) return null;
  const paths = [...changed.keys()];
  if (paths.some((path) => typeof path !== 'string' || EXTRACTION_FORBIDDEN.test(path))) return null;
  if (!paths.includes('Cargo.toml')) return null;
  if (paths.some((path) => GHOST_SOURCE_PATH.test(path))) return null;
  for (const [path, lines] of changed) {
    if (!isBuildInput(path) || !RUST_SOURCE_PATH.test(path)) continue;
    if (!Array.isArray(lines) || lines.some(ghostCodeLeak)) return null;
  }
  return {
    mode: 'extraction',
    irTolerance: 0.001,
    reason: 'declared crate extraction with Cargo build-graph changes',
  };
}

export function layerRestructuringPolicy(messages, changed, dirty, evidence) {
  if (!Array.isArray(dirty) || dirty.length || !(changed instanceof Map) || !evidence) return null;
  const commits = Array.isArray(messages) ? messages : [messages];
  let declarations = 0;
  for (const message of commits) {
    if (typeof message !== 'string') return null;
    const lines = message.split(/\r?\n/);
    let inBody = false;
    for (const line of lines.slice(1)) {
      if (line === '') { inBody = true; continue; }
      if (/^Extraction:\s*\S/.test(line) || (/^Restructuring:/.test(line) && line !== 'Restructuring: layers')) return null;
      if (!/^\s*Restructuring\s*:/i.test(line)) continue;
      if (!inBody || line !== 'Restructuring: layers') return null;
      declarations++;
    }
  }
  if (declarations !== 1) return null;

  const paths = [...changed.keys()];
  const baselineEdits = changed.get('docs/layering_baseline.csv');
  if (!Array.isArray(baselineEdits) || baselineEdits.length === 0
    || evidence.baseBaselineText === evidence.headBaselineText) return null;
  if (paths.some((path) => typeof path !== 'string' || LAYER_FORBIDDEN_PATH.test(path))) return null;
  if (paths.some((path) => GHOST_SOURCE_PATH.test(path))) return null;

  const renames = evidence.renames;
  if (!Array.isArray(renames) || renames.length === 0
    || !renames.every((rename) => rename && /^[RC]\d{1,3}$/.test(rename.status)
      && Number(rename.status.slice(1)) >= 1 && Number(rename.status.slice(1)) <= 100
      && typeof rename.from === 'string' && typeof rename.to === 'string')) return null;
  const validRenames = renames.filter((rename) => rename.status[0] === 'R'
    && rename.from !== rename.to
    && LAYER_SOURCE_PATH.test(rename.from) && LAYER_SOURCE_PATH.test(rename.to)
    && rename.sourceAbsent === true && rename.destinationPresent === true);
  if (validRenames.length === 0 || new Set(validRenames.map(({ from, to }) => `${from}\0${to}`)).size !== validRenames.length) return null;

  const scoped = extractionChangedLines(changed, validRenames, evidence.renameHunks);
  if (!(scoped instanceof Map)) return null;
  for (const [path, lines] of scoped) {
    if (!RUST_SOURCE_PATH.test(path)) continue;
    if (!Array.isArray(lines) || lines.some(ghostCodeLeak)) return null;
  }

  let baseRows, headRows;
  try {
    baseRows = validateLayerBaseline(evidence.baseBaselineText, parseBaselineText(evidence.baseBaselineText, 'base docs/layering_baseline.csv'));
    headRows = validateLayerBaseline(evidence.headBaselineText, parseBaselineText(evidence.headBaselineText, 'head docs/layering_baseline.csv'));
  } catch {
    return null;
  }
  const base = baselineMetrics(baseRows), head = baselineMetrics(headRows);
  if (head.edges > base.edges || head.total > base.total || (head.edges === base.edges && head.total === base.total)) return null;
  return {
    mode: 'layers',
    irTolerance: 0.001,
    reason: 'declared intra-crate layer restructuring with a strictly non-increasing, improving A1 baseline',
  };
}

const LAYER_SOURCE_PATH = /^src\/.+\.rs$/;
const LAYER_FORBIDDEN_PATH = /(?:^|\/)(?:Cargo\.(?:toml|lock)|build\.rs|\.cargo(?:\/.*)?|rust-toolchain[^/]*)$/;

function validateLayerBaseline(text, rows) {
  const header = typeof text === 'string'
    ? text.replace(/\r/g, '').split('\n').find((line) => line !== '' && !line.startsWith('#'))
    : null;
  if (header !== 'kind,path,count,reason' || !Array.isArray(rows)) throw new Error('baseline header or rows are malformed');
  const seen = new Set();
  let total = 0;
  for (const row of rows) {
    if (!row || row.kind !== 'backedge' || typeof row.edge !== 'string' || seen.has(row.edge)
      || !Number.isSafeInteger(row.count) || row.count <= 0 || !/^[^\s,]+->[^\s,]+$/.test(row.edge)) throw new Error('baseline row is malformed or duplicated');
    seen.add(row.edge);
    total += row.count;
    if (!Number.isSafeInteger(total)) throw new Error('baseline total is unsafe');
  }
  return rows;
}

function baselineMetrics(rows) {
  return { edges: rows.length, total: rows.reduce((sum, row) => sum + row.count, 0) };
}

// Git adapter boundary for range-derived evidence. The runner supplies only
// read-only git commands; tests inject an adapter to prove endpoint wiring.
export function deriveLayerRestructuringEvidence({ base, head, git }) {
  if (typeof base !== 'string' || typeof head !== 'string' || typeof git !== 'function') return null;
  try {
    const records = git(['diff', '--name-status', '--find-renames=50%', base, head]).split(/\r?\n/).filter(Boolean);
    const renames = [];
    for (const record of records) {
      const fields = record.split('\t');
      if (/^R\d{1,3}$/.test(fields[0] ?? '') && Number(fields[0].slice(1)) >= 1 && Number(fields[0].slice(1)) <= 100) {
        if (fields.length !== 3) return null;
        const [status, from, to] = fields;
        let sourceAbsent = false, destinationPresent = false;
        try { git(['cat-file', '-e', `${head}:${from}`]); } catch { sourceAbsent = true; }
        try { git(['cat-file', '-e', `${head}:${to}`]); destinationPresent = true; } catch { /* absent */ }
        renames.push({ status, from, to, sourceAbsent, destinationPresent });
      }
    }
    if (!renames.length) return null;
    const renameHunks = new Map();
    for (const rename of renames) {
      if (rename.status[0] !== 'R') continue;
      renameHunks.set(rename.to, changedLines(git([
        'diff', '-U0', '--no-color', '--no-ext-diff', '--find-renames=50%', base, head, '--', rename.from, rename.to,
      ])));
    }
    return {
      renames,
      renameHunks,
      baseBaselineText: git(['show', `${base}:docs/layering_baseline.csv`]),
      headBaselineText: git(['show', `${head}:docs/layering_baseline.csv`]),
    };
  } catch {
    return null;
  }
}

export function hasLayerRestructuringAttempt(messages) {
  const commits = Array.isArray(messages) ? messages : [messages];
  return commits.some((message) => typeof message === 'string'
    && message.split(/\r?\n/).slice(1).some((line) => /^\s*Restructuring\s*:/i.test(line)));
}

export function productionGhostTokens(text) {
  return [...new Set(text.match(/sefer_ghost|ghost_assert|ghost_event|ghost check|ghost::shadow|shadow::(?:record|count|reset|overflowed)/gi) ?? [])];
}

export function zeroCostVerdict({ policy = null, textSame, irBase, irHead, requireIr = false, ghostTokens = [] }) {
  const errors = [];
  if (ghostTokens.length) errors.push('production ghost tokens detected');

  const bounded = policy?.mode === 'extraction' || policy?.mode === 'layers';
  const irBaseValid = Number.isSafeInteger(irBase) && (bounded ? irBase > 0 : irBase >= 0);
  const irHeadValid = Number.isSafeInteger(irHead) && (bounded ? irHead > 0 : irHead >= 0);
  const validIr = irBaseValid && irHeadValid;

  if (policy?.mode === 'layers') {
    if (!irBaseValid || !irHeadValid) errors.push('layer restructuring requires positive safe-integer IR counts');
    else if (Math.abs(irHead - irBase) * 1000 > irBase) errors.push('IR delta exceeds layer-restructuring tolerance');
  } else if (policy?.mode === 'extraction') {
    if (!irBaseValid || !irHeadValid) errors.push('extraction requires positive safe-integer IR counts');
    else if (Math.abs(irHead - irBase) * 1000 > irBase) errors.push('IR delta exceeds extraction tolerance');
  } else {
    if (textSame !== true) errors.push('production text differs');
    const baseNull = irBase === null;
    const headNull = irHead === null;
    if ((!irBaseValid && !baseNull) || (!irHeadValid && !headNull)) errors.push('invalid IR count');
    if (requireIr && (baseNull || headNull)) errors.push('IR counts are required');
    else if (!baseNull && !headNull && irBaseValid && irHeadValid && irBase !== irHead) errors.push('IR counts differ');
  }

  return {
    ok: errors.length === 0,
    errors,
    mode: policy?.mode ?? 'exact',
    ir_delta: validIr ? irHead - irBase : null,
  };
}
