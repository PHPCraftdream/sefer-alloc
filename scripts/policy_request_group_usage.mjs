// P1.3 `LargeThpRequestGroup` usage report (cargo-free, node, no deps).
//
// Why: P1.3 moved the enum from the root crate to `crates/core/sefer-policy`
// and PRESERVES its `#[non_exhaustive]` (the public `sefer_alloc::
// LargeThpRequestGroup` keeps its contract). Cross-crate that attribute forces
// a `_ =>` arm on every `match`, so the total group->index mapping lives in
// the crate as `LargeThpRequestGroup::index()` (an exhaustive match that
// compiles in-crate) and callers use `of_bytes(..).index()`. This script lists
// every use of the type, asserts the attribute is present on the definition,
// asserts the root crate has NO match on the enum, and asserts the only match
// on the enum in sefer-policy is the exhaustive one inside the enum file.
//
// Sections (each scanned separately):
//   1. root crate  : src/**            (other crate than sefer-policy -> a
//                                       cross-crate #[non_exhaustive] would bite)
//   2. integration : tests/**          (each test file is its own crate -> would
//                                       also have needed wildcards)
//   3. sefer-policy: crates/core/sefer-policy/src/** (same crate: never needs one)
//
// Derivation (script-owned numbers, never typed): comments/strings are blanked,
// every `LargeThpRequestGroup` token (plus `Self::<Variant>` inside the file
// that defines the enum) is an occurrence; every `match` block whose arm
// PATTERNS name a variant is a match on the enum (a match merely producing a
// variant in an arm body, like `of_bytes`'s `bytes / MIB`, is not). Arms are
// parsed with brace/paren depth tracking; an unguarded `_` / lowercase
// binding alternative is a wildcard. Asserts: rows == sum of per-kind counts,
// per-match arm rows == arm rows found, every wildcard-free match covers all
// variants (else it would not compile), and the listed exhaustive-match count
// equals the derived one.
//
// Usage: node scripts/policy_request_group_usage.mjs
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const TYPE = 'LargeThpRequestGroup';
const VARIANTS = ['Under1MiB', 'MiB1To8', 'MiB8To32', 'MiB32To128', 'MiB128To512', 'Over512MiB'];
const ENUM_FILE = 'crates/core/sefer-policy/src/policy/large_thp/large_thp_request_group.rs';
const VAR_ALT = VARIANTS.join('|');

function fail(msg) {
  console.error(`[policy-request-group-usage] ASSERT FAILED: ${msg}`);
  process.exit(1);
}
function assert(cond, msg) {
  if (!cond) fail(msg);
}

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith('.rs')) out.push(p.replace(/\\/g, '/'));
  }
  return out.sort();
}

// Blank comments and string/char literals (same length, newlines kept).
function blank(src) {
  const out = src.split('');
  const n = src.length;
  const wipe = (a, b) => {
    for (let k = a; k < b; k++) if (out[k] !== '\n') out[k] = ' ';
  };
  let i = 0;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === '/' && d === '/') {
      let j = i;
      while (j < n && src[j] !== '\n') j++;
      wipe(i, j);
      i = j;
    } else if (c === '/' && d === '*') {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; }
        else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; }
        else j++;
      }
      wipe(i, j);
      i = j;
    } else if (c === 'r' && /^r#*"/.test(src.slice(i, i + 12)) && !/[A-Za-z0-9_]/.test(src[i - 1] ?? ' ')) {
      const hashes = /^r(#*)"/.exec(src.slice(i, i + 12))[1];
      const close = '"' + hashes;
      const j = src.indexOf(close, i + 2 + hashes.length);
      const end = j < 0 ? n : j + close.length;
      wipe(i, end);
      i = end;
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j += src[j] === '\\' ? 2 : 1;
      wipe(i, j + 1);
      i = j + 1;
    } else if (c === "'") {
      const m = /^'(\\.[^']*|[^\\'])'/.exec(src.slice(i, i + 12));
      if (m) { wipe(i, i + m[0].length); i += m[0].length; } else i++;
    } else i++;
  }
  return out.join('');
}

function matchClose(code, open) {
  let depth = 0;
  for (let k = open; k < code.length; k++) {
    const ch = code[k];
    if (ch === '{' || ch === '(' || ch === '[') depth++;
    else if (ch === '}' || ch === ')' || ch === ']') {
      depth--;
      if (depth === 0) return k;
    }
  }
  return -1;
}

function parseArms(code, start, end) {
  const arms = [];
  let i = start;
  while (i < end) {
    while (i < end && /[\s,]/.test(code[i])) i++;
    if (i >= end) break;
    let j = i;
    let depth = 0;
    while (j < end) {
      const ch = code[j];
      if (ch === '(' || ch === '[' || ch === '{') depth++;
      else if (ch === ')' || ch === ']' || ch === '}') depth--;
      else if (depth === 0 && ch === '=' && code[j + 1] === '>') break;
      j++;
    }
    if (j >= end) break; // trailing text without `=>`
    arms.push({ patStart: i, patEnd: j, pattern: code.slice(i, j) });
    j += 2;
    while (j < end && /\s/.test(code[j])) j++;
    if (code[j] === '{') {
      let k = matchClose(code, j);
      if (k < 0) k = end;
      j = k + 1;
      while (j < end && /\s/.test(code[j])) j++;
      if (code[j] === ',') j++;
    } else {
      let d = 0;
      while (j < end) {
        const ch = code[j];
        if (ch === '(' || ch === '[' || ch === '{') d++;
        else if (ch === ')' || ch === ']' || ch === '}') d--;
        else if (d === 0 && ch === ',') { j++; break; }
        j++;
      }
    }
    i = j;
  }
  return arms;
}

function splitTop(text, sep) {
  const parts = [];
  let depth = 0;
  let cur = '';
  for (const ch of text) {
    if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') depth--;
    if (depth === 0 && ch === sep) { parts.push(cur); cur = ''; } else cur += ch;
  }
  parts.push(cur);
  return parts;
}

function lineOf(src, off) {
  let line = 1;
  for (let k = 0; k < off; k++) if (src.charCodeAt(k) === 10) line++;
  return line;
}

// All `use ...;` statement spans in blanked code.
function useSpans(code) {
  const spans = [];
  const re = /(?<![A-Za-z0-9_])(?:pub(?:\s*\([^)]*\))?\s+)?use\s[^;]*;/g;
  let m;
  while ((m = re.exec(code))) spans.push({ start: m.index, end: m.index + m[0].length, text: m[0] });
  return spans;
}

function analyzeFile(file, section) {
  const raw = readFileSync(file, 'utf8');
  const code = blank(raw);
  const definesEnum = new RegExp(`\\benum\\s+${TYPE}\\b`).test(code);
  const uses = useSpans(code);

  // matches
  const matches = [];
  const mre = /\bmatch\b/g;
  let mm;
  while ((mm = mre.exec(code))) {
    let k = mm.index + 5;
    let depth = 0;
    while (k < code.length) {
      const ch = code[k];
      if (ch === '(' || ch === '[') depth++;
      else if (ch === ')' || ch === ']') depth--;
      else if (ch === '{' && depth === 0) break;
      k++;
    }
    if (k >= code.length) continue;
    const close = matchClose(code, k);
    if (close < 0) continue;
    const arms = parseArms(code, k + 1, close);
    const variantTok = new RegExp(`(?:\\b${TYPE}|\\bSelf)\\s*::\\s*(${VAR_ALT})\\b|\\b(${VAR_ALT})\\b`, 'g');
    let wildcard = false;
    const covered = new Set();
    let onEnum = false;
    let armTokens = 0;
    let guarded = 0;
    for (const a of arms) {
      const guardIdx = a.pattern.search(/\sif\s/);
      const pre = guardIdx < 0 ? a.pattern : a.pattern.slice(0, guardIdx);
      const vs = [...pre.matchAll(variantTok)].map((x) => x[1] ?? x[2]);
      if (vs.length) onEnum = true;
      if (guardIdx >= 0) { if (vs.length) guarded++; continue; }
      for (const v of vs) covered.add(v);
      for (const alt of splitTop(pre, '|')) {
        const t = alt.trim();
        if (t === '_' || /^(?:ref\s+|mut\s+)*[a-z_][a-z0-9_]*$/.test(t)) wildcard = true;
      }
    }
    if (onEnum) {
      for (const a of arms) {
        const qual = new RegExp(`(?:\\b${TYPE}|${definesEnum ? '\\bSelf' : '(?!)'})\\s*::\\s*(?:${VAR_ALT})\\b`, 'g');
        armTokens += [...a.pattern.matchAll(qual)].length;
      }
    }
    matches.push({
      file, section, offset: mm.index, line: lineOf(raw, mm.index), scrutStart: mm.index + 5, scrutEnd: k,
      arms, onEnum, wildcard, covered, guarded, armTokens,
    });
  }

  // occurrences
  const occ = [];
  const tre = new RegExp(`\\b${TYPE}\\b${definesEnum ? `|\\bSelf\\s*::\\s*(?:${VAR_ALT})\\b` : ''}`, 'g');
  let om;
  while ((om = tre.exec(raw))) occ.push({ off: om.index, text: om[0] });
  const rows = [];
  for (const o of occ) {
    const off = o.off;
    const line = lineOf(raw, off);
    const isComment = code[off] === ' ' || code[off] === undefined;
    const after = raw.slice(off + o.text.length, off + o.text.length + 40);
    const qual = /^\s*::\s*([A-Za-z0-9_]+)/.exec(after);
    const selfVar = o.text.startsWith('Self');
    const member = selfVar ? o.text.replace(/^Self\s*::\s*/, '') : qual ? qual[1] : null;
    let kind;
    let matchRef = '';
    let note = '';
    const armOf = matches.find((m) => m.onEnum && m.arms.some((a) => off >= a.patStart && off < a.patEnd));
    const scrutOf = matches.find((m) => off >= m.scrutStart && off < m.scrutEnd);
    if (isComment) { kind = 'other'; note = 'comment/doc mention'; }
    else if (uses.some((u) => off >= u.start && off < u.end)) {
      kind = 'import-reexport';
      const u = uses.find((x) => off >= x.start && off < x.end);
      note = /^pub/.test(u.text) ? 'pub use (re-export)' : 'use (import)';
    } else if (armOf && member && VARIANTS.includes(member)) {
      kind = 'match arm';
      matchRef = `match@${armOf.line}`;
    } else if (member === 'of_bytes') {
      kind = 'of_bytes call';
      const po = code.indexOf('(', off);
      const pc = po < 0 ? -1 : matchClose(code, po);
      if (pc >= 0 && /^\s*\.\s*index\s*\(\s*\)/.test(code.slice(pc + 1, pc + 40))) {
        const ix = code.indexOf('index', pc);
        rows.push({ file, line: lineOf(raw, ix), section, kind: 'index call', matchRef: '', note: '\`.index()\` on the \`of_bytes\` result', text: 'index' });
      }
      if (scrutOf) { matchRef = `scrutinee of match@${scrutOf.line}`; }
    } else if (member && VARIANTS.includes(member)) {
      kind = 'variant construction';
      if (scrutOf) matchRef = `in scrutinee of match@${scrutOf.line}`;
    } else if (member) { kind = 'other'; note = `associated item \`${member}\``; }
    else { kind = 'type mention'; }
    rows.push({ file, line, section, kind, matchRef, note, text: o.text.replace(/\s+/g, '') + (member && !selfVar ? '::' + member : '') });
  }
  return { file, matches, rows };
}

const SECTIONS = [
  { id: 'root', title: 'Root crate `src/**` (cross-crate vs `sefer-policy`: a `#[non_exhaustive]` enum would force wildcards here)', files: () => walk('src'), policyCrossCrate: true },
  { id: 'tests', title: 'Integration tests `tests/**` (each test file is its own crate: would also have needed wildcards)', files: () => walk('tests'), policyCrossCrate: true },
  { id: 'policy', title: 'Crate `crates/core/sefer-policy/src/**` (same crate as the enum: never needs a wildcard)', files: () => walk('crates/core/sefer-policy/src'), policyCrossCrate: false },
];

// ── enum definition sanity ────────────────────────────────────────────────
{
  const code = blank(readFileSync(ENUM_FILE, 'utf8'));
  const m = new RegExp(`enum\\s+${TYPE}\\s*\\{([^}]*)\\}`).exec(code);
  assert(m, 'enum definition not found');
  const declared = m[1].split(',').map((s) => s.trim()).filter(Boolean);
  assert(JSON.stringify(declared) === JSON.stringify(VARIANTS), `variant list drifted: ${declared}`);
  assert(new RegExp(`#\\[non_exhaustive\\]\\s*(?:#\\[[^\\]]*\\]\\s*)*pub\\s+enum\\s+${TYPE}\\b`).test(code), '#[non_exhaustive] must be PRESENT on the enum definition');
}

const out = [];
const P = (s = '') => out.push(s);
P('<!-- Generated by scripts/policy_request_group_usage.mjs — do not edit by hand. Command: `node scripts/policy_request_group_usage.mjs > docs/perf/P1_3_request_group_usage.md` -->');
P('');
P(`# P1.3 \`${TYPE}\` usage report`);
P('');
P(`\`#[non_exhaustive]\` on the enum definition: asserted PRESENT. Root \`src/**\` matches on the enum: asserted 0. sefer-policy matches on the enum: asserted exactly one (the exhaustive \`index\` match in the enum file).`);
P('');
P(`Variants (asserted equal to the enum definition in \`${ENUM_FILE}\`): ${VARIANTS.map((v) => `\`${v}\``).join(', ')} (${VARIANTS.length}).`);
P('');

const sectionResults = [];
for (const sec of SECTIONS) {
  const files = sec.files();
  const results = files.map((f) => analyzeFile(f, sec.id));
  const rows = results.flatMap((r) => r.rows);
  const matches = results.flatMap((r) => r.matches).filter((m) => m.onEnum);
  sectionResults.push({ sec, files, rows, matches });
}

let n = 0;
for (const { sec, files, rows, matches } of sectionResults) {
  n++;
  P(`## ${n}. ${sec.title}`);
  P('');
  const kinds = ['match arm', 'of_bytes call', 'index call', 'type mention', 'variant construction', 'import-reexport', 'other'];
  const perKind = Object.fromEntries(kinds.map((k) => [k, rows.filter((r) => r.kind === k).length]));
  const sum = Object.values(perKind).reduce((a, b) => a + b, 0);
  assert(sum === rows.length, `[${sec.id}] rows ${rows.length} != sum of per-kind ${sum}`);
  const armRowsFromMatches = matches.reduce((a, m) => a + m.armTokens, 0);
  assert(armRowsFromMatches === perKind['match arm'], `[${sec.id}] arm rows ${perKind['match arm']} != arm tokens in enum-matches ${armRowsFromMatches}`);
  for (const m of matches) {
    if (!m.wildcard) assert(m.covered.size === VARIANTS.length, `[${sec.id}] ${m.file}:${m.line} wildcard-free match covers ${m.covered.size}/${VARIANTS.length} variants`);
  }
  if (sec.id === 'root') assert(matches.length === 0, `[root] ${matches.length} match(es) on ${TYPE} in root src/** (must be 0: use .index())`);
  if (sec.id === 'policy') {
    assert(matches.length === 1 && matches[0].file === ENUM_FILE, `[policy] expected exactly one match on ${TYPE}, inside ${ENUM_FILE}; got ${matches.map((m) => m.file + ':' + m.line).join(', ')}`);
    assert(!matches[0].wildcard && matches[0].covered.size === VARIANTS.length && matches[0].arms.length === VARIANTS.length, '[policy] the index match must be exhaustive, wildcard-free, one arm per variant');
  }
  const exhaustive = matches.filter((m) => !m.wildcard && m.covered.size === VARIANTS.length);
  const withWildcard = matches.filter((m) => m.wildcard);
  P(`Files scanned: ${files.length}; files with a use: ${new Set(rows.map((r) => r.file)).size}; rows: ${rows.length} = ${kinds.map((k) => `${k} ${perKind[k]}`).join(' + ')}.`);
  P(`Matches on the enum: ${matches.length} (exhaustive without wildcard: ${exhaustive.length}; with an existing wildcard: ${withWildcard.length}; guarded variant arms ignored for coverage: ${matches.reduce((a, m) => a + m.guarded, 0)}).`);
  P('');
  if (rows.length) {
    P('| file:line | kind | match | note |');
    P('|---|---|---|---|');
    for (const r of rows) P(`| ${r.file}:${r.line} | ${r.kind} | ${r.matchRef || '—'} | ${r.note ? r.note : `\`${r.text}\``} |`);
    P('');
  }
  if (matches.length) {
    P('Matches on the enum:');
    P('');
    P('| match (file:line) | variants listed | wildcard arm | exhaustive w/o wildcard | wildcard required if `#[non_exhaustive]` cross-crate |');
    P('|---|---|---|---|---|');
    const needs = [];
    const tableRows = [];
    for (const m of matches) {
      const isEx = !m.wildcard && m.covered.size === VARIANTS.length;
      const req = sec.policyCrossCrate ? (m.wildcard ? 'no (already has one)' : 'YES') : 'no (same crate)';
      if (sec.policyCrossCrate && !m.wildcard) needs.push(m);
      const row = `| ${m.file}:${m.line} | ${[...m.covered].join(', ')} | ${m.wildcard ? 'yes' : 'no'} | ${isEx ? 'yes' : 'no'} | ${req} |`;
      tableRows.push(row);
      P(row);
    }
    P('');
    const listed = tableRows.filter((l) => l.endsWith('| YES |')).length;
    if (sec.policyCrossCrate) {
      assert(listed === needs.length, `[${sec.id}] listed YES rows ${listed} != derived ${needs.length}`);
      assert(needs.length === exhaustive.length, `[${sec.id}] needs-wildcard ${needs.length} != exhaustive-no-wildcard ${exhaustive.length}`);
      P(`Counters: exhaustive matches without wildcard = ${exhaustive.length}; would need a \`_ =>\` arm under cross-crate \`#[non_exhaustive]\` = ${needs.length} (asserted equal to the YES rows above).`);
    } else {
      P(`Counters: exhaustive matches without wildcard = ${exhaustive.length}; wildcard needed = 0 (same crate).`);
    }
    P('');
  }
}

// ── public reachability ──────────────────────────────────────────────────
function attrsAbove(rawLines, lineIdx) {
  const attrs = [];
  let i = lineIdx - 1;
  while (i >= 0) {
    const t = rawLines[i].trim();
    if (t.startsWith('#[') || t.startsWith('#![')) { attrs.unshift(t); i--; }
    else if (t.startsWith('//')) { i--; }
    else if (t.endsWith(']') || t.endsWith(')]')) {
      const buf = [t];
      let j = i - 1;
      while (j >= 0 && !rawLines[j].trim().startsWith('#[')) { buf.unshift(rawLines[j].trim()); j--; }
      if (j < 0) break;
      buf.unshift(rawLines[j].trim());
      attrs.unshift(buf.join(' '));
      i = j - 1;
    } else break;
  }
  return attrs;
}
function stmts(file, re, kw = 'use') {
  const raw = readFileSync(file, 'utf8');
  const code = blank(raw);
  const lines = raw.split('\n');
  const found = [];
  const sre = kw === 'use' ? /(?<![A-Za-z0-9_])(?:pub(?:\s*\([^)]*\))?\s+)?use\s[^;]*;/g : /(?<![A-Za-z0-9_])(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z0-9_]+\s*;/g;
  let m;
  while ((m = sre.exec(code))) {
    if (!re.test(m[0])) continue;
    const line = lineOf(raw, m.index);
    found.push({ file, line, text: m[0].replace(/\s+/g, ' '), attrs: attrsAbove(lines, line - 1) });
  }
  return found;
}
const hops = [
  { label: 'crate root re-export', stmts: stmts('src/lib.rs', new RegExp(`^pub\\s+use\\s+alloc_core::[\\s\\S]*\\b${TYPE}\\b`)) },
  { label: '`alloc_core` group re-export', stmts: stmts('src/alloc_core/mod.rs', new RegExp(`^pub\\s+use\\s+policy::large_thp::[\\s\\S]*\\b${TYPE}\\b`)) },
  { label: '`alloc_core::policy` module alias', stmts: stmts('src/alloc_core/mod.rs', /^pub\s+use\s+config::policy\s*;/) },
  { label: '`alloc_core::config` forwarding to sefer-policy', stmts: stmts('src/alloc_core/config/mod.rs', /^pub\s+use\s+sefer_policy::[\s\S]*\bpolicy\b/) },
  { label: '`mod alloc_core` declarations in lib.rs (decides external visibility of the `alloc_core::` path)', stmts: stmts('src/lib.rs', /alloc_core\s*;/, 'mod') },
];
P(`## ${++n}. Public reachability of \`${TYPE}\` from the root crate`);
P('');
for (const h of hops) assert(h.stmts.length >= 1, `reachability hop not found: ${h.label}`);
P('| hop | file:line | statement | attributes directly above |');
P('|---|---|---|---|');
for (const h of hops) {
  for (const s of h.stmts) P(`| ${h.label} | ${s.file}:${s.line} | \`${s.text}\` | ${s.attrs.length ? s.attrs.map((a) => `\`${a}\``).join(' ') : '—'} |`);
}
P('');
const top = hops[0].stmts[0];
const topAttrs = top.attrs.join(' ');
const hidden = /doc\s*\(\s*hidden\s*\)/.test(topAttrs);
const feats = [...topAttrs.matchAll(/feature\s*=\s*"([^"]+)"/g)].map((x) => x[1]);
const internalsGated = feats.some((f) => /internals/.test(f));
P(`Public path: \`sefer_alloc::${TYPE}\` via ${top.file}:${top.line}.`);
P(`- \`#[doc(hidden)]\` on that re-export: ${hidden ? 'YES' : 'no'}.`);
P(`- Feature gate on that re-export: ${feats.length ? feats.map((f) => `\`${f}\``).join(' + ') : 'none'}; \`internals\`-style gate: ${internalsGated ? 'YES' : 'no'}.`);
P(`- Verdict: ${!hidden && !internalsGated ? `part of the stable public API surface (when ${feats.map((f) => `\`${f}\``).join(' + ') || 'always'} enabled)` : 'NOT part of the stable public API surface (hidden / internals-gated)'}.`);
const alloc = hops[4].stmts;
P(`- The \`alloc_core::\` path itself is ${alloc.map((s) => `${/^pub\(crate\)/.test(s.text) ? 'crate-private' : s.attrs.some((a) => /doc\s*\(\s*hidden\s*\)/.test(a)) ? 'pub + doc-hidden' : 'pub'} at ${s.file}:${s.line}`).join(' / ')} (see the table); the root re-export above is what makes the type nameable without \`internals\`.`);
let headAttr = 'unavailable (git show failed)';
try {
  const head = execFileSync('git', ['show', 'HEAD:src/alloc_core/config/policy/large_thp/large_thp_request_group.rs'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  const hc = blank(head);
  headAttr = /#\[non_exhaustive\]\s*(?:#\[[^\]]*\]\s*)*pub\s+enum\s+LargeThpRequestGroup/.test(hc) ? '`#[non_exhaustive]` present' : '`#[non_exhaustive]` absent';
} catch { /* leave default */ }
const nowCode = blank(readFileSync(ENUM_FILE, 'utf8'));
const nowAttr = /#\[non_exhaustive\]/.test(nowCode) ? '`#[non_exhaustive]` present' : '`#[non_exhaustive]` absent';
P(`- Enum attribute at \`HEAD\` (old root location): ${headAttr}; in the working tree (\`${ENUM_FILE}\`): ${nowAttr}.`);
P('');

console.log(out.join('\n'));
